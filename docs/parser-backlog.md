# ABC parser/model backlog (from the Score→ABC writer round-trip proof)

Defects and model gaps in `parse_document`/`lower_score` discovered while
proving `write_abc` over the 10k corpus (2026-06, PRs #65/#66/#67). The
2026-06 parser-fix phase (branch `work/parser-bug-fixes`) fixed the silent
data-loss items and several coverage caps; the resolved entries are kept below
(marked **FIXED**/**TRIAGED**) for archaeology.

As of phase 38 (2026-06-12, branch `codex/phase-38-empty-backlog`), the active
Croma parser/model/export backlog in this file is empty. The formerly open
items below are either fixed with regression tests and target evidence, or
re-verdicted with fresh evidence as not an active Croma bug.

The round-trip harness (`tools/prove_abc_roundtrip.py`, local-only) remains the
regression net for writer/model fixes: each one should keep affected tunes
in-scope with 0 structural diffs, and must not regress the proven set.

## Resolved: coverage-capping

1. ~~**Mid-tune key/meter changes are not modeled**~~ **FIXED (PR #70,
   phase-32):** `TimedEventKind::KeyChange/MeterChange` events now flow
   through lowering → MusicXML `<attributes>` → writer `[K:]`/`[M:]` tokens.
   Round-trip 99.25%, reference matches +262. Residual (new, small):
   mid-tune K: lines that CHANGE the voice octave shift (`treble+8` →
   `treble-8`, 1 tune — writer needs position-aware octave compensation);
   trailing changes after the final barline are silently dropped; `[M:]`
   mid-tuplet force-closes the tuplet while `[K:]` does not; dedupe is
   display-string-only (`K:C` vs `K:Cmaj` both record). Original text: (~5.5% of tunes — the
   single biggest unlock). `[K:...]`/`[M:...]` inline fields and standalone
   body `K:`/`M:` lines change parser state, but the lowered `Score` keeps no
   key/meter-change event — key effects are baked into note alters, meter
   changes only affect measure validation. No writer can reproduce them.
   Fix: add key/meter-change events (or per-measure attributes) to the model,
   emit them in MusicXML `<attributes>`, and re-emit them in `write_abc`.
   Deserves its own brainstorm+spec phase (model/exporter design change).
   Harness gate: `has_mid_tune_field_change`.
   Note (parser-fix phase): lowering no longer clears measure-accidental
   carry at mid-tune `M:`/`K:` fields (ABC 2.1 §11.3: carry runs to the end
   of the bar; abc2xml agrees) — only barlines reset.

2. ~~**Bare-grace slurs `({Bc})`**~~ **FIXED (phase-38, 2026-06-12):**
   slur starts/stops around a closed bare grace group now attach to that grace
   group instead of drifting to the previous/next timed event. Closed bare
   groups before a barline remain after-graces on the previous note and
   `write_abc` preserves `({Bc})` without producing a stray `)`. The
   `_BARE_GRACE_SLUR_RE` round-trip gate was removed. Regression tests cover
   the model and MusicXML/ABC emission. Target evidence:
   `docs/untracked/phase-38-empty-backlog/target-bare-grace-slur/abc-roundtrip-report.json`
   is 2/2 in-scope with 0 structural diffs; the old seven-file regex corpus
   still has 3 structural diffs in `tune_001365`-family malformed-header lyric
   cascades, not the bare-grace slur symptom.

3. ~~**Nested tuplets**~~ **FIXED / RE-VERDICTED (phase-38, 2026-06-12):**
   Croma now preserves nested tuplet markers in the lowered model, MusicXML
   exporter, and ABC writer. The XML writer emits composite
   `<time-modification>` ratios and ordered start/stop tuplet notations instead
   of collapsing to the innermost tuplet or saturating overflowed products.
   The `_NESTED_TUPLET_RE` round-trip gate was removed. Target Croma
   self-roundtrip evidence for `tune_003732` is 1/1 in-scope with 0 structural
   diffs:
   `docs/untracked/phase-38-empty-backlog/target-nested-tuplets/abc-roundtrip-report.json`.
   The refreshed abc2xml comparison for the same tune still has 60 rows
   (`tuplet`: 8 plus malformed-feature cascades), but that reflects reference
   interpretation and surrounding malformed corpus features rather than a
   remaining Croma nested-tuplet writer gap.

## Resolved: silent data loss

4. ~~**Quoted text before a barline is silently dropped.**~~ **FIXED
   (phase-33a, 2026-06-11; refined phase-41/42, 2026-06-12):** pending chord
   symbols, annotations, decorations AND grace groups flushed at a barline /
   line end / multirest / malformed token now buffer in `LoweringState` instead
   of being dropped (ABC 2.1 §§4.12/4.14/4.18/4.19); leftovers at the end of a
   voice warn (`abc.music.dangling_quoted_text` / `dangling_grace_group`).
   Phase 41 refined the barline case so annotations and direction-style
   decorations written immediately before a barline attach at the current
   measure's barline position, while chord symbols, graces, and note-attached
   decorations keep the next-event behavior. Phase 42 extended that placement
   rule to unprefixed quoted text that is not likely harmony immediately before
   non-final barlines; valid chord-like text still binds across the barline,
   and quoted text before a final barline still reaches the dangling diagnostic
   path. The same phase-33 batch closed multi-measure volta brackets (`<ending
   type="stop">` carried across measures per §4.9 — 905 affected files), fixed
   `<ending>`-before-`<repeat>` order, and mapped `!crescendo(!`-family to
   `<wedge>` and `!+!` to `<technical><stopped/>`. Corpus matches 8,118 →
   8,734, 0 regressions; the phase-41 placement refinement moved full-corpus
   matches 9,206 → 9,223, and the phase-42 refinement moved them 9,223 →
   9,237.

## Resolved: phase-33 triage ledger (2026-06-11)

The forensic triage of all 15 mismatch categories produced per-cause,
adversarially re-verified verdicts in
`docs/comparison/abc2xml-divergences/12-phase33-triage-ledger.md` — the
canonical record for phase-33 residuals. Phases 35-38 fixed or re-verdicted
the active `OPEN`/`known_backlog_model_gap` entries. The remaining mismatch
families in that ledger are documented as reference quirks, equivalent
serializations, malformed-input recovery differences, or comparator
normalization candidates rather than active Croma parser/export bugs.

## Resolved: exporter (MusicXML writer) issues

5. ~~**Overlay `<voice>` number collision**~~ **FIXED (phase-38,
   2026-06-12):** grouped-part overlays now use part-wide stable overlay voice
   numbers keyed by source voice plus overlay index, so simultaneous overlays
   from different source voices do not collide and repeated overlay identities
   remain deterministic across measures. Regression tests cover same-measure
   collision and cross-measure determinism.

6. ~~**`<syllabic>` is always `single`**~~ **FIXED (phase-38, 2026-06-12):**
   MusicXML lyric export now tracks open hyphenated words by source voice key
   and verse, emitting `begin`/`middle`/`end`/`single` from stored lyric hyphen
   controls. Regression tests cover a normal `A-des-te all` chain and a
   source-order lyric chain crossing an overlay. The music21/Polars comparison
   projection now emits `lyric/text` and `lyric/syllabic` facts. Target
   20-file hyphenated lyric comparison:
   `docs/untracked/phase-38-empty-backlog/target-lyrics/hyphen-compare-report.json`
   has 19 structural matches, 1 non-lyric measure-number/direction mismatch,
   0 lyric mismatches, and exact Croma/reference syllabic fact counts
   (`single`: 797, `begin`: 757, `middle`: 402, `end`: 757).

7. ~~**`+:` continuation lines join with a raw newline**~~ **FIXED
   (phase-38, 2026-06-12):** lyric continuation text is tokenized across the
   inserted continuation newline as a separator, so `w:a b` + `+:c d` stores
   four syllables `a b c d` rather than `"b\nc"`. Regression coverage asserts
   zero lyric-count diagnostics and Croma self-roundtrip evidence for the
   synthetic target is clean.

8. ~~**Orphan lyric hyphens**~~ **FIXED (phase-38, 2026-06-12):** lyric
   alignment now stores a hyphen control only when a later same-verse syllable
   can attach in the current lyric line or a future lyric block, respecting
   skips, extenders, and lyric bar markers. `w:a-b` over one note stores only
   `a`, warns on the overflowed `b`, dumps as `w:a`, and exports
   `<syllabic>single</syllabic>`. The valid two-note `a-b` case, including
   same-verse continuation across non-adjacent lyric blocks, still stores and
   exports `begin`/`end`. Synthetic lyric
   roundtrip target:
   `docs/untracked/phase-38-empty-backlog/target-lyrics/abc-roundtrip-report.json`
   is 3/3 in-scope with 0 structural diffs.

## Resolved in the parser-fix phase (2026-06)

9. **FIXED — Quoted text before a grace group, slur-open, or tuplet marker
   was silently dropped.** `"F"{AB}c`, `"G7"(DE)`, `"F"(3CDE` all lost the
   quoted text (pending decorations too: `!trill!{AB}c`). abc2xml keeps the
   `<harmony>` in every case. Fixed at parse level: `parse_grace_group`
   stashes/restores the pending bundle, `parse_slur` flushes only on
   slur-close, `parse_tuplet` no longer flushes. The writer's canonical
   emission order (grace → slur-opens → quoted → decorations) is unchanged.

10. **FIXED — Ties to non-adjacent same-pitch notes were dropped but their
    accidental carry survived the barline.** `^a- | b a a` gave A#-B-A#-A#;
    abc2xml gives A#-B-A♮-A♮. `MeasureAccidental` now carries
    `from_pending_tie` provenance; a pending tie finished without a stop note
    undoes its barline-preserved carry (mismatch, rest, and chord-sibling
    drop paths), while matched ties confirm theirs. The writer's accidental
    safety-net (`note_accidental` divergence emission, `measure_alters`,
    `key_alter`, `alter_glyph`, `has_tie_stop`, overlay parallel state) was
    REMOVED — plain written-accidental emission round-trips the corpus with
    0 diffs.
    Follow-on found by the corpus judge: mid-tune `M:`/`K:` fields wrongly
    hard-cleared the measure-accidental ledger (the 2 corpus diffs were
    barline-less tunes with mid-body `M:3/2`); both resets removed per
    §11.3 + oracle parity.

11. **FIXED — Rest-led tuplets dropped the group start.** `(3zBA` (incl.
    `(3 z`, `(3{a}z`, `(3"C"z`, `(6:4:6(z/...`) had no Start attachment;
    `(3BAz` had no Stop (writer emitted a wrong `(3:2:2`); `(3zzz` had
    nothing. `attach_completed_tuplet` now attaches roles to any timed event
    (rests included, full Start/Continue/Stop symmetry). Forward XML gained
    `<tuplet>` notation on rests (matches abc2xml). The writer's dead
    `has_start` skip in `tuplet_layout` was removed; the twin guard in
    `overlay_tuplet_layout` is KEPT — verified non-dead: a tuplet straddling
    an overlay `&` (`C (3DE & FGA z |`) leaves a Stop-only pair in the
    overlay segment, and removing the guard would emit a bogus marker there.
    Harness gate `_REST_LED_TUPLET_RE` removed (+14 tunes in scope).

12. **RECLASSIFIED — `[I:tuplets ...]` does not change parsing.** It is the
    abcm2ps `%%tuplets` DISPLAY directive (bracket/number/ratio rendering);
    abc2xml skips it; the old claim that it "changes how later tuplets
    parse" was false, and it occurs 4 times in 1 tune (not 4 tunes).
    Inline `[I:...]` fields are no longer silently dropped — lowering warns
    `abc.field.inline_ignored` (mirroring the header path's
    `abc.field.unknown_instruction`). Harness gate `_INLINE_INFO_RE` removed
    (tune_003732 remains out of scope via the nested-tuplet gate above).

13. **TRIAGED — the 65 `lower_fail` tunes are corpus truncation artifacts,
    not parser bugs.** All 65 fail with `error[abc.file.no_music]`: each is a
    header-only fragment (file ends at the `K:`/`V:` lines, zero body music)
    from the Zenodo extraction. abc2xml degrades by synthesizing an empty
    voice instead of failing; matching that would add no round-trip coverage.
    The harness now labels these records `status="lower_fail"` with a
    normalized `reason`, and the summary buckets reasons
    (`lower_fail_reasons`).

14. **FIXED — Octave-shift arithmetic overflow panics.** `V:1 clef=treble+15
    octave=125` (and absurd octave-mark runs) panicked in debug builds at
    four i8 sites. `voice_octave_shift` now accumulates in i32, clamps
    `octave=` to ±9 (abc2xml's effective single-digit domain) and the total
    to ±12; octave marks sum in i32; the note/chord additions saturate; the
    writer's duplicate `voice_octave_shift` clamps identically (the two must
    stay value-for-value equal — sync comments at both sites), and
    `pitch_letter_str` computes octave-mark repeat counts in i32. Behavior
    note: `octave=99999` now clamps to +9 instead of being silently ignored
    (abc2xml reads the first digit). Croma's additive clef+octave semantics
    are deliberately kept (abc2xml lets `octave=` override the clef shift).

## Notes

- The octave=/clef±8/middle= written→stored pitch shift is *not* a bug — the
  writer compensates (`voice_octave_shift` replica in `to_abc.rs`); the two
  implementations must stay value-for-value identical (clamps included).
- `barline_lowering_kinds` splits `||:`→[Double,RepeatStart] and
  `[|:`→[Initial,RepeatStart] sharing one span; the writer's re-join detects
  span equality. A refactor that rewrites barline spans breaks that detection
  (unit tests cover both directions).
- A tuplet straddling an overlay `&` re-parses with the `<tuplet>` stop
  notation migrated from the overlay note to the last main-voice member
  (durations identical). Pre-existing; no in-scope corpus tune hits it.
- **`%%MIDI` directive handling** is documented in
  [`docs/midi-directives.md`](midi-directives.md): the score-meaningful
  `%%MIDI program`/`channel` are forward-translated to MusicXML `<part-list>`
  `<score-instrument>`/`<midi-instrument>` (per-voice scoped), while all
  directives stay preserved verbatim for round-trip/`croma fmt`. Deferred
  items (transpose, channel-only, inline `[I:MIDI]`) and the abc2xml-isms not
  mimicked (visible `prog:` words, drummap percussion) are tabled there, along
  with the writer-side projection-coverage gap.
