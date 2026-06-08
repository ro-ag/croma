# croma-core Module Breakdown Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorganize `croma-core` into a stage→feature module tree where every implementation file is ≤1,000 lines and tests live in separate sibling files, with zero behavior change.

**Architecture:** Pure code-movement. `music.rs` splits across three pipeline stages (`syntax/`, `parse/`, `lower/`); `musicxml.rs` splits into per-element writer modules; `fields.rs`→`parse/field/`; `surface.rs`→`syntax/tune.rs`. The public API is frozen via `lib.rs` re-exports. Correctness is guaranteed by the existing test suite plus byte-identical 10k MusicXML output before/after.

**Tech Stack:** Rust 1.96 (croma-core lib), `cargo test`/`clippy`/`fmt`, `uv`+pytest for tooling tests, the local 10k corpus testbed.

**Spec:** `docs/superpowers/specs/2026-06-07-croma-core-module-breakdown-design.md`

**Branch:** `work/phase-12-core-module-breakdown` (already created; spec already committed here).

---

## Global mechanics (read once, apply to every task)

### This is a refactor, not feature work
There is **no new behavior and no new test code**. The usual TDD loop is replaced by a **move → compile → test-green → verify-identical** loop. The safety net is: the existing tests keep passing, and the 10k MusicXML output stays byte-identical.

### History preservation (git mv) — REQUIRED
`git mv` is 1:1 but we split files one→many. For each source file, preserve history like this:
1. **Rename the whole file** to its *primary* new home with `git mv`, and **commit that alone** (no content edits in the same commit) so git records a clean rename.
2. **Extract** other portions into sibling files in **separate commits**, moving lines **verbatim** (no reformatting, no renaming symbols in the same commit) so `git blame -C -C -C` / `git log --follow` can track them.
3. Make `use`/visibility/`pub use` adjustments in their **own** commit, separate from the moves.

Never combine a move and a reformat in one commit — it defeats rename/copy detection.

### Per-task verification loop (run after every task)
```bash
cd /Users/rodox/dev/rs/croma
cargo build -p croma-core 2>&1 | tail -3          # must compile
cargo test -p croma-core 2>&1 | tail -5           # must stay green
```
Run the full gate only at checkpoints and at the end (it is slower):
```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```
Do **not** run `cargo fmt` during moves (it would reformat moved lines and break rename detection). Run `cargo fmt --all` only once, in the dedicated formatting task near the end, as its own commit.

### Splitting one `impl` across files
Rust allows multiple `impl` blocks for the same type across files of the same module. `MusicLineParser` and `Writer` methods are distributed by writing, in each destination file, an `impl<'a> TheType<'a> { … subset of methods … }`. Helper free functions move with the methods that use them.

### Size-cap check (the acceptance gate)
```bash
find crates/croma-core/src -name '*.rs' ! -name '*_tests.rs' | xargs wc -l \
  | awk '$2!="total" && $1>1000 {print}'
```
Must print nothing. Run it after each stage and at the end.

### Test-file convention (fixed)
Each impl file `foo.rs` declares:
```rust
#[cfg(test)]
#[path = "foo_tests.rs"]
mod tests;
```
and its tests live in sibling `foo_tests.rs`. Split a test file further (e.g. `harmony_tests.rs` + `harmony_chord_quality_tests.rs`) when large; test files are not capped.

---

## Task 0: Capture the behavioral baseline

**Files:** none (produces reference artifacts under `docs/untracked/`, git-ignored).

- [ ] **Step 1: Ensure clean green starting point**

Run:
```bash
cd /Users/rodox/dev/rs/croma
git status --short          # expect only the spec/plan docs on this branch
cargo test --workspace 2>&1 | tail -5
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3
```
Expected: tests pass, clippy clean.

- [ ] **Step 2: Capture the byte-exact 10k output baseline**

Run:
```bash
cargo build -p croma-cli
PHASE=pre-refactor ABC_ROOT=docs/untracked/corpus/zenodo-10k/abc \
  REF_ROOT=docs/untracked/corpus/zenodo-10k/musicxml \
  tools/session_bootstrap.sh --testbed 2>&1 | tail -8
```
Expected tail includes: `structural matches: 6581`, `mismatch rows: 202005`, `croma export failures: 65`.

