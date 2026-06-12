# Empty Parser Backlog Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Drive the active parser/model/export backlog in `docs/parser-backlog.md` to empty or to evidence-backed blocked/re-verdicted status.

**Architecture:** Use the progress tracker and phase-33 ledger as evidence, with `docs/parser-backlog.md` as the canonical active inventory. Implement parser/export fixes with TDD, run targeted corpus evidence for affected tunes, and update docs plus `docs/progress/croma-progress.sql` after each slice. Keep generated comparison reports under `docs/untracked/phase-38-empty-backlog/`.

**Tech Stack:** Rust 1.96.0, `cargo test --workspace`, Croma CLI MusicXML export, Python/uv corpus tooling, music21/Polars comparison cache.

---

## Issue Inventory

1. `nested-tuplets`: active `known_backlog_model_gap`, tracked by `docs/parser-backlog.md` item 3 and ledger `tuplet-nested-tuplets`; corpus tune `tune_003732`; harness gate `_NESTED_TUPLET_RE`.
2. `bare-grace-slurs`: active parser-backlog item 2; harness gate `_BARE_GRACE_SLUR_RE`; triage/fix after nested tuplets.
3. `overlay-voice-number-collision`: active exporter item 5; no corpus co-occurrence known; requires constructed MusicXML regression.
4. `lyric-syllabic`: active exporter item 6; `musicxml/lyric.rs` emits `single` for every syllable; requires projection decision.
5. `lyric-continuation-newline`: active parser/writer item 7; parse stores `+:` joins as newline, writer folds to `~`; requires parse-level decision.
6. `orphan-lyric-hyphen`: active low-value parser/writer item 8; likely re-verdict as intentionally dropped XML-invisible state or parser normalization.
7. `stale-docs-tracker`: active documentation cleanup; parser backlog and progress memory still mention phase-33 OPEN work that phase 37 superseded.

### Task 1: Nested Tuplets

**Files:**
- Modify: `crates/croma-core/src/lower/mod_tests.rs`
- Modify: `crates/croma-core/src/musicxml/mod_tests.rs`
- Modify: `crates/croma-core/src/to_abc.rs`
- Modify if required: `crates/croma-core/src/musicxml/note.rs`
- Modify if required: `tools/prove_abc_roundtrip.py`
- Modify docs/tracker: `docs/parser-backlog.md`, `docs/comparison/abc2xml-divergences/12-phase33-triage-ledger.md`, `docs/progress/croma-progress.sql`

- [x] **Step 1: Write failing lowerer regression**

Add a test near `one_note_tuplet_carries_start_and_stop_on_same_event`:

```rust
#[test]
fn nested_tuplets_carry_outer_and_inner_roles() {
    let source = "X:1\nM:C\nL:1/4\nK:C\n(7:8:8(3A/A/A/ A/A/A/A/A/|\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
    let events: Vec<_> = tune.score.parts[0].voices[0]
        .events
        .iter()
        .filter(|event| matches!(event.kind, TimedEventKind::Note(_)))
        .collect();
    assert_eq!(events.len(), 8);
    assert_eq!(events[0].attachments.tuplets.len(), 2);
    assert!(events[0].attachments.tuplets.iter().any(|t| {
        t.actual_notes == 7 && t.normal_notes == 8 && t.role == TupletRole::Start
    }));
    assert!(events[0].attachments.tuplets.iter().any(|t| {
        t.actual_notes == 3 && t.normal_notes == 2 && t.role == TupletRole::Start
    }));
    assert!(events[2].attachments.tuplets.iter().any(|t| {
        t.actual_notes == 3 && t.normal_notes == 2 && t.role == TupletRole::Stop
    }));
    assert!(events[7].attachments.tuplets.iter().any(|t| {
        t.actual_notes == 7 && t.normal_notes == 8 && t.role == TupletRole::Stop
    }));
}
```

- [x] **Step 2: Run lowerer regression**

Run: `cargo test -p croma-core nested_tuplets_carry_outer_and_inner_roles`

Expected before any implementation: this may already pass, proving the model stores nested roles. If it passes, keep it as a guard and write the exporter/writer failing tests in Step 3.

- [x] **Step 3: Write failing MusicXML/export regression**

Add a test near existing tuplet export tests:

```rust
#[test]
fn nested_tuplets_export_distinct_brackets_and_combined_time_modification() {
    let source = "X:1\nT:Nested Tuplets\nM:C\nL:1/4\nK:C\n(7:8:8(3A/A/A/ A/A/A/A/A/|\n";
    let export = export_musicxml(source).expect("nested tuplets should export");

    assert_balanced_xml(&export.musicxml);
    assert_eq!(count(&export.musicxml, "<tuplet type=\"start\" number=\"1\"/>"), 1);
    assert_eq!(count(&export.musicxml, "<tuplet type=\"stop\" number=\"1\"/>"), 1);
    assert_eq!(count(&export.musicxml, "<tuplet type=\"start\" number=\"2\"/>"), 1);
    assert_eq!(count(&export.musicxml, "<tuplet type=\"stop\" number=\"2\"/>"), 1);
    assert!(export.musicxml.contains("<actual-notes>21</actual-notes>"));
    assert!(export.musicxml.contains("<normal-notes>16</normal-notes>"));
}
```

