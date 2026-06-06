# Corpus Comparison Reproducibility

This recipe rebuilds the Croma corpus testbed artifacts used by the phase 10
parser work. It is intentionally a committed text recipe only; generated XML,
JSONL, Parquet, and reports stay under `docs/untracked/`.

## Required Inputs

These two corpus roots live **outside** the Croma repository and are not
committed. They must be made available wherever you run this recipe (mounted,
copied, or downloaded into the sandbox). Point the `ABC_ROOT` / `REF_ROOT`
environment variables at wherever they actually live in your environment.

- ABC corpus root (`ABC_ROOT`): directory tree of `*.abc` source files
  (10000 expected).
- Reference MusicXML root (`REF_ROOT`): matching `*.musicxml` / `*.xml`
  reference files.
- Optional 10k manifest: a `manifest.jsonl` index of the corpus, if available.
- Croma repository root: the checkout you run these commands from.
- Rust toolchain: Rust 1.96.0, pinned by `rust-toolchain.toml` and provided by
  whichever development environment you use (`rustup` in the Linux cloud
  sandbox, or the Nix flake locally — see `docs/development-environment.md`).

The corpus originated on a macOS workstation under
`…/trd_obsolete/test/real/{abc,musicxml}` (Phase 10-i export results still
record that provenance path). A fresh Linux cloud sandbox does **not** contain
the corpus; provisioning it is the prerequisite step before any corpus phase.

## Provision Original Corpus

The original ABC source corpus is reproducible from the Zenodo dataset used by
the older TRD work:

- Dataset: **ABC Notation Dataset (10k samples)**
- DOI: <https://doi.org/10.5281/zenodo.17694747>
- Zenodo record: <https://zenodo.org/records/17694747>
- Downloaded JSON: `dataset_10k.json`
- License: Creative Commons Attribution 4.0 International

Download and import the 10k ABC sources into ignored Croma storage:

```sh
tools/session_bootstrap.sh --fetch-corpus
```

Bootstrap first looks for a verified Git LFS cache archive:

- `docs/corpus/zenodo-10k-abc.tar.gz`
- `docs/corpus/zenodo-10k-abc.tar.gz.sha256`

If the archive is present and its SHA-256 matches, bootstrap extracts it. If the
archive is absent, unresolved as a Git LFS pointer, or fails checksum
verification, bootstrap falls back to the Zenodo download URL above.

The generated local corpus lives under ignored storage:

- `docs/untracked/corpus/zenodo-10k/cache/dataset_10k.json`
- `docs/untracked/corpus/zenodo-10k/abc/*.abc`
- `docs/untracked/corpus/zenodo-10k/manifest.jsonl`
- `docs/untracked/corpus/zenodo-10k/license-report.json`

When cloning with Git LFS smudge disabled, fetch only this corpus cache with:

```sh
git lfs pull --include docs/corpus/zenodo-10k-abc.tar.gz
```

Generate the reference MusicXML files with Willem Vree's `abc2xml.py`:

```sh
tools/session_bootstrap.sh --fetch-corpus --fetch-reference
```

This also downloads `abc2xml.py-268.zip` into ignored storage and writes:

- `docs/untracked/corpus/zenodo-10k/tools/abc2xml/`
- `docs/untracked/corpus/zenodo-10k/musicxml/*.xml`
- `docs/untracked/corpus/zenodo-10k/abc2xml-report.jsonl`

After this, bootstrap auto-detects the ignored corpus paths. You can also set
the roots explicitly:

```sh
export ABC_ROOT=docs/untracked/corpus/zenodo-10k/abc
export REF_ROOT=docs/untracked/corpus/zenodo-10k/musicxml
```

The lower-level provisioner is available when you need a limited test download
or an already downloaded JSON:

```sh
uv run python tools/provision_corpus.py fetch-zenodo-10k --output docs/untracked/corpus/zenodo-10k
uv run python tools/provision_corpus.py import-zenodo-10k /path/to/dataset_10k.json --output docs/untracked/corpus/zenodo-10k
uv run python tools/provision_corpus.py import-archive --archive docs/corpus/zenodo-10k-abc.tar.gz --output docs/untracked/corpus/zenodo-10k
uv run python tools/provision_corpus.py abc2xml-real --output docs/untracked/corpus/zenodo-10k
```

To rebuild the LFS cache archive after regenerating the ABC sources:

```sh
uv run python tools/provision_corpus.py build-archive --output docs/untracked/corpus/zenodo-10k --archive docs/corpus/zenodo-10k-abc.tar.gz
git lfs track "docs/corpus/*.tar.gz"
git add .gitattributes docs/corpus/zenodo-10k-abc.tar.gz docs/corpus/zenodo-10k-abc.tar.gz.sha256
```

## Environment

Run all commands from the Croma repository root.

`rust-toolchain.toml` pins Rust 1.96.0, so `cargo`/`rustc` automatically select
the correct toolchain on any host — no absolute toolchain path is needed. If you
are not already inside the project dev shell, provision the toolchain and Python
deps first:

```sh
rustup show   # installs Rust 1.96.0 + clippy + rustfmt per rust-toolchain.toml
uv sync       # installs pinned Python deps (music21, polars, pytest)
```

Set the corpus roots and a per-phase output directory. If you used
`tools/session_bootstrap.sh --fetch-corpus --fetch-reference`, these are the
default ignored local roots:

```sh
# Point these at the corpus locations in *your* environment.
export ABC_ROOT="${ABC_ROOT:-docs/untracked/corpus/zenodo-10k/abc}"
export REF_ROOT="${REF_ROOT:-docs/untracked/corpus/zenodo-10k/musicxml}"

# Use a new phase directory for new work, for example phase-10j.
export PHASE=phase-10j
export OUT=docs/untracked/$PHASE
mkdir -p "$OUT"
```

Build the CLI used by the harness:

```sh
cargo build -p croma-cli   # produces target/debug/croma
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
