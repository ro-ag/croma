#!/usr/bin/env bash
# Croma session bootstrap: run once at the start of every agent/chat session.
#
# It is idempotent and safe to re-run. It:
#   1. reports git + environment state,
#   2. provisions the pinned Rust toolchain and uv Python env,
#   3. restores the progress-tracker database from the committed SQL snapshot,
#   4. downloads/recreates the external corpus when requested,
#   5. recreates the corpus testbed when the external corpus is available
#      (set ABC_ROOT and REF_ROOT, or fetch it into docs/untracked/corpus).
#
# Usage:
#   tools/session_bootstrap.sh            # env + DB + testbed status
#   tools/session_bootstrap.sh --fetch-corpus
#                                         # download original Zenodo ABC sources
#   tools/session_bootstrap.sh --fetch-corpus --fetch-reference
#                                         # also build abc2xml reference MusicXML
#   ABC_ROOT=... REF_ROOT=... tools/session_bootstrap.sh --testbed
#                                         # also rebuild the full 10k testbed
#
# Everything runs from the repository root. No absolute host paths are assumed.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

REBUILD_TESTBED=0
FETCH_CORPUS=0
FETCH_REFERENCE=0

for arg in "$@"; do
  case "$arg" in
    --testbed) REBUILD_TESTBED=1 ;;
    --fetch-corpus) FETCH_CORPUS=1 ;;
    --fetch-reference) FETCH_REFERENCE=1 ;;
    *)
      echo "ERROR: unknown argument: $arg" >&2
      echo "Usage: tools/session_bootstrap.sh [--fetch-corpus] [--fetch-reference] [--testbed]" >&2
      exit 2
      ;;
  esac
done

if [ "$FETCH_REFERENCE" = "1" ] && [ "$FETCH_CORPUS" != "1" ]; then
  echo "ERROR: --fetch-reference requires --fetch-corpus" >&2
  exit 2
fi

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
UV_AVAILABLE=0
if have uv; then
  UV_AVAILABLE=1
  echo "uv: $(uv --version)"
else
  echo "WARN: uv not found; see docs/development-environment.md"
fi

section "Provision Python env (uv sync)"
if [ "$UV_AVAILABLE" = "1" ]; then
  uv sync
else
  echo "skipped: uv missing"
fi

section "Build CLI (cargo build -p croma-cli)"
if have cargo; then
  cargo build -p croma-cli && echo "binary: target/debug/croma"
else
  echo "skipped: cargo missing"
fi

section "Restore progress tracker DB"
# Recreates docs/untracked/croma-progress.sqlite from the committed SQL snapshot.
if [ "$UV_AVAILABLE" = "1" ]; then
  uv run python tools/progress/progress.py restore --force
  uv run python tools/progress/progress.py status
elif have python3; then
  python3 tools/progress/progress.py restore --force
  python3 tools/progress/progress.py status
else
  echo "ERROR: neither uv nor python3 is available; cannot restore progress tracker." >&2
  exit 1
fi

section "Corpus testbed"
ABC_ROOT="${ABC_ROOT:-}"
REF_ROOT="${REF_ROOT:-}"
CROMA_CORPUS_BASE="${CROMA_CORPUS_BASE:-docs/untracked/corpus/zenodo-10k}"
LFS_CORPUS_ARCHIVE="${LFS_CORPUS_ARCHIVE:-docs/corpus/zenodo-10k-abc.tar.gz}"

