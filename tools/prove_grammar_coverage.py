#!/usr/bin/env python3
"""Prove the tree-sitter-abc grammar's parse coverage over the 10k ABC corpus.

Parses every `*.abc` under ABC_ROOT with `tree-sitter parse --quiet` (run from
the grammar directory). A non-zero exit code means the parse tree contains an
ERROR or MISSING node. Reports: files parsed, clean-parse count, clean-parse %,
and the top error categories, bucketed by the construct on the first error line.

This is the grammar's analog of croma's corpus proofs (LSP/fmt/reader): evidence,
not a vibe. The bar is "common constructs parse clean + the residual is measured
and categorized," not a hard 100%.

Usage:
    uv run python tools/prove_grammar_coverage.py [ABC_ROOT]
    ABC_ROOT defaults to docs/untracked/corpus/zenodo-10k/abc
"""

from __future__ import annotations

import collections
import concurrent.futures
import os
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
GRAMMAR_DIR = REPO_ROOT / "tree-sitter-abc"
DEFAULT_ABC_ROOT = REPO_ROOT / "docs" / "untracked" / "corpus" / "zenodo-10k" / "abc"

# `tree-sitter parse --quiet` prints, on failure, lines like:
#   (ERROR [r, c] - [r, c])
# and the row of the first error is captured from the `[r, c]` of the first
# ERROR/MISSING. We re-derive the offending source line to categorize.
ERROR_POS_RE = re.compile(r"\((?:ERROR|MISSING)[^\[]*\[(\d+), (\d+)\]")


def categorize(line: str) -> str:
    """Bucket an offending source line by the ABC construct it most likely is."""
    s = line.strip()
    if not s:
        return "blank/whitespace line"
    # Field / directive lines.
    if s.startswith("%%"):
        return "stylesheet directive (%%)"
    if s.startswith("%"):
        return "comment (%)"
    if re.match(r"^[A-Za-z+]:", s):
        code = s[0]
        return f"field line ({code}:)"
    # Music-line constructs.
    if s[0] in "wW" and s[1:2] == ":":
        return "lyric/words line"
    if "\\" in s:
        return "line-continuation backslash"
    if s.startswith("[") or "[" in s[:3]:
        return "inline-field / bracket construct"
    if any(ch in s for ch in "{}"):
        return "grace group { }"
    if '"' in s:
        return "quoted text (chord-symbol/annotation)"
    if any(ch in s for ch in "!+"):
        return "decoration (! / +)"
    if any(ch in s for ch in "|:[]"):
        return "barline / repeat construct"
    if s[0] in "ABCDEFGabcdefg^_=zxyZXZ":
        return "music line (note/rest run)"
    return "other / free text"


def parse_one(path: Path) -> tuple[bool, str | None]:
    """Return (clean, offending_line). clean=True means no ERROR/MISSING node."""
    try:
        proc = subprocess.run(
            ["tree-sitter", "parse", "--quiet", str(path)],
            cwd=GRAMMAR_DIR,
            capture_output=True,
            text=True,
            timeout=60,
        )
    except subprocess.TimeoutExpired:
        return False, "<timeout>"
    if proc.returncode == 0:
        return True, None
    # Find the first ERROR/MISSING position in stdout and map it to a source line.
    row = None
    for m in ERROR_POS_RE.finditer(proc.stdout):
        row = int(m.group(1))
        break
    if row is None:
        # Failure with no parsed position (e.g. read error); bucket generically.
        return False, "<no-position>"
    try:
        src_lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        offending = src_lines[row] if 0 <= row < len(src_lines) else ""
    except OSError:
        offending = ""
    return False, offending


def main() -> int:
    abc_root = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_ABC_ROOT
    if not abc_root.is_dir():
        print(f"error: ABC_ROOT not found: {abc_root}", file=sys.stderr)
        return 2
    if not (GRAMMAR_DIR / "src" / "parser.c").exists():
        print(
            "error: generated parser missing; run `tree-sitter generate` in "
            f"{GRAMMAR_DIR} first",
            file=sys.stderr,
        )
        return 2

    files = sorted(abc_root.glob("*.abc"))
    if not files:
        print(f"error: no *.abc files under {abc_root}", file=sys.stderr)
        return 2

    total = len(files)
    clean = 0
    categories: collections.Counter[str] = collections.Counter()
    samples: dict[str, str] = {}

    workers = min(32, (os.cpu_count() or 4) * 2)
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        for path, (is_clean, offending) in zip(
            files, pool.map(parse_one, files)
        ):
            if is_clean:
                clean += 1
            else:
                cat = categorize(offending or "")
                categories[cat] += 1
                samples.setdefault(cat, f"{path.name}: {(offending or '').strip()[:80]}")

    pct = 100.0 * clean / total if total else 0.0
    print("=" * 70)
    print("tree-sitter-abc — 10k parse-coverage")
    print("=" * 70)
    print(f"ABC_ROOT:          {abc_root}")
    print(f"files parsed:      {total}")
    print(f"clean parses:      {clean}")
    print(f"clean-parse rate:  {pct:.2f}%")
    print(f"files with ERROR:  {total - clean}")
    print("-" * 70)
    print("top residual categories (offending construct on first error line):")
    for cat, count in categories.most_common(15):
        share = 100.0 * count / total
        print(f"  {count:6d}  ({share:5.2f}%)  {cat}")
        print(f"            e.g. {samples.get(cat, '')}")
    print("=" * 70)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
