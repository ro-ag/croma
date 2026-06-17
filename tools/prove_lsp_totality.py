#!/usr/bin/env python3
"""Prove the croma LSP analysis layer is *total* over a corpus (promotion-bar leg C).

This is the black-box companion to the in-process `corpus_proof` module in
`crates/croma-lsp`. It runs that module's totality test via `cargo test` with
`ABC_ROOT` pointed at the corpus and parses the harness's summary line:

    lsp totality: <N> files, <P> panics

reporting a human-readable PASS/FAIL plus the file/panic counts. The harness
asserts 0 panics and >= 9000 files internally, so a green cargo exit *and* a
parsed `panics == 0` over a full corpus is the bar.

LOCAL ONLY — the corpus is external; provision it per AGENTS.md. Mirrors
`tools/prove_fmt_lossless.py` in spirit (thin wrapper over the proven gate).
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

SUMMARY_RE = re.compile(r"lsp totality:\s*(\d+)\s*files,\s*(\d+)\s*panics")

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
        help="run the harness in debug mode",
    )
    args = ap.parse_args()

    # `cargo test` runs the test binary with cwd = the package dir
    # (crates/croma-lsp), so a relative ABC_ROOT would resolve there. Make it
    # absolute against the *current* working directory.
    abc_root = Path(args.abc_root).resolve()
    if not abc_root.is_dir():
        print(f"ABC_ROOT does not exist: {abc_root}", file=sys.stderr)
        print("RESULT: FAIL (corpus directory missing)", file=sys.stderr)
        return 2

    cmd = [
        "cargo",
        "test",
        "-p",
        "croma-lsp",
    ]
    if args.release:
        cmd.append("--release")
    cmd += [
        "lsp_analysis_is_total_over_the_corpus",
        "--",
        "--nocapture",
    ]

    print(f"running: ABC_ROOT={abc_root} {' '.join(cmd)}", file=sys.stderr)

    proc = subprocess.run(
        cmd,
        env={**os.environ, "ABC_ROOT": str(abc_root)},
        capture_output=True,
        text=True,
    )

    # The summary line is printed to stderr by the harness (eprintln!).
    combined = proc.stdout + proc.stderr
    match = SUMMARY_RE.search(combined)

    if match is None:
        print(combined, file=sys.stderr)
        print(
            "RESULT: FAIL (no summary line found — did the harness run? "
            "is ABC_ROOT correct and the corpus present?)",
            file=sys.stderr,
        )
        return 1

    files = int(match.group(1))
    panics = int(match.group(2))

    print(f"files processed: {files}")
    print(f"panics:          {panics}")
    print(f"cargo exit:      {proc.returncode}")

    ok = proc.returncode == 0 and panics == 0 and files >= MIN_CORPUS_FILES
    if not ok:
        # Surface the tail of the output to explain a failure.
        print("\n--- harness output (tail) ---", file=sys.stderr)
        print("\n".join(combined.splitlines()[-40:]), file=sys.stderr)
    print("\nRESULT:", "PASS" if ok else "FAIL", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
