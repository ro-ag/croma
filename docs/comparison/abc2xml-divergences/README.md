# Croma vs abc2xml — divergence justification

This directory documents **every class of difference** between Croma's MusicXML
output and the `abc2xml` reference output over the 10,000-tune Zenodo corpus, and
justifies each one against the **ABC 2.1 specification** — the sole authority for
"correct". `abc2xml` is only a comparison baseline; where it departs from the
spec, its output is a *reference artifact* and Croma is correct.

> **Authority order:** ABC 2.1 spec (what the music means) → MusicXML spec (how to
> write it) → `abc2xml` / other parsers (orientation only, never the oracle).
> Spec citations below reference the ABC 2.1 standard text by section and line.

## Methodology (2026-06 — supersedes the mechanical headline below)

The "Headline numbers" and [`00-SUMMARY.md`](00-SUMMARY.md) /
[`per-file-manifest.csv`](per-file-manifest.csv) were produced by a **mechanical**
prover (`tools/prove_divergences.py`). Its headline — *"0 genuine croma issues"* —
was **refuted** by the phase-33 triage ledger
([doc 12](12-phase33-triage-ledger.md)): real croma bugs hid inside the
POSITIONAL_CASCADE / BARLINE_STYLE classes. A mechanical signature cannot tell
whether croma's output is musically *correct*. Treat those counts as a prior pass,
not the current verdict.

The current methodology is a **raw comparator + evidence-based triage**:

1. **Raw comparator.** The six match-forcing normalizations were stripped from
   `tools/music21_polars_corpus_compare.py`; it now reports raw structural
   differences and forces no matches. Baseline: [`RAW-BASELINE.md`](RAW-BASELINE.md)
   — **8,583 matches / 1,352 mismatches** on the 10k corpus.
2. **Whitelist** — [`whitelist.csv`](whitelist.csv): the raw-match files, and the
   **regression baseline** (a croma change that breaks one is a regression).
3. **Worklist** — the mismatched files, triaged **one file at a time**. An
   **investigator agent** reads the ABC source, runs croma + abc2xml + music21,
   consults the ABC 2.1 spec KB, and returns a **structured verdict** naming which
   of the four fallible instruments — croma, abc2xml, music21, or the comparator
   itself — diverges from the spec-correct output. A **triage process** then decides
   per file: a real croma bug → fix it; an abc2xml / music21 / comparator artifact
   or equivalence → [`dropped.csv`](dropped.csv) with a subcategory. The agent is
   **evidence-only and keep-biased** — on doubt it returns `undetermined`, because
   wrongly dropping a file hides a croma bug.
4. **Drops are explicit and auditable** — every excluded file carries a reason; the
   denominator is never silently shrunk. A file that gets a croma fix **graduates
   into the whitelist**; the whitelist only grows, the worklist only shrinks.

> **Provider/model-agnostic.** The investigator protocol and the verdict schema are
> tool-neutral — bind them to any agent runner or model. The reference (Claude Code)
> binding is `.claude/agents/abc-divergence-investigator.md` (the investigator) and
> `.claude/skills/divergence-triage/` (the triage process); the canonical, runner-
> independent kickoff is [`TRIAGE.md`](TRIAGE.md). The verdict is a fixed
> `=== VERDICT ===` block (`at_fault`, `confidence`, `subcategory`, `spec_cite`, …)
> any orchestrator can parse.

The per-class docs below ([01](01-export-failures.md)–[12](12-phase33-triage-ledger.md))
remain the **subcategory taxonomy** the triage uses.

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
