# Phase 48 - divergence triage batch

Date: 2026-06-13

Branch: `codex/phase-48-divergence-triage`

## Target selection

Phase 48 continued from the raw croma-vs-abc2xml worklist using the
`TRIAGE.md` protocol. Candidate selection followed the requested order:
remaining content categories first, single-category and fewest-row files first,
and no structural-cascade bulk decisions.

The first eligible batch was the four remaining single-category lyric files:

- `tune_002758.abc`
- `tune_011626.abc`
- `tune_011627.abc`
- `tune_008447.abc`

Each file had its own fresh investigator verdict. All four found the same
Croma-side lyric syllabic bug: a sung syllable followed by a trailing lyric
hyphen was exported as `single` or `end` when the ABC word remained open.

After that fix, the next eligible content batch was the smallest
duration/measure-alignment files:

- `tune_008233.abc`
- `tune_015262.abc`
- `tune_005486.abc`
- `tune_012981.abc`
- `tune_015465.abc`

Again, each file had a separate investigator verdict. Four were adjudicated as
non-Croma artifacts and appended to `dropped.csv`; `tune_015465.abc` remains in
the worklist because its verdict was low-confidence `undetermined`.

## Fix

Lyric lowering now preserves a literal trailing lyric hyphen as an open word
when the remainder of the current `w:` line has no later syllable token. This
allows MusicXML export to emit:

- `begin` for word-initial trailing-hyphen tokens such as `be-*`, `af-`, and
  `Sav-`;
- `middle` for an already-open word ending in another trailing-hyphen token such
  as `au- di-`.

The existing orphan-overflow behavior is preserved: `a-b` on a single available
note still does not attach an orphan hyphen or fabricate an open word for the
unattached `b`.

Regression coverage:

- `trailing_lyric_hyphen_keeps_syllabic_word_open`

## Drops

Four file-specific non-Croma verdicts were appended to
`docs/comparison/abc2xml-divergences/dropped.csv`:

| File | Category | Subcategory | Reason |
| --- | --- | --- | --- |
| `tune_008233.abc` | duration | `abc2xml-duration` | abc2xml encodes `F////` and `F///` with the same 157/2520 duration instead of exact 1/128 and 1/64. |
| `tune_015262.abc` | duration | `abc2xml-duration` | abc2xml truncates `(4` tuplet sixteenth durations to 59/315 on its 2520-division grid. |
| `tune_005486.abc` | duration | `abc2xml-broken-rhythm` | abc2xml leaves the post-grace melodic note in `g/>{a}g/` unhalved. |
| `tune_012981.abc` | duration | `equivalence` | Unequal-length broken rhythm is undefined; both tools choose the same notated 64th recovery, but abc2xml truncates it to 157/2520. |

## Evidence

Baseline at the start of the batch:

| Metric | Before |
| --- | ---: |
| Structural matches | 9,259 |
| Structural mismatches | 600 |
| Dropped files | 76 in the stale raw-baseline report; 114 in the committed CSV |
| Mismatch rows | 42,822 |
| Lyric rows | 7 |

After the lyric fix and fresh full export:

| Metric | After lyric fix |
| --- | ---: |
| Structural matches | 9,264 |
| Structural mismatches | 557 |
| Dropped files | 114 |
| Mismatch rows | 42,737 |
| Lyric rows | 2 |

After appending the four duration drops and rerunning comparison:

| Metric | Final |
| --- | ---: |
| Files selected | 9,882 |
| Structural matches | 9,264 |
| Structural mismatches | 553 |
| Dropped files | 118 |
| Mismatch rows | 42,692 |
| Lyric rows | 2 |
| Duration rows | 7,181 |
| Measure-alignment rows | 4,512 |

The four fixed lyric files now compare as structural matches with fresh Croma
XML:

- `tune_002758.abc`
- `tune_011626.abc`
- `tune_011627.abc`
- `tune_008447.abc`

## Artifacts

- `docs/untracked/phase-48-divergence-triage/full-10k/export-results.jsonl`
- `docs/untracked/phase-48-divergence-triage/full-10k/export-report.json`
- `docs/untracked/phase-48-divergence-triage/full-10k/report.json`
- `docs/untracked/phase-48-divergence-triage/full-10k/mismatches.parquet`
- `docs/untracked/phase-48-divergence-triage/full-10k/report-after-drops.json`
- `docs/untracked/phase-48-divergence-triage/full-10k/mismatches-after-drops.parquet`

## Validation

- `cargo test -p croma-core trailing_lyric_hyphen_keeps_syllabic_word_open -- --nocapture`
- `cargo test -p croma-core orphan_lyric_hyphen_does_not_start_syllabic_word -- --nocapture`
- `cargo test -p croma-core lyric -- --nocapture`
- targeted fresh `music21_compare.py` checks for all four lyric files
- full 10k Croma XML export with `tools/corpus_harness.py --mode xml`
- full 10k raw comparison before and after duration drops
- `cargo test --workspace`
- `cargo run -p croma-cli -- xml examples/basic.abc`
- `uv run pytest`

## Next target

Continue from
`docs/untracked/phase-48-divergence-triage/full-10k/mismatches-after-drops.parquet`
and `report-after-drops.json`. The smallest remaining duration candidate is
`tune_015465.abc`, but its file-specific verdict was low-confidence
`undetermined` for an overlapping chained broken-rhythm construct, so keep it
unless a human policy decision clarifies the intended interpretation.

Next clean duration/measure-alignment candidates by row count are
`tune_009575.abc`, `tune_010143.abc`, `tune_009683.abc`, and
`tune_009684.abc`. Continue to dispatch one fresh investigator per file, and do
not bulk-drop structural cascades or the remaining missing/extra families.
