# How many .abc files have a genuine Croma issue?

**Short answer: 0 of 10,000.**

Every one of the 10,000 corpus tunes is given a per-file verdict by
[`tools/prove_divergences.py`](../../../tools/prove_divergences.py); the full
result is the auditable manifest [`per-file-manifest.csv`](per-file-manifest.csv)
(one row per differing file: filename, verdict, categories, measure counts).

## The numbers (current `main`)

| Outcome | Files | Meaning |
|---|--:|---|
| **MATCH** | **7,031** | Identical to the reference (pitch/duration/structure). |
| **Differ** | **2,969** | At least one differing row — every one classified below. |

### Of the 2,969 differing files

| Verdict | Files | Croma correct? | Doc |
|---|--:|:--:|---|
| CASCADE | 1,949 | ✅ positional cascade of a structural artifact | [07](07-cascade-artifacts.md) |
| ARTIFACT_ACCIDENTAL_ALTER | 577 | ✅ redundant `<alter>0>` — semantically identical | [05](05-accidental-alter.md) |
| ARTIFACT_PHANTOM_MEASURE | 240 | ✅ abc2xml empty measure at annotation/section/`\|>\|` | [02](02-phantom-measures.md) |
| ARTIFACT_BARLINE | 69 | ✅ spaced `\| \|` / line-split / repeat bar-style | [04](04-barline-spaced-double.md) |
| EXPORT_FAILURE_NO_MUSIC | 65 | ✅ header-only tune — nothing to export | [01](01-export-failures.md) |
| DIRECTION | 24 | ✅ tempo/annotation text edge | [10](10-directions.md) |
| TIE_SLUR_EDGE | 16 | ✅ dropped-legal / malformed / endpoint | [09](09-ties-and-slurs.md) |
| ARTIFACT_DURATION | 10 | ✅ §4.6 default length / abc2xml rounding | [06](06-duration.md) |
| ARTIFACT_TUPLET | 7 | ✅ bracket-marker placement | [08](08-tuplet.md) |
| ARTIFACT_MULTIREST | 6 | ✅ `Z`/`X` expansion (spec: "equivalent") | [03](03-multi-measure-rest.md) |
| ARTIFACT_ABC2XML_DROPS_MUSIC | 4 | ✅ abc2xml dropped a line / parse-failed; Croma kept it | [02](02-phantom-measures.md) |
| ARTIFACT_ABC2XML_DROPS_TACET | 2 | ✅ abc2xml dropped multi-voice tacet bars; Croma kept them | [11](11-multipart-and-partgroup.md) |

All **2,969 differing files** are Croma-correct (abc2xml artifact, benign
serialization, positional cascade, or Croma feature). There are **0 known genuine
Croma issues** in this manifest.

## Note on direction of correctness

Several verdicts are cases where Croma **diverges from abc2xml because Croma is
the correct one** (the spec, not abc2xml, is the authority):

- the 240 `ARTIFACT_PHANTOM_MEASURE` include 4 `|>|` tunes where abc2xml keeps a
  trailing empty measure after a void broken-rhythm `>`; Croma correctly drops it
  (`|>|` ≡ `| |`, §4.4 + §4.8);
- `ARTIFACT_ABC2XML_DROPS_MUSIC` / `DROPS_TACET` are tunes where abc2xml dropped
  real music or tacet bars and Croma preserved them.

## Reproduce

```sh
uv run python tools/prove_divergences.py \
  --phase-dir docs/untracked/<phase> \
  --abc-root  docs/untracked/corpus/zenodo-10k/abc \
  --ref-root  docs/untracked/corpus/zenodo-10k/musicxml \
  --out docs/comparison/abc2xml-divergences/per-file-manifest.csv
```

Prints the verdict breakdown and writes the per-file manifest. `REVIEW` (an
unclassified file) must be 0 — any non-zero count is a file needing a human look.
