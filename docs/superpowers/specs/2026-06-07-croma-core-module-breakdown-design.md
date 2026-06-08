# croma-core module breakdown — design

Date: 2026-06-07
Status: approved (brainstorming) — pending spec review

## Problem

`croma-core` concentrates most of its logic in a few oversized files, which makes
the code hard to navigate and forces agents (and humans) to load large payloads
to work on any one concern.

Current source sizes (lines):

| File | Lines | ~Impl | ~Tests | Concerns fused together |
|------|------:|------:|-------:|-------------------------|
| `music.rs` | 7,906 | ~6,060 | ~1,844 | surface AST types **+** music-line parser **+** syntax→semantic lowering |
| `musicxml.rs` | 4,241 | ~2,470 | ~1,771 | the entire MusicXML writer |
| `fields.rs` | 2,234 | ~1,990 | ~241 | header/inline field parsing |
| `surface.rs` | 1,290 | ~1,070 | ~218 | tune/line surface structure |
| `model.rs` | 785 | — | — | semantic model types (already coherent, under cap) |

`music.rs` is the core problem: it welds three distinct pipeline stages into one
file. The two largest files also carry ~3.6k lines of inline `#[cfg(test)]` tests
that must be loaded alongside the implementation.

## Goal

Reorganize `croma-core` into a stage→feature module tree so that:

- Every **implementation** `.rs` file is **≤ 1,000 lines** (hard acceptance criterion).
- Tests live in **separate sibling files** so reading an implementation file never
  forces loading its tests (and vice-versa). Test files are split by feature for
  sanity but are **not** subject to the 1k line cap.
- An agent can find everything about a feature (e.g. harmony, decorations,
  barlines) in a small, well-named file.

## Non-goals / constraints

- **Behavior-preserving.** This is pure code-movement. No parser/exporter behavior
  changes. Output must be byte-identical before and after.
- **Public API frozen.** `croma-core` is crates.io-compatible and consumed by
  `croma-cli`/`croma-fmt`/`croma-lsp`. Every currently-public name keeps its
  current path via `pub use` re-exports in `lib.rs`. Only *internal* module paths
  move. `croma-cli/fmt/lsp` compile unchanged.
- **Scope = `croma-core` only.** `croma-cli` (696-line `main.rs`), `croma-fmt`,
  `croma-lsp` are out of scope.
- `model.rs` is left as a single file (already coherent and under the cap).
- No unrelated refactors; no logic changes smuggled in.

## Rollout

A **single big-bang PR** on `work/phase-12-core-module-breakdown`, verified by the
acceptance checks below before merge. Rationale: the change is mechanical and the
byte-identical-output check is a strong safety net, so a single coherent reorg is
preferable to a partially-migrated tree across several PRs.

## Target module tree

```
croma-core/src/
  lib.rs            # module decls + pub re-exports (public API frozen)
  source.rs diagnostic.rs error.rs options.rs test_support.rs   # unchanged
  model.rs          # semantic model types (Score/Part/Measure…) — unchanged

  syntax/           # surface AST            (from music.rs types + surface.rs)
    mod.rs          # shared syntax types + re-exports
    tune.rs         # tune/line surface structure (from surface.rs)
    music.rs        # music-line item AST: Note/Chord/GraceGroup/Tuplet/Slur/Tie/… Syntax
    field.rs        # InlineFieldSyntax, MusicFieldLine(Kind), directive syntax
    lyric.rs        # LyricLineSyntax / SymbolLineSyntax token AST

  parse/            # text → syntax          (from parser.rs + fields.rs + music.rs parser)
    mod.rs          # document/tune parse entry (from parser.rs)
    field/          # header/inline field parsing (from fields.rs), split by group
      mod.rs key.rs meter.rs voice.rs tempo.rs misc.rs
    music.rs        # MusicLineParser core + parse_music_code_line
    note.rs         # parse_note/accidental/octave/length/rest/chord/grace (MusicLineParser methods)
    decoration.rs   # decorations, shorthands, annotations, quoted text
    barline.rs      # barline spellings
    lyric.rs        # parse_lyric_line/tokens + parse_symbol_line/tokens
    directive.rs    # stylesheet / preserved directive parsing

  lower/            # syntax → semantic model (from music.rs lowering half)
    mod.rs          # MultiVoiceLowering orchestration + lower entry
    voice.rs        # LoweringState (per-voice lowering)
    timeline.rs     # VoiceTimelineBuilder + measure segmentation
    semantic.rs     # semantic_*_from_timeline (timeline → Score/Part/Measure)
    accidental.rs   # accidental propagation/state (effective_accidental, MeasureAccidental)
    tie.rs          # tie lowering
    tuplet.rs       # tuplet lowering
    align.rs        # lyric/symbol alignment (alignable_refs, attach_*)
    tempo.rs        # parse_tempo_model / parse_tempo_beat (tempo struct parsing)

  musicxml/         # semantic model → MusicXML (from musicxml.rs)
    mod.rs          # Writer struct + write_score_partwise entry + low-level xml (indent/attrs/escape)
    score.rs        # credits/metadata/part-list/part scaffolding
    attributes.rs   # attributes/clef/transpose + meter_parts/clef_model
    note.rs         # write_sequence/event/note/chord/pitch/ties + note_spelling
    grace.rs        # grace groups + grace duration helpers
    notation.rs     # write_notations + decoration_notation + symbol_direction + time_modification
    harmony.rs      # write_chord_symbol/harmony + parse_chord_symbol (the chord base)
    direction.rs    # directions/tempo/dynamics/words + beat_unit_model/sound_tempo_qpm
    lyric.rs        # write_lyrics
    barline.rs      # write_barline / write_ending_barline
```

