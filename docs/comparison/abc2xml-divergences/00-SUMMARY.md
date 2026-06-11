# Croma vs abc2xml — per-file verdict summary

> **Superseded in part (2026-06-11):** the phase-33 forensic triage
> ([12-phase33-triage-ledger.md](12-phase33-triage-ledger.md)) re-verified
> every category per cause and REFUTED this document's headline — several
> genuine croma bugs hid inside the POSITIONAL_CASCADE / BARLINE_STYLE
> classes (dropped boundary attachments, never-closed volta brackets, wedge
> decorations as words, …). The phase-33a batch fixed the first nine
> (matches 8,118 → 8,734, 0 regressions); the ledger tracks the rest. Where
> this file and the ledger disagree, the ledger wins.

**Genuine Croma issues across the 10,000-tune corpus: 0.** *(historical
claim — see banner above)*

Every tune is given a forensic, per-file verdict by
[`tools/prove_divergences.py`](../../../tools/prove_divergences.py). The decisive
signal is the **actual note sequence**: the prover extracts Croma's ordered pitches
(step + alter + octave) and the reference's, and compares them. The full result is
the auditable manifest [`per-file-manifest.csv`](per-file-manifest.csv), one row per
differing file with columns:

`filename, verdict, music_identical, croma_correct, mismatch_rows, categories,
croma_measures, ref_measures, measure_delta, justification`

`justification` is a plain-language, ABC-2.1-cited reason for that file's
divergence — so each row stands on its own.

## Headline

| | Files | % |
|---|--:|--:|
| Note content **identical** to abc2xml (7,031 exact + 2,707 pitch-identical) | **9,738** | **97.4%** |
| Differ in some non-note way / Croma more correct | 262 | 2.6% |
| **Genuine Croma issues** | **0** | 0% |
| Unclassified (`REVIEW`) | 0 | 0% |

`croma_correct` over all 2,969 differing files: **2,929 `yes`**, **40 `defensible`**,
0 genuine issues, 0 review.

## Verdict breakdown (the 2,969 differing files)

| Verdict | Files | `music_identical` | Croma correct? |
|---|--:|:--:|:--:|
| `POSITIONAL_CASCADE` — pitches identical, comparison alignment shifted | 1,447 | yes | ✅ |
| `ALTER_SERIALIZATION` — redundant `<alter>0>` on a carried natural | 577 | mostly | ✅ |
| `BARLINE_STYLE` — same notes/measures, only a bar-line glyph differs | 389 | yes | ✅ |
| `PHANTOM_MEASURE` — abc2xml inserts an empty measure Croma omits | 240 | mostly | ✅ |
| `DURATION_EXACT_VS_ROUNDED` — abc2xml hardcodes L:1/8 / rounds; Croma exact | 104 | mostly | ✅ |
| `CASCADE` — positional cascade of a structural artifact | 88 | — | ✅ |
| `EXPORT_FAILURE_NO_MUSIC` — header-only tune, nothing to export | 65 | n/a | ✅ |
| `DIRECTION_TEXT` — tempo/annotation text edge | 24 | yes | 🟡 defensible |
| `TIE_SLUR_EDGE` — abc2xml drops a legal tie / malformed input | 16 | yes | 🟡 defensible |
| `TUPLET_BRACKET` — same ratio, bracket marker placement | 7 | yes | ✅ |
| `MULTIREST_EXPANSION` — abc2xml expands `Z`/`X`; spec calls them equivalent | 6 | yes | ✅ |
| `ABC2XML_DROPS_MUSIC` — abc2xml dropped a line; Croma kept the music | 4 | no | ✅ |
| `ABC2XML_DROPS_TACET` — abc2xml dropped multi-voice tacet bars | 2 | yes | ✅ |

In **every** verdict Croma either matches abc2xml exactly, is the *more*
spec-correct of the two, or differs only in a presentation glyph the spec does not
mandate. See [`MIGRATION.md`](MIGRATION.md) for the case this makes, and docs
[01–11](.) for the spec citation behind each class.

## The two formerly-residual tunes

`tune_014316` / `tune_014317` (the only genuine Croma issue ever surfaced by this
prover — a phantom measure from a `]||:` bar-line run) were **fixed**
([PR #59](https://github.com/ro-ag/croma/pull/59)); they now match the reference
measure count (22) and reduce to a small `BARLINE_STYLE` residual.

## Reproduce

```sh
uv run python tools/prove_divergences.py \
  --phase-dir docs/untracked/<phase> \
  --abc-root  docs/untracked/corpus/zenodo-10k/abc \
  --ref-root  docs/untracked/corpus/zenodo-10k/musicxml \
  --out docs/comparison/abc2xml-divergences/per-file-manifest.csv
```

Prints the breakdown and the `GENUINE Croma issues` / `REVIEW` counts (both must be
0) and writes the manifest. Any file can be re-checked against
`target/debug/croma xml <file>` and its reference XML.
