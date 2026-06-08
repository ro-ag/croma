# `croma fmt` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Add a real `fmt` subcommand backed by a rebuilt `croma-fmt` crate: a canonical, idempotent, lossless ABC formatter plus a `--auto-fix` mode that applies only pitch-sequence-preserving curations, each gated by runtime re-parse.

**Architecture:** Span-anchored, token-preserving engine over `croma-core`. Musical tokens are copied verbatim by source span; only inter-token whitespace, blank-line runs, and the final newline are normalized. `--auto-fix` applies targeted source edits, re-parses, and keeps a change only if the ordered pitch sequence (step+alter+octave) is unchanged. CLI is migrated to clap (derive) with owo-colors/anstream colored output.

**Tech Stack:** Rust 2024 / MSRV 1.96.0; `croma-core`; `clap` 4 (derive), `owo-colors` 4, `anstream`; dev `assert_cmd`/`predicates` (optional).

**Key investigated facts (do not re-derive):**
- Pitch model: `Score.parts: Vec<Part>` → `Part.voices: Vec<Voice>` → `Voice.events: Vec<TimedEvent>` (ordered). `TimedEventKind::Note(NoteEvent{ pitch })`, `Chord(ChordEvent{ members: Vec<ChordMemberEvent{ pitch }> })`, `Rest(_)` (skip). `Pitch { step: char, alter: i8, octave: i8 }`.
- Lower: `let doc = parse_document(src, opts).value; let score = lower_score(&doc, LowerOptions).value; // Option<Score>`.
- Music tokens: `ParsedTuneMusic.lines: Vec<MusicLine>`, `MusicLine { line_index, tokens: Vec<MusicToken{ kind: MusicTokenKind, span }>, items, .. }`. A run of spaces is ONE `Whitespace` token. `MusicTokenKind::{Whitespace, Comment, ScoreLineBreak, ...}`.
- `document.source: SourceText`; `SourceText::slice(span) -> Option<&str>`; `Span { start, end }`.
- Malformed detectors (verified): detached length → `MusicItem::Malformed{ kind: StandaloneLength }` / diag `abc.music.malformed_length`; chord-symbol-in-brackets → `MusicItem::Chord` whose first member has non-empty `chord_symbols`; doubled tempo → `ParsedFieldKind::Tempo` present but `ScoreMetadata.tempo_model == None`. Detached accidental (`^ g`) is EXCLUDED (changes a pitch → gate reverts).
- CLI tests live only in `crates/croma-cli/tests/cli.rs` (spawns `CARGO_BIN_EXE_croma`). Exact strings to preserve OR update: `unknown command \`X\``, `usage: croma`, `choose only one of --strict, --loose, or --recover`. Diagnostics (text+JSON) on **stderr**; xml/dump on **stdout**. CI gates: `cargo fmt --all --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace --all-targets`. Workspace lints: `unwrap_used`/`todo`/`dbg_macro` = warn (→ error under `-D warnings`), `unsafe_code` = forbid.

---

## Phase A — `croma-fmt` library

### Task A1: pitch-sequence extraction (`verify.rs`)

**Files:** Create `crates/croma-fmt/src/verify.rs`; modify `crates/croma-fmt/src/lib.rs` (add `mod verify;`).

- [ ] Write failing test: `pitch_seq` of `X:1\nK:C\nCEG\n` is `[("C",0,5),("E",0,5),("G",0,5)]` (octave per croma; assert the actual values once observed) and a chord `[CEG]` yields the three member pitches in order; a rest `z` contributes nothing.
- [ ] Implement:
```rust
use croma_core::{LowerOptions, ParseOptions, Score, TimedEventKind, lower_score, parse_document};

pub(crate) type PitchSeq = Vec<(char, i8, i8)>;

pub(crate) fn pitch_seq_of(source: &str, options: ParseOptions) -> Option<PitchSeq> {
    let report = parse_document(source, options);
    let score = lower_score(&report.value, LowerOptions).value?;
    Some(pitch_seq(&score))
}

pub(crate) fn pitch_seq(score: &Score) -> PitchSeq {
    let mut out = Vec::new();
    for part in &score.parts {
        for voice in &part.voices {
            for event in &voice.events {
                match &event.kind {
                    TimedEventKind::Note(n) => out.push((n.pitch.step, n.pitch.alter, n.pitch.octave)),
                    TimedEventKind::Chord(c) => {
                        for m in &c.members { out.push((m.pitch.step, m.pitch.alter, m.pitch.octave)); }
                    }
                    _ => {}
                }
            }
        }
    }
    out
}
```
- [ ] Run `cargo test -p croma-fmt verify`; expect PASS. Commit.

### Task A2: canonical engine (`engine.rs`) + `FormatOptions` + `format()`

**Files:** Create `crates/croma-fmt/src/engine.rs`; rewrite `crates/croma-fmt/src/lib.rs`.

