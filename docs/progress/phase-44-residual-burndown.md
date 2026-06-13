# Phase 44 - tuplet bracket-marker comparison burn-down

Date: 2026-06-12

Branch: `codex/phase-44-residual-burndown`

## Target selection

Phase 44 started from the phase-43 full 10k residual tables and tracker note.
The selected target was the residual tuplet bracket-marker family:

- phase-43 had 318 rows categorized as `tuplet` across 35 files;
- 41 of those rows across 12 files had identical tuplet timing ratios after
  removing only music21's visible bracket `type` marker;
- the existing divergence documentation already classified this as a
  typesetting edge: ABC defines the tuplet ratio/timing, while bracket
  start/stop placement can vary between renderers.

The target was therefore a comparison-policy hardening, not a parser/export
change.

## Fix

`tools/music21_polars_corpus_compare.py` now normalizes tuplet fact values by
dropping `type` from tuplet dictionaries that still carry timing information
(`actual` and/or `normal`). This preserves comparison of missing tuplets and
timing-ratio differences, but stops counting visible bracket start/stop marker
placement as a structural mismatch.

Regression coverage:

- `test_tuplet_bracket_marker_difference_is_equivalent`

## Evidence

Targeted 12-file type-only tuplet slice selected from the phase-43 residual
mismatch table:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Structural matches | 0 | 6 | +6 |
| Structural mismatches | 12 | 6 | -6 |
| Mismatch rows | 1,366 | 1,325 | -41 |
| Tuplet category rows | 93 | 52 | -41 |

Six files still mismatch in the target slice because they retain unrelated
residual rows or non-type-only tuplet rows.

Full 10k comparison against the phase-43 final mismatch table:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Croma export successes | 9,935 | 9,935 | 0 |
| Croma export failures | 65 | 65 | 0 |
| Structural matches | 9,261 | 9,267 | +6 |
| Structural mismatches | 674 | 668 | -6 |
| Mismatch rows | 101,080 | 101,039 | -41 |
| Tuplet category rows | 318 | 277 | -41 |
| Tuplet component rows | 5,871 | 5,830 | -41 |
| Files with tuplet category rows | 35 | 26 | -9 |

Full-report baseline delta:

- 6 files resolved.
- 6 files improved.
- 0 files regressed.
- 662 files were unchanged.

No Croma MusicXML import failures, reference MusicXML import failures,
comparison harness issues, or worker failures were reported in the final full
compare.

## Artifacts

- `docs/untracked/phase-44-residual-burndown/target-tuplet-type-only-files.txt`
- `docs/untracked/phase-44-residual-burndown/target-tuplet-type-only-compare-report.json`
- `docs/untracked/phase-44-residual-burndown/target-tuplet-type-only-mismatches.parquet`
- `docs/untracked/phase-44-residual-burndown/full-10k-compare-report.json`
- `docs/untracked/phase-44-residual-burndown/full-10k-mismatches.parquet`
- `docs/untracked/phase-44-residual-burndown/full-10k-per-file-summary.parquet`
- `docs/untracked/phase-44-residual-burndown/full-10k-per-component-summary.parquet`

## Validation

- `uv run pytest tests/test_music21_polars_corpus_compare.py::test_tuplet_bracket_marker_difference_is_equivalent -q`
- `uv run pytest tests/test_music21_polars_corpus_compare.py -q`
- targeted 12-file type-only tuplet comparison
- full 10k comparison with baseline delta
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo test --workspace`
- `cargo run -p croma-cli -- xml examples/basic.abc`
- `uv run pytest`

## Next target

Start from `docs/untracked/phase-44-residual-burndown/full-10k-mismatches.parquet`
and the phase-44 full report. Tuplet bracket-marker placement rows are no
longer counted; remaining tuplet rows are not the same type-only family and
need separate evidence before any comparison or exporter change. Continue with
single-category slices only after proving either a Croma-side parser/export
fault or a clearly non-structural comparison artifact.