Each implementation file above has a sibling test file holding the tests for that
module's code, e.g. `musicxml/harmony.rs` ↔ `musicxml/harmony_tests.rs`. **Naming
convention (fixed, applied uniformly):** the implementation file declares its test
module with an explicit path so files stay flat (no extra directories):

```rust
#[cfg(test)]
#[path = "harmony_tests.rs"]
mod tests; // separate sibling file; white-box access to pub(crate)/private items
```

A test file may itself be split when a feature has a lot of tests (e.g.
`harmony_tests.rs` + `harmony_chord_quality_tests.rs`, the second declared the
same way); test files are not subject to the 1k cap.

## How the big files map

### `music.rs` (the main win) → three stages
- `*Syntax` types and the surface AST enums → `syntax/{music,field,lyric}.rs`.
- `MusicLineParser` (impl ~1,780 lines) + free `parse_*` functions → `parse/*`.
  The single large `impl<'line> MusicLineParser<'line>` is distributed across
  `parse/{music,note,decoration,barline,lyric}.rs`, each holding an
  `impl<'line> MusicLineParser<'line>` block with a subset of methods.
- `MultiVoiceLowering`, `VoiceTimelineBuilder`, `LoweringState`,
  `semantic_*_from_timeline`, alignment, and tempo parsing → `lower/*`.

### `musicxml.rs` → per-element writer modules
- The `Writer` struct and `write_score_partwise` entry stay in `musicxml/mod.rs`
  with the low-level XML helpers (`write_attrs`, `write_indent`, escaping).
- The rest of the `impl<'score> Writer<'score>` is carved into per-element
  `impl<'score> Writer<'score>` blocks across `musicxml/*.rs`.
- Topical free helpers move to their feature file: `parse_chord_symbol` →
  `harmony.rs`, `clef_model`/`meter_parts` → `attributes.rs`,
  `note_spelling`/`grace_*` → `note.rs`/`grace.rs`,
  `decoration_notation`/`symbol_direction` → `notation.rs`,
  `beat_unit_model`/`sound_tempo_qpm` → `direction.rs`.

### `fields.rs` → `parse/field/`
Split the field parser by field group (key, meter, voice, tempo, misc) so each
file stays well under the 1k cap.

### `surface.rs` → `syntax/tune.rs`
Tune/line surface structure, split into a second helper file if it exceeds the cap.

## Mechanics

- A struct's `impl` can be split across multiple files of the same module; Rust
  allows multiple `impl` blocks for the same type. This is how `MusicLineParser`
  and `Writer` are distributed.
- Visibility: symbols that newly cross a module boundary become `pub(crate)`.
  Nothing new becomes `pub`.
- `lib.rs` keeps the public surface identical via `pub use`/`pub mod` re-exports.

## Verification (acceptance criteria)

A reviewer must be able to confirm all of:

1. **Size cap:** no implementation `.rs` file in `croma-core/src` exceeds 1,000
   lines. (Test files exempt.) Checkable with a one-liner:
   `find crates/croma-core/src -name '*.rs' ! -name '*_tests.rs' | xargs wc -l | awk '$1>1000'`
   returns nothing (excluding the `total` line). The exact `*_tests.rs` naming is
   whatever convention the implementation adopts.
2. **Tests separated:** implementation files contain no inline test bodies; tests
   live in sibling files.
3. **Green gates:** `cargo test --workspace`, `cargo clippy --workspace
   --all-targets -- -D warnings`, `cargo fmt --all -- --check`,
   `uv run python -m pytest tests -q`, `git diff --check` all pass.
4. **Byte-identical output:** run the full local 10k testbed before and after the
   refactor; `diff -r` the two `full-10k-xml/` trees → identical, and the compare
   report is unchanged (baseline at time of writing: 6,581 structural matches /
   202,005 mismatch rows / 65 export failures).
5. **Public API frozen:** `croma-cli`, `croma-fmt`, `croma-lsp` compile with no
   source changes.
6. **No behavior change:** the diff is moves + visibility + re-exports only; no
   altered logic.

## Risks & mitigations

- **Large mechanical diff is error-prone.** Mitigated by criterion 4
   (byte-identical 10k output) and the full test suite — a behavior change cannot
   slip through silently.
- **Accidental public-API change.** Mitigated by criterion 5 (downstream crates
   compile unchanged) and explicit `pub use` re-exports.
- **A module lands over 1k after the split.** Criterion 1 forces a further split;
   the plan must call out any file projected near the cap.

## Out of scope

- Any parser/exporter behavior change or bug fix.
- `croma-cli`/`croma-fmt`/`croma-lsp` reorganization.
- Splitting `model.rs`.
- Changing the public API.
