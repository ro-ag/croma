# Phase 45 - full-measure rest duration comparison burn-down

Date: 2026-06-12

Branch: `codex/phase-45-residual-burndown`

## Target selection

Phase 45 started from the phase-44 residual tables and tracker note. The
selected target was the compact duration-only full-measure-rest family:

- phase 44 had 13,083 rows categorized as `duration` across 214 files;
- eight single-category files had exactly two rows each: `duration/quarter_length`
  and `duration/type`;
- all eight rows came from leading rests in no-barline Renaissance transcriptions
  where music21 rewrote the abc2xml reference rest as a whole-note full-measure
  rest despite the next event offset proving a breve span.

The target was a comparison-extractor hardening, not a parser/export change.

## Fix

`tools/music21_compare.py` now passes the next musical event into event fact
extraction. For rests only, if music21 marks the rest as `fullMeasure` and the
next event offset proves a longer span than music21's rewritten duration, the
duration fact uses that structural offset span. The duration type is re-derived
for simple note values such as breve, preserving existing note-duration
comparison for genuine duration mismatches.

Regression coverage:

- `test_full_measure_rest_reinterpretation_uses_event_offset_span`

## Evidence

Targeted eight-file full-measure-rest duration slice selected from the phase-44
residual mismatch table:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Structural matches | 0 | 8 | +8 |
| Structural mismatches | 8 | 0 | -8 |
| Mismatch rows | 16 | 0 | -16 |
| Duration category rows | 16 | 0 | -16 |

Full 10k comparison against the phase-44 final mismatch table:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Croma export successes | 9,935 | 9,935 | 0 |
| Croma export failures | 65 | 65 | 0 |
| Structural matches | 9,267 | 9,275 | +8 |
| Structural mismatches | 668 | 660 | -8 |
| Mismatch rows | 101,039 | 101,023 | -16 |
| Duration category rows | 13,083 | 13,067 | -16 |
| Files with duration category rows | 214 | 206 | -8 |

Full-report baseline delta:

- 8 files resolved.
- 0 files improved but still mismatched.
- 0 files regressed.
- 660 files were unchanged.

No Croma MusicXML import failures, reference MusicXML import failures,
comparison harness issues, or worker failures were reported in the final full
compare.

Resolved files:

- `tune_000464.abc`
- `tune_000816.abc`
- `tune_001028.abc`
- `tune_001029.abc`
- `tune_001728.abc`
- `tune_002218.abc`
- `tune_002757.abc`
- `tune_003144.abc`

## Artifacts

- `docs/untracked/phase-45-residual-burndown/target-full-measure-rest-duration-files.txt`
- `docs/untracked/phase-45-residual-burndown/target-full-measure-rest-duration-compare-report.json`
- `docs/untracked/phase-45-residual-burndown/target-full-measure-rest-duration-mismatches.parquet`
- `docs/untracked/phase-45-residual-burndown/full-10k-compare-report.json`
- `docs/untracked/phase-45-residual-burndown/full-10k-mismatches.parquet`
- `docs/untracked/phase-45-residual-burndown/full-10k-per-file-summary.parquet`
- `docs/untracked/phase-45-residual-burndown/full-10k-per-component-summary.parquet`

## Validation

- `uv run pytest tests/test_music21_polars_corpus_compare.py::test_full_measure_rest_reinterpretation_uses_event_offset_span -q`
- `uv run pytest tests/test_music21_polars_corpus_compare.py -q`
- targeted eight-file full-measure-rest duration comparison
- full 10k comparison with baseline delta
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo test --workspace`
- `cargo run -p croma-cli -- xml examples/basic.abc`
- `uv run pytest`

## Next target

Start from `docs/untracked/phase-45-residual-burndown/full-10k-mismatches.parquet`
and the phase-45 full report. The full-measure-rest duration extractor artifact
is cleared. Remaining duration rows are not this family; continue with
single-category slices only after proving either a Croma-side parser/export
fault or a clearly non-structural comparison artifact. Likely candidates remain
documented barline rendering policy rows, malformed/source-preservation
direction rows, harmony/chord-symbol placement policy, accidental grace-scope
edges, or another clean single-category repro.