- [x] **Step 4: Run MusicXML regression**

Run: `cargo test -p croma-core nested_tuplets_export_distinct_brackets_and_combined_time_modification`

Expected before implementation: FAIL because `write_note` currently uses only the first tuplet attachment for `<time-modification>` and `to_abc` uses a single scale per event.

- [x] **Step 5: Implement minimal nested tuplet support**

Implement only the smallest needed behavior:

```text
1. For MusicXML note spelling, derive a combined effective time modification from every active tuplet attachment on the event.
2. Keep all start/stop notation attachments so nested brackets get independent MusicXML numbers.
3. For `write_abc`, allow more than one marker per event and multiply all active tuplet ratios when recovering written durations.
4. Preserve existing sequential, rest-led, one-note, overlay, and malformed-short-tuplet behavior.
```

- [x] **Step 6: Verify nested slice**

Run:

```sh
cargo test -p croma-core nested_tuplets
cargo test -p croma-core tuplet
cargo run -p croma-cli -- xml docs/untracked/corpus/zenodo-10k/abc/tune_003732.abc > docs/untracked/phase-38-empty-backlog/tune_003732-nested.musicxml
```

- [x] **Step 7: Targeted corpus evidence**

Run targeted export/compare for `tune_003732` under `docs/untracked/phase-38-empty-backlog/target-nested-tuplets/`. Use the repository's existing corpus tooling and parse the JSON summary line/report for mismatch counts. Keep generated XML/report files ignored under `docs/untracked/`.

### Task 2: Bare-Grace Slurs

**Files:**
- Modify after triage: parser/lowerer files handling slur tokens around grace groups.
- Test: `crates/croma-core/src/lower/mod_tests.rs`, `crates/croma-core/src/musicxml/mod_tests.rs`
- Modify if fixed: `tools/prove_abc_roundtrip.py`, backlog docs, progress SQL.

- [x] Write failing tests for `({Bc})` or the exact corpus shape behind `_BARE_GRACE_SLUR_RE`.
- [x] Run the focused test and verify it fails for missing/incorrect slur placement.
- [x] Implement the smallest parser/lowerer change, or re-verdict as blocked if the source construct has no principled Score representation after investigation.
- [x] Run targeted tests and round-trip/corpus evidence for the seven gated tunes.

### Task 3: Overlay Voice Collision

**Files:**
- Modify after triage: `crates/croma-core/src/musicxml/score.rs` or sequence voice-number assignment code.
- Test: `crates/croma-core/src/musicxml/mod_tests.rs`

- [x] Write a constructed failing MusicXML regression with two merged voices in one part, both carrying overlays in the same measure.
- [x] Verify duplicate `<voice>` numbers appear before the fix.
- [x] Allocate overlay voice numbers per merged part/measure without collisions.
- [x] Run focused MusicXML tests and a full workspace test before marking fixed.

### Task 4: Lyric Export Items

**Files:**
- Modify after triage: `crates/croma-core/src/musicxml/lyric.rs`, lyric parsing/lowering modules, `crates/croma-core/src/to_abc.rs`, comparison projection if syllabic becomes compared.
- Test: `crates/croma-core/src/musicxml/mod_tests.rs`, lowerer/parser tests.

- [x] For syllabic export, write a failing test for `w:hel-lo` expecting `begin`/`end`, or re-verdict if the model lacks enough information and the correct task is model expansion.
- [x] For `+:` lyric continuation, write a failing parser/lowerer or writer fixed-point test proving newline vs space behavior.
- [x] For orphan lyric hyphens, write a failing test only if the parser should drop them at parse time; otherwise update docs with an evidence-backed intentional-drop verdict.

### Task 5: Docs, Tracker, Review, and Final Validation

**Files:**
- Modify: `docs/parser-backlog.md`
- Modify: `docs/comparison/abc2xml-divergences/12-phase33-triage-ledger.md`
- Modify/export: `docs/progress/croma-progress.sql`

- [x] Update stale backlog headings and phase-33 OPEN text after implementation/re-verdicts.
- [x] Update the runtime tracker DB with phase-38 selected targets, results, metrics, artifacts, and next target.
- [x] Run `uv run python tools/progress/progress.py export`.
- [x] After each implementation slice, dispatch spec/compliance and code-quality reviewer subagents and fix or document every finding.
- [x] Periodically run:

```sh
cargo fmt --all -- --check
git diff --check
cargo test --workspace
cargo run -p croma-cli -- xml examples/basic.abc
uv run pytest
```

- [x] Run a full 10k export/compare after parser/export behavior changes and record final metrics.
- [x] Commit coherent slices and push `codex/phase-38-empty-backlog`.
