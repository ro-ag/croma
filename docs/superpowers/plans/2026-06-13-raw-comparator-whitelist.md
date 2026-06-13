# Raw Comparator + Whitelist Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Strip the six match-forcing normalizations from the corpus comparator, leaving a raw comparator that emits a `whitelist.csv` (raw matches), a worklist (raw mismatches), and excludes files listed in `dropped.csv`.

**Architecture:** Restore the two comparator source files to the last pre-normalization commit (`2976ca9`, schema-v3 = raw extraction + all infrastructure), re-apply the one non-normalization change that came after it (lyric `syllabic` enrichment), and delete the five orphaned normalization tests. Then add a `dropped.csv` exclusion at `select_results` and a `whitelist.csv` emission derived from the per-file summary. Reverting whole commits is rejected — they also carried docs/progress changes we keep.

**Tech Stack:** Python (`uv`), pytest, music21, polars, git.

---

## File Structure

- `tools/music21_compare.py` — fact extraction. Restored to `2976ca9`; loses phantom drop, full-measure-rest, tuplet-bracket, alter-0 sounding logic. Re-gains the lyric `syllabic` field.
- `tools/music21_polars_corpus_compare.py` — comparison driver. Restored to `2976ca9`; loses phantom-drop detection, playback-tempo and barline-style normalization, alter-0. Re-gains lyric `syllabic`. Gains `--dropped-csv` exclusion and `--whitelist-csv` emission.
- `tests/test_music21_polars_corpus_compare.py` — delete the 5 normalization tests; add exclusion + whitelist tests.
- `docs/comparison/abc2xml-divergences/dropped.csv` — new, header only.
- `docs/comparison/abc2xml-divergences/whitelist.csv` — generated; committed as the regression baseline.

---

## Task 1: Branch and clean slate

**Files:** working tree, git.

- [ ] **Step 1: Discard the uncommitted phase-48 changes**

```bash
git restore tools/music21_compare.py tools/music21_polars_corpus_compare.py tests/test_music21_polars_corpus_compare.py
```
Expected: working tree now has only the two untracked docs (spec + this plan); comparator files at merged `main`/phase-47 state.

- [ ] **Step 2: Drop the shelved harmony-offset stash**

```bash
git stash list   # confirm the top entry is "phase-48 harmony-offset-attachment (shelved...)"
git stash drop   # drop that entry
```

- [ ] **Step 3: Create the work branch**

```bash
git checkout -b codex/raw-comparator-whitelist
```

- [ ] **Step 4: Commit the design + plan docs**

```bash
git add docs/superpowers/specs/2026-06-13-divergence-triage-design.md \
        docs/superpowers/plans/2026-06-13-raw-comparator-whitelist.md
git commit -m "docs: divergence-triage design + raw-comparator plan"
```

- [ ] **Step 5: Record the pre-strip baseline**

```bash
uv run pytest tests/test_music21_polars_corpus_compare.py -q
```
Expected: all pass (phase-47 normalizations still present). Note the count for comparison.

---

## Task 2: Strip the comparator source to raw

**Files:**
- Modify: `tools/music21_compare.py`
- Modify: `tools/music21_polars_corpus_compare.py`

- [ ] **Step 1: Restore both comparator files to the pre-normalization anchor**

```bash
git checkout 2976ca9 -- tools/music21_compare.py tools/music21_polars_corpus_compare.py
```
This is schema-v3: raw fact extraction + every infra commit (cache, columnar pipeline, orjson, parallelization), and **none** of the six normalizations.

- [ ] **Step 2: Re-apply the one non-normalization change made after the anchor (lyric `syllabic`, from `554c824`)**

In `tools/music21_compare.py`, replace `lyric_facts`:

```python
def lyric_facts(element: Any) -> list[dict[str, str]]:
    return [
        {
            "text": lyric.text if lyric.text is not None else "",
            "syllabic": optional_string(getattr(lyric, "syllabic", None)),
        }
        for lyric in getattr(element, "lyrics", [])
    ]
```

