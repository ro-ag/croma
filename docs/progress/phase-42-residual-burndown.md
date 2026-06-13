# Phase 42 - residual direction quoted-text burn-down

Date: 2026-06-12

Branch: `codex/phase-42-residual-burndown`

## Target selection

Phase 42 started from the phase-41 full 10k residual evidence. The next
candidate was not chosen by category volume alone; each candidate had to show a
Croma-side fault rather than a documented abc2xml policy difference.

Rejected or deferred candidates:

1. Chord-member slur residuals: a prototype that treated chord-member slurs as
   member-specific in the target slice increased slur rows from 23 to 41, so it
   was reverted.
2. Remaining compact-key accidentals, orphan lyric hyphens, dangling ties, and
   several barline rows still matched documented reference quirks or
   malformed-input recovery differences.
3. Valid chord symbols before a barline continue to bind to the next timed
   event; that behavior is covered by an existing guard.

The confirmed Croma-side target was the residual direction placement family for
unprefixed quoted text that is not likely harmony, written immediately before a
barline. Examples from the corpus include `"I"|`, `"*"|`, `"ad lib."|`, and
`"4f":|`. These strings were parsed as pending chord-symbol text, skipped by
the phase-41 barline-direction flush, and later fell back to words on the next
measure's first event. The reference places them at the current measure's
barline position.

## Fix

`LoweringState::flush_pending_barline_directions` now drains pending quoted
chord-symbol text at a barline and splits it by a conservative harmony check:

- likely harmony text, currently anything whose trimmed first character is
  `A` through `G`, remains pending and can bind to the next note across the
  barline;
- non-harmony text is converted to a words attachment on the zero-duration
  barline anchor, matching the phase-41 annotation/direction-decoration path;
- final barlines keep pending text untouched so the existing end-of-voice
  dangling quoted text diagnostic still fires.

Regression coverage:

- `non_harmony_quoted_text_before_barline_binds_to_barline_position`
- `chord_symbol_before_barline_binds_to_next_note`
- `dangling_quoted_text_at_tune_end_warns_instead_of_silent_drop`
- the existing phase-41 barline direction placement tests

## Evidence

Focused 39-file direction target selected from the phase-41 residual
single-category direction rows:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Structural matches | 0 | 9 | +9 |
| Structural mismatches | 39 | 30 | -9 |
| Mismatch rows | 48 | 36 | -12 |
| Direction rows | 48 | 36 | -12 |

The 9 matched files were:

- `tune_001693.abc`
- `tune_005176.abc`
- `tune_006560.abc`
- `tune_006562.abc`
- `tune_007215.abc`
- `tune_007216.abc`
- `tune_007217.abc`
- `tune_015044.abc`
- `tune_015047.abc`

Broader 168-file target covering every phase-41 file with a direction component:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Structural matches | 0 | 12 | +12 |
| Structural mismatches | 168 | 156 | -12 |
| Mismatch rows | 49,888 | 46,570 | -3,318 |
| Direction rows | 304 | 284 | -20 |

Full 10k comparison against the phase-41 baseline:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Croma export successes | 9,935 | 9,935 | 0 |
| Croma export failures | 65 | 65 | 0 |
| Structural matches | 9,223 | 9,237 | +14 |
| Structural mismatches | 712 | 698 | -14 |
| Mismatch rows | 109,807 | 101,106 | -8,701 |
| Direction rows | 304 | 284 | -20 |
| Missing-in-Croma rows | 37,351 | 34,206 | -3,145 |
| Extra-in-Croma rows | 30,447 | 27,635 | -2,812 |
| Duration rows | 13,999 | 13,083 | -916 |
| Pitch rows | 8,649 | 7,822 | -827 |

Per-file status diff:

- 14 files moved from mismatch to match.
- 0 files moved from match to mismatch.
- 29 files changed mismatch-row counts.
- `tune_003837.abc` gained 5 rows while staying mismatched; it remains in the
  documented phantom-measure/reference-policy family.

The 14 full-corpus mismatch-to-match files were:

- `tune_001693.abc`
- `tune_005176.abc`
- `tune_006186.abc`
- `tune_006277.abc`
- `tune_006560.abc`
- `tune_006562.abc`
- `tune_007215.abc`
- `tune_007216.abc`
- `tune_007217.abc`
- `tune_010735.abc`
- `tune_011491.abc`
- `tune_011501.abc`
- `tune_015044.abc`
- `tune_015047.abc`

No Croma MusicXML import failures, reference MusicXML import failures,
comparison harness issues, or worker failures were reported in the final full
compare.

## Artifacts

- `docs/untracked/phase-42-residual-burndown/target-direction-files.txt`
- `docs/untracked/phase-42-residual-burndown/target-direction-final-compare-report.json`
- `docs/untracked/phase-42-residual-burndown/target-all-direction-files.txt`
- `docs/untracked/phase-42-residual-burndown/target-all-direction-final-compare-report.json`
- `docs/untracked/phase-42-residual-burndown/full-10k-baseline-compare-report.json`
- `docs/untracked/phase-42-residual-burndown/full-10k-final-export-report.json`
- `docs/untracked/phase-42-residual-burndown/full-10k-final-compare-report.json`
- `docs/untracked/phase-42-residual-burndown/full-10k-final-per-file-summary.parquet`
- `docs/untracked/phase-42-residual-burndown/full-10k-final-mismatches.parquet`

## Validation

- `cargo test -p croma-core dangling_quoted_text_at_tune_end_warns_instead_of_silent_drop -- --nocapture`
- `cargo test -p croma-core barline_binds_to -- --nocapture`
- targeted 39-file direction export and comparison
- targeted 168-file direction-component export and comparison
- full 10k Croma XML export
- full 10k comparison with artifact tables
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo test --workspace`
- `cargo run -p croma-cli -- xml examples/basic.abc`
- `uv run pytest`

## Next target

Start from the phase-42 full 10k residual tables. Do not chase `tune_003837`
phantom-measure rows as a Croma bug. The next target should again come from the
largest clean residual slice after filtering reference-policy families; likely
candidates are remaining text-only `Q:` tempo policy rows, malformed-input
recovery cases with proven source preservation loss, or another single-category
slice that can first be reduced to a minimal Croma-side repro.
