# Corpus Comparison Reproducibility

This recipe rebuilds the Croma corpus testbed artifacts used by the phase 10
parser work. It is intentionally a committed text recipe only; generated XML,
JSONL, Parquet, and reports stay under `docs/untracked/`.

## Required Inputs

- ABC corpus root: `/Users/rodox/dev/rs/trd_obsolete/test/real/abc`
- Reference MusicXML root: `/Users/rodox/dev/rs/trd_obsolete/test/real/musicxml`
- Optional 10k manifest: `/Users/rodox/dev/rs/trd_obsolete/test/real/manifest.jsonl`
- Croma repository root: `/Users/rodox/dev/rs/croma`
- Rust toolchain: `/Users/rodox/.rustup/toolchains/1.96.0-aarch64-apple-darwin`

The ABC and reference roots are outside the Croma repository. Phase 10-i export
results record this `trd_obsolete` corpus path, and this local machine currently
does not have `/Users/rodox/dev/rs/trd/test/real`.

## Environment

Run from `/Users/rodox/dev/rs/croma`.

```sh
export TOOLCHAIN=/Users/rodox/.rustup/toolchains/1.96.0-aarch64-apple-darwin
export PATH="$TOOLCHAIN/bin:$PATH"
export RUSTC="$TOOLCHAIN/bin/rustc"

export ABC_ROOT=/Users/rodox/dev/rs/trd_obsolete/test/real/abc
export REF_ROOT=/Users/rodox/dev/rs/trd_obsolete/test/real/musicxml

# Use a new phase directory for new work, for example phase-10j.
export PHASE=phase-10j
export OUT=docs/untracked/$PHASE
mkdir -p "$OUT"
```

Build the CLI used by the harness:

```sh
"$TOOLCHAIN/bin/cargo" build -p croma-cli
```

## Full 10k Croma XML Export

This recreates the corpus input file list implicitly by recursively discovering
all `.abc` files under `ABC_ROOT`, sorted by path. Expected count is 10000.

```sh
uv run python tools/corpus_harness.py \
  --croma target/debug/croma \
  --corpus "$ABC_ROOT" \
  --mode xml \
  --report "$OUT/full-10k-export-report.json" \
  --results-jsonl "$OUT/full-10k-export-results.jsonl" \
  --keep-xml-dir "$OUT/full-10k-xml" \
  --progress-every 500
```

Expected outputs:

- `$OUT/full-10k-export-report.json`
- `$OUT/full-10k-export-results.jsonl`
- `$OUT/full-10k-xml/*.croma.musicxml`

Phase 10-i reference counts were 10000 attempted files, 9935 successful Croma
exports, and 65 Croma export failures.

## Full 10k Music21/Polars Comparison

This compares the Croma XML export against reference MusicXML files under
`REF_ROOT`. Reference paths are resolved by matching each exported ABC relative
path with `.musicxml` or `.xml` under `REF_ROOT`.

Use this report-only form for routine before/after checks. It avoids writing the
large normalized fact/comparison/mismatch tables.

```sh
uv run python tools/music21_polars_corpus_compare.py \
  --results-jsonl "$OUT/full-10k-export-results.jsonl" \
  --croma-xml-root "$OUT/full-10k-xml" \
  --reference-root "$REF_ROOT" \
  --report "$OUT/full-10k-report-only-compare-report.json" \
  --jobs 0 \
  --progress-every 500
```

Use this artifact form when the next phase needs queryable Polars tables. It can
write large files, so keep it under `docs/untracked/`.

