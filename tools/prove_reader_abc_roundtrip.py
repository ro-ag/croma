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

LOCAL ONLY — never wire this into CI. The corpus is external; provision it per
AGENTS.md. Report is written under docs/untracked/abc/.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import xml.etree.ElementTree as ET
from collections import Counter
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

    # The whole point: 0 structural diffs (and 0 reader/round-trip errors) on the
    # in-scope set.
    ok = not diffs and not errors
    print("\nRESULT:", "PASS" if ok else "FAIL", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
