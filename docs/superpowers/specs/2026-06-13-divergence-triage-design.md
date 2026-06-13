# Divergence Triage — per-file investigation design

Date: 2026-06-13
Status: approved design, pre-implementation

## Problem

The 10k corpus comparison against abc2xml is a measuring instrument, not the
product. Recent phases improved the *instrument* (comparator normalizations)
rather than croma. Two facts make the raw mismatch numbers misleading:

- **Worst files by mismatch-rows are mostly abc2xml's bugs, not croma's.**
  `tune_003837`'s ~3,000 rows came from abc2xml stranding section-label chords in
  phantom empty measures; croma was already correct. Ranking by row-count aims us
  at abc2xml artifacts.
- **The existing mechanical prover gives false negatives.**
  `tools/prove_divergences.py` once headlined *"0 genuine croma bugs"*; the
  phase-33 triage ledger ([doc 12](../../comparison/abc2xml-divergences/12-phase33-triage-ledger.md))
  refuted it — real croma bugs hid inside the POSITIONAL_CASCADE / BARLINE_STYLE
  classes (dropped boundary attachments, never-closed voltas, wedge-as-words).
  Fixing the first nine moved matches 8,118 → 8,734.

We need per-file **AI reasoning** to separate real croma bugs from artifacts,
quarantine the artifacts out of the comparison, and converge on clean software.

## Principle

Model-cleaning discipline: investigate each anomaly with a neutral fact-finder,
separate real defects from measurement artifacts, quarantine the artifacts, and
what remains is signal.

There are **four fallible instruments** — croma, abc2xml, music21, and the
comparison tool itself. The investigator trusts none of them and reasons from the
ABC source plus the ABC 2.1 specification.

## Corpus states

After the comparator is stripped to raw, every corpus file is in exactly one state:

- **Whitelist** (`docs/comparison/abc2xml-divergences/whitelist.csv`) — files whose
  raw music21 structure matches abc2xml with **no normalization**. The strongest
  "croma is correct here" signal; needs no investigation — the raw match *is* the
  verdict. Doubles as the **regression baseline**: a future croma change that
  breaks a whitelisted file is a regression, caught immediately.
- **Dropped** (`docs/comparison/abc2xml-divergences/dropped.csv`) — files
  investigated and found to be non-croma-bugs (abc2xml / music21 artifact, or
  equivalence), excluded from the comparison with a reasoned, subcategory-tagged
  justification.
- **Worklist** — everything else: raw mismatches not yet adjudicated. The only set
  the investigator touches.

Lifecycle: a file starts in the **worklist** → investigated → either a **croma
bug** (fix croma → graduates to the **whitelist**) or **not croma** (→
**dropped.csv**). The whitelist only grows; the worklist only shrinks.

Caveat: a raw match is strong but not absolute evidence — croma and abc2xml could
share the same wrong reading (rare). Acceptable for the baseline; spot-audit a
sample.

## Sequence

0. **Strip to raw.** Remove all six match-forcing normalizations from the
   comparator: phantom-measure drop (incl. the uncommitted phase-48 work),
   barline visual-style, full-measure rest, tuplet bracket, playback-only tempo,
   and alter-0 / sounding-pitch. Keep all infrastructure (cache, columnar
   pipeline, schema v3, harness, parallelization). The match rate drops toward the
   raw ~8,700 — that is the honest baseline.
1. **Run raw → emit artifacts.** `whitelist.csv` (raw matches) and the raw
   worklist (mismatches with categories). The whitelist is the first tangible
   output, produced the moment the comparator is clean — no investigation needed.
2. **Triage the worklist** file-by-file with the investigator → fix-croma
   (graduate) or drop (`dropped.csv`).

Equivalence classes previously normalized in the comparator (e.g. the ~577
redundant-natural / alter-0 files) are adjudicated **once** as a batch in step 2
and graduate to the whitelist. The judgment moves out of the tool and into the
manifests — explicit and auditable.

## Components

### 1. Investigator subagent — `.claude/agents/abc-divergence-investigator.md`

A neutral fact-finder. Produces **evidence only** — it does not fix anything and
does not make the keep/drop decision.

- **Equipped with** the ABC 2.1 spec KB (`docs/reference/abc-spec-kb/`,
  authoritative source `raw/abc-2.1.dokuwiki.txt`, index
  `generated/section-index.md`). Cite the 2.1 section first, per the KB working
  rule.
