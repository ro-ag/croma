# Phase 40 - corpus mismatch follow-ups

Date: 2026-06-12

Branch: `codex/phase-40-mismatch-followups`

## Target selection

Phase 40 continued from the phase-39 evidence instead of treating that phase as
an ultimate fix. The next listed candidates were:

1. default `4/4` MusicXML export when no `M:` appears before music;
2. mid-measure repeat-ending framing for `[1` / `[2`;
3. lower-volume tie-chain residuals.

The default-meter candidate was rejected after rechecking the local ABC 2.1 and
2.2 references: when no `M:` field is defined, free meter is assumed. Croma's
Score model represents that as missing meter metadata; abc2xml's fabricated
`4/4` `<time>` is a reference-compatibility artifact, not a Croma bug. A
regression guard now verifies that omitted header meter stays free until a real
body meter change.

The tie residuals were also rechecked against the phase-33 ledger. The
remaining documented tie rows are reference quirks, malformed-input recoveries,
or alignment cascades, not an active Croma parser/export bug.

## Fix

Mid-measure repeat endings now start a pickup measure when `[1` / `[2` appears
after timed music but before an explicit barline. Lowering inserts the same
implicit regular boundary used for other measure splits before recording the
variant-ending event. This matches abc2xml's measure framing for compact repeats
such as:

```abc
|: A2 A2 A2[1 B2 | C8 :|
[2 D2 | E8 |]
```

Regression tests:

- `mid_measure_repeat_ending_starts_a_pickup_measure`
- `missing_header_meter_stays_free_until_body_meter_change`

The second test intentionally protects the rejected default-meter candidate:
temporarily reintroducing a synthetic default `<time>4/4</time>` makes it fail
with two `<time>` elements instead of one.

## Evidence

Targeted mid-measure ending files:

- `tune_015281.abc`
- `tune_015280.abc`
- `tune_001062.abc`

| Scope | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Structural matches | 0 | 3 | +3 |
| Structural mismatches | 3 | 0 | -3 |
| Mismatch rows | 5,220 | 0 | -5,220 |

Full 10k report-only comparison, using phase-39 final as the baseline:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Croma export successes | 9,935 | 9,935 | 0 |
| Croma export failures | 65 | 65 | 0 |
| Structural matches | 9,187 | 9,206 | +19 |
| Structural mismatches | 748 | 729 | -19 |
| Mismatch rows | 160,504 | 145,986 | -14,518 |
| Barline rows | 2,313 | 1,992 | -321 |
| Duration rows | 18,426 | 17,099 | -1,327 |
| Measure-alignment rows | 9,346 | 8,572 | -774 |

No Croma MusicXML import failures, reference MusicXML import failures,
comparison harness issues, or worker failures were reported in the final full
compare.

Per-file status diff against phase-39 final:

- 19 files moved from mismatch to match.
- 0 files moved from match to mismatch.
- 25 already-mismatching files had fewer mismatch rows.
- 1 already-mismatching file (`tune_011281.abc`) gained 11 rows while moving
  its first ending closer to the reference split; the residual remains dominated
  by abc2xml empty-measure policy around labels/finals.

## Artifacts

- `docs/untracked/phase-40-mismatch-followups/full-10k-export-report.json`
- `docs/untracked/phase-40-mismatch-followups/full-10k-report-only-compare-report.json`
- `docs/untracked/phase-40-mismatch-followups/full-10k-compare-report.json`
- `docs/untracked/phase-40-mismatch-followups/midmeasure-ending-before-compare-report.json`
- `docs/untracked/phase-40-mismatch-followups/midmeasure-ending-compare-report.json`
- `docs/untracked/phase-40-mismatch-followups/full-10k-per-file-summary.parquet`
- `docs/untracked/phase-40-mismatch-followups/full-10k-mismatches.parquet`

## Validation

- `cargo test -p croma-core mid_measure_repeat_ending_starts_a_pickup_measure -- --nocapture`
- `cargo test -p croma-core missing_header_meter_stays_free_until_body_meter_change -- --nocapture`
- `cargo test -p croma-core repeat_ending -- --nocapture`
- full 10k Croma XML export
- full 10k report-only comparison
- phase-39 vs phase-40 per-file summary diff
- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo test --workspace`
- `cargo run -p croma-cli -- xml examples/basic.abc`
- `uv run pytest`
- `uv run python tools/prove_abc_roundtrip.py --abc-root docs/untracked/corpus/zenodo-10k/abc --croma target/debug/croma --jobs 0 --out docs/untracked/phase-40-mismatch-followups/final/abc-roundtrip-report.json`

Final ABC roundtrip proof: 9,933 files in scope, 0 structural diffs, 0 errors,
and the expected 65 `abc.file.no_music` lower failures.

## Next target

Do not resurrect the default-meter candidate unless the project explicitly
chooses abc2xml compatibility over the ABC 2.1/2.2 free-meter rule. The next
parser/export phase should start from the current residual per-file/component
tables and first prove that a candidate is a Croma bug rather than a documented
phantom-measure, malformed-input, or reference-policy divergence.
