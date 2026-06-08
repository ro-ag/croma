#!/usr/bin/env python3
"""Prove `croma fmt --auto-fix` is note-lossless and idempotent over a corpus.

For every ABC file under --abc-root this runs, via the built `croma` binary:

  1. `croma fmt --auto-fix FILE`            -> the auto-fixed source
  2. `croma fmt <auto-fixed>`               -> idempotency check (must be stable)
  3. `croma xml FILE`  and  `croma xml <auto-fixed>`

then extracts the ordered pitch sequence (step, alter, octave) from BOTH
MusicXML outputs (mirroring tools/prove_divergences.py's `pitch_seq`) and asserts
they are identical. The bar mirrors the parser phases: **0 tunes may change their
note sequence**. Idempotency failures must also be 0.

LOCAL ONLY — never wire this into CI. The corpus is external; provision it per
AGENTS.md. Report is written under docs/untracked/fmt/.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

PITCH_RE = re.compile(
    r"<step>([A-Ga-g])</step>\s*(?:<alter>(-?\d+(?:\.\d+)?)</alter>\s*)?<octave>(\d+)</octave>"
)

CROMA = "target/debug/croma"


def pitch_seq(xml: str):
    """Ordered (step, alter, octave) for every <pitch>, absent alter -> 0.0."""
    return [
        (s.upper(), float(a) if a else 0.0, int(o))
        for s, a, o in PITCH_RE.findall(xml)
    ]


def _init_worker(croma: str) -> None:
    global CROMA
    CROMA = croma


def run(args: list[str]) -> tuple[int, str]:
    proc = subprocess.run(args, capture_output=True, text=True)
    return proc.returncode, proc.stdout


def check_one(abc_path_str: str) -> dict:
    abc_path = Path(abc_path_str)
    name = abc_path.name
    rec = {
        "file": name,
        "notes_changed": False,
        "not_idempotent": False,
        "auto_fixed": False,
        "fmt_error": False,
    }

    original = abc_path.read_text(errors="replace")
    code, fixed = run([CROMA, "fmt", "--auto-fix", str(abc_path)])
    if code != 0:
        rec["fmt_error"] = True
        return rec
    rec["auto_fixed"] = fixed != original

    with tempfile.NamedTemporaryFile(
        "w", suffix=".abc", delete=False, errors="replace"
    ) as tmp:
        tmp.write(fixed)
        tmp_path = tmp.name
    try:
        # Idempotency: re-formatting the fixed source must be stable.
        _, reformatted = run([CROMA, "fmt", tmp_path])
        rec["not_idempotent"] = reformatted != fixed

        _, xml_original = run([CROMA, "xml", str(abc_path)])
        _, xml_fixed = run([CROMA, "xml", tmp_path])
        rec["notes_changed"] = pitch_seq(xml_original) != pitch_seq(xml_fixed)
    finally:
        Path(tmp_path).unlink(missing_ok=True)

    return rec


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--abc-root", required=True, help="directory of .abc files")
    ap.add_argument("--croma", default=CROMA, help="path to the croma binary")
    ap.add_argument("--jobs", type=int, default=0, help="workers (0 = cpu count)")
    ap.add_argument("--limit", type=int, default=0, help="cap files (0 = all)")
    ap.add_argument(
        "--out",
        default="docs/untracked/fmt/fmt-lossless-report.json",
        help="report JSON path",
    )
    args = ap.parse_args()

    files = sorted(str(p) for p in Path(args.abc_root).glob("*.abc"))
    if args.limit:
        files = files[: args.limit]
    if not files:
        print(f"no .abc files under {args.abc_root}", file=sys.stderr)
        return 2

    jobs = args.jobs or None
    records = []
    with ProcessPoolExecutor(
        max_workers=jobs, initializer=_init_worker, initargs=(args.croma,)
    ) as pool:
        for index, rec in enumerate(pool.map(check_one, files, chunksize=16), 1):
            records.append(rec)
            if index % 1000 == 0:
                print(f"  {index}/{len(files)}", file=sys.stderr)

    notes_changed = [r["file"] for r in records if r["notes_changed"]]
    not_idempotent = [r["file"] for r in records if r["not_idempotent"]]
    auto_fixed = sum(r["auto_fixed"] for r in records)
    fmt_errors = sum(r["fmt_error"] for r in records)

    summary = {
        "total": len(records),
        "notes_changed": len(notes_changed),
        "not_idempotent": len(not_idempotent),
        "auto_fixed": auto_fixed,
        "fmt_errors": fmt_errors,
        "notes_changed_files": notes_changed[:50],
        "not_idempotent_files": not_idempotent[:50],
    }

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps({"summary": summary, "records": records}, indent=2))

    print(json.dumps(summary, indent=2))
    print(f"\nreport: {out_path}", file=sys.stderr)

    # The whole point: auto-fix must change 0 notes and stay idempotent.
    ok = not notes_changed and not not_idempotent
    print("\nRESULT:", "PASS" if ok else "FAIL", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