In `tools/music21_polars_corpus_compare.py`, in `add_event_rows`, replace the lyric loop:

```python
    for lyric_index, lyric in enumerate(event.get("lyrics", [])):
        lyric_base = {**event_base, "alignment_index": lyric_index}
        if isinstance(lyric, dict):
            lyric_text = lyric.get("text", "")
            lyric_syllabic = lyric.get("syllabic")
        else:
            lyric_text = lyric
            lyric_syllabic = None
        builder.add("lyric", "text", lyric_text, **lyric_base)
        builder.add("lyric", "syllabic", lyric_syllabic, **lyric_base)
```

- [ ] **Step 3: Verify the strip removed only normalizations**

```bash
git diff --stat HEAD -- tools/music21_compare.py tools/music21_polars_corpus_compare.py
grep -nE "phantom|reference_only|playback_only|drop_measure_indices|rest_offset_span|duration_type_from_quarter_length|barline_style|sounding" tools/music21_compare.py tools/music21_polars_corpus_compare.py
```
Expected: the grep finds **nothing** (all six normalizations gone). If anything remains, the anchor was wrong — stop and re-investigate.

- [ ] **Step 4: Do NOT run the suite yet** — the test file still asserts the normalizations and will fail. That is fixed in Task 3.

---

## Task 3: Delete the orphaned normalization tests

**Files:**
- Modify: `tests/test_music21_polars_corpus_compare.py`

- [ ] **Step 1: Delete these six test functions** (each asserts a normalized equivalence is a match — no longer true):

- `test_redundant_alter_zero_is_not_a_mismatch`
- `test_text_only_tempo_playback_bpm_difference_is_equivalent`
- `test_tuplet_bracket_marker_difference_is_equivalent`
- `test_full_measure_rest_reinterpretation_uses_event_offset_span`
- `test_visual_only_barline_style_difference_is_equivalent`
- `test_reference_empty_leading_measure_is_normalized_with_harmony`  *(phase-47 phantom)*

**Keep** `test_sounding_alteration_difference_is_still_flagged` and `test_repeat_barline_direction_difference_is_still_flagged` — they assert a *real* difference is flagged, which is correct raw behavior. (The phase-48 phantom-interior and paired-empty tests were already removed with the phase-48 discard in Task 1; this list is what remains in the phase-47-state file.)

- [ ] **Step 2: Remove now-unused fixture helpers** if any test deletion orphaned them (e.g. a `tuplet_note`/`rest`/barline helper used only by a deleted test). Run a grep for each helper name; delete only if zero remaining references.

- [ ] **Step 3: Run the suite — must be green**

```bash
uv run pytest tests/test_music21_polars_corpus_compare.py -q
```
Expected: PASS. The remaining tests cover infra (encode/columnar/typed-values), lyrics, and the two "still flagged" cases.

- [ ] **Step 4: Commit**

```bash
git add tools/music21_compare.py tools/music21_polars_corpus_compare.py tests/test_music21_polars_corpus_compare.py
git commit -m "strip: remove six match-forcing comparator normalizations (raw baseline)"
```

---

## Task 4: dropped.csv exclusion

**Files:**
- Create: `docs/comparison/abc2xml-divergences/dropped.csv`
- Modify: `tools/music21_polars_corpus_compare.py` (`parse_args` ~line 536, `select_results` ~line 694, summary report ~line 457)
- Test: `tests/test_music21_polars_corpus_compare.py`

- [ ] **Step 1: Create the empty manifest**

```bash
printf 'filename,category,subcategory,instrument_at_fault,justification,spec_cite,confidence,investigated_at\n' \
  > docs/comparison/abc2xml-divergences/dropped.csv
```

- [ ] **Step 2: Write the failing test**