if [ "$FETCH_CORPUS" = "1" ]; then
  if [ "$UV_AVAILABLE" != "1" ]; then
    echo "ERROR: --fetch-corpus requires uv for Python corpus provisioning." >&2
    exit 1
  fi
  if [ -f "$LFS_CORPUS_ARCHIVE" ] && [ -f "$LFS_CORPUS_ARCHIVE.sha256" ]; then
    echo "Provisioning original ABC corpus from verified archive $LFS_CORPUS_ARCHIVE..."
    if ! uv run python tools/provision_corpus.py import-archive \
      --archive "$LFS_CORPUS_ARCHIVE" --output "$CROMA_CORPUS_BASE"; then
      echo "Archive import failed; falling back to Zenodo download."
      uv run python tools/provision_corpus.py fetch-zenodo-10k --output "$CROMA_CORPUS_BASE"
    fi
  else
    echo "Provisioning original Zenodo ABC corpus into $CROMA_CORPUS_BASE..."
    uv run python tools/provision_corpus.py fetch-zenodo-10k --output "$CROMA_CORPUS_BASE"
  fi
  if [ "$FETCH_REFERENCE" = "1" ]; then
    echo "Generating abc2xml reference MusicXML into $CROMA_CORPUS_BASE/musicxml..."
    uv run python tools/provision_corpus.py abc2xml-real --output "$CROMA_CORPUS_BASE"
  fi
fi

if [ -z "$ABC_ROOT" ] && [ -d "$CROMA_CORPUS_BASE/abc" ]; then
  ABC_ROOT="$CROMA_CORPUS_BASE/abc"
fi
if [ -z "$REF_ROOT" ] && [ -d "$CROMA_CORPUS_BASE/musicxml" ]; then
  REF_ROOT="$CROMA_CORPUS_BASE/musicxml"
fi

if [ -n "$ABC_ROOT" ] && [ -d "$ABC_ROOT" ] && [ -n "$REF_ROOT" ] && [ -d "$REF_ROOT" ]; then
  abc_count=$(find "$ABC_ROOT" -type f -name '*.abc' | wc -l | tr -d ' ')
  ref_count=$(find "$REF_ROOT" -type f \( -name '*.musicxml' -o -name '*.xml' \) | wc -l | tr -d ' ')
  echo "ABC_ROOT=$ABC_ROOT ($abc_count .abc files)"
  echo "REF_ROOT=$REF_ROOT ($ref_count reference files)"
  if [ "$REBUILD_TESTBED" = "1" ]; then
    if [ "$UV_AVAILABLE" != "1" ]; then
      echo "ERROR: --testbed requires uv for corpus and music21/Polars tooling." >&2
      exit 1
    fi
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
elif [ -n "$ABC_ROOT" ] && [ -d "$ABC_ROOT" ]; then
  abc_count=$(find "$ABC_ROOT" -type f -name '*.abc' | wc -l | tr -d ' ')
  echo "ABC corpus available: ABC_ROOT=$ABC_ROOT ($abc_count .abc files)"
  echo "Reference MusicXML is not available yet."
  echo "Generate it with: tools/session_bootstrap.sh --fetch-corpus --fetch-reference"
  echo "Then run: tools/session_bootstrap.sh --testbed"
  echo "Alternatively, export REF_ROOT=/path/to/musicxml if reference XML already exists elsewhere."
elif [ -n "$REF_ROOT" ] && [ -d "$REF_ROOT" ]; then
  ref_count=$(find "$REF_ROOT" -type f \( -name '*.musicxml' -o -name '*.xml' \) | wc -l | tr -d ' ')
  echo "Reference MusicXML available: REF_ROOT=$REF_ROOT ($ref_count reference files)"
  echo "ABC corpus is not available yet."
  echo "Provision it with: tools/session_bootstrap.sh --fetch-corpus"
  echo "Alternatively, export ABC_ROOT=/path/to/abc if the ABC corpus already exists elsewhere."
else
  echo "Corpus NOT available in this environment (external, not committed)."
  echo "To provision the original ABC sources locally, from Git LFS archive when present or Zenodo otherwise:"
  echo "  tools/session_bootstrap.sh --fetch-corpus"
  echo "To also build abc2xml reference MusicXML:"
  echo "  tools/session_bootstrap.sh --fetch-corpus --fetch-reference"
  echo "Then run: tools/session_bootstrap.sh --testbed"
  echo "Source: ABC Notation Dataset (10k samples), https://doi.org/10.5281/zenodo.17694747"
  echo "Alternatively, mount/copy the corpus and export:"
  echo "  export ABC_ROOT=/path/to/abc   REF_ROOT=/path/to/musicxml"
  echo "See docs/testing/corpus-reproducibility.md and docs/reference/corpus-inventory.md."
fi

section "Bootstrap complete"
