---
name: divergence-triage
description: Triage the raw-comparator worklist (croma-vs-abc2xml structural mismatches) one file at a time. Use when working through a mismatch category or worst-file list to separate real croma bugs from abc2xml/music21/comparator artifacts and equivalences. Dispatches the abc-divergence-investigator subagent for evidence, then decides: fix croma, fix the comparator, or drop the file to dropped.csv with a reasoned subcategory.
---

# Divergence triage

This skill is the Claude Code binding of the runner-neutral process in
[`docs/comparison/abc2xml-divergences/TRIAGE.md`](../../../docs/comparison/abc2xml-divergences/TRIAGE.md)
— the canonical kickoff (state, file-selection guidance, loop, verdict schema). Keep
the two in sync.

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

1. **Investigate — one subagent PER FILE (no exceptions).** Dispatch a fresh
   `abc-divergence-investigator` subagent (Agent tool,
   `subagent_type: "abc-divergence-investigator"`) for **every** candidate file,
   with its `filename` and `category`. Do **not** investigate inline, and do **not**
   bulk-drop a whole category on the `missing_in_croma`/`extra` heuristic — that is
   the deviation a prior run made, and it mislabels *phantom-stuffed-m1* cascades
   (which look croma-suspect but are abc2xml's fault). You may run a batch of
   subagents in parallel, but **every dropped file must have its own verdict**.

   It returns **exactly one delimited verdict block** — parse it mechanically:

   ```
   === VERDICT ===
   file: ...
   category: ...
   at_fault: croma | abc2xml | music21 | comparator | equivalence | undetermined
   confidence: high | medium | low
   subcategory: <drop-class token, or - when at_fault is croma>
   construct: ...
   spec_correct: ...
   spec_cite: ...
   croma: ... / abc2xml: ... / music21: ... / comparator: ...
   reasoning: ...
   === END VERDICT ===
   ```

2. **Decide from `at_fault` + `confidence`:**

   | `at_fault` | `confidence` | Action |
   |---|---|---|
   | **croma** | any | Real croma bug. **Keep** in the comparison. Record a fix candidate: `file`, `construct`, `spec_cite`. Do not drop. |
   | **comparator** | any | The comparator reported false figures — a real bug. Fix the comparator + add a no-happy-path test. **Keep** the file. |
   | **abc2xml** / **music21** / **equivalence** | high or medium | Append to `dropped.csv` using the block's `subcategory`, `spec_cite`, and a one-line `justification`. |
   | any | **low** | **Keep** + flag for a human. Never drop on doubt. |
   | **undetermined** | any | **Keep** + flag for a human. |

3. **Adversarial check before any drop** (cheap insurance against false drops):
   for a file about to be dropped, dispatch a *second* investigator framed to
   **refute** — "find a real croma error in this file." Drop only if it also
   fails to find one. Always do this for files with large row counts or where the
   first verdict was medium confidence.

## Recording a drop

Append one row to `docs/comparison/abc2xml-divergences/dropped.csv`, filling each
column straight from the verdict block:

```
filename,category,subcategory,instrument_at_fault,justification,spec_cite,confidence,investigated_at
```

| Column | From the verdict block |
|---|---|
| `filename` | `file` |
| `category` | `category` |
| `subcategory` | `subcategory` |
| `instrument_at_fault` | `at_fault` |
| `justification` | `reasoning`, condensed to one line |
| `spec_cite` | `spec_cite` |
| `confidence` | `confidence` |
| `investigated_at` | today's date (passed in — do not invent timestamps) |

Quote any field containing a comma. `subcategory` mirrors the divergence-catalog
classes (`abc2xml-phantom-measure`, `abc2xml-drops-music`, `abc2xml-barline-style`,
`abc2xml-multirest`, `music21-reinterpretation`, `equivalence`,
`comparator-false-positive`).

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