```python
def test_dropped_csv_files_are_excluded_and_counted(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["keep_me", "drop_me"])
    write_musicxml(paths.croma_xml("keep_me"), [note(step="C")])
    write_musicxml(paths.reference_xml("keep_me"), [note(step="C")])
    write_musicxml(paths.croma_xml("drop_me"), [note(step="C")])
    write_musicxml(paths.reference_xml("drop_me"), [note(step="D")])
    dropped = paths.output / "dropped.csv"
    dropped.write_text("filename,subcategory\ndrop_me.abc,equivalence\n", encoding="utf-8")

    report = run_compare(paths, jobs=1, output_name="excl",
                         extra=["--dropped-csv", str(dropped)])

    assert report["dropped_files"] == 1
    assert report["files_selected"] == 1   # only keep_me compared
    assert report["structural_mismatches"] == 0
```

- [ ] **Step 3: Run it — expect failure**

```bash
uv run pytest tests/test_music21_polars_corpus_compare.py::test_dropped_csv_files_are_excluded_and_counted -v
```
Expected: FAIL (unrecognized `--dropped-csv`).

- [ ] **Step 4: Implement**

Add to `parse_args`: `parser.add_argument("--dropped-csv", type=Path, default=None)`.

Add a loader near `load_results`:

```python
def load_dropped_filenames(path: Path | None) -> set[str]:
    if path is None or not path.exists():
        return set()
    import csv
    with path.open(encoding="utf-8") as handle:
        return {
            row["filename"].strip()
            for row in csv.DictReader(handle)
            if row.get("filename", "").strip()
        }
```

In `select_results`, after the existing selection, drop excluded files and stash the count on `args`:

```python
    dropped = load_dropped_filenames(getattr(args, "dropped_csv", None))
    if dropped:
        before = len(results)
        results = [r for r in results if Path(relative_path_for(r) or "").name not in dropped]
        args._dropped_files = before - len(results)
    else:
        args._dropped_files = 0
```

In the summary dict (~line 457), add: `"dropped_files": getattr(args, "_dropped_files", 0),`.

- [ ] **Step 5: Run the test — expect pass**

```bash
uv run pytest tests/test_music21_polars_corpus_compare.py::test_dropped_csv_files_are_excluded_and_counted -v
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add tools/music21_polars_corpus_compare.py tests/test_music21_polars_corpus_compare.py docs/comparison/abc2xml-divergences/dropped.csv
git commit -m "feat(compare): exclude dropped.csv files from the comparison, report count"
```

---

## Task 5: whitelist.csv emission

**Files:**
- Modify: `tools/music21_polars_corpus_compare.py` (`parse_args`; summary/report assembly ~line 398–470)
- Test: `tests/test_music21_polars_corpus_compare.py`

The per-file summary already records each file's mismatch count. The whitelist is every selected file whose mismatch count is 0.

- [ ] **Step 1: Write the failing test**

```python
def test_whitelist_csv_lists_only_raw_matches(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["good", "bad"])
    write_musicxml(paths.croma_xml("good"), [note(step="C")])
    write_musicxml(paths.reference_xml("good"), [note(step="C")])
    write_musicxml(paths.croma_xml("bad"), [note(step="C")])
    write_musicxml(paths.reference_xml("bad"), [note(step="D")])
    whitelist = paths.output / "whitelist.csv"

    run_compare(paths, jobs=1, output_name="wl", extra=["--whitelist-csv", str(whitelist)])

    names = {row["filename"] for row in read_csv(whitelist)}
    assert "good.abc" in names
    assert "bad.abc" not in names
```

(Add a small `read_csv` helper beside `read_jsonl` if absent.)

- [ ] **Step 2: Run it — expect failure**

```bash
uv run pytest tests/test_music21_polars_corpus_compare.py::test_whitelist_csv_lists_only_raw_matches -v
```
Expected: FAIL (unrecognized `--whitelist-csv`).

- [ ] **Step 3: Implement**

Add to `parse_args`: `parser.add_argument("--whitelist-csv", type=Path, default=None)`.

Read `PER_FILE_SUMMARY_COLUMNS` to confirm the per-file fields (`relative_path`/`filename` and the mismatch-count column). After the per-file summary is finalized in `main`, write the whitelist:

