# Phase 43 - text-only tempo comparison burn-down

Date: 2026-06-12

Branch: `codex/phase-43-residual-burndown`

## Target selection

Phase 43 started from the phase-42 full 10k residual tables, as recorded in
the tracker. The selected target was the remaining text-only `Q:` tempo policy
slice, not a parser/export change:

- phase-42 residuals contained 26 `MetronomeMark` rows in 26 files where both
  sides preserved the visible tempo words but music21 surfaced different
  playback-only default BPM values;
- ABC 2.1 does not prescribe a playback BPM for string-only tempo fields such
  as `Q:"Allegretto"`;
- documented direction residual triage already classified these rows as a
  comparator-policy issue rather than a Croma-side MusicXML bug.

The target was therefore to normalize only music21's playback-only
`MetronomeMark` text in the comparison layer.

## Fix

`tools/music21_polars_corpus_compare.py` now normalizes direction facts for
music21 `MetronomeMark` values whose text is explicitly marked
`(playback only)`. The normalization keeps the direction kind, measure, and
offset, and only collapses the invented BPM text to a stable playback-only
sentinel. Explicit numeric tempo markings are not normalized.

While running the full baseline-delta comparison, the phase also exposed a
baseline-loader bug: sparse JSONL mismatch files could fail Polars schema
inference when early rows contained only nulls in a typed value column. The
baseline JSONL loader now counts filenames with the JSON parser directly;
parquet baselines still use Polars.

Regression coverage:

- `test_text_only_tempo_playback_bpm_difference_is_equivalent`
- `test_baseline_mismatch_loader_handles_sparse_jsonl_columns`

## Evidence

Targeted 26-file playback-only tempo slice selected from the phase-42 residual
mismatch table:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Structural matches | 0 | 24 | +24 |
| Structural mismatches | 26 | 2 | -24 |
| Mismatch rows | 515 | 489 | -26 |
| Direction rows | 26 | 0 | -26 |

The two files still mismatching in the target slice (`tune_013765.abc` and
`tune_014745.abc`) retained unrelated residual rows; the playback-only tempo
row was removed in both.

Full 10k comparison against the phase-42 final mismatch table:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Croma export successes | 9,935 | 9,935 | 0 |
| Croma export failures | 65 | 65 | 0 |
| Structural matches | 9,237 | 9,261 | +24 |
| Structural mismatches | 698 | 674 | -24 |
| Mismatch rows | 101,106 | 101,080 | -26 |
| Direction category rows | 284 | 258 | -26 |
| Direction component rows | 434 | 408 | -26 |
| Files with direction component rows | 152 | 126 | -26 |

Full-report baseline delta:

- 24 files resolved.
- 2 files improved by one row.
- 0 files regressed.
- 672 files were unchanged.

No Croma MusicXML import failures, reference MusicXML import failures,
comparison harness issues, or worker failures were reported in the final full
compare.

## Artifacts

- `docs/untracked/phase-43-residual-burndown/target-text-tempo-playback-files.txt`
- `docs/untracked/phase-43-residual-burndown/target-text-tempo-playback-compare-report.json`
- `docs/untracked/phase-43-residual-burndown/target-text-tempo-playback-mismatches.parquet`
- `docs/untracked/phase-43-residual-burndown/full-10k-compare-report.json`
- `docs/untracked/phase-43-residual-burndown/full-10k-mismatches.parquet`
- `docs/untracked/phase-43-residual-burndown/full-10k-per-file-summary.parquet`
- `docs/untracked/phase-43-residual-burndown/full-10k-per-component-summary.parquet`

## Validation

- `uv run pytest tests/test_music21_polars_corpus_compare.py::test_text_only_tempo_playback_bpm_difference_is_equivalent -q`
- `uv run pytest tests/test_music21_polars_corpus_compare.py::test_baseline_mismatch_loader_handles_sparse_jsonl_columns -q`
- `uv run pytest tests/test_music21_polars_corpus_compare.py -q`
- targeted 26-file playback-only tempo comparison
- full 10k comparison with baseline delta
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo test --workspace`
- `cargo run -p croma-cli -- xml examples/basic.abc`
- `uv run pytest`

## Next target

Start from `docs/untracked/phase-43-residual-burndown/full-10k-mismatches.parquet`
and the phase-43 full report. The text-only `Q:` playback-only BPM policy rows
are no longer counted. Remaining direction rows are now dominated by malformed
input recovery, annotation/text preservation differences, chord-symbol/harmony
placement policy, and positional cascades. Continue selecting targets from
evidence that proves a Croma-side parser/export fault; do not chase
abc2xml-reference policy or music21 playback-default differences as parser bugs.
