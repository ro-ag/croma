# Divergence triage — start here

Runner-independent kickoff for working the croma-vs-abc2xml **worklist** down. Any
agent or model can follow this (Claude Code, Codex, Gemini, a plain script, or a
human). The Claude Code binding is `.claude/skills/divergence-triage/` +
`.claude/agents/abc-divergence-investigator.md`; this file is the canonical,
tool-neutral version of what they do. Background: [`README.md`](README.md).

## Copy-paste start prompt

> Read `docs/comparison/abc2xml-divergences/TRIAGE.md` and triage the croma-vs-abc2xml
> worklist. **Investigate every file with its own fresh investigator-subagent run —
> never reason inline, never bulk-drop a whole category on a heuristic.** Do **not**
> default to the biggest files (those are abc2xml cascades) — select per "How to pick
> files". For each file the subagent treats croma, abc2xml, music21, and the comparator
> as all fallible (reasoning from the ABC 2.1 spec KB) and returns one `=== VERDICT ===`
> block; then you decide — real croma bug → fix it; abc2xml / music21 / comparator
> artifact or equivalence → append a reasoned `dropped.csv` row; any doubt → keep. After
> a batch, **re-export croma** (`tools/corpus_harness.py --mode xml`, ~6 s) if you fixed
> the parser, re-run the comparison, and report matches / worklist / dropped counts.
> Never re-add a comparator normalization to recover the match rate.

## State

- The comparator is **raw** — `tools/music21_polars_corpus_compare.py` reports raw
  structural differences and forces no matches. **Never re-add a normalization to
  recover the match rate**; fix croma (files graduate into the whitelist) or drop with
  a reasoned record.
- **`whitelist.csv`** — ~9,259 raw matches (8,583 at the original baseline; grew as
  croma fixes landed); the regression baseline (breaking one is a regression).
- **`dropped.csv`** — adjudicated non-croma-bugs (76 so far), excluded via `--dropped-csv`.
- **worklist** — ~600 mismatched files (down from 1,352). This is the remaining work.
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

1. **Remaining content categories** — `octave`, `pitch`, `duration`, `lyric`, `voice`
   (`accidental` and `tie` are already triaged). These still skew toward *real croma
   faults*, the highest-value work.
2. **Structural cascades** — `missing_in_croma`, `extra_in_croma`, `measure_alignment`.
   ~245–306 remain, overwhelmingly `abc2xml-phantom-measure`. They are the bulk of the
   worklist but skew toward abc2xml artifacts, so do them **last** and **never
   bulk-drop**: the *phantom-stuffed-m1* trap looks like croma dropping music but is
   abc2xml cramming a corrupt measure 1 (signature `missing_in_croma >> extra`).
   Ground-truth check the per-file subagent must do: croma's note count should equal
   the ABC source note-letter count. Each cascade still needs **its own verdict**.
3. **Single-category, fewest-rows files first** — cleanest; build confidence first.

> **Resolved — do not re-pick.** The two comparator-fix leads — *sounding-pitch* (now a
> comparator fix) and *full-measure-rest* (now `music21-reinterpretation` drops) — plus
> three croma parser bugs (`^/c`, `K:exp` list, post-barline tie) landed in **PR #86**.
> Open: the cascades above, and two deferred *policy* calls in
> [`croma-fix-candidates.md`](croma-fix-candidates.md) (empty-bar collapse;
> whitespace-surrounded `:`) — these need an explicit human decision, not a drive-by fix.

List candidates for a category (single-category, fewest rows first):

```sh
uv run python - <<'PY'
import polars as pl
mm = pl.read_ndjson("docs/untracked/raw-baseline/mismatches.jsonl", infer_schema_length=None)
per = mm.group_by("filename").agg(
    pl.col("mismatch_category").unique().alias("cats"), pl.len().alias("rows"))
CAT = "octave"   # <- a remaining content category (accidental/tie are done)
cand = per.filter(pl.col("cats").list.contains(CAT) & (pl.col("cats").list.len() == 1)).sort("rows")
print(cand.select(["filename", "rows"]).head(20).to_dicts())
PY
```

## Investigate — ONE investigator subagent PER FILE (no exceptions)

> **This is the rule the last run broke.** Dispatch a **fresh investigator subagent for
> every candidate file** and let its verdict drive the decision. Do **not** reason
> inline, and do **not** bulk-drop a whole category on the `missing_in_croma`/`extra`
> heuristic. Per-file investigation is the safeguard: it is the only thing that catches
> *phantom-stuffed-m1* cascades (which look croma-suspect but are abc2xml's fault) and a
> real croma bug hiding inside an otherwise-artifact cascade. You may run a batch of
> subagents in parallel, but **every dropped file must have its own verdict** — never a
> shared, heuristic one.

### Human-approved bulk exception: forced-4/4 bar-duration artifact

If the user explicitly approves bulk handling, it is acceptable to bulk-drop a
**narrow same-signature** set when per-file subagents would only burn tokens. The
current approved case is the pure `measure_alignment` / `measure.bar_duration`
signature where abc2xml inserts an unsourced first-measure `4/4` and croma either
emits free meter/no time signature or the source meter from a body `M:` field.

Bulk handling is still fail-closed. Before appending any `dropped.csv` row,
verify each original ABC file directly with a source regex/token scan and XML
checks:

- the file has only `measure_alignment` rows, and every row is
  `component=measure`, `field_name=bar_duration`;
- every reference value is `4.0`, and every croma value differs from `4.0`;
- abc2xml MusicXML measure 1 starts with `<time>4/4</time>`;
- croma MusicXML measure 1 does **not** start with `<time>4/4</time>`;
- the regex/token scan of the original ABC source's sounding note letters equals
  croma's pitched-note count and abc2xml's pitched-note count.

If any check fails, keep the file in the worklist and send it through the normal
one-investigator-per-file path. This exception does **not** apply to
`missing_in_croma`/`extra_in_croma` cascades, measure-numbering rows, or any file
with pitch/duration/voice/lyric content rows.

The subagent treats **croma, abc2xml, music21, and the comparator** as all fallible;
trusts none; reasons from the ABC source and the ABC 2.1 spec KB. Its full protocol and
runnable commands live in `.claude/agents/abc-divergence-investigator.md` (the body is
tool-neutral — read it in any runner). Four passes: (1) identify the score; (2) locate
the category, derive the spec-correct output and cite the section; (3) read all four
instruments; (4) adjudicate **keep-biased** — clearing croma needs strong evidence, any
doubt → `undetermined`. Wrongly clearing a file permanently hides a croma bug.

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