```sh
uv run python tools/music21_polars_corpus_compare.py \
  --results-jsonl "$OUT/full-10k-export-results.jsonl" \
  --croma-xml-root "$OUT/full-10k-xml" \
  --reference-root "$REF_ROOT" \
  --report "$OUT/full-10k-compare-report.json" \
  --facts-jsonl "$OUT/full-10k-facts.jsonl" \
  --facts-parquet "$OUT/full-10k-facts.parquet" \
  --comparison-jsonl "$OUT/full-10k-comparison.jsonl" \
  --comparison-parquet "$OUT/full-10k-comparison.parquet" \
  --mismatches-jsonl "$OUT/full-10k-mismatches.jsonl" \
  --mismatches-parquet "$OUT/full-10k-mismatches.parquet" \
  --per-file-summary-jsonl "$OUT/full-10k-per-file-summary.jsonl" \
  --per-file-summary-parquet "$OUT/full-10k-per-file-summary.parquet" \
  --per-component-summary-jsonl "$OUT/full-10k-per-component-summary.jsonl" \
  --per-component-summary-parquet "$OUT/full-10k-per-component-summary.parquet" \
  --jobs 0 \
  --progress-every 500
```

Report-only output:

- `$OUT/full-10k-report-only-compare-report.json`

Artifact-mode outputs:

- `$OUT/full-10k-compare-report.json`
- `$OUT/full-10k-facts.{jsonl,parquet}`
- `$OUT/full-10k-comparison.{jsonl,parquet}`
- `$OUT/full-10k-mismatches.{jsonl,parquet}`
- `$OUT/full-10k-per-file-summary.{jsonl,parquet}`
- `$OUT/full-10k-per-component-summary.{jsonl,parquet}`

Phase 10-i reference counts were 3086 structural matches, 6849 structural
mismatches, 3578140 mismatch rows, zero Croma MusicXML import failures, zero
reference MusicXML import failures, and zero comparison harness issues.

## Create A Targeted Corpus From Evidence

Use a component-filtered comparison to create a file list and copy the original
ABC sources for files that still have mismatches in that component.

For the residual lyric target used after phase 10-i:

```sh
uv run python tools/music21_polars_corpus_compare.py \
  --results-jsonl "$OUT/full-10k-export-results.jsonl" \
  --croma-xml-root "$OUT/full-10k-xml" \
  --reference-root "$REF_ROOT" \
  --report "$OUT/residual-lyric-selector-report.json" \
  --component lyric \
  --facts-jsonl "$OUT/residual-lyric-facts.jsonl" \
  --facts-parquet "$OUT/residual-lyric-facts.parquet" \
  --comparison-jsonl "$OUT/residual-lyric-comparison.jsonl" \
  --comparison-parquet "$OUT/residual-lyric-comparison.parquet" \
  --mismatches-jsonl "$OUT/residual-lyric-mismatches.jsonl" \
  --mismatches-parquet "$OUT/residual-lyric-mismatches.parquet" \
  --per-file-summary-jsonl "$OUT/residual-lyric-per-file-summary.jsonl" \
  --per-file-summary-parquet "$OUT/residual-lyric-per-file-summary.parquet" \
  --per-component-summary-jsonl "$OUT/residual-lyric-per-component-summary.jsonl" \
  --per-component-summary-parquet "$OUT/residual-lyric-per-component-summary.parquet" \
  --write-file-list "$OUT/residual-lyric-files.txt" \
  --write-target-corpus-dir "$OUT/residual-lyric-target-corpus" \
  --jobs 0 \
  --progress-every 500
```

Expected outputs:

- `$OUT/residual-lyric-files.txt`
- `$OUT/residual-lyric-target-corpus/*.abc`
- `$OUT/residual-lyric-selector-report.json`
- `$OUT/residual-lyric-*.{jsonl,parquet}`

Phase 10-i used a 107-file lyric target corpus for the selected NBSP/melisma
fix before the fix was applied. After that fix, a direct lyric selector over the
full 10k comparison should narrow the residual set to 89 rows in 7 files.

## Targeted Export And Comparison

Run targeted exports from the copied corpus directory:

```sh
uv run python tools/corpus_harness.py \
  --croma target/debug/croma \
  --corpus "$OUT/residual-lyric-target-corpus" \
  --mode xml \
  --report "$OUT/target-after-export-report.json" \
  --results-jsonl "$OUT/target-after-export-results.jsonl" \
  --keep-xml-dir "$OUT/target-after-xml" \
  --progress-every 25
```

