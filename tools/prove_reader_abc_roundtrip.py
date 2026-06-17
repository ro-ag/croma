#!/usr/bin/env python3
"""Prove the MusicXML *reader* round-trips the corpus with no structural diff.

This is the real R1 metric. Full-ABC-document **byte** identity through the
reader is the wrong bar (it gets ~0/9935): `write_abc` emits ABC-only state
(the `X:` reference number, `W:` post-tune lyrics, `V:` ids, an empty display
`K:`) that the lossy MusicXML round-trip legitimately drops. The correct proof
is **structural**, mirroring `tools/prove_abc_roundtrip.py`: extract a
normalized musical-fact projection from the MusicXML and assert it survives the
round-trip unchanged.

This script REUSES the sibling's structural projection, in-scope filter, and
lower-failure bucketing verbatim (imported from `prove_abc_roundtrip` — the
projection semantics are defined in exactly one place and must NOT diverge). The
ONLY pipeline change is inserting the reader. Per in-scope `.abc` file, via the
`croma` binary built `--features musicxml-reader`:

  1. `croma xml FILE`              -> X1  (forward MusicXML)
  2. `croma read X1 --format abc`  -> ABC' (reader -> write_abc)   <- the NEW step
  3. `croma xml <ABC'>`            -> X2  (round-tripped MusicXML)

then extracts the structural projection from X1 and X2 and asserts they are
identical. A match proves the reader preserved every structural musical fact:
ABC -> X1 -> Score -> ABC' -> X2 is a fixed point of the projection.

Bar: **0 structural diffs over the in-scope subset.** Coverage (in_scope/total)
is reported so later slices can track growth. The `lower_fail` reason buckets
are carried over unchanged (files that never lower are not a reader concern).

For every structural-diff file the report also records the FIRST diverging fact
category (`classify_first_divergence`), mirroring the XML idempotence
first-divergence histogram, so the residual can be triaged by *what* broke
rather than a flat file list. The summary carries a `first_divergence_histogram`
{category: count} and a `category_files` {category: [tune ids]} map (id lists
capped, full counts in the histogram).

LOCAL ONLY — never wire this into CI. The corpus is external; provision it per
AGENTS.md. Report is written under docs/untracked/abc/.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import xml.etree.ElementTree as ET
from collections import Counter, defaultdict
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

# Reuse the projection semantics, in-scope filter, lower-failure bucketing, and
# temp-file helper from the sibling — the structural projection is defined ONCE
# (see tools/prove_abc_roundtrip.py) and must not be redefined differently here.
from prove_abc_roundtrip import (  # noqa: E402
    is_in_scope,
    lower_failure_reason,
    projection,
    write_temp,
)

CROMA = "target/debug/croma"

# Cap on per-category example id lists in the report. Full counts are always
# kept in `first_divergence_histogram`; only the file-id lists are truncated.
CATEGORY_FILES_CAP = 40

# Field index -> bucket for the note tuples ("N"/"C") emitted by `projection`:
#   ("N"|"C", step, alter, octave, dur, ties, slurs, decos, ratio, grace,
#    lyrics, voice_num)
# Index 0 is the tag itself (handled by the tag-mismatch path). The first
# differing field, scanned left to right, decides the bucket — so a slur-only
# drop classifies as `slur` even though the dur field sits earlier and matches.
_NOTE_FIELD_BUCKET = {
    1: "pitch",       # step
    2: "pitch",       # alter
    3: "pitch",       # octave
    4: "duration",    # dur (normalized by divisions)
    5: "tie",         # ties
    6: "slur",        # slurs
    7: "decoration",  # decos (fermata/articulation/ornament/technical)
    8: "tuplet",      # ratio (time-modification actual:normal)
    9: "grace",       # is_grace
    10: "lyric",      # lyrics
    11: "voice_structure",  # voice number
}

# Field index -> bucket for the rest tuple:
#   ("R", dur, slurs, decos, ratio, voice_num)
_REST_FIELD_BUCKET = {
    1: "duration",
    2: "slur",
    3: "decoration",
    4: "tuplet",
    5: "voice_structure",
}


def _first_field_diff(field_map: dict[int, str], a: tuple, b: tuple) -> str:
    """First left-to-right field index where two same-tag tuples differ.

    Returns the mapped bucket, or `other` if the only difference is in a field
    not present in the map (e.g. a length mismatch in two tuples that share a
    tag but were built by different code paths).
    """
    for index in range(1, max(len(a), len(b))):
        av = a[index] if index < len(a) else None
        bv = b[index] if index < len(b) else None
        if av != bv:
            return field_map.get(index, "other")
    return "other"


def classify_first_divergence(proj_a: list, proj_b: list) -> str:
    """Bucket the FIRST element where two structural projections diverge.

    Walks both projections in lock-step (they are emitted in document order)
    and classifies the earliest mismatch. The intent mirrors the XML
    idempotence first-divergence histogram: one stable, targetable category per
    failing file, derived purely from what `projection` already exposes.

    Buckets:
      measure_count   - PART/MEASURE skeleton differs (count or position); the
                        classic `y`-spacer / empty trailing-measure length bug.
      voice_structure - PART id, BACKUP/FORWARD overlay separators, or a note's
                        voice number differs (multi-voice `V:` vs `&` overlay).
      key_time        - KEY (fifths/accidentals) or TIME (meter) mismatch.
      barline_kind    - barline location / bar-style differs.
      repeat_order    - barline repeat direction or ending structure differs.
      pitch / duration / tie / slur / tuplet / decoration / grace / lyric
                      - the corresponding note/rest field differs.
      harmony         - chord-symbol <harmony> mismatch.
      element_kind    - same position, different element tag (e.g. a note where
                        the other side has a rest/barline) — a structural shape
                        change not captured by a single field.
      other           - none of the above (should be rare; worth inspecting).
    """
    skeleton = {"PART", "MEASURE"}
    for a, b in zip(proj_a, proj_b):
        if a == b:
            continue
        tag_a, tag_b = a[0], b[0]
        if tag_a != tag_b:
            # Different element shapes at the same position. A PART/MEASURE
            # boundary appearing where the other side has content (or vice
            # versa) is a skeleton-length divergence; anything else is a kind
            # swap (e.g. note vs rest, note vs barline).
            if tag_a in skeleton or tag_b in skeleton:
                return "measure_count"
            if {tag_a, tag_b} <= {"BACKUP", "FORWARD"}:
                return "voice_structure"
            return "element_kind"
        # Same tag, fields differ.
        if tag_a == "PART":
            return "voice_structure"
        if tag_a == "MEASURE":
            return "measure_count"
        if tag_a in ("BACKUP", "FORWARD"):
            return "voice_structure"
        if tag_a == "KEY" or tag_a == "TIME":
            return "key_time"
        if tag_a == "BAR":
            # ("BAR", location, bar-style, repeat-direction, endings)
            if a[3] != b[3] or a[4] != b[4]:
                return "repeat_order"
            return "barline_kind"
        if tag_a in ("N", "C"):
            return _first_field_diff(_NOTE_FIELD_BUCKET, a, b)
        if tag_a == "R":
            return _first_field_diff(_REST_FIELD_BUCKET, a, b)
        if tag_a == "HARMONY":
            return "harmony"
        return "other"
    # No positional mismatch within the common prefix: the projections differ
    # only in length, so one side has trailing elements. Classify by what the
    # longer side trails with — a trailing MEASURE/PART (or BACKUP/FORWARD) is
    # the empty-trailing-measure / overlay-length signature.
    longer = proj_a if len(proj_a) > len(proj_b) else proj_b
    tail = longer[min(len(proj_a), len(proj_b))]
    tail_tag = tail[0]
    if tail_tag in skeleton:
        return "measure_count"
    if tail_tag in ("BACKUP", "FORWARD"):
        return "voice_structure"
    if tail_tag in ("N", "C", "R"):
        return "duration"
    if tail_tag == "BAR":
        return "barline_kind"
    return "other"


def _init_worker(croma: str) -> None:
    global CROMA
    CROMA = croma


def run(args: list[str]) -> tuple[int, str, str]:
    proc = subprocess.run(args, capture_output=True, text=True)
    return proc.returncode, proc.stdout, proc.stderr


def check_one(abc_path_str: str) -> dict:
    name = Path(abc_path_str).name
    rec = {"file": name, "in_scope": False, "diff": False, "error": False}

    # Lower gate (mirrors the sibling): a parse/lower failure is not a reader
    # concern, so it is out of scope and bucketed by reason.
    code, score_dump, score_err = run([CROMA, "dump", "score", abc_path_str])
    if code != 0 or not score_dump:
        rec["status"] = "lower_fail"
        rec["reason"] = lower_failure_reason(score_err)
        return rec
    source = Path(abc_path_str).read_text(errors="replace")
    if not is_in_scope(source):
        return rec
    rec["in_scope"] = True

    # 1. forward: ABC -> X1
    code_x1, xml_original, _ = run([CROMA, "xml", abc_path_str])
    if code_x1 != 0 or not xml_original:
        rec["error"] = True
        return rec

    # 2. the NEW step: read X1 -> ABC' (reader -> write_abc). The reader needs
    # the forward MusicXML on disk, so X1 is staged to a temp file.
    x1_path = write_temp(xml_original)
    try:
        code_abc, regenerated, _ = run(
            [CROMA, "read", x1_path, "--format", "abc"]
        )
    finally:
        Path(x1_path).unlink(missing_ok=True)
    if code_abc != 0 or not regenerated:
        rec["error"] = True
        return rec

    # 3. round-trip: ABC' -> X2
    regen_path = write_temp(regenerated)
    try:
        code_x2, xml_roundtrip, _ = run([CROMA, "xml", regen_path])
    finally:
        Path(regen_path).unlink(missing_ok=True)
    if code_x2 != 0 or not xml_roundtrip:
        rec["error"] = True
        return rec

    try:
        proj_original = projection(xml_original)
        proj_roundtrip = projection(xml_roundtrip)
        rec["diff"] = proj_original != proj_roundtrip
        if rec["diff"]:
            # Record the FIRST diverging fact category so fixes can be targeted
            # (the flat diff-file list alone gives no signal about *what* broke).
            rec["category"] = classify_first_divergence(
                proj_original, proj_roundtrip
            )
    except ET.ParseError:
        rec["error"] = True
    return rec


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--abc-root", required=True, help="directory of .abc files")
    ap.add_argument("--croma", default=CROMA, help="path to the croma binary")
    ap.add_argument("--jobs", type=int, default=0, help="workers (0 = cpu count)")
    ap.add_argument("--limit", type=int, default=0, help="cap files (0 = all)")
    ap.add_argument(
        "--out",
        default="docs/untracked/abc/reader-abc-roundtrip-report.json",
        help="report JSON path",
    )
    args = ap.parse_args()

    files = sorted(str(p) for p in Path(args.abc_root).glob("*.abc"))
    if args.limit:
        files = files[: args.limit]
    if not files:
        print(f"no .abc files under {args.abc_root}", file=sys.stderr)
        return 2

    records = []
    with ProcessPoolExecutor(
        max_workers=args.jobs or None,
        initializer=_init_worker,
        initargs=(args.croma,),
    ) as pool:
        for index, rec in enumerate(pool.map(check_one, files, chunksize=16), 1):
            records.append(rec)
            if index % 1000 == 0:
                print(f"  {index}/{len(files)}", file=sys.stderr)

    in_scope = [r for r in records if r["in_scope"]]
    diff_recs = [r for r in in_scope if r["diff"]]
    diffs = [r["file"] for r in diff_recs]
    errors = [r["file"] for r in in_scope if r["error"]]
    lower_fails = [r for r in records if r.get("status") == "lower_fail"]
    lower_fail_reasons = Counter(r["reason"] for r in lower_fails)
    total = len(records)
    coverage = (len(in_scope) / total * 100.0) if total else 0.0

    # First-divergence histogram + per-category example ids (counts are full;
    # the id lists are capped so the report stays small but every bucket keeps
    # a handful of concrete tunes to triage).
    first_divergence = Counter(r.get("category", "other") for r in diff_recs)
    category_files: dict[str, list[str]] = defaultdict(list)
    for r in diff_recs:
        category_files[r.get("category", "other")].append(r["file"])
    category_files_capped = {
        cat: files[:CATEGORY_FILES_CAP]
        for cat, files in sorted(
            category_files.items(), key=lambda kv: (-len(kv[1]), kv[0])
        )
    }

    summary = {
        "total": total,
        "in_scope": len(in_scope),
        "coverage_pct": round(coverage, 2),
        "structural_diffs": len(diffs),
        "errors": len(errors),
        "lower_fail": len(lower_fails),
        "lower_fail_reasons": dict(lower_fail_reasons.most_common()),
        "first_divergence_histogram": dict(first_divergence.most_common()),
        "category_files": category_files_capped,
        "structural_diff_files": diffs[:50],
        "error_files": errors[:50],
    }

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps({"summary": summary, "records": records}, indent=2))

    print(json.dumps(summary, indent=2))
    print(f"\nreport: {out_path}", file=sys.stderr)

    # The whole point: 0 structural diffs (and 0 reader/round-trip errors) on the
    # in-scope set.
    ok = not diffs and not errors
    print("\nRESULT:", "PASS" if ok else "FAIL", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
