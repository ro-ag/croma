# Phase 46 - visual-only barline style comparison burn-down

Date: 2026-06-13

Branch: `codex/phase-46-residual-burndown`

## Target selection

Phase 46 started from the phase-45 full 10k residual tables and the documented
abc2xml barline rendering policy note. The selected target was the
single-category `barline` family where only visual barline styles differed:

- phase 45 had 1,460 residual rows categorized as `barline` across 335 files;
- a 183-file clean slice had only `barline` rows and no other mismatch
  categories;
- the dominant pattern was documented abc2xml rendering policy for spaced or
  line-split plain bars, where music21 surfaces `light-light`/plain style
  differences that do not carry repeat semantics.

The target was comparison-policy hardening, not a parser/export change.

## Fix

`tools/music21_polars_corpus_compare.py` now reduces barline facts to structural
repeat semantics before comparing sides. `direction` and `times` are preserved,
while plain visual styles such as `regular`, `double`, `final`, `dotted`, and
`none` normalize away when there is no repeat direction or repeat-ending span.

Regression coverage:

- `test_visual_only_barline_style_difference_is_equivalent`
- `test_repeat_barline_direction_difference_is_still_flagged`

## Evidence

Targeted 183-file visual-only barline style slice selected from the phase-45
residual mismatch table:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Structural matches | 0 | 108 | +108 |
| Structural mismatches | 183 | 75 | -108 |
| Mismatch rows | 437 | 185 | -252 |
| Barline category rows | 437 | 185 | -252 |

Full 10k comparison against the phase-45 final mismatch table:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Croma export successes | 9,935 | 9,935 | 0 |
| Croma export failures | 65 | 65 | 0 |
| Structural matches | 9,275 | 9,383 | +108 |
| Structural mismatches | 660 | 552 | -108 |
| Mismatch rows | 101,023 | 100,394 | -629 |
| Barline category rows | 1,460 | 831 | -629 |
| Files with barline category rows | 335 | 177 | -158 |

Full-report baseline delta:

- 108 files resolved.
- 155 files improved but remained mismatched.
- 0 files regressed.
- 397 phase-45 mismatched files were unchanged.

No Croma MusicXML import failures, reference MusicXML import failures,
comparison harness issues, or worker failures were reported in the final full
compare.

The remaining 185 target rows preserve structural repeat information, including
repeat direction and repeat-ending spans; they are not part of the visual-only
barline style family.

## Artifacts

- `docs/untracked/phase-46-residual-burndown/target-visual-barline-style-files.txt`
- `docs/untracked/phase-46-residual-burndown/target-visual-barline-style-compare-report.json`
- `docs/untracked/phase-46-residual-burndown/target-visual-barline-style-mismatches.parquet`
- `docs/untracked/phase-46-residual-burndown/full-10k-compare-report.json`
- `docs/untracked/phase-46-residual-burndown/full-10k-mismatches.parquet`
- `docs/untracked/phase-46-residual-burndown/full-10k-per-file-summary.parquet`
- `docs/untracked/phase-46-residual-burndown/full-10k-per-component-summary.parquet`

## Validation

- `uv run pytest tests/test_music21_polars_corpus_compare.py::test_visual_only_barline_style_difference_is_equivalent tests/test_music21_polars_corpus_compare.py::test_repeat_barline_direction_difference_is_still_flagged -q`
- `uv run pytest tests/test_music21_polars_corpus_compare.py -q`
- targeted 183-file visual-only barline style comparison
- full 10k comparison with phase-45 baseline delta
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo test --workspace`
- `cargo run -p croma-cli -- xml examples/basic.abc`
- `uv run pytest`

## Next target

Start from `docs/untracked/phase-46-residual-burndown/full-10k-mismatches.parquet`
and the phase-46 full report. Visual-only plain barline style rows are cleared.
Remaining barline rows include repeat semantics or repeat-ending spans and must
not be normalized away without a separate proof. Continue with the largest clean
single-category slice only after proving either a Croma-side parser/export fault
or a clearly non-structural comparison artifact. Likely candidates remain
malformed/source-preservation direction rows, harmony/chord-symbol placement
policy, accidental grace-scope edges, remaining duration families, or another
clean single-category repro.