Then compare the targeted exports:

```sh
uv run python tools/music21_polars_corpus_compare.py \
  --results-jsonl "$OUT/target-after-export-results.jsonl" \
  --croma-xml-root "$OUT/target-after-xml" \
  --reference-root "$REF_ROOT" \
  --report "$OUT/target-after-compare-report.json" \
  --facts-jsonl "$OUT/target-after-facts.jsonl" \
  --facts-parquet "$OUT/target-after-facts.parquet" \
  --comparison-jsonl "$OUT/target-after-comparison.jsonl" \
  --comparison-parquet "$OUT/target-after-comparison.parquet" \
  --mismatches-jsonl "$OUT/target-after-mismatches.jsonl" \
  --mismatches-parquet "$OUT/target-after-mismatches.parquet" \
  --per-file-summary-jsonl "$OUT/target-after-per-file-summary.jsonl" \
  --per-file-summary-parquet "$OUT/target-after-per-file-summary.parquet" \
  --per-component-summary-jsonl "$OUT/target-after-per-component-summary.jsonl" \
  --per-component-summary-parquet "$OUT/target-after-per-component-summary.parquet" \
  --jobs 0 \
  --progress-every 25
```

Expected outputs:

- `$OUT/target-after-export-report.json`
- `$OUT/target-after-export-results.jsonl`
- `$OUT/target-after-xml/*.croma.musicxml`
- `$OUT/target-after-compare-report.json`
- `$OUT/target-after-*.{jsonl,parquet}`

When measuring before/after improved, regressed, and unchanged file counts, add
`--baseline-mismatches "$OUT/target-before-baseline-minimal-mismatches.jsonl"`
to the comparison command. That file should contain the pre-change minimal
mismatch rows for the same target.

## Validation Queries

Confirm input availability:

```sh
find "$ABC_ROOT" -type f -name '*.abc' | wc -l
find "$REF_ROOT" -type f \( -name '*.musicxml' -o -name '*.xml' \) | wc -l
```

Confirm full export and comparison summary:

```sh
jq '{
  files_discovered,
  files_selected,
  files_attempted,
  croma_export_successes,
  croma_export_failures,
  structural_matches,
  structural_mismatches,
  mismatch_rows,
  croma_musicxml_import_failures,
  reference_musicxml_import_failures,
  comparison_harness_issues,
  mismatch_category_counts
}' "$OUT/full-10k-report-only-compare-report.json"
```

Confirm direct lyric residual rows:

```sh
uv run python - <<'PY'
import os
from pathlib import Path
import polars as pl

out = Path(os.environ["OUT"])
mismatches = pl.read_parquet(out / "full-10k-mismatches.parquet")
direct_lyric = mismatches.filter(pl.col("mismatch_category") == "lyric")
print({
    "direct_lyric_rows": direct_lyric.height,
    "direct_lyric_files": direct_lyric.select("filename").unique().height,
})
PY
```

Confirm targeted corpus and targeted comparison:

```sh
find "$OUT/residual-lyric-target-corpus" -maxdepth 1 -type f -name '*.abc' | wc -l

jq '{
  files_discovered,
  files_selected,
  files_attempted,
  croma_export_successes,
  croma_export_failures,
  structural_matches,
  structural_mismatches,
  mismatch_rows,
  mismatch_category_counts,
  baseline
}' "$OUT/target-after-compare-report.json"
```

## Progress Tracker

Before selecting a new parser phase, restore and query the tracker:

```sh
uv run python tools/progress/progress.py restore
uv run python tools/progress/progress.py status
uv run python tools/progress/progress.py metrics --phase phase-10i
uv run python tools/progress/progress.py artifacts --phase phase-10i
```

After a completed phase, update the runtime tracker DB, export it back to the
committed SQL snapshot, and commit only the SQL snapshot plus source/docs/tests:

```sh
uv run python tools/progress/progress.py export
```