Design: `FormatOptions { parse: ParseOptions }` (default strict V2.1). `format(source, opts)`:
1. Parse; collect the set of music-body line indices and a `HashMap<usize, &MusicLine>` from `document.music.tunes[].lines`.
2. Iterate source lines (preserve count). For each line index `i`:
   - if a `MusicLine` maps to `i` and has tokens → reconstruct via token-walk (below);
   - else → trailing-trim the source line text.
3. Track blank lines (trimmed-empty); collapse runs to a single blank line; never leading blank; end with exactly one `\n`.

Token-walk (lossless + beaming-preserving):
```rust
fn format_music_line(src: &SourceText, line: &MusicLine) -> String {
    let mut out = String::new();
    let mut pending_space = false;
    for tok in &line.tokens {
        match tok.kind {
            MusicTokenKind::Whitespace => pending_space = true,
            _ => {
                let slice = src.slice(tok.span).unwrap_or("");
                if !out.is_empty() && pending_space { out.push(' '); }
                out.push_str(slice);
                pending_space = false;
            }
        }
    }
    out
}
```
(If `line.tokens` is empty, fall back to trailing-trim of the source line.)

- [ ] Failing test: music spaces collapse but beaming boundaries preserved — `format("X:1\nK:C\nCDE   FGA  |  c2\n")` contains `CDE FGA | c2` and ends with one `\n`.
- [ ] Failing test (idempotent): `format(format(x)) == format(x)` for a multi-tune sample with comments, `w:`, blank-line runs.
- [ ] Failing test (lossless): `pitch_seq_of(x) == pitch_seq_of(format(x))` for the same sample.
- [ ] Failing test (verbatim header/lyrics): `w: do  re   mi` and `%%MIDI program 1` survive byte-stable (trailing-trim only).
- [ ] Implement engine + `pub fn format`. Run tests; PASS. Commit.

### Task A3: `is_formatted()` for `--check`

**Files:** `crates/croma-fmt/src/lib.rs`.

- [ ] Failing test: `is_formatted(already_canonical)` true; `is_formatted("X:1\nK:C\nC   D\n")` false.
- [ ] Implement `pub fn is_formatted(source, opts) -> bool { format(source, opts) == source }`. PASS. Commit.

### Task A4: auto-fix infrastructure + gate (`lib.rs` types, `fixes.rs`, `auto_fix`)

**Files:** Create `crates/croma-fmt/src/fixes.rs`; modify `lib.rs`.

Types:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixKind { DetachedLength, ChordSymbolInBrackets, DoubledTempo }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change { pub kind: FixKind, pub span: croma_core::Span, pub before: String, pub after: String }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixResult { pub output: String, pub changes: Vec<Change>, pub skipped: Vec<Change> }
```
`auto_fix(source, opts)`:
1. `let baseline = pitch_seq_of(source, opts)`.
2. `let mut current = format(source, opts)` (start from canonical).
3. For each detector (A5–A7), gather candidate `Change`s against `current`; apply them one at a time to a trial string; re-parse trial; if `pitch_seq_of(trial) == baseline` → accept (`current = trial`, push to `changes`); else revert (push to `skipped`).
4. Re-run `format` once at the end to keep idempotency; return `FixResult { output: current, changes, skipped }`.

- [ ] Failing test: `auto_fix` on already-clean input returns `output == format(input)`, empty `changes`. Implement skeleton + gate loop with zero detectors. PASS. Commit.

### Task A5: repair — detached length `g 2` → `g2`

**Files:** `crates/croma-fmt/src/fixes.rs`.

Detector: scan `document.music.tunes[].lines[].items` for `MusicItem::Malformed{ kind: StandaloneLength }`. The malformed digit is preceded (in `tokens`) by a `Whitespace` token preceded by a `Pitch`/`Rest`/`Chord` token. Candidate edit = delete that whitespace span (join note and length).

- [ ] Failing test: `auto_fix("X:1\nK:C\ng 2\n").output` contains `g2`; `changes` has one `DetachedLength`; pitch_seq unchanged. Also assert `g2` (no detached digit warning) when re-parsed.
- [ ] Failing test (gate revert path): a contrived case where joining would change pitch_seq is recorded in `skipped`, not applied. (If none exists for length, assert detached-accidental `^ g` is left unchanged by `auto_fix` — it is never attempted.)
- [ ] Implement detector + edit. PASS. Commit.

### Task A6: repair — chord-symbol in brackets `["t"abc]` → `"t"abc`

**Files:** `crates/croma-fmt/src/fixes.rs`.

Detector: `MusicItem::Chord` whose first member has non-empty `chord_symbols`. Candidate edit = remove the opening `[` immediately before the chord-symbol and the matching closing `]`.

- [ ] Failing test: `auto_fix("X:1\nK:C\n[\"Cmaj\"abc]\n").output` contains `"Cmaj"abc` (no brackets); `changes` has `ChordSymbolInBrackets`; pitch_seq (A,B,C) unchanged.
- [ ] Implement. PASS. Commit.

### Task A7: repair — doubled tempo `Q:1/4=1/4=160` → `Q:1/4=160`

**Files:** `crates/croma-fmt/src/fixes.rs`.

Detector: `ParsedFieldKind::Tempo` whose raw value matches `^\s*(\d+/\d+)=\1=(\d+)\s*$` (no regex dep — parse by hand: split on `=`, three parts, first two equal beat specs). Candidate edit = rewrite the value to `\1=\2`.

- [ ] Failing test: `auto_fix("X:1\nQ:1/4=1/4=160\nK:C\nC\n").output` contains `Q:1/4=160`; `changes` has `DoubledTempo`; pitch_seq unchanged; re-parsed tempo_model is now `Some`.
- [ ] Implement (string parse, avoid `.unwrap()`). PASS. Commit. Run full `cargo test -p croma-fmt`, `cargo clippy -p croma-fmt --all-targets -- -D warnings`, `cargo fmt`.

## Phase B — CLI (`croma-cli`) clap refactor

### Task B1: add deps + clap scaffold preserving xml/check/dump

**Files:** `crates/croma-cli/Cargo.toml`, `crates/croma-cli/src/main.rs` (split into modules if it grows: `cli.rs` for clap structs).

Add: `croma-fmt = { path = "../croma-fmt" }`, `clap = { version = "4", features = ["derive"] }`, `owo-colors = "4"`, `anstream = "0.6"`. Build a clap `Parser` with subcommands `Xml`, `Check`, `Dump`, `Fmt` and a shared `#[command(flatten)] CommonOpts` (`--strict/--loose/--recover` as a conflicting group, `--abc-2.2-draft`, `--diagnostics <text|json>`, `--warnings-as-errors`). Preserve stdout/stderr routing and exit codes. Use clap's `error` / a custom check to keep the three exact strings, OR update the three assertions in `tests/cli.rs` to match clap output (decide during impl; prefer updating tests to idiomatic clap messages while keeping `usage`/conflict semantics).