```python
def write_whitelist(path: Path | None, per_file_summary_rows: list[dict[str, Any]]) -> int:
    if path is None:
        return 0
    import csv
    matches = [r for r in per_file_summary_rows if int(r.get("structural_mismatches", 0)) == 0]
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(["filename"])
        for row in matches:
            writer.writerow([Path(row["relative_path"]).name])
    return len(matches)
```

Wire it where the per-file summary rows are available; add `"whitelist_files": <count>` to the summary. (If per-file rows aren't retained in memory, read them back from `per_file_summary_jsonl` with `read_jsonl`.)

- [ ] **Step 4: Run the test — expect pass**

```bash
uv run pytest tests/test_music21_polars_corpus_compare.py::test_whitelist_csv_lists_only_raw_matches -v
```
Expected: PASS.

- [ ] **Step 5: Full suite green**

```bash
uv run pytest tests/test_music21_polars_corpus_compare.py -q && cargo test --workspace 2>&1 | grep -E "test result.*failed" | grep -v " 0 failed" || echo "all green"
```

- [ ] **Step 6: Commit**

```bash
git add tools/music21_polars_corpus_compare.py tests/test_music21_polars_corpus_compare.py
git commit -m "feat(compare): emit whitelist.csv of raw matches"
```

---

## Task 6: Run the raw 10k and capture the baseline

**Files:**
- Create: `docs/comparison/abc2xml-divergences/whitelist.csv` (committed baseline)
- Create: `docs/comparison/abc2xml-divergences/RAW-BASELINE.md` (numbers + provenance)

- [ ] **Step 1: Run the raw comparison over the corpus**

```bash
OUT=docs/untracked/raw-baseline; mkdir -p $OUT
uv run python tools/music21_polars_corpus_compare.py \
  --results-jsonl docs/untracked/phase-42-residual-burndown/full-10k-final-results.jsonl \
  --croma-xml-root docs/untracked/phase-42-residual-burndown/full-10k-final-xml \
  --reference-root docs/untracked/corpus/zenodo-10k/musicxml \
  --report $OUT/report.json \
  --per-file-summary-jsonl $OUT/per-file.jsonl \
  --mismatches-jsonl $OUT/mismatches.jsonl \
  --whitelist-csv docs/comparison/abc2xml-divergences/whitelist.csv \
  --dropped-csv docs/comparison/abc2xml-divergences/dropped.csv
```
Expected: `structural_matches` drops to the raw level (~8,700 ± , well below phase-47's 9,411). `dropped_files: 0` (manifest empty). This drop is the point.

- [ ] **Step 2: Sanity-check the whitelist count equals structural_matches**

```bash
python3 -c "import json,csv; r=json.load(open('docs/untracked/raw-baseline/report.json')); n=sum(1 for _ in open('docs/comparison/abc2xml-divergences/whitelist.csv'))-1; print('matches',r['structural_matches'],'whitelist',n); assert r['structural_matches']==n"
```

- [ ] **Step 3: Write `RAW-BASELINE.md`** with: the raw `structural_matches` / `structural_mismatches`, the per-category mismatch counts (from `report.json`), the whitelist size, and a one-line note that this is the honest pre-triage baseline the worklist is drawn from.

- [ ] **Step 4: Commit the baseline**

```bash
git add docs/comparison/abc2xml-divergences/whitelist.csv docs/comparison/abc2xml-divergences/RAW-BASELINE.md
git commit -m "baseline: raw comparator whitelist + numbers (pre-triage)"
```

---

## Notes for the executor

- The `whitelist.csv` is committed and large (~8,700 rows). That is intentional — it is the regression baseline.
- `dropped.csv` starts empty; the triage flow (separate plan) fills it.
- Do **not** re-introduce any normalization to "recover" the match rate. The drop is honest; recovery happens by fixing croma (graduates files into the whitelist) or by reasoned drops.
- Follow-on plans: (2) the `abc-divergence-investigator` subagent, (3) the `divergence-triage` skill.
