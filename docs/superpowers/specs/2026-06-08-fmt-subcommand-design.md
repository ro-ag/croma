# `croma fmt` — ABC formatter subcommand (design)

Status: approved design, pre-implementation
Date: 2026-06-08
Branch: `work/fmt-subcommand`

## Why now

The parser-quality gate in `AGENTS.md` ("Formatter and LSP are gated until parser
quality is proven") is **lifted**. `docs/comparison/abc2xml-divergences/`
establishes 0 genuine Croma issues across the 10k Zenodo corpus and 97.4%
note-identical output vs `abc2xml`. The per-file manifest's verdict classes
(malformed / detached-token / barline / etc.) double as the catalogue of what is
safe to auto-fix.

This is the first cut of a real `fmt` subcommand. `crates/croma-fmt/src/lib.rs` is
currently an 18-line stub that only trims trailing whitespace; it is replaced by a
proper formatter built on `croma-core`.

## Goals

1. `croma fmt FILE.abc` — emit a **canonical, idempotent, lossless** formatting of
   the ABC source.
2. `croma fmt --auto-fix FILE.abc` — additionally apply **safe, score-preserving**
   curations, reverting any change that would alter the rendered notes.
3. A surface modelled on rustfmt/gofmt: stdout default, `--check`, in-place
   `-w/--write`.

### Invariants (the whole point)

- **Idempotent:** `fmt(fmt(x)) == fmt(x)`.
- **Lossless:** the MusicXML pitch sequence (step+alter+octave) of `fmt(x)` is
  identical to that of `x`. `--auto-fix` changes **0 notes** across the corpus.

## Non-goals (milestone 1)

- Per-field re-spacing / field reordering (information-field *internals* stay
  byte-stable). Deferred to a later iteration with individual safety proofs.
- Redundant bar-line collapse (catalogue class 2) and `%%MIDI`/stylesheet
  directive placement (class 4). Deferred.
- Multi-file invocation (single `FILE` arg, matching xml/check/dump). Deferred.
- LSP. Out of scope entirely.

## Architecture

### `croma-fmt` crate (rebuilt)

Span-anchored, token-preserving engine over `croma-core`'s parsed model. Every
musical token (`MusicToken` / `MusicItem` / `ParsedField`) carries a byte `Span`,
and `SourceText::slice(span)` recovers the original bytes exactly. The engine
copies each musical token's source slice **verbatim** and only ever rewrites the
whitespace *between* tokens. Musical bytes are never reconstructed, so pitch and
rhythm cannot change — losslessness of canonical `fmt` is true *by construction*.

Public API:

```rust
/// Canonical formatting. Idempotent and lossless by construction.
pub fn format(source: &str, opts: FormatOptions) -> String;

/// Canonical formatting plus safe curations. Every fix is runtime-verified.
pub fn auto_fix(source: &str, opts: FormatOptions) -> FixResult;

/// True when `format(source) == source` (drives `--check`).
pub fn is_formatted(source: &str, opts: FormatOptions) -> bool;

pub struct FixResult {
    pub output: String,
    pub changes: Vec<Change>,   // fixes applied
    pub skipped: Vec<Change>,   // fixes reverted because runtime verification failed
}

pub struct Change {
    pub kind: FixKind,          // DetachedLength | DetachedAccidental | ChordSymbolInBrackets | DoubledTempo
    pub span: croma_core::Span, // location in the ORIGINAL source
    pub before: String,
    pub after: String,
}
```

`FormatOptions` carries the parse mode / spec version so the formatter parses the
same way the rest of the CLI does.

Module layout (files stay focused, well under the 1000-line breakdown rule):

- `src/lib.rs` — public API, `FixResult` / `Change` / `FixKind`, `FormatOptions`.
- `src/engine.rs` — the canonical token-walk.
- `src/fixes.rs` — the four milestone-1 repairs.
- `src/verify.rs` — pitch-sequence extraction from a lowered `Score` + equality,
  the gate used by `auto_fix`.

### Canonical `fmt` rules (milestone 1)

Walk `croma-core`'s line classification (`document.surface.line_map`); per line:

- **Music body lines** — walk `MusicLine.tokens`: emit each non-whitespace token
  by its span verbatim; collapse each run of `Whitespace` tokens to **exactly one
  space**; emit nothing for leading/trailing whitespace (leading indentation
  trimmed). This preserves beaming exactly — a space-break stays a break and a
  no-break stays joined; only the *width* of a space run changes. `Comment` and
  `ScoreLineBreak` tokens are copied verbatim.
- **Header / information-field lines** (`X:`,`T:`,`K:`,`M:`,`L:`,`Q:`,`V:`, …),
  `w:`/`s:` alignment lines, `%%` stylesheet directives, and `%` comment lines —
  trailing-trim only; otherwise byte-stable (alignment- and content-sensitive).
- **Free text** between tunes — trailing-trim only.
- **Blank lines** — runs collapsed to a single blank line; the file ends with
  exactly one `\n`.

### `--auto-fix` curations (milestone 1 subset)

Applied after canonical formatting; each grounded in the divergence docs:

| Fix | Action |
|---|---|
| detached length `g 2` | remove the gap → `g2` |
| detached accidental `^ g` | remove the gap → `^g` |
| chord-symbol in brackets `["text"…]` | unwrap → `"text"…` |
| doubled tempo `Q:1/4=1/4=160` | collapse the duplicate |

Each fix is **identified** from `croma-core`'s parse (the recovered interpretation
+ the relevant diagnostic / token kinds — exact detection confirmed during TDD).

