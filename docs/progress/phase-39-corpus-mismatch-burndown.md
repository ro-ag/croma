# Phase 39 - corpus mismatch burn-down

Date: 2026-06-12

Branch: `codex/phase-39-corpus-mismatch-burndown`

## Target selection

Phase 38 left the active parser/model/export backlog empty, so phase 39 started
from fresh corpus evidence rather than stale backlog entries. The baseline run
used the 10k Zenodo corpus under `docs/untracked/corpus/zenodo-10k` and the
phase output root `docs/untracked/phase-39-corpus-mismatch-burndown`.

Fresh baseline:

- Exports: 9,935 successes, 65 expected `abc.file.no_music` failures.
- Compare: 8,898 structural matches, 1,037 structural mismatches, 161,725
  mismatch rows.
- Largest single-category clean-up opportunity: 389 barline-only files from
  659 files containing barline rows.

Two read-only subagents triaged the broader mismatch set. Their findings were:

- Barline-only repeat style and ending-stop placement had the clearest
  high-confidence Croma fix path.
- Pitch, octave, accidental, duration, and measure-alignment rows were mostly
  cascades from malformed-input recovery differences, repeat-ending framing, or
  reference-only empty measure policy.
- Independent next candidates are default 4/4 time export when no `M:` appears
  before first music, and repeat-ending framing for mid-measure `[1` / `[2`
  forms.

## Fixes

1. Repeat barlines now emit explicit MusicXML bar styles:

   - `RepeatStart` -> `heavy-light`
   - `RepeatEnd` / `RepeatBoth` -> `light-heavy`

   abc2xml/music21 reports those styles explicitly; Croma previously emitted
   only repeat direction.

2. Repeat endings now close before a new repeated section begins:

   - a trailing `|:` after an ending closes the open ending on the previous
     measure;
   - a leading `|:` on the next measure closes the open ending on the prior
     measure.

   This prevents variant endings such as `[2 ... |: ...` from spanning into
   the following repeated section.

Regression tests:

- `repeat_barlines_emit_explicit_musicxml_bar_styles`
- `repeat_ending_closes_before_trailing_repeat_start`
- `repeat_ending_closes_before_next_leading_repeat_start`

## Evidence

Targeted barline-only before/after over the 389 baseline barline-only files:

| Stage | Structural matches | Structural mismatches | Mismatch rows |
| --- | ---: | ---: | ---: |
| Baseline | 0 | 389 | 1,349 |
| Repeat styles only | 0 | 389 | 1,349 |
| Add trailing repeat-start ending close | 207 | 182 | 514 |
| Add leading repeat-start ending close | 288 | 101 | 202 |

Final full 10k report-only comparison:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Croma export successes | 9,935 | 9,935 | 0 |
| Croma export failures | 65 | 65 | 0 |
| Structural matches | 8,898 | 9,187 | +289 |
| Structural mismatches | 1,037 | 748 | -289 |
| Mismatch rows | 161,725 | 160,504 | -1,221 |
| Barline rows | 3,533 | 2,313 | -1,220 |

No Croma MusicXML import failures, reference MusicXML import failures,
comparison harness issues, or worker failures were reported in the final full
compare.

Key artifacts:

- `docs/untracked/phase-39-corpus-mismatch-burndown/full-10k-compare-report.json`
- `docs/untracked/phase-39-corpus-mismatch-burndown/after-leading-repeat-boundary/barline-only-compare-report.json`
- `docs/untracked/phase-39-corpus-mismatch-burndown/after-leading-repeat-boundary/full-10k-report-only-compare-report.json`

## Residual verdicts

The remaining 101 barline-only target residual files have 202 barline rows:
181 right-barline rows, 16 left-barline rows, and 5 repeat-ending rows.

The dominant residual family is split or adjacent plain barlines such as
`| |`, including line-split variants. abc2xml often serializes these as
MusicXML `light-light` doubles; Croma preserves the measure structure while
collapsing the split delimiter to a plain boundary. The existing phase-33
ledger classifies this as `barline-spaced-and-newline-split-coalesced` and a
reference/comparator-policy issue, not an active Croma bug.

The remaining repeat-ending residuals, represented by `tune_000205.abc`, still
look like a separate framing nuance around repeated sections and should be
investigated with the broader repeat-ending family, especially mid-measure
`[1` / `[2` forms.

## Next target

Recommended next evidence-driven parser/export target:

1. Missing default 4/4 meter export when no `M:` appears before the first music.
   This is a small, likely Croma-side bug that can cascade into duration and
   measure-alignment rows.
2. Repeat-ending framing for mid-measure `[1` / `[2` forms. This is higher
   volume than default meter but needs a tighter design because some residuals
   overlap with malformed/reference recovery behavior.
3. Tie-chain export around leading carried ties, after the above structural
   groups, because the isolated file count is lower.