- [ ] **Step 3: Snapshot the generated XML tree for later diffing**

Run:
```bash
cp -r docs/untracked/pre-refactor/full-10k-xml docs/untracked/pre-refactor-xml-snapshot
ls docs/untracked/pre-refactor-xml-snapshot | wc -l   # ~9935
```
This snapshot is the reference for the final byte-identical check. (Git-ignored; never committed.)

- [ ] **Step 4: Record the file-size starting point**

Run:
```bash
find crates/croma-core/src -name '*.rs' | xargs wc -l | sort -rn | head -8
```
Note the baseline (music.rs 7906, musicxml.rs 4241, fields.rs 2234, surface.rs 1290).

No commit (baseline only).

---

## Task 1: Create the empty module skeleton + wire `lib.rs`

Establish the directory tree and `mod`/`pub use` wiring first, so subsequent moves drop into place. Start by making the *new* module names alias the *existing* files, then physically move code into them in later tasks.

**Files:**
- Read first: `crates/croma-core/src/lib.rs:1-165`
- Modify: `crates/croma-core/src/lib.rs`
- Create: `crates/croma-core/src/{syntax,parse,lower,musicxml}/mod.rs` (created during the relevant move tasks; here only plan the `lib.rs` shape)

- [ ] **Step 1: Read the current public surface**

Run: `sed -n '1,165p' crates/croma-core/src/lib.rs`
Record every `pub mod`, `pub use`, and re-exported name. This list is the frozen public API — it must remain importable at the same paths after the refactor.

- [ ] **Step 2: Decide re-export shape (no code yet)**

For each currently-public item (e.g. `pub use music::{NoteSyntax, …}`, `pub use musicxml::write_score_partwise`, `pub use fields::…`), the new `lib.rs` will re-export the same name from its new module. Write the mapping into the commit message body in Step 3 of the final wiring task. (No file change in this step.)

- [ ] **Step 3: No-op checkpoint**

This task is planning-only for `lib.rs`; the actual `mod` declarations are added by each move task as its module is created, and the final re-export reconciliation happens in Task 9. Proceed to Task 2.

---

## Task 2: Extract surface AST → `syntax/`

`syntax/` holds the surface AST: the `*Syntax` types from `music.rs` (lines ~27–690), the lyric/symbol token AST, the field syntax types, and the tune surface structure from `surface.rs`.

**Files:**
- Source: `crates/croma-core/src/music.rs` (types block ~27–690), `crates/croma-core/src/surface.rs`
- Create: `crates/croma-core/src/syntax/mod.rs`, `syntax/music.rs`, `syntax/field.rs`, `syntax/lyric.rs`, `syntax/tune.rs`
- Create tests: `syntax/tune_tests.rs` (from `surface.rs` test mod), others as needed

- [ ] **Step 1: Move `surface.rs` wholesale (history-preserving rename)**

```bash
git mv crates/croma-core/src/surface.rs crates/croma-core/src/syntax/tune.rs
```
Update `lib.rs`: replace `mod surface;`/`pub use surface::…` with `pub mod syntax;` and add `mod tune;`+re-exports inside a new `syntax/mod.rs`. Create `syntax/mod.rs` with `pub mod tune;` and `pub use tune::*;` (mirroring the names `surface` previously exported). Commit (rename + minimal wiring) on its own:
```bash
git add -A && git commit -m "Move surface.rs to syntax/tune.rs"
```
Verify: `cargo build -p croma-core` compiles (fix only `use crate::surface` → `use crate::syntax::tune` references across the crate in this same commit if the build requires it; prefer a follow-up commit if large).

- [ ] **Step 2: Extract the music-line `*Syntax` types from `music.rs` into `syntax/music.rs`**

