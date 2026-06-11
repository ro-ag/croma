# Mid-tune `K:`/`M:` change events — implementation plan

> Read the design first:
> `docs/superpowers/specs/2026-06-10-midtune-key-meter-design.md`.
> Branch `work/midtune-key-meter`, never `main`. TDD per task; corpus +
> comparison runs are the orchestrator's job.

## Task 1: model + lowering + plumbing (events reach the Score)
- [ ] Failing test: lower `"X:1\nL:1/4\nK:C\nCDEF|[K:F]GABc|[M:3/4]ABc|\n"`,
      assert `Voice.events` contains `KeyChange` (display "F") and
      `MeterChange` (display "3/4") at the right indices; standalone-line and
      multi-voice broadcast variants.
- [ ] Add `LoweredEvent::KeyChange/MeterChange`, `TimelineEventKind::` +
      `TimedEventKind::` variants; push in `apply_key_change` (per voice),
      `apply_inline_key_change`, `apply_meter_change` (+ inline `[M:]` path).
- [ ] Fix every exhaustive match the compiler surfaces (timeline build,
      semantic events, alignable predicate stays Note-only, exporter +
      writer get TODO arms completed in Tasks 2–3).
- [ ] Commit: `feat(croma-core): record mid-tune key/meter changes as Score events`

## Task 2: writer emission
- [ ] Failing tests: round-trip alters across `[K:F]` (barline + mid-measure +
      same-measure ledger), `[M:3/4]` measure structure, modes/`K:none`/`C|`
      display echo, multi-voice broadcast, no-op K: restatement.
- [ ] `write_voice` arms: `[K:{display}] ` / `[M:{display}] `.
- [ ] Commit: `feat(croma-core): write_abc emits mid-tune [K:]/[M:] changes`

## Task 3: exporter emission + grace key fix
- [ ] Failing tests: `croma xml` emits `<attributes><key>/<time>` at change
      positions (boundary + mid-measure), shapes matching the header forms
      (`fifths`; `beats/beat-type` + `symbol` for C/C|); grace implicit alter
      after a key change uses the active key.
- [ ] Admit events through `measure_sequences`; arms in `write_sequence`/
      `write_event`; factor `write_attributes` key/time blocks; thread the
      position-active key into `grace_export_pitch`.
- [ ] Commit: `feat(croma-core): export mid-tune key/meter changes to MusicXML`

## Task 4: harness + corpus proof
- [ ] Projection: append `("KEY", fifths)` / `("TIME", beats, beat_type,
      symbol)` tokens; delete `has_mid_tune_field_change` and regexes.
- [ ] Corpus run: 0 structural diffs; expect ≈99.27%. Diagnose any diffs
      (4 octave-modifier files are the known risk → documented exclusion if
      needed). Re-run until clean.
- [ ] Commit: `test: prove mid-tune key/meter round-trip over the corpus`

## Task 5: reference comparison + close-out
- [ ] Full report-only comparison run (command from tracker phase-31
      validations); report divergence delta — mid-tune attribute rows should
      improve, no category regressions.
- [ ] Gates: workspace tests, clippy -D warnings, fmt --check, pytest,
      publish dry-run, `git diff --check`, no Co-Authored-By.
- [ ] Adversarial review (single reviewer agent, repro-required).
- [ ] PR; merge on green CI; delete branch; tracker phase row + metrics +
      validations; update `docs/parser-backlog.md` (mark mid-tune cap fixed;
      add broadcast-scoping divergence note) and the session memory.
