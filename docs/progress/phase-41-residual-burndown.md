# Phase 41 - residual direction boundary burn-down

Date: 2026-06-12

Branch: `codex/phase-41-residual-burndown`

## Target selection

Phase 41 started from the phase-40 residual per-file/component evidence rather
than from a stale backlog item. The first candidates checked were:

1. duration-only breve/whole rows;
2. harmony-only positional rows;
3. direction-only rows around annotations and decorations before barlines.

The duration-only slice was rejected as the already-documented abc2xml
full-measure-rest / overlong-duration rewriting artifact. Most harmony-only
rows were also positional shifts; chord symbols before a barline remain on the
existing Croma policy for this phase.

The direction slice produced a repeatable Croma-side placement bug. ABC 2.1
allows annotations to target the following bar line (§4.19), and decorations may
apply to bar lines (§4.14). Croma preserved `"_text"|` and `!f!|`, but lowered
them onto the first timed event after the boundary instead of at the current
measure's barline position.

## Fix

Lowering now flushes pending barline-bound annotations and direction-style
decorations to a zero-duration timed spacer anchor immediately before the
barline is lowered. The MusicXML writer already emits attachments before a
sequence event, so this places the direction at the current measure offset.

The fix is deliberately scoped:

- placement-prefixed annotations before a barline bind to the barline position;
- direction-style decorations such as dynamics before a barline bind to the
  barline position;
- code-line-only boundaries still do not void pending decorations;
- chord symbols before a barline keep the existing next-note policy;
- note-attached notation decorations such as trills, articulations, fingerings,
  and arpeggios still bind to the next timed event.

Regression tests:

- `dynamic_decoration_before_barline_binds_to_barline_position`
- `annotation_before_barline_binds_to_barline_position`
- `dynamic_decoration_at_line_end_without_barline_binds_to_next_note`

## Evidence

Minimal probe after the fix:

| ABC construct | Croma after | abc2xml |
| --- | --- | --- |
| `C D E F !f!| G...` | measure 1, offset 4.0 | measure 1, offset 4.0 |
| `C D E F "_f"| G...` | measure 1, offset 4.0 | measure 1, offset 4.0 |
| `C D E F "F"| G...` | measure 2, offset 0.0 | measure 1, offset 4.0 |

Targeted comparison over the 257 files that had a phase-40 direction-component
mismatch:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Mismatch rows | 86,260 | 50,440 | -35,820 |
| Direction-component rows | 605 | 457 | -148 |
| Files with direction-component rows | 257 | 168 | -89 |

Clean phase-40 direction-only slice:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Files | 57 | 40 | -17 |
| Mismatch rows | 76 | 50 | -26 |
| Structural matches in slice | 0 | 17 | +17 |

Full 10k report-only comparison against phase 40:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Croma export successes | 9,935 | 9,935 | 0 |
| Croma export failures | 65 | 65 | 0 |
| Structural matches | 9,206 | 9,223 | +17 |
| Structural mismatches | 729 | 712 | -17 |
| Mismatch rows | 145,986 | 109,807 | -36,179 |
| Direction rows | 451 | 304 | -147 |
| Missing-in-Croma rows | 50,370 | 37,351 | -13,019 |
| Extra-in-Croma rows | 42,143 | 30,447 | -11,696 |
| Pitch rows | 11,709 | 8,649 | -3,060 |
| Duration rows | 17,099 | 13,999 | -3,100 |

No Croma MusicXML import failures, reference MusicXML import failures,
comparison harness issues, or worker failures were reported in the final full
compare.

## Artifacts

- `docs/untracked/phase-41-residual-burndown/direction-target-files.txt`
- `docs/untracked/phase-41-residual-burndown/direction-target-compare-report.json`
- `docs/untracked/phase-41-residual-burndown/direction-target-mismatches.parquet`
- `docs/untracked/phase-41-residual-burndown/full-10k-export-report.json`
- `docs/untracked/phase-41-residual-burndown/full-10k-report-only-compare-report.json`

## Validation

- `cargo test -p croma-core barline_binds_to_barline_position -- --nocapture`
- `cargo test -p croma-core dynamic_decoration_at_line_end_without_barline_binds_to_next_note -- --nocapture`
- `cargo test -p croma-core chord_symbol_before_barline_binds_to_next_note -- --nocapture`
- targeted 257-file direction export and comparison
- full 10k Croma XML export
- full 10k report-only comparison
- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo run -p croma-cli -- xml examples/basic.abc`
- `uv run pytest`

## Next target

Start from the phase-41 full 10k residual report. The remaining direction rows
are mostly text-only `Q:` default-tempo policy, chord-symbol/harmony barline
placement differences, and broader positional cascades; prove Croma-side fault
before coding. The best next technical candidate is likely the largest remaining
single-category slice after filtering documented reference quirks, not another
default-meter compatibility change.
