#!/usr/bin/env python3
"""Prove the croma LSP analysis layer is *faithful* to the core over a corpus.

This is the black-box companion to the in-process `corpus_proof` module in
`crates/croma-lsp`. It runs that module's fidelity test (legs A/B/D of the LSP
promotion bar) via `cargo test` with `ABC_ROOT` pointed at the corpus and parses
the harness's three summary lines:

    lsp leg A diagnostics: <N> files, <M> mismatches
    lsp leg B formatting: <N> files, <M> mismatches
    lsp leg D tokens: <N> files, <M> violations

reporting a human-readable PASS/FAIL plus the per-leg counts. Each leg must be
0 over a full (>= 9000-file) corpus, which is exactly what the harness asserts
internally; this wrapper surfaces the numbers and a green/red verdict.

  * Leg A — diagnostics fidelity: the LSP `diagnostics()` set equals the core
    `analyze_document()` diagnostics (same count, matching (severity, code) in
    order, each Range reversing to the originating byte span).
  * Leg B — formatting identity: applying the `formatting()` edit equals
    `croma_fmt::format(src)` byte-for-byte.
  * Leg D — semantic-token correctness: emitted token spans cover exactly the
    non-whitespace parser-token bytes, are non-overlapping and in-bounds, and the
    delta-encoding is monotonic.

LOCAL ONLY — the corpus is external; provision it per AGENTS.md. Mirrors
`tools/prove_lsp_totality.py` (leg C) and `tools/prove_fmt_lossless.py` in spirit
(a thin wrapper over the proven in-process gate).
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

LEG_A_RE = re.compile(r"lsp leg A diagnostics:\s*(\d+)\s*files,\s*(\d+)\s*mismatches")
LEG_B_RE = re.compile(r"lsp leg B formatting:\s*(\d+)\s*files,\s*(\d+)\s*mismatches")
LEG_D_RE = re.compile(r"lsp leg D tokens:\s*(\d+)\s*files,\s*(\d+)\s*violations")

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

    cmd = ["cargo", "test", "-p", "croma-lsp"]
    if args.release:
        cmd.append("--release")
    cmd += [
        "lsp_fidelity_legs_abd_over_the_corpus",
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

    combined = proc.stdout + proc.stderr
    leg_a = LEG_A_RE.search(combined)
    leg_b = LEG_B_RE.search(combined)
    leg_d = LEG_D_RE.search(combined)

    if not (leg_a and leg_b and leg_d):
        print(combined, file=sys.stderr)
        print(
            "RESULT: FAIL (missing a summary line — did the harness run? "
            "is ABC_ROOT correct and the corpus present?)",
            file=sys.stderr,
        )
        return 1

    a_files, a_bad = int(leg_a.group(1)), int(leg_a.group(2))
    b_files, b_bad = int(leg_b.group(1)), int(leg_b.group(2))
    d_files, d_bad = int(leg_d.group(1)), int(leg_d.group(2))

    print(f"leg A diagnostics: {a_files} files, {a_bad} mismatches")
    print(f"leg B formatting:  {b_files} files, {b_bad} mismatches")
    print(f"leg D tokens:      {d_files} files, {d_bad} violations")
    print(f"cargo exit:        {proc.returncode}")

    enough = min(a_files, b_files, d_files) >= MIN_CORPUS_FILES
    clean = a_bad == 0 and b_bad == 0 and d_bad == 0
    ok = proc.returncode == 0 and clean and enough
    if not ok:
        print("\n--- harness output (tail) ---", file=sys.stderr)
        print("\n".join(combined.splitlines()[-40:]), file=sys.stderr)
    print("\nRESULT:", "PASS" if ok else "FAIL", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