Move (verbatim) the type block from `music.rs` (the structs/enums spanning ~`NoteSyntax` at line 142 through `MalformedSyntaxKind` ~528, plus `MusicItem`/`AttachmentBundle`/`ParsedMusicDocument`/`ParsedTuneMusic`/`MusicLine`/`MusicToken*` at the top ~27–141) into a new `syntax/music.rs`. Keep `LoweredMusic` (line ~547) OUT — it belongs to `lower/`. Add `pub mod music;` + re-exports to `syntax/mod.rs`. Commit:
```bash
git add -A && git commit -m "Move music-line surface AST types to syntax/music.rs"
```
Verify build (expect unresolved paths in `music.rs`'s parser/lowering halves — fix imports in Step 5).

- [ ] **Step 3: Extract field + lyric/symbol syntax types**

Move the field syntax types (`InlineFieldSyntax`, `MusicFieldLine`, `MusicFieldLineKind`, `ScoreDirectiveSyntax`, `PreservedDirectiveSyntax` ~400–502) into `syntax/field.rs`, and the lyric/symbol token AST (`LyricLineSyntax`, `LyricTokenSyntax`, `LyricTokenKind`, `SymbolLineSyntax`, `SymbolTokenSyntax`, `SymbolTokenKind` ~434–483) into `syntax/lyric.rs`. Add both to `syntax/mod.rs`. Commit each move separately:
```bash
git add -A && git commit -m "Move field syntax types to syntax/field.rs"
git add -A && git commit -m "Move lyric/symbol syntax types to syntax/lyric.rs"
```

- [ ] **Step 4: Relocate the surface tests**

In `syntax/tune.rs`, replace the inline `#[cfg(test)] mod tests { … }` with `#[cfg(test)] #[path = "tune_tests.rs"] mod tests;` and move the test body into `syntax/tune_tests.rs`. Do the same for any syntax type tests that came from `music.rs`. Commit:
```bash
git add -A && git commit -m "Separate syntax tests into sibling files"
```

- [ ] **Step 5: Fix imports and verify**

Update `use` paths crate-wide so the parser/lowering halves of `music.rs` (still in place) import the moved types from `crate::syntax::*`. Make moved types `pub(crate)` where needed.
```bash
cargo build -p croma-core 2>&1 | tail -3
cargo test -p croma-core 2>&1 | tail -5
git add -A && git commit -m "Update imports for syntax module extraction"
```
Expected: compiles, tests green.

- [ ] **Step 6: Size check**

Run the size-cap check. `syntax/*.rs` must each be ≤1k. If `syntax/tune.rs` (~1,070 impl) exceeds, split its lowest-cohesion half into `syntax/tune_extra.rs` and commit.

---

## Task 3: Extract the parser → `parse/`

`parse/` holds text→syntax: the document/tune parser (`parser.rs`), the field parser (`fields.rs`), and the music-line parser (the parser half of `music.rs`).

**Files:**
- Source: `crates/croma-core/src/parser.rs`, `fields.rs`, music-line parser in `music.rs` (`parse_music_code_line` ~695, lyric/symbol parse fns ~1067–1300, `MusicLineParser` impl ~4279–6060)
- Create: `parse/mod.rs`, `parse/music.rs`, `parse/note.rs`, `parse/decoration.rs`, `parse/barline.rs`, `parse/lyric.rs`, `parse/directive.rs`, `parse/field/{mod,key,meter,voice,tempo,misc}.rs`
- Tests: sibling `*_tests.rs` for each

- [ ] **Step 1: Move `parser.rs` → `parse/mod.rs` (rename)**

```bash
git mv crates/croma-core/src/parser.rs crates/croma-core/src/parse/mod.rs
```
Update `lib.rs`: `mod parser;` → `pub mod parse;` (preserve any `pub use parser::…` as `pub use parse::…`). Commit alone:
```bash
git add -A && git commit -m "Move parser.rs to parse/mod.rs"
```
Verify build.

- [ ] **Step 2: Move `fields.rs` → `parse/field/mod.rs` (rename), then split by field group**

```bash
git mv crates/croma-core/src/fields.rs crates/croma-core/src/parse/field/mod.rs
git add -A && git commit -m "Move fields.rs to parse/field/mod.rs"
```
Then extract field-group parsers (key, meter, voice, tempo/length, misc) into `parse/field/{key,meter,voice,tempo,misc}.rs`, verbatim moves, one commit per group:
```bash
git add -A && git commit -m "Split key field parsing into parse/field/key.rs"
# …meter, voice, tempo, misc similarly
```
Add `pub mod {key,meter,voice,tempo,misc};` to `parse/field/mod.rs`. After splitting, `parse/field/mod.rs` must be ≤1k.

- [ ] **Step 3: Move the music-line parser half of `music.rs` into `parse/music.rs`**

Move (verbatim) `parse_music_code_line` (~695) and its immediate free helpers, plus the `MusicLineParser` struct + impl (~4279–6060) into `parse/music.rs`. Add `pub mod music;` to `parse/mod.rs`. Commit:
```bash
git add -A && git commit -m "Move music-line parser to parse/music.rs"
```

- [ ] **Step 4: Distribute `MusicLineParser` methods to feature files (sub-1k)**

`parse/music.rs` now holds ~1,780 lines of `MusicLineParser` methods — over the cap. Distribute method groups into sibling files, each an `impl<'line> MusicLineParser<'line> { … }`:
- `parse/note.rs`: `parse_note`, `parse_accidental_token`, `parse_octave_marks`, `parse_rest`, `parse_multi_measure_rest`, length/number parsing, chord + grace parsing.
- `parse/decoration.rs`: decoration/shorthand/annotation/quoted-text parsing.
- `parse/barline.rs`: barline-spelling parsing.
- `parse/lyric.rs`: `parse_lyric_line`/`parse_lyric_tokens`/`parse_symbol_line`/`parse_symbol_tokens` (~1067–1300).
- `parse/directive.rs`: `parse_score_stylesheet_directive`/`parse_preserved_stylesheet_directive` (~977–1066).
Leave only the parser core (constructor, dispatch loop, whitespace) in `parse/music.rs`.
Commit each group separately (`Move <X> parsing to parse/<X>.rs`). After each, `cargo build -p croma-core`.

- [ ] **Step 5: Separate parser tests**

Move inline parser tests into sibling `*_tests.rs` files matching where the code landed. Commit `Separate parse tests into sibling files`.

- [ ] **Step 6: Fix imports, verify, size-check**

```bash
cargo build -p croma-core && cargo test -p croma-core 2>&1 | tail -5
git add -A && git commit -m "Update imports for parse module extraction"
find crates/croma-core/src/parse -name '*.rs' ! -name '*_tests.rs' | xargs wc -l | awk '$1>1000'
```
Expected: green; size check prints nothing.

---

## Task 4: Extract lowering → `lower/`

`lower/` holds syntax→semantic-model: everything from the lowering half of `music.rs`.

**Files:**
- Source: `music.rs` lowering region (`LoweredMusic` ~547; `MultiVoiceLowering` ~1445–1780; `build_voice_timeline`/`VoiceTimelineBuilder` ~1890–2660; `semantic_*_from_timeline` ~2664–2930; tempo parsing ~2933–3090; `alignable_refs`/`attach_*` ~3431–3550; `LoweringState` ~3555–4279)
- Create: `lower/mod.rs`, `lower/voice.rs`, `lower/timeline.rs`, `lower/semantic.rs`, `lower/accidental.rs`, `lower/tie.rs`, `lower/tuplet.rs`, `lower/align.rs`, `lower/tempo.rs`
- Tests: sibling `*_tests.rs`

- [ ] **Step 1: Move remaining `music.rs` (lowering) → `lower/mod.rs`**

At this point `music.rs` should contain only the lowering region. Rename it:
```bash
git mv crates/croma-core/src/music.rs crates/croma-core/src/lower/mod.rs
```
Update `lib.rs`: drop `mod music;`; add `mod lower;` (lowering is largely `pub(crate)`; re-export any public lowering entry points the old `music` module exposed). Commit alone:
```bash
git add -A && git commit -m "Move lowering half to lower/mod.rs"
```
Verify build.

- [ ] **Step 2: Distribute lowering into feature files (sub-1k)**

Move verbatim, one commit each:
- `lower/voice.rs`: `LoweringState` struct + impl.
- `lower/timeline.rs`: `VoiceTimelineBuilder` + `build_voice_timeline` + measure segmentation.
- `lower/semantic.rs`: `semantic_voice_from_timeline`/`semantic_measure_from_timeline`/`semantic_events_for_measure`/`*_event_from_timeline`/`pitch_from_timeline`.
- `lower/accidental.rs`: accidental state/propagation helpers (`effective_accidental`, `MeasureAccidental`, set/reset).
- `lower/tie.rs`, `lower/tuplet.rs`: tie/tuplet lowering bits.
- `lower/align.rs`: `alignable_refs`/`attach_lyric`/`attach_symbol`.
- `lower/tempo.rs`: `parse_tempo_model`/`parse_tempo_beat` + fraction helpers used only there.
Leave `MultiVoiceLowering` orchestration + `LoweredMusic` in `lower/mod.rs`.
After each move: `cargo build -p croma-core`.

- [ ] **Step 3: Separate lowering tests**

Move inline lowering tests to sibling `*_tests.rs` files where the code landed. Commit `Separate lower tests into sibling files`.

- [ ] **Step 4: Fix imports, verify, size-check**

```bash
cargo build -p croma-core && cargo test -p croma-core 2>&1 | tail -5
git add -A && git commit -m "Update imports for lower module extraction"
find crates/croma-core/src/lower -name '*.rs' ! -name '*_tests.rs' | xargs wc -l | awk '$1>1000'
```
Expected: green; size check empty.

---

## Task 5: Extract the writer → `musicxml/`

**Files:**
- Source: `crates/croma-core/src/musicxml.rs`
- Create: `musicxml/mod.rs`, `musicxml/{score,attributes,note,grace,notation,harmony,direction,lyric,barline}.rs`
- Tests: sibling `*_tests.rs`

- [ ] **Step 1: Move `musicxml.rs` → `musicxml/mod.rs` (rename)**

```bash
git mv crates/croma-core/src/musicxml.rs crates/croma-core/src/musicxml/mod.rs
```
`lib.rs`: `mod musicxml;` → `pub mod musicxml;` keeping `pub use musicxml::write_score_partwise`. Commit alone:
```bash
git add -A && git commit -m "Move musicxml.rs to musicxml/mod.rs"
```
Verify build.

- [ ] **Step 2: Distribute `Writer` methods + helpers (sub-1k)**

Move verbatim into per-element files, each containing an `impl<'score> Writer<'score> { … }` plus the topical free helpers, one commit per file:
- `musicxml/score.rs`: `write_credits`/`write_metadata`/`write_part_list`/`write_part`.
- `musicxml/attributes.rs`: `write_attributes`/`write_clefs`/`write_transpose_if_available` + `meter_parts`/`clef_model` + `ClefModel`.
- `musicxml/note.rs`: `write_sequence`/`write_event`/`write_chord`/`write_note`/`write_pitch`/`write_ties` + `note_spelling`.
- `musicxml/grace.rs`: `write_grace_groups`/`write_grace_group`/`write_grace_note` + `grace_base_unit`/`grace_display_duration`/`grace_export_pitch`.
- `musicxml/notation.rs`: `write_notations`/`write_time_modification` + `decoration_notation`/`symbol_direction` + their enums.
- `musicxml/harmony.rs`: `write_chord_symbol`/`write_harmony` + `parse_chord_symbol` + `ParsedChordSymbol`.
- `musicxml/direction.rs`: `write_initial_directions`/`write_tempo_direction`/`write_preserved_directive`/`write_dynamic`/`write_direction_type`/`write_direction_words` + `beat_unit_model`/`sound_tempo_qpm`/`BeatUnit`.
- `musicxml/lyric.rs`: `write_lyrics`.
- `musicxml/barline.rs`: `write_barline`/`write_ending_barline`.
Keep in `musicxml/mod.rs`: `Writer` struct, `write_score_partwise`, `write_forward`/`write_backup`, `write_attrs`/`write_indent`/escaping.
After each move: `cargo build -p croma-core`.

- [ ] **Step 3: Separate writer tests**

The ~1,771 inline test lines move to sibling `*_tests.rs` files per element (e.g. `harmony_tests.rs`, `note_tests.rs`, `barline_tests.rs`, `direction_tests.rs`). Split any oversized test file by feature. Commit `Separate musicxml tests into sibling files`.

- [ ] **Step 4: Fix imports, verify, size-check**

```bash
cargo build -p croma-core && cargo test -p croma-core 2>&1 | tail -5
git add -A && git commit -m "Update imports for musicxml module extraction"
find crates/croma-core/src/musicxml -name '*.rs' ! -name '*_tests.rs' | xargs wc -l | awk '$1>1000'
```
Expected: green; size check empty.

---

## Task 6: Reconcile `lib.rs` public API

**Files:** Modify `crates/croma-core/src/lib.rs`

- [ ] **Step 1: Re-export every previously-public name from its new path**

Using the inventory from Task 1 Step 1, ensure `lib.rs` re-exports each item at the same external path (`pub use syntax::…`, `pub use parse::…`, `pub use musicxml::…`, etc.). Commit:
```bash
git add -A && git commit -m "Re-export public API from new module paths"
```

- [ ] **Step 2: Prove downstream crates compile unchanged**

```bash
git stash --include-untracked >/dev/null 2>&1 || true   # ensure clean
cargo build -p croma-cli -p croma-fmt -p croma-lsp 2>&1 | tail -5
```
Expected: all compile with **no changes** to those crates. If any fails, the public re-export is incomplete — fix `lib.rs` only (do not edit downstream crates), commit, repeat.

---

## Task 7: Format pass

**Files:** all of `crates/croma-core/src`

- [ ] **Step 1: Single formatting commit**

Now that all moves are done and rename detection is no longer at risk:
```bash
cargo fmt --all
cargo fmt --all -- --check && echo FMT_OK
git add -A && git commit -m "cargo fmt after module breakdown"
```

---

## Task 8: Full verification (acceptance gate)

**Files:** none

- [ ] **Step 1: Green gates**

```bash
cargo test --workspace 2>&1 | tail -6
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --all -- --check && echo FMT_OK
uv run python -m pytest tests -q 2>&1 | tail -3
git diff --check && echo DIFF_OK
```
Expected: all pass.

- [ ] **Step 2: Size cap satisfied**

```bash
find crates/croma-core/src -name '*.rs' ! -name '*_tests.rs' | xargs wc -l \
  | awk '$2!="total" && $1>1000 {print}'
```
Expected: prints nothing.

- [ ] **Step 3: Byte-identical 10k output**

```bash
cargo build -p croma-cli
PHASE=post-refactor ABC_ROOT=docs/untracked/corpus/zenodo-10k/abc \
  REF_ROOT=docs/untracked/corpus/zenodo-10k/musicxml \
  tools/session_bootstrap.sh --testbed 2>&1 | tail -6
diff -r docs/untracked/pre-refactor-xml-snapshot docs/untracked/post-refactor/full-10k-xml \
  && echo "IDENTICAL"
```
Expected: `IDENTICAL`, and the post report shows `structural matches: 6581`, `mismatch rows: 202005`, `croma export failures: 65` (unchanged from Task 0).

- [ ] **Step 4: History preserved spot-check**

```bash
git log --follow --oneline -- crates/croma-core/src/syntax/tune.rs | head
git log --follow --oneline -- crates/croma-core/src/musicxml/mod.rs | head
```
Expected: history extends back through the pre-refactor `surface.rs` / `musicxml.rs` lineage.

---

## Task 9: Open PR

**Files:** none

- [ ] **Step 1: Push and open PR**

```bash
git push -u origin work/phase-12-core-module-breakdown
gh pr create --base main --title "Break croma-core into a stage/feature module tree" \
  --body "Pure code-movement refactor per docs/superpowers/specs/2026-06-07-croma-core-module-breakdown-design.md. No behavior change: every croma-core impl file is now ≤1000 lines, tests live in sibling *_tests.rs files, public API frozen (cli/fmt/lsp compile unchanged). Verified: full test suite green, clippy/fmt clean, and the full local 10k MusicXML output is byte-identical (diff -r) to pre-refactor (6581 matches / 202005 rows / 65 failures)."
```

- [ ] **Step 2: Merge when both CI checks are green** (Rust + Linux/nixos), then delete the branch and update the progress tracker.

---

## Self-review notes (author)

- **Spec coverage:** hybrid tree (Tasks 2–5), ≤1k impl cap (size checks in every stage + Task 8), tests in sibling files (per-task test steps + fixed convention), frozen public API (Task 6), model.rs untouched (not in any task), big-bang single PR (Task 9), byte-identical verification (Task 0 snapshot + Task 8 diff), cli/fmt/lsp unchanged (Task 6 Step 2). All covered.
- **History preservation:** every file move uses `git mv` for the primary rename in its own commit; extractions are verbatim, separate commits; formatting deferred to Task 7. Covered per maintainer request.
- **No behavior change:** the only non-move commits are import/visibility/re-export adjustments and one format pass; correctness pinned by byte-identical 10k output.
- **Risk:** if any module lands >1k after distribution, the stage's size check catches it and the task says to split further before proceeding.
