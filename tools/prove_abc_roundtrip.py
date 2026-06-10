#!/usr/bin/env python3
"""Prove the `Score -> ABC` writer round-trips the corpus with no structural diff.

For every ABC file under --abc-root this runs, via the built `croma` binary:

  1. `croma dump score FILE`     -> must lower; a failure is recorded as
                                    `lower_fail` with a normalized reason
  2. `croma xml FILE`            -> original MusicXML
  3. `croma dump abc FILE`       -> regenerated ABC (the writer under test)
  4. `croma xml <regenerated>`   -> round-tripped MusicXML

then extracts a STRUCTURAL PROJECTION from both MusicXML outputs and asserts
they are identical. The projection is the normalized musical-fact stream:
ordered pitches (step, alter, octave) + per-event durations (normalized by
`<divisions>`, so a different `L:` choice is not a false diff) + rest durations
+ measure boundaries + barline / repeat / ending structure + ties.

Bar: **0 structural diffs over the in-scope subset.** Coverage (in_scope/total)
is reported so later slices can track growth.

In-scope filter (`is_in_scope`, applied to the ABC source): a tune is out of
scope if its body carries a mid-tune key change (a second `K:` line or an
inline `[K:...]` — the writer emits only the header `K:`), a bare-grace slur
(`({Bc})`, a slur wrapping only a grace group with no main note), or a nested
tuplet (`(7:8:8(3...` — the writer flattens the outer tuplet). Tunes that
fail to lower are not a writer concern; they are excluded from scope and
tallied separately in the summary as `lower_fail` with a reason→count bucket.

LOCAL ONLY — never wire this into CI. The corpus is external; provision it per
AGENTS.md. Report is written under docs/untracked/abc/.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from fractions import Fraction
from pathlib import Path

CROMA = "target/debug/croma"

# Mid-tune key field, inline (`[K:...]`) or as a standalone body line. The writer
# emits only the header `K:`, and a mid-tune key change is not even preserved in
# the lowered `Score` (its effect is baked into note alters), so such tunes
# cannot round-trip and are out of scope. Detected from the source.
_HEADER_KEY_LINE_RE = re.compile(r"^\s*K:", re.MULTILINE)
_INLINE_KEY_RE = re.compile(r"\[K:")
# A slur that wraps only a grace group with no main note (`({Bc})`): the grace
# close is immediately followed by the slur close. Degenerate; out of scope.
_BARE_GRACE_SLUR_RE = re.compile(r"\}\)")
# A tuplet opened inside another tuplet (`(7:8:8(3A/A/ ...`, abcm2ps nested
# tuplets): the writer keeps only the innermost tuplet — doubly-nested notes
# get the outer ratio baked into their written durations, while outer-only
# notes are written plain, so both the outer <tuplet> notation and the
# outer-only durations are lost on the round trip. Out of scope until the
# writer models nested tuplets. Detected as two consecutive tuplet opens.
_NESTED_TUPLET_RE = re.compile(r"\(\d+(?::\d*){0,2}\s*\(\d")


def _init_worker(croma: str) -> None:
    global CROMA
    CROMA = croma


def run(args: list[str]) -> tuple[int, str, str]:
    proc = subprocess.run(args, capture_output=True, text=True)
    return proc.returncode, proc.stdout, proc.stderr


def lower_failure_reason(stderr: str) -> str:
    """Normalized bucket key for a `croma dump score` failure.

    Takes the first `error[...]` or `panicked` line of stderr and strips file
    paths / byte spans so identical failures on different tunes bucket
    together (e.g. `error[abc.file.no_music]: ABC source does not contain
    body music`).
    """
    lines = stderr.splitlines()
    for index, line in enumerate(lines):
        if "error[" in line:
            # `<path>:<span>: error[code]: message` -> `error[code]: message`.
            return line[line.index("error["):].strip()
        if "panicked" in line:
            # `thread '...' panicked at <path>:<line>:<col>:` with the message
            # on the following line: drop the location, keep the message.
            head = re.sub(r"\s+at\s+\S+:\d+:\d+:?", "", line).strip()
            message = lines[index + 1].strip() if index + 1 < len(lines) else ""
            return f"{head}: {message}" if message else head
    return "no error/panic line on stderr"


def has_mid_tune_field_change(source: str) -> bool:
    """True iff the ABC body carries a key change after the header `K:`.

    Anchored on a header `K:` (which ABC requires to terminate the tune header).
    A pathological tune with only an inline `[K:...]` and no header key is not
    flagged here, but such input fails to lower and is dropped at `check_one`
    (the parser, not this regex, is the backstop for that case).
    """
    first = _HEADER_KEY_LINE_RE.search(source)
    if first is None:
        return False
    body = source[first.end():]
    return bool(_HEADER_KEY_LINE_RE.search(body) or _INLINE_KEY_RE.search(body))


def is_in_scope(source: str) -> bool:
    """True iff the ABC source uses only currently-supported constructs."""
    if has_mid_tune_field_change(source):
        return False
    if _BARE_GRACE_SLUR_RE.search(source):
        return False
    return not _NESTED_TUPLET_RE.search(source)


def projection(xml: str):
    """Structural projection of a MusicXML document, in document order.

    Durations are normalized by the active `<divisions>` so the absolute unit
    (`L:`) is irrelevant — only the musical fraction matters.
    """
    root = ET.fromstring(xml)
    proj: list[tuple] = []
    for part in root.findall("part"):
        proj.append(("PART", part.get("id")))
        divisions = 1
        for measure in part.findall("measure"):
            proj.append(("MEASURE",))
            for el in measure:
                if el.tag == "attributes":
                    div = el.findtext("divisions")
                    if div:
                        divisions = int(div)
                elif el.tag in ("backup", "forward"):
                    # Overlay/multi-voice separators: position + duration.
                    proj.append((
                        el.tag.upper(),
                        Fraction(int(el.findtext("duration") or 0), divisions),
                    ))
                elif el.tag == "note":
                    dur_text = el.findtext("duration")
                    dur = (
                        Fraction(int(dur_text), divisions)
                        if dur_text is not None
                        else None
                    )
                    ties = tuple(sorted(t.get("type") for t in el.findall("tie")))
                    # Slur start/stop on this note (number is not compared — it
                    # can be renumbered — only the per-note start/stop pattern).
                    slurs = tuple(sorted(s.get("type") for s in el.iter("slur")))
                    # Decorations: all element tags under <notations> except the
                    # slur/tied markers handled separately (fermata, articulations
                    # /staccato, ornaments/trill-mark, technical/up-bow, ...).
                    notations = el.find("notations")
                    decos = (
                        tuple(sorted(
                            e.tag for e in notations.iter()
                            if e.tag not in ("notations", "slur", "tied")
                        ))
                        if notations is not None
                        else ()
                    )
                    # Tuplet ratio (actual:normal) from <time-modification>.
                    tmod = el.find("time-modification")
                    ratio = (
                        (tmod.findtext("actual-notes"), tmod.findtext("normal-notes"))
                        if tmod is not None
                        else None
                    )
                    grace = el.find("grace")
                    # None / "grace" / "grace:yes" (acciaccatura slash).
                    is_grace = (
                        f"grace:{grace.get('slash')}" if grace is not None else None
                    )
                    # Lyric syllables: (verse number, text or extend marker),
                    # in document order.
                    lyrics = tuple(
                        (
                            ly.get("number"),
                            "<extend>" if ly.find("extend") is not None
                            else ly.findtext("text"),
                        )
                        for ly in el.findall("lyric")
                    )
                    voice_num = el.findtext("voice")
                    is_chord = el.find("chord") is not None
                    if el.find("rest") is not None:
                        proj.append(("R", dur, slurs, decos, ratio, voice_num))
                    else:
                        pitch = el.find("pitch")
                        step = pitch.findtext("step") if pitch is not None else None
                        alter = pitch.findtext("alter") if pitch is not None else None
                        octave = pitch.findtext("octave") if pitch is not None else None
                        proj.append(
                            (
                                "C" if is_chord else "N",
                                step,
                                alter or "0",
                                octave,
                                dur,
                                ties,
                                slurs,
                                decos,
                                ratio,
                                is_grace,
                                lyrics,
                                voice_num,
                            )
                        )
                elif el.tag == "harmony":
                    # Chord symbol -> <harmony>: root + chord text (kind@text).
                    root = el.find("root")
                    kind = el.find("kind")
                    proj.append((
                        "HARMONY",
                        root.findtext("root-step") if root is not None else None,
                        root.findtext("root-alter") if root is not None else None,
                        kind.get("text") if kind is not None else None,
                    ))
                # NOTE: <direction> (annotations, tempo text, dynamics) is
                # intentionally NOT projected. It conflates annotation `"text"`
                # with tempo text (`Q:"Moderato"`), which the writer drops by
                # design (tempo is metadata, not structural music — see design
                # doc). Annotations are emitted verbatim and unit-tested instead.
                elif el.tag == "barline":
                    rep = el.find("repeat")
                    proj.append(
                        (
                            "BAR",
                            el.get("location"),
                            el.findtext("bar-style"),
                            rep.get("direction") if rep is not None else None,
                            tuple(
                                (e.get("number"), e.get("type"))
                                for e in el.findall("ending")
                            ),
                        )
                    )
    return proj


def write_temp(text: str) -> str:
    with tempfile.NamedTemporaryFile(
        "w", suffix=".abc", delete=False, errors="replace"
    ) as tmp:
        tmp.write(text)
        return tmp.name


def check_one(abc_path_str: str) -> dict:
    name = Path(abc_path_str).name
    rec = {"file": name, "in_scope": False, "diff": False, "error": False}

    code, score_dump, score_err = run([CROMA, "dump", "score", abc_path_str])
    if code != 0 or not score_dump:
        # Parse/lower failure -> not in scope, not a writer concern; labeled
        # so the summary can bucket why tunes never reach the writer.
        rec["status"] = "lower_fail"
        rec["reason"] = lower_failure_reason(score_err)
        return rec
    source = Path(abc_path_str).read_text(errors="replace")
    if not is_in_scope(source):
        return rec
    rec["in_scope"] = True

    _, xml_original, _ = run([CROMA, "xml", abc_path_str])
    code_abc, regenerated, _ = run([CROMA, "dump", "abc", abc_path_str])
    if code_abc != 0 or not regenerated:
        rec["error"] = True
        return rec

    regen_path = write_temp(regenerated)
    try:
        _, xml_roundtrip, _ = run([CROMA, "xml", regen_path])
    finally:
        Path(regen_path).unlink(missing_ok=True)

    try:
        rec["diff"] = projection(xml_original) != projection(xml_roundtrip)
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
        default="docs/untracked/abc/abc-roundtrip-report.json",
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
    diffs = [r["file"] for r in in_scope if r["diff"]]
    errors = [r["file"] for r in in_scope if r["error"]]
    lower_fails = [r for r in records if r.get("status") == "lower_fail"]
    lower_fail_reasons = Counter(r["reason"] for r in lower_fails)
    total = len(records)
    coverage = (len(in_scope) / total * 100.0) if total else 0.0

    summary = {
        "total": total,
        "in_scope": len(in_scope),
        "coverage_pct": round(coverage, 2),
        "structural_diffs": len(diffs),
        "errors": len(errors),
        "lower_fail": len(lower_fails),
        "lower_fail_reasons": dict(lower_fail_reasons.most_common()),
        "structural_diff_files": diffs[:50],
        "error_files": errors[:50],
    }

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps({"summary": summary, "records": records}, indent=2))

    print(json.dumps(summary, indent=2))
    print(f"\nreport: {out_path}", file=sys.stderr)

    # The whole point: 0 structural diffs (and 0 writer errors) on the in-scope set.
    ok = not diffs and not errors
    print("\nRESULT:", "PASS" if ok else "FAIL", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
