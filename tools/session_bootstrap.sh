#!/usr/bin/env bash
# Croma session bootstrap — lean dev repo. Run once at the start of every session.
#
# Idempotent and safe to re-run. It:
#   1. reports git + environment state,
#   2. provisions the pinned Rust toolchain,
#   3. builds target/debug/croma,
#   4. optionally clones/updates the croma-test proving suite into ./croma-test/.
#
# The corpus-scale proving harness, abc2xml comparator, progress tracker, and ABC
# spec knowledge base live in the separate, private croma-test repository:
#     https://github.com/ro-ag/croma-test
#
# Plain `cargo test --workspace` works standalone here: the corpus-scale
# corpus_proof tests are ABC_ROOT-gated and skip cleanly when the corpus is
# absent. To run the full gate suite, clone croma-test (--with-suite) and follow
# its bootstrap.
#
# Usage:
#   tools/session_bootstrap.sh               # env + build CLI
#   tools/session_bootstrap.sh --with-suite  # also clone/update ./croma-test/

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

WITH_SUITE=0
SUITE_REMOTE="${CROMA_TEST_REMOTE:-https://github.com/ro-ag/croma-test.git}"
SUITE_DIR="${CROMA_TEST_DIR:-croma-test}"

for arg in "$@"; do
  case "$arg" in
    --with-suite) WITH_SUITE=1 ;;
    *)
      echo "ERROR: unknown argument: $arg" >&2
      echo "Usage: tools/session_bootstrap.sh [--with-suite]" >&2
      exit 2
      ;;
  esac
done

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

section "Build CLI (cargo build -p croma-cli)"
if have cargo; then
  cargo build -p croma-cli && echo "binary: target/debug/croma"
else
  echo "skipped: cargo missing"
fi

section "Proving suite (croma-test)"
if [ "$WITH_SUITE" = "1" ]; then
  if ! have git; then
    echo "ERROR: git is required to fetch the proving suite." >&2
    exit 1
  fi
  if [ -d "$SUITE_DIR/.git" ]; then
    echo "Updating $SUITE_DIR ..."
    git -C "$SUITE_DIR" pull --ff-only || echo "WARN: could not fast-forward $SUITE_DIR; resolve manually."
  else
    echo "Cloning $SUITE_REMOTE into $SUITE_DIR ..."
    git clone "$SUITE_REMOTE" "$SUITE_DIR" || \
      echo "WARN: clone failed (private repo — check gh/ssh auth)."
  fi
  if [ -d "$SUITE_DIR" ]; then
    echo "Suite ready under ./$SUITE_DIR/ (git-ignored). Run its bootstrap to provision the corpus + gates."
  fi
else
  echo "Skipped. The corpus-scale proving suite, comparator, and tracker live in croma-test:"
  echo "  https://github.com/ro-ag/croma-test"
  echo "Clone it alongside croma with: tools/session_bootstrap.sh --with-suite"
  echo "Plain 'cargo test --workspace' runs standalone (corpus_proof skips without ABC_ROOT)."
fi

section "Bootstrap complete"
