# ABC parser/model backlog (from the Score→ABC writer round-trip proof)

Defects and model gaps in `parse_document`/`lower_score` discovered while
proving `write_abc` over the 10k corpus (2026-06, PRs #65/#66/#67). The
2026-06 parser-fix phase (branch `work/parser-bug-fixes`) fixed the silent
data-loss items and several coverage caps; the resolved entries are kept below
(marked **FIXED**/**TRIAGED**) for archaeology, with the still-open items
re-prioritized.

The round-trip harness (`tools/prove_abc_roundtrip.py`, local-only) is the
regression net for every fix: each one should flip its excluded tunes to
in-scope with 0 structural diffs, and must not regress the proven set
(92.63% as of the parser-fix phase, up from 92.49%).

## Open: coverage-capping (the remaining ~7.4% of the corpus)

1. **Mid-tune key/meter changes are not modeled** (~5.5% of tunes — the
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

2. **Bare-grace slurs `({Bc})`** (a slur wrapping only a grace group, no main
   note): slur halves land somewhere unreproducible. Degenerate; 7 tunes.
   Harness gate: `_BARE_GRACE_SLUR_RE`.

3. **Nested tuplets** (new, found while removing the `[I:tuplets` gate). The
   writer keeps only the innermost tuplet of `(7:8:8(3A/A/A/ ...`:
   doubly-nested notes get the outer ratio baked into written durations,
   outer-only notes are written plain — outer `<tuplet>` notation and
   outer-only durations are lost. 1 corpus tune (tune_003732). Out of scope
   until the writer models nested tuplets. Harness gate: `_NESTED_TUPLET_RE`.

## Open: silent data loss

4. **Quoted text before a barline is silently dropped.** `C "F"| c` loses the
   chord symbol (`parse_barline` flushes pendings into standalone items that
   lowering discards at the catch-all arm). REFERENCE VERDICT (verified
   2026-06-09): abc2xml KEEPS the `<harmony>`, binding it to the next note
   across the barline. Follow-up to the fixed grace/slur/tuplet cases below —
   needs a decision on carrying pendings across barlines (today barlines
   deliberately void pending graces too).

## Open: exporter (MusicXML writer) issues

5. **Overlay `<voice>` number collision**: two voices in one merged part that
   both carry an overlay in the same measure get the same `<voice>` number
   (`musicxml/score.rs` base_count+overlay_index+1 restarts per voice).
   Currently invisible to the projection only because such tunes don't
   co-occur in the corpus subset.

6. **`<syllabic>` is always `single`** (`musicxml/lyric.rs`): Hyphen lyric
   attachments never become `begin`/`middle`/`end`, so hyphenation is
   dropped from MusicXML. If fixed, the writer's hyphen emission is already
   faithful and the projection's lyric tuple should grow a syllabic field.

7. **`+:` continuation lines join with a raw newline** inside the stored
   lyric syllable (`w:a b` + `+:c d` → syllable `"b\nc"`). Questionable
   tokenization; the writer folds the newline into `~` (a space) when
   re-emitting, so the MusicXML `<text>` differs (newline vs space) for such
   tunes. Consider joining with a space at parse time.

8. **Orphan lyric hyphens.** A `w:` line that overflows mid-hyphenated-word
   attaches a lone `{text:"-", control:Hyphen}` to the last in-block note;
   XML-invisible and unencodable in any clean `w:` emission (a bare `--`
   token would re-parse as two skips). Low value; consider not storing
   orphan hyphens.

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
