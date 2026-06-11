# Mid-tune `K:` / `M:` change events (design)

Status: approved design, pre-implementation
Date: 2026-06-10
Branch: `work/midtune-key-meter`

## Why

664 corpus tunes are excluded from the `Score → ABC` round-trip proof because
mid-tune key changes (inline `[K:..]` or standalone body `K:` lines) are
applied during lowering (alters are baked into pitches) but **never recorded
in the `Score`** — so neither the MusicXML exporter nor the ABC writer can
reproduce them. The forward pipeline also diverges from the abc2xml reference,
which emits a `<attributes><key>/<time>` element at every change (verified:
croma 2 elements vs reference 4 on a two-change probe). Removing this cap
lifts the round-trip ceiling from 92.63% to **99.27%** (9,927/10,000; the
remainder is 65 header-only corpus artifacts + 8 degenerates).

Adjacent fact that forces scope: 425 currently-passing tunes have mid-tune
`M:` only — invisible today because the projection ignores `<key>/<time>`.
Strengthening the projection (required to actually prove this feature) puts
them under the bar, so **meter-change emission must land in the same change**.

## Model

Add change events to the timeline, reusing the existing models verbatim
(`display` already round-trips raw text, including mode names, `C`/`C|`,
`exp` accidental lists, and clef property tokens):

- `TimedEventKind::KeyChange(KeySignatureModel)` and
  `TimedEventKind::MeterChange(MeterModel)` (`model.rs:207`)
- matching `TimelineEventKind::KeyChange/MeterChange` variants and
  `LoweredEvent` variants in `lower/voice.rs`
- zero-duration, `alignable=false` (must not consume `w:`/`s:` positions);
  not time-bearing (must not count toward tuplet spans)
- `Score.metadata.key/meter` stay the **header** values (already true)
- the legacy `model::Event` enum is NOT extended

## Lowering

Push the events where croma already applies the change effects — application
semantics are NOT changed in this phase:

- `apply_key_change` (`lower/mod.rs:255-274`): standalone `K:` broadcasts to
  all voices; push one `KeyChange` per voice at that voice's current position.
  (Croma's broadcast diverges from abc2xml's current-voice-only scoping; that
  is a behavior change with comparison risk — recorded in the backlog, not
  done here. Per-voice event recording + the writer's per-voice inline
  emission reproduces the broadcast exactly on re-parse.)
- `apply_inline_key_change` (`:282-289`): current voice only. Clef-only
  `[K:clef=bass]` keeps its existing no-key-change guard but must still emit
  (display text carries the clef token verbatim).
- `apply_meter_change` (`:227-246`) + the `[M:]` inline path: global; push a
  `MeterChange` to every voice at its current position.

The accidental machinery needs **nothing new**: `set_key` swaps the
per-voice key alter base and deliberately keeps the measure ledger (ABC 2.1
§11.3), alters are baked at lowering, and probes confirm same-measure /
next-measure behavior. The writer reproduces alters purely by re-emitting the
token at the right position.

## Exporter (MusicXML)

- `measure_sequences` (`musicxml/score.rs:189-215`) admits the new kinds;
  `write_sequence`/`write_event` (`musicxml/note.rs`) gain arms emitting
  `<attributes><key><fifths>` / `<time>` at the cursor — covering both
  at-boundary and mid-measure changes (reference emits mid-measure
  `<attributes>` between notes; verified). Factor the key/time blocks of
  `write_attributes` (`musicxml/attributes.rs:10-33`) to accept the change
  models; `meter_parts` already covers every observed corpus `M:` value
  (fractions, `C`, `C|`).
- Keep croma's existing shapes: `<key><fifths>N</fifths></key>` (no `<mode>`
  child today) and `<time symbol=..>` for `C`/`C|`.
- Fix the latent grace bug in the same pass: `grace_export_pitch`
  (`musicxml/grace.rs:180-215`) infers implicit grace alters from the HEADER
  key — wrong after a mid-tune change; it must use the position-active key.

## Writer (`to_abc.rs`)

- New match arms in the `write_voice` event loop emitting `"[K:{display}] "` /
  `"[M:{display}] "` — display verbatim. Inline form ≡ standalone form
  (projection-identical, probed) and is fmt-byte-stable, so the writer always
  emits inline tokens on its single music line.
- `unit_length` / `L:` / duration math stay on the header meter (event
  durations are absolute Rationals; verified no interaction).
- `note_accidental` needs no change (post-PR#68 it echoes written accidentals
  only).
- Known residual risk: 4 corpus files carry octave-shifting modifiers on
  mid-tune `K:` lines (`+8`/`octave=`/`middle=`); the writer's per-voice
  octave compensation uses the FINAL voice properties, so these may diff —
  if they do, exclude them with a documented filter (mid-tune octave-shift
  change), not a silent skip.

## Harness (`tools/prove_abc_roundtrip.py`)

- Delete `has_mid_tune_field_change` + its regexes + scope docstring line.
- Extend `projection()`'s `<attributes>` branch to append
  `("KEY", fifths)` and `("TIME", beats, beat-type, symbol)` tuples in
  document order, so K: and M: changes are *asserted*, not just implied via
  alters.

## Proof gates

- Unit/TDD per construct: change at barline, mid-measure, same-measure-ledger
  interaction, mode keys, `K:none`, `M:C|`, multi-voice broadcast vs inline
  scoping, no-op K: restatements (49 files restate the header key — must
  still round-trip), clef-bearing K: changes.
- Corpus round-trip: **0 structural diffs**, expected coverage ≈ 99.27%
  (or minus documented octave-modifier exclusions).
- Full report-only reference comparison (forward output changes): mid-tune
  `<key>/<time>` rows should improve; no category regressions.
- Standard gates + publish dry-run + no co-author trailers; PR; green CI;
  tracker phase row + backlog updates (mark item 1 of the
  coverage-capping list fixed; add the broadcast-scoping divergence).
