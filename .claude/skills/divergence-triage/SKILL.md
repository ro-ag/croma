---
name: divergence-triage
description: Triage the raw-comparator worklist (croma-vs-abc2xml structural mismatches) one file at a time. Use when working through a mismatch category or worst-file list to separate real croma bugs from abc2xml/music21/comparator artifacts and equivalences. Dispatches the abc-divergence-investigator subagent for evidence, then decides: fix croma, fix the comparator, or drop the file to dropped.csv with a reasoned subcategory.
---

# Divergence triage

The corpus comparator is now **raw** — it forces no matches. Every mismatched
file (the *worklist*) is either a real croma bug or an artifact/equivalence to
quarantine. This skill works the worklist down, file by file, with evidence.

**Model-cleaning discipline:** investigate each anomaly, separate real defects
from measurement artifacts, quarantine the artifacts with a reasoned record, and
what remains is signal. The error costs are asymmetric — wrongly dropping a file
**permanently hides a croma bug** — so the bar to drop is high and the default on
doubt is **keep**.

## Preconditions

- Raw comparator in place; `docs/comparison/abc2xml-divergences/whitelist.csv`,
  `dropped.csv`, and a raw run (`docs/untracked/raw-baseline/`) exist.
- Pick a worklist slice. Prefer **content categories** (`accidental`, `octave`,
  `pitch`, `duration`, `tie`) — they skew toward real croma faults. Structural
  categories (`missing_in_croma`, `extra_in_croma`, `measure_alignment`) skew
  toward abc2xml artifacts (cascades) — higher row counts, lower croma-bug yield.

## The loop (per file)

1. **Investigate.** Dispatch the `abc-divergence-investigator` subagent (Agent
   tool, `subagent_type: "abc-divergence-investigator"`) with the `filename` and
   the `category`. It returns the structured "real figures" verdict. Do **not**
   investigate inline — use the subagent so the reasoning is isolated and the
   spec-KB context is fresh.

2. **Decide from the verdict's `AT FAULT` + `CONFIDENCE`:**

   | Verdict | Action |
   |---|---|
   | **croma** | Real croma bug. **Keep** the file in the comparison. Record it as a fix candidate (filename, the construct, the spec cite). Do not drop. |
   | **comparator** | The comparator reported false figures. That is a real bug — fix the comparator (distinct from the normalizations we removed). Add a no-happy-path test. |
   | **abc2xml / music21 / none (equivalence)**, confidence high/medium | Append to `dropped.csv` with the subcategory + justification + spec cite. |
   | **undetermined**, or confidence **low** | **Keep** + flag for a human look. Never drop on doubt. |

3. **Adversarial check before any drop** (cheap insurance against false drops):
   for a file about to be dropped, dispatch a *second* investigator framed to
   **refute** — "find a real croma error in this file." Drop only if it also
   fails to find one. Always do this for files with large row counts or where the
   first verdict was medium confidence.

## Recording a drop

Append one row to `docs/comparison/abc2xml-divergences/dropped.csv`:

```
filename,category,subcategory,instrument_at_fault,justification,spec_cite,confidence,investigated_at
```

`subcategory` mirrors the divergence catalog classes (e.g.
`abc2xml-phantom-measure`, `abc2xml-drops-music`, `music21-reinterpretation`,
`equivalence`, `comparator-false-positive`). `justification` is a one-line,
spec-cited reason. Stamp `investigated_at` with today's date (passed in — do not
invent timestamps).

## After a batch — transparency, never silent

Re-run the comparison with the updated `dropped.csv` and report:

```
Investigated: N
  -> croma bug (kept, fix candidate): a   [list filenames + construct]
  -> comparator bug (kept, fix):       b
  -> dropped (by subcategory):         c   [counts per subcategory]
  -> kept / undetermined:              d
Match rate: <structural_matches> / <files_selected> (after excluding <dropped_files> dropped)
```

The denominator is never silently shrunk — the dropped count is always shown with
its reasons. A file that gets a croma fix later **graduates into the whitelist**
when it next matches; the whitelist only grows, the worklist only shrinks.

## Don't

- Don't drop a file the investigator could not clear with confidence.
- Don't re-introduce a comparator normalization to "recover" the match rate —
  fix croma (graduates files in) or drop with a reason.
- Don't let the investigator fix anything; it reports, you decide.
