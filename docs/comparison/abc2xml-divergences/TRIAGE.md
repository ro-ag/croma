# Divergence triage — start here

Runner-independent kickoff for working the croma-vs-abc2xml **worklist** down. Any
agent or model can follow this (Claude Code, Codex, Gemini, a plain script, or a
human). The Claude Code binding is `.claude/skills/divergence-triage/` +
`.claude/agents/abc-divergence-investigator.md`; this file is the canonical,
tool-neutral version of what they do. Background: [`README.md`](README.md).

## Copy-paste start prompt

> Read `docs/comparison/abc2xml-divergences/TRIAGE.md` and triage the croma-vs-abc2xml
> worklist. Do **not** default to the biggest files — select per "How to pick files"
> (the comparator-fix leads first, then one content category, single-category files
> first). For each file: investigate it (four fallible instruments — croma, abc2xml,
> music21, and the comparator itself — reasoning from the ABC 2.1 spec KB), produce a
> `=== VERDICT ===` block, then decide — real croma bug → fix it; abc2xml / music21 /
> comparator artifact or equivalence → append a reasoned row to `dropped.csv`; any
> doubt → keep. After a batch, re-run the comparison and report matches / worklist /
> dropped counts. Never re-add a comparator normalization to recover the match rate.

## State

- The comparator is **raw** — `tools/music21_polars_corpus_compare.py` reports raw
  structural differences and forces no matches. **Never re-add a normalization to
  recover the match rate**; fix croma (files graduate into the whitelist) or drop with
  a reasoned record.
- **`whitelist.csv`** — ~8,583 raw matches; the regression baseline (breaking one is a regression).
- **`dropped.csv`** — adjudicated non-croma-bugs, excluded via `--dropped-csv`.
- **worklist** — ~1,352 mismatched files. This is the work.
- Numbers / reproduce: [`RAW-BASELINE.md`](RAW-BASELINE.md). Spec authority: `docs/reference/abc-spec-kb/`.

## Reproduce the worklist

The worklist is the raw run's mismatches. Regenerate it (corpus recipe:
[`docs/testing/corpus-reproducibility.md`](../../testing/corpus-reproducibility.md)):

```sh
OUT=docs/untracked/raw-baseline; mkdir -p $OUT
uv run python tools/music21_polars_corpus_compare.py \
  --results-jsonl <croma-export-results.jsonl> \
  --croma-xml-root <croma-xml-root> --reference-root <reference-musicxml-root> \
  --report $OUT/report.json --mismatches-jsonl $OUT/mismatches.jsonl \
  --whitelist-csv docs/comparison/abc2xml-divergences/whitelist.csv \
  --dropped-csv docs/comparison/abc2xml-divergences/dropped.csv
```

## How to pick files — do NOT default to the worst

The largest files by mismatch-rows are mostly **abc2xml cascades** (phantom measures,
dropped music) — huge row counts, low croma-bug yield. "Worst first" burns effort on
abc2xml's own bugs. Pick in this order instead:

1. **Comparator-fix leads (do these first).** Validation showed two of the stripped
   normalizations removed *correct* handling — re-add them as principled comparisons
   (compare the real musical value, never force a match), each clearing a whole class:
   - **sounding-pitch** — a display-only courtesy natural (`alter` 0 on both sides) is
     scored as an `accidental` mismatch. Compare the sounding `pitch.alter`, not the
     display accidental name. (A large share of the ~4,160 `accidental` rows.)
   - **full-measure-rest** — music21 collapses an explicit breve rest to a whole note
     when abc2xml tags the *next* rest `measure="yes"` (`fullMeasure`). Compare the
     structural event span so equivalent rests match.
2. **Content categories** — `accidental`, `octave`, `pitch`, `duration`, `tie`,
   `lyric`. These skew toward *real croma faults*. **Avoid leading** with the
   structural categories (`missing_in_croma`, `extra_in_croma`, `measure_alignment`):
   they skew toward abc2xml artifacts (cascades), not croma bugs.
3. **Single-category, fewest-rows files first** — cleanest to adjudicate; build
   confidence before the multi-category cascades.

List candidates for a category (single-category, fewest rows first):

```sh
uv run python - <<'PY'
import polars as pl
mm = pl.read_ndjson("docs/untracked/raw-baseline/mismatches.jsonl", infer_schema_length=None)
per = mm.group_by("filename").agg(
    pl.col("mismatch_category").unique().alias("cats"), pl.len().alias("rows"))
CAT = "accidental"   # <- choose a content category
cand = per.filter(pl.col("cats").list.contains(CAT) & (pl.col("cats").list.len() == 1)).sort("rows")
print(cand.select(["filename", "rows"]).head(20).to_dicts())
PY
```

## Investigate (one file at a time)

Treat **croma, abc2xml, music21, and the comparator** as all fallible; trust none,
reason from the ABC source and the ABC 2.1 spec KB. The full protocol and the runnable
commands live in `.claude/agents/abc-divergence-investigator.md` (its body is
tool-neutral — read it in any runner). Four passes: (1) identify the score; (2) locate
the category, derive the spec-correct output and cite the section; (3) read all four
instruments; (4) adjudicate **keep-biased** — clearing croma needs strong evidence,
any doubt → `undetermined`. Wrongly clearing a file permanently hides a croma bug.

The deliverable is exactly one verdict block:

```
=== VERDICT ===
file: <filename>
category: <category>
at_fault: croma | abc2xml | music21 | comparator | equivalence | undetermined
confidence: high | medium | low
subcategory: <drop-class token, or - when at_fault is croma>
construct: <the ABC token(s)/line(s) at issue>
spec_correct: <what the MusicXML should be>
spec_cite: <ABC 2.1 §x.y — KB line N — quote>
croma: ... / abc2xml: ... / music21: ... / comparator: ...
reasoning: <2–6 sentences, every claim tied to a quoted value or spec line>
=== END VERDICT ===
```

## Decide (the orchestrator)

| `at_fault` | `confidence` | Action |
|---|---|---|
| **croma** | any | Real croma bug. **Keep**; record a fix candidate (`file`, `construct`, `spec_cite`). |
| **comparator** | any | Comparator reported false figures — a real bug. Fix the comparator + add a no-happy-path test. **Keep**. |
| **abc2xml** / **music21** / **equivalence** | high / medium | Append to `dropped.csv` using the block's `subcategory`, `spec_cite`, condensed `reasoning`. |
| any | **low** | **Keep** + flag for a human. Never drop on doubt. |
| **undetermined** | any | **Keep** + flag for a human. |

## Record

Append one `dropped.csv` row per drop, columns filled straight from the verdict block:

```
filename,category,subcategory,instrument_at_fault,justification,spec_cite,confidence,investigated_at
```

`filename←file`, `instrument_at_fault←at_fault`, `justification←reasoning` (one line),
the rest by name; `investigated_at` = today's date. Quote any field with a comma.
Subcategory vocabulary: `abc2xml-phantom-measure`, `abc2xml-drops-music`,
`abc2xml-barline-style`, `abc2xml-multirest`, `abc2xml-dangling-tie`,
`music21-reinterpretation`, `equivalence`, `comparator-false-positive`.

## Transparency

After a batch, re-run the comparison with the updated `dropped.csv` and report:

```
Investigated: N  ->  croma bug: a | comparator bug: b | dropped (by subcategory): c | kept/undetermined: d
Match rate: <structural_matches> / <files_selected>   (after excluding <dropped_files>)
```

The denominator is never silently shrunk — always show the dropped count and its
reasons. A file that later gets a croma fix **graduates into the whitelist**; the
whitelist only grows, the worklist only shrinks.
