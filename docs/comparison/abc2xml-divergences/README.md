# Croma vs abc2xml — divergence justification

This directory documents **every class of difference** between Croma's MusicXML
output and the `abc2xml` reference output over the 10,000-tune Zenodo corpus, and
justifies each one against the **ABC 2.1 specification** — the sole authority for
"correct". `abc2xml` is only a comparison baseline; where it departs from the
spec, its output is a *reference artifact* and Croma is correct.

> **Authority order:** ABC 2.1 spec (what the music means) → MusicXML spec (how to
> write it) → `abc2xml` / other parsers (orientation only, never the oracle).
> Spec citations below reference the ABC 2.1 standard text by section and line.

## Headline numbers (current `main`)

Full 10k report-only comparison (`music21` + Polars, positional alignment):

| Quantity | Count | % of 10k |
|---|--:|--:|
| Files attempted | 10,000 | 100% |
| Croma export **failures** | 65 | 0.65% |
| Exported & importable | 9,935 | 99.35% |
| **Structural matches** (identical pitch/duration/structure) | **7,029** | 70.3% |
| **Files with ≥1 differing row** | **2,906** | 29.1% |
| Total differing rows | 187,908 | — |

The 2,906 differing files are explained **in full** by the classes below. After
the parser-quality phases (see `docs/progress/`), **no class contains a
remaining genuine Croma bug** — every residual is an `abc2xml` reference
artifact, a benign serialization difference, malformed input, a positional
**cascade** of one of those, or an intentional Croma feature.

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
