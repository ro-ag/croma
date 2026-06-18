#!/usr/bin/env python3
"""Benchmark croma's forward paths' throughput over a corpus (parse / export / fmt).

This is the thin black-box wrapper around the in-process throughput harness in
`crates/croma-fmt/tests/corpus_throughput.rs`. It runs that harness via
`cargo test --release … -- --ignored` with `ABC_ROOT` pointed at the corpus and
parses the harness's stable summary lines:

    bench corpus parse:  <N> files, <MB> MB total, <s> s, <files/s> files/s, <MB/s> MB/s
    bench corpus export: ...
    bench corpus fmt:    ...

reporting files/s + MB/s per path. The harness asserts >= 9000 files internally,
so a green cargo exit *and* the three parsed summary lines over a full corpus is
the bar. It is in-process (one process, corpus in memory) so the numbers reflect
library throughput, not per-file process-spawn overhead.

LOCAL ONLY — the corpus is external; provision it per AGENTS.md. Mirrors
`tools/prove_lsp_totality.py` in spirit (thin wrapper over a proven harness).
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

# One regex per path; each captures (files, MB_total, seconds, files/s, MB/s).
SUMMARY_RE = re.compile(
    r"bench corpus (\w+):\s*"
    r"(\d+)\s*files,\s*"
    r"([\d.]+)\s*MB total,\s*"
    r"([\d.]+)\s*s,\s*"
    r"([\d.]+)\s*files/s,\s*"
    r"([\d.]+)\s*MB/s"
)

# The minimum the in-process harness also enforces; surfaced here so the wrapper
# fails loudly on a mis-set ABC_ROOT even if cargo somehow exits 0.
MIN_CORPUS_FILES = 9_000


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--abc-root",
        default="docs/untracked/corpus/zenodo-10k/abc",
        help="directory of .abc files (sets ABC_ROOT for the harness)",
    )
    ap.add_argument(
        "--release",
        action="store_true",
        default=True,
        help="run the harness in release mode (default: on)",
    )
    ap.add_argument(
        "--debug",
        dest="release",
        action="store_false",
        help="run the harness in debug mode (slower, unrepresentative)",
    )
    args = ap.parse_args()

    # `cargo test` runs the test binary with cwd = the package dir
    # (crates/croma-fmt), so a relative ABC_ROOT would resolve there. The
    # in-process harness requires an ABSOLUTE ABC_ROOT — resolve it against the
    # *current* working directory before exporting.
    abc_root = Path(args.abc_root).resolve()
    if not abc_root.is_dir():
        print(f"ABC_ROOT does not exist: {abc_root}", file=sys.stderr)
        print("RESULT: FAIL (corpus directory missing)", file=sys.stderr)
        return 2

    cmd = [
        "cargo",
        "test",
        "-p",
        "croma-fmt",
    ]
    if args.release:
        cmd.append("--release")
    cmd += [
        "--test",
        "corpus_throughput",
        "--",
        "--ignored",
        "--nocapture",
    ]

    print(f"running: ABC_ROOT={abc_root} {' '.join(cmd)}", file=sys.stderr)

    proc = subprocess.run(
        cmd,
        env={**os.environ, "ABC_ROOT": str(abc_root)},
        capture_output=True,
        text=True,
    )

    # The summary lines are printed to stderr by the harness (eprintln!).
    combined = proc.stdout + proc.stderr
    results = {m.group(1): m for m in SUMMARY_RE.finditer(combined)}

    if not results:
        print(combined, file=sys.stderr)
        print(
            "RESULT: FAIL (no summary lines found — did the harness run? "
            "is ABC_ROOT correct and the corpus present?)",
            file=sys.stderr,
        )
        return 1

    print("corpus throughput (in-process, release):")
    files_seen = 0
    for path in ("parse", "export", "fmt"):
        m = results.get(path)
        if m is None:
            print(f"  {path:<7} (missing summary line)")
            continue
        files = int(m.group(2))
        mb_total = float(m.group(3))
        secs = float(m.group(4))
        files_per_s = float(m.group(5))
        mb_per_s = float(m.group(6))
        files_seen = max(files_seen, files)
        print(
            f"  {path:<7} {files} files, {mb_total:.1f} MB, {secs:.2f} s "
            f"-> {files_per_s:.0f} files/s, {mb_per_s:.1f} MB/s"
        )

    print(f"\ncargo exit:      {proc.returncode}")

    ok = (
        proc.returncode == 0
        and files_seen >= MIN_CORPUS_FILES
        and all(path in results for path in ("parse", "export", "fmt"))
    )
    if not ok:
        # Surface the tail of the output to explain a failure.
        print("\n--- harness output (tail) ---", file=sys.stderr)
        print("\n".join(combined.splitlines()[-40:]), file=sys.stderr)
    print("\nRESULT:", "PASS" if ok else "FAIL", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
