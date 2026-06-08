# Croma vs abc2xml — divergence justification

This directory documents **every class of difference** between Croma's MusicXML
output and the `abc2xml` reference output over the 10,000-tune Zenodo corpus, and
justifies each one against the **ABC 2.1 specification** — the sole authority for
"correct". `abc2xml` is only a comparison baseline; where it departs from the
spec, its output is a *reference artifact* and Croma is correct.

> **Authority order:** ABC 2.1 spec (what the music means) → MusicXML spec (how to
> write it) → `abc2xml` / other parsers (orientation only, never the oracle).
> Spec citations below reference the ABC 2.1 standard text by section and line.

## Start here

- **[`00-SUMMARY.md`](00-SUMMARY.md)** — the authoritative per-file verdict
  breakdown (the numbers below are derived there).
- **[`MIGRATION.md`](MIGRATION.md)** — the evidence-based case for moving an
  ABC → MusicXML pipeline from abc2xml to Croma.
- **[`per-file-manifest.csv`](per-file-manifest.csv)** — every differing file with
  its verdict, `music_identical`, `croma_correct`, and a spec-cited `justification`.

## Headline numbers (current `main`)

| Quantity | Count | % of 10k |
|---|--:|--:|
| Note content **identical** to abc2xml (7,031 exact + 2,707 pitch-identical) | **9,738** | **97.4%** |
| Files that differ in some way | 2,969 | 29.7% |
| **Genuine Croma issues** | **0** | 0% |
| Unclassified (`REVIEW`) | 0 | 0% |

Every one of the 2,969 differing files is classified `croma_correct` (2,929 `yes`,
40 `defensible`): an `abc2xml` reference artifact, a benign serialization
difference, malformed input, a positional **cascade** of one of those, or a case
where Croma is the more spec-correct of the two. The two formerly-residual tunes
(`tune_014316`/`tune_014317`) were fixed in
[PR #59](https://github.com/ro-ag/croma/pull/59).

The per-class table below is a **doc index** (class → spec citation); the
authoritative per-file counts live in [`00-SUMMARY.md`](00-SUMMARY.md).

## Verdict summary

| Class | Files | Verdict | Doc |
|---|--:|---|---|
| Export failures (no tune body) | 65 | Malformed input — Croma correctly refuses | [01](01-export-failures.md) |
| Phantom measures (annotation/section boundary) | 238¹ | abc2xml artifact — Croma correct | [02](02-phantom-measures.md) |
| Multi-measure rest `Z`/`X` expansion | many² | abc2xml representation — spec calls them equivalent | [03](03-multi-measure-rest.md) |
| Barline: spaced `\| \|` / line-split → `light-light` | 66³ | abc2xml artifact — not spec-mandated | [04](04-barline-spaced-double.md) |
| Accidental: redundant `<alter>0>` serialization | 963 (575⁴) | abc2xml artifact — semantically identical | [05](05-accidental-alter.md) |
| Duration: default unit length + rounding | 368 | abc2xml deviates from §4.6 / rounds; Croma exact | [06](06-duration.md) |
| Cascade artifacts (pitch, octave, harmony, lyric) | 262/258/381/9 | Positional cascade of a structural artifact | [07](07-cascade-artifacts.md) |
| Tuplet: bracket start/stop markers | 95 | abc2xml artifact — ratios/durations agree | [08](08-tuplet.md) |
| Tie & slur residuals | 45 / 31 | abc2xml drops legal cases / malformed / edge | [09](09-ties-and-slurs.md) |
| Direction residuals (tempo/annotation text) | 291 | abc2xml artifact / malformed / default-tempo | [10](10-directions.md) |
| Multipart & `<part-group>` | 0 / 342⁵ | Croma feature; part-group is a cosmetic gap | [11](11-multipart-and-partgroup.md) |

¹ Files where Croma emits fewer measures than the reference; the dominant
missing/extra/measure_alignment cascade driver.
² Every tune containing a standalone `Zn`/`Xn` (n>1).
³ Files whose *only* difference is the barline category (759 total touch it).
⁴ Single-category accidental files.
⁵ Reference files emitting `<part-group>` brackets; Croma emits none. Part
**counts** are identical in all 9,935 files.

## Method / reproduce

```sh
# Build the 10k testbed (corpus is external; see docs/testing/corpus-reproducibility.md)
PHASE=audit ABC_ROOT=…/zenodo-10k/abc REF_ROOT=…/zenodo-10k/musicxml \
  tools/session_bootstrap.sh --testbed

# Per-category detail for a set of files
uv run python tools/music21_polars_corpus_compare.py \
  --results-jsonl  docs/untracked/<phase>/full-10k-export-results.jsonl \
  --croma-xml-root docs/untracked/<phase>/full-10k-xml \
  --reference-root docs/untracked/corpus/zenodo-10k/musicxml \
  --only-files <newline-list> --component <category> \
  --mismatches-jsonl /tmp/x.jsonl --report /tmp/xr.json --jobs 0
```

Each per-class doc cites concrete tunes (measure numbers, pitches, ABC snippets)
so any claim can be re-checked against `target/debug/croma xml <file>` and the
reference XML.