- [ ] Run existing `cargo test -p croma-cli` to capture the baseline (all 14 pass pre-refactor).
- [ ] Implement clap scaffold; route xml/check/dump to the existing pipeline fns (keep those fns). Update `tests/cli.rs` assertions only where clap necessarily changes wording; keep behavior (exit codes, streams, subcommands) identical.
- [ ] Run `cargo test -p croma-cli`; PASS. Commit.

### Task B2: `fmt` subcommand

**Files:** `crates/croma-cli/src/main.rs`/`cli.rs`.

Flags: positional `FILE`; `--check`; `-w/--write`; `--auto-fix`. `--check` + `--write` = usage error. Behavior:
- default → print `format`/`auto_fix(...).output` to stdout.
- `--check` → exit 1 + `would reformat: <file>` on stderr if not formatted (for `--auto-fix`, compare against `auto_fix` output); exit 0 silent otherwise.
- `-w` → write output back to FILE; stdout empty.
- `--auto-fix` → use `auto_fix`; report each `Change` to stderr (e.g. `fixed [detached-length] line N: `g 2` -> `g2``); also report `skipped`.

- [ ] Failing CLI tests (in `tests/cli.rs`): stdout formatting; `--check` exit codes (0 when clean, 1 + message when not); `-w` writes file; `--auto-fix` applies `g 2`→`g2` and reports it; `--check --write` errors.
- [ ] Implement. PASS. Commit.

### Task B3: colored diagnostics/reports

**Files:** `crates/croma-cli/src/main.rs` (diagnostics + fmt reporting).

- [ ] Route stderr through `anstream::stderr()` and color severities/fix labels with `owo-colors` (auto-disabled non-TTY / `NO_COLOR`). Keep JSON output uncolored and byte-stable (tests parse it).
- [ ] Run full `cargo test -p croma-cli`; PASS. Commit. Run workspace clippy/fmt.

## Phase C — corpus losslessness proof (LOCAL only)

### Task C1: `tools/prove_fmt_lossless.py`

**Files:** Create `tools/prove_fmt_lossless.py`.

- [ ] For each tune in `ABC_ROOT`: run `croma fmt --auto-fix` (capture formatted source), then `croma xml` on BOTH original and formatted; extract `pitch_seq` (reuse `prove_divergences.py`'s function/regex) from both XMLs; assert identical. Also assert `fmt(fmt(x)) == fmt(x)`. Tally: total, notes-changed (must be 0), idempotency failures (must be 0), and how many files were auto-fixed (informational). Write report to `docs/untracked/fmt/`. Never wire into CI.
- [ ] Run locally over the 10k (provision corpus per AGENTS.md). Confirm 0 notes changed, 0 idempotency failures. Record the numbers.

## Phase D — close out

- [ ] `cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --all -- --check`; `uv run pytest -q` (if Python tests touched); `git diff --check`. All green.
- [ ] Verify no co-author trailer: `git log main..HEAD --format=%b | grep -ci Co-Authored-By` == 0.
- [ ] Update tracker runtime DB (new phase row, status, selected/next target) + `export` SQL snapshot; commit snapshot with the code.
- [ ] Push branch; open PR; merge only when both CI checks green; delete branch.
