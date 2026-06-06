#!/usr/bin/env bash
# Croma session bootstrap: run once at the start of every agent/chat session.
#
# It is idempotent and safe to re-run. It:
#   1. reports git + environment state,
#   2. provisions the pinned Rust toolchain and uv Python env,
#   3. restores the progress-tracker database from the committed SQL snapshot,
#   4. recreates the corpus testbed when the external corpus is available
#      (set ABC_ROOT and REF_ROOT), otherwise reports how to provision it.
#
# Usage:
#   tools/session_bootstrap.sh            # env + DB + testbed status
#   ABC_ROOT=... REF_ROOT=... tools/session_bootstrap.sh --testbed
#                                         # also rebuild the full 10k testbed
#
# Everything runs from the repository root. No absolute host paths are assumed.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

REBUILD_TESTBED=0
[ "${1:-}" = "--testbed" ] && REBUILD_TESTBED=1

section() { printf '\n=== %s ===\n' "$1"; }
have() { command -v "$1" >/dev/null 2>&1; }

section "Git state"
git status --short --branch
echo "HEAD: $(git rev-parse --short HEAD) $(git log -1 --pretty=%s)"

section "Toolchain"
if have rustup; then
  rustup show active-toolchain 2>/dev/null || rustup show
else
  echo "WARN: rustup not found. Install per docs/development-environment.md, or use the Nix flake (nix develop)."
fi
have cargo && echo "cargo: $(cargo --version)"
have uv    && echo "uv: $(uv --version)" || echo "WARN: uv not found; see docs/development-environment.md"

section "Provision Python env (uv sync)"
if have uv; then uv sync; else echo "skipped: uv missing"; fi

section "Build CLI (cargo build -p croma-cli)"
if have cargo; then
  cargo build -p croma-cli && echo "binary: target/debug/croma"
else
  echo "skipped: cargo missing"
fi

section "Restore progress tracker DB"
# Recreates docs/untracked/croma-progress.sqlite from the committed SQL snapshot.
uv run python tools/progress/progress.py restore --force
uv run python tools/progress/progress.py status

section "Corpus testbed"
ABC_ROOT="${ABC_ROOT:-}"
REF_ROOT="${REF_ROOT:-}"
if [ -n "$ABC_ROOT" ] && [ -d "$ABC_ROOT" ] && [ -n "$REF_ROOT" ] && [ -d "$REF_ROOT" ]; then
  abc_count=$(find "$ABC_ROOT" -type f -name '*.abc' | wc -l | tr -d ' ')
  ref_count=$(find "$REF_ROOT" -type f \( -name '*.musicxml' -o -name '*.xml' \) | wc -l | tr -d ' ')
  echo "ABC_ROOT=$ABC_ROOT ($abc_count .abc files)"
  echo "REF_ROOT=$REF_ROOT ($ref_count reference files)"
  if [ "$REBUILD_TESTBED" = "1" ]; then
    PHASE="${PHASE:-session-testbed}"
    OUT="docs/untracked/$PHASE"
    mkdir -p "$OUT"
    echo "Rebuilding full export + comparison into $OUT (this can take a couple of minutes)..."
    uv run python tools/corpus_harness.py \
      --croma target/debug/croma --corpus "$ABC_ROOT" --mode xml \
      --report "$OUT/full-10k-export-report.json" \
      --results-jsonl "$OUT/full-10k-export-results.jsonl" \
      --keep-xml-dir "$OUT/full-10k-xml" --progress-every 500
    uv run python tools/music21_polars_corpus_compare.py \
      --results-jsonl "$OUT/full-10k-export-results.jsonl" \
      --croma-xml-root "$OUT/full-10k-xml" --reference-root "$REF_ROOT" \
      --report "$OUT/full-10k-report-only-compare-report.json" \
      --jobs 0 --progress-every 500
    echo "Testbed rebuilt under $OUT (ignored, not committed)."
  else
    echo "Corpus available. Rebuild the testbed with: tools/session_bootstrap.sh --testbed"
    echo "Full recipe: docs/testing/corpus-reproducibility.md"
  fi
else
  echo "Corpus NOT available in this environment (external, not committed)."
  echo "To enable the testbed, mount/copy the corpus and export:"
  echo "  export ABC_ROOT=/path/to/abc   REF_ROOT=/path/to/musicxml"
  echo "Then re-run: tools/session_bootstrap.sh --testbed"
  echo "See docs/testing/corpus-reproducibility.md and docs/reference/corpus-inventory.md."
fi

section "Bootstrap complete"
