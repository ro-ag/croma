# Phase 47 - leading harmony phantom-measure comparison burn-down

Date: 2026-06-13

Branch: `codex/phase-47-residual-burndown`

## Target selection

Phase 47 started from the phase-46 full 10k residual tables and a concrete
issue report for `tune_004767.abc`: abc2xml can emit an empty leading measure
that carries only harmony after liberal/malformed barline input such as
`"A":c4`. Croma does not emit that phantom measure; its first measure contains
the real music. The positional shift then cascades into note, duration, pitch,
harmony, and measure-alignment rows.

The selected target was the pair-aware affected set:

- reference part starts with a zero-note measure carrying harmony;
- Croma's corresponding first measure contains musical notes;
- the file is still mismatched in the phase-46 residual table.

This produced 23 files with 20,774 phase-46 residual rows. The target was
comparison-extractor hardening, not a parser/export change.

## Fix

`tools/music21_compare.py` can now drop leading harmony-only phantom measures
when instructed by the paired comparison task. Retained measures are renumbered
canonically, measure offsets are normalized relative to the first retained
measure, and harmony/direction/repeat-ending context measure values consult the
same remap.

`tools/music21_polars_corpus_compare.py` decides the drop list per task and per
part by inspecting both XML files. It drops a reference leading harmony-only
measure only when Croma's corresponding first measure has notes. This protects
legitimate Croma leading direction-only measures and cases where both sides
start with an empty measure. The side-specific facts cache key includes the
drop list so cached facts cannot mix normalization modes.

Regression coverage:

- `test_reference_empty_leading_measure_is_normalized_with_harmony`

## Evidence

Targeted 23-file empty-leading-harmony-measure slice selected from the phase-46
residual mismatch table:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Structural matches | 0 | 3 | +3 |
| Structural mismatches | 23 | 20 | -3 |
| Mismatch rows | 20,774 | 13,669 | -7,105 |
| Measure alignment rows | 631 | 358 | -273 |
| Harmony rows | 698 | 388 | -310 |
| Missing-in-Croma rows | 7,504 | 5,024 | -2,480 |
| Extra-in-Croma rows | 6,574 | 4,439 | -2,135 |

Full 10k comparison against the phase-46 final mismatch table:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Croma export successes | 9,935 | 9,935 | 0 |
| Croma export failures | 65 | 65 | 0 |
| Structural matches | 9,383 | 9,411 | +28 |
| Structural mismatches | 552 | 524 | -28 |
| Mismatch rows | 100,394 | 92,745 | -7,649 |
| Measure alignment rows | 7,232 | 6,433 | -799 |
| Harmony rows | 1,211 | 901 | -310 |
| Missing-in-Croma rows | 34,206 | 31,726 | -2,480 |
| Extra-in-Croma rows | 27,635 | 25,500 | -2,135 |

Full-report baseline delta:

- 28 files resolved.
- 25 files improved but remained mismatched.
- 0 files regressed from match to mismatch.
- 0 mismatched files got worse.
- 499 phase-46 mismatched files were unchanged.

No Croma MusicXML import failures, reference MusicXML import failures,
comparison harness issues, or worker failures were reported in the final full
compare.

The earlier broad skip attempt was rejected because files such as
`tune_013004.abc` and `tune_007424.abc` showed legitimate leading empty
measures. The accepted implementation is pair-aware and leaves those files at
their phase-46 row counts or better.

## Artifacts

- `docs/untracked/phase-47-residual-burndown/target-empty-leading-harmony-measure-files.txt`
- `docs/untracked/phase-47-residual-burndown/target-empty-leading-harmony-measure-compare-report.json`
- `docs/untracked/phase-47-residual-burndown/target-empty-leading-harmony-measure-mismatches.parquet`
- `docs/untracked/phase-47-residual-burndown/full-10k-compare-report.json`
- `docs/untracked/phase-47-residual-burndown/full-10k-mismatches.parquet`
- `docs/untracked/phase-47-residual-burndown/full-10k-per-file-summary.parquet`
- `docs/untracked/phase-47-residual-burndown/full-10k-per-component-summary.parquet`

## Validation

- `uv run pytest tests/test_music21_polars_corpus_compare.py::test_reference_empty_leading_measure_is_normalized_with_harmony -q`
- `uv run pytest tests/test_music21_polars_corpus_compare.py -q`
- targeted 23-file empty-leading-harmony-measure comparison
- full 10k comparison with phase-46 baseline delta
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo test --workspace`
- `cargo run -p croma-cli -- xml examples/basic.abc`
- `uv run pytest`

## Next target

Start from `docs/untracked/phase-47-residual-burndown/full-10k-mismatches.parquet`
and the phase-47 full report. The confirmed reference-side leading harmony
phantom-measure artifact is normalized. Remaining measure-alignment rows still
include legitimate leading empty measures, bar-duration policy rows, and
downstream structural cascades; do not generalize the phantom-measure skip
without paired evidence. Likely candidates remain bar-duration-only comparison
policy, malformed/source-preservation direction rows, harmony/chord-symbol
placement policy, accidental grace-scope edges, remaining duration families, or
another clean single-category repro.