- **Input:** one file + the mismatch category under investigation.
- **Tools:** `croma xml <file>`, `abc2xml.py <file>`, music21 via
  `tools/music21_compare.py`, plus reading the raw `.abc` and the spec KB.
- **Multi-pass protocol** (when in doubt, widen evidence and re-pass; never guess):
  1. **Identify the score** — genre, meter(s), key(s), voice count, macro-structure;
     inventory annotations (chords / text / lyrics / graces / ornaments / inline
     fields).
  2. **Locate the category** — find the exact ABC constructs that produce this
     category's rows; derive the spec-correct output, with a section citation.
  3. **Read all instruments** — croma's MusicXML, abc2xml's MusicXML, music21's
     reading of each, and what the comparator *reported* — at those points.
  4. **Adjudicate, keep-biased** — which instrument diverges from the
     spec-correct output: croma / abc2xml / music21 / comparator / none
     (real equivalence)? On any doubt → re-pass, then flag low confidence.

**Output (structured "real figures"):**

```
file, category
score_summary: { genre, meters, keys, voice_count, structure, annotations_present }
construct:            the ABC token(s)/line(s) at issue
spec_correct_output:  what the MusicXML should be, per ABC 2.1
spec_citation:        section + KB line reference
instruments: {
  croma:        what croma produced here
  abc2xml:      what abc2xml produced here
  music21:      how music21 read each side
  comparator:   what the comparison tool reported as the mismatch
}
at_fault:   croma | abc2xml | music21 | comparator | none_equivalence | undetermined
confidence: high | medium | low
reasoning:  prose, evidence-cited
recommended_subcategory:  if at_fault != croma
```

### 2. Triage skill — `.claude/skills/divergence-triage/SKILL.md`

The main-agent recipe. Given a category worklist, for each file it dispatches the
investigator, reads the evidence, and the **main agent decides**:

| Investigator verdict | Main-agent action |
|---|---|
| **croma wrong** | real croma bug → keep, queue for a fix |
| **comparator wrong** | comparator bug → fix it *(distinct from the frozen "no new normalizations": this fixes false figures, a real bug)* |
| **croma correct; abc2xml / music21 / malformed input** | move to drop manifest under its subcategory → next file |
| **real equivalence** | drop (equivalence subcategory); a comparator normalization only if cheap and cascade-clean |
| **low / undetermined confidence** | keep + flag for a human look — never silently drop |

The investigator step also audits the comparator: when its fresh figures disagree
with what the comparator reported, that is itself a comparator-bug signal
(false-positive = a missed equivalence; false-negative = over-normalization, e.g.
the phantom over-drop class).

## Drop manifest

`docs/comparison/abc2xml-divergences/dropped.csv` (committed), columns:

```
filename, category, subcategory, instrument_at_fault, justification, spec_cite,
confidence, investigated_at
```

A committed manifest (not physically moving files) because the corpus is
regenerated from Zenodo by the bootstrap; moved files would not survive a rebuild,
a manifest does. Subcategories mirror the existing divergence catalog classes
(phantom-measure, drops-music, reinterpretation, equivalence, barline-style, …).

## Comparison integration

The corpus comparison reads `dropped.csv` and excludes listed files. Reporting is
always transparent: **"N files dropped (by subcategory), match-rate on the
remaining M"** — the denominator is never silently shrunk.

## Reuses (don't rebuild)

- ABC 2.1 spec KB — `docs/reference/abc-spec-kb/`.
- Divergence catalog taxonomy and the `per-file-manifest.csv` precedent —
  `docs/comparison/abc2xml-divergences/`.
- Single-file tooling — `croma xml`, `abc2xml.py`, `tools/music21_compare.py`.

## Out of scope (YAGNI)

- The investigator never fixes; it only reports.
- v1 is single-file, main-agent-driven, sequential triage. Parallel fan-out across
  many agents is a later option requiring explicit opt-in (token cost).
- No auto-quarantine on a mechanical signature alone — a reasoned verdict is
  required for every drop.

## Resolved decisions

- **phase-48 and phase-47 normalizations are reverted** as part of Sequence step 0,
  along with the other four. Their phantom-detection insight survives only as a
  triage subcategory, not as comparator code.
- **Strip all six** (including alter-0). No "honest equality" stays in the tool;
  equivalences are adjudicated once in the worklist and graduate to the whitelist.
- **Equivalence handling:** equivalent files are **dropped** (equivalence
  subcategory), not re-normalized in the comparator.