### The safety gate (runtime self-verification)

Every candidate fix is verified before it is kept:

1. Parse + lower the original to a `Score`; extract the pitch sequence
   (step+alter+octave) — `verify::pitch_seq`.
2. Apply the candidate fix; parse + lower the result; extract its pitch sequence.
3. Keep the change **only if the sequences are identical**; otherwise revert that
   fix and record it under `FixResult.skipped`.

So "never break the score" holds at runtime by construction, not merely in tests.
Canonical `fmt` is purely constructive (no reparse needed); the runtime gate is
specific to `--auto-fix`.

### CLI (`croma-cli`) — full clap refactor

The CLI is migrated to **clap (derive API)**; xml/check/dump and the new fmt all
become clap subcommands sharing the common options (`--strict/--loose/--recover`,
`--abc-2.2-draft`, `--diagnostics`, `--warnings-as-errors`). Colored diagnostics
and fmt change-reports use **owo-colors + anstream** (auto-disabled when output is
not a TTY and under `NO_COLOR`). `croma-cli` is a binary, so these deps do not
affect the crates.io-publishability of `croma-core` / `croma-fmt`.

```
croma fmt FILE.abc              # canonical formatting to stdout (default)
croma fmt --check FILE.abc      # exit 1 + "would reformat: FILE" on stderr if unformatted; else exit 0, silent
croma fmt -w/--write FILE.abc   # format in place
croma fmt --auto-fix FILE.abc   # also apply safe curations; report changes on stderr
```

- `--auto-fix` composes with `--check` (report would-fix) and `-w` (write fixed).
- `--check` together with `-w` is a usage error.
- Behavioural parity for the existing subcommands is preserved across the refactor
  (verified by the existing CLI integration tests).

## Testing & proof

### (a) Unit / TDD (in `cargo test --workspace`)

- Canonical: specific output assertions; idempotency `format(format(x)) ==
  format(x)`; losslessness `pitch_seq(x) == pitch_seq(format(x))`.
- Each fix: a no-happy-path case asserting `pitch_seq(input) ==
  pitch_seq(auto_fix(input).output)`, plus the expected textual repair, plus a
  case that *must* be reverted (verification fails → recorded in `skipped`).
- CLI: integration tests for stdout / `--check` exit codes / `-w` / `--auto-fix`
  reporting, and unchanged behaviour of xml/check/dump.

### (b) Corpus round-trip (LOCAL only, never CI)

A `tools/` script (extends the `prove_divergences.py` `pitch_seq` idea and the
corpus harness): for every tune in the external 10k,

1. `croma fmt --auto-fix` → formatted source;
2. `croma xml` on **both** original and formatted source;
3. assert the extracted pitch sequences are identical (0 notes changed);
4. assert idempotency (`fmt(fmt(x)) == fmt(x)`).

The bar mirrors the parser phases: **0 tunes may change their note sequence**;
ideally some move closer to the spec. Provision the corpus per `AGENTS.md`
(`git lfs pull` / `--fetch-corpus`, `PHASE=fmt … --testbed`). Full corpus runs are
local only.

## Process / gates

- Branch `work/fmt-subcommand` (never `main`). TDD throughout; subagents for
  investigation/implementation, orchestrator runs the corpus round-trip and judges
  net effect.
- Before every commit: `cargo test --workspace`;
  `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo fmt --all -- --check`; `uv run pytest -q` (if Python touched);
  `git diff --check`.
- Commit messages carry **no Co-Authored-By / AI / tool trailer**
  (`git log main..HEAD --format=%b | grep -ci Co-Authored-By` == 0).
- `croma-core` / `croma-fmt` stay crates.io-publishable (no path-only runtime
  assumptions; CLI-only deps live in `croma-cli`).
- Generated corpus output stays under `docs/untracked/`.
- Open a PR when the milestone is met; merge only when both CI checks
  (Rust + Linux/nixos) are green; then delete the branch and update the tracker
  (runtime DB + exported SQL snapshot).

## Definition of done (milestone 1)

`croma fmt` and `croma fmt --auto-fix` exist, documented, with `--check` and
`-w/--write`; `croma-fmt` is a real idempotent + lossless formatter; the corpus
round-trip proves 0 notes changed by `--auto-fix`; tests/clippy/fmt green; PR
merged on green CI; tracker updated. Then iterate on the fix catalogue
(barline collapse, directive placement, field tidy).
