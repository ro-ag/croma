# ABC parser/model backlog (from the Score→ABC writer round-trip proof)

Defects and model gaps in `parse_document`/`lower_score` discovered while
proving `write_abc` over the 10k corpus (2026-06, PRs #65/#66 and the
coverage-2 branch). The writer treats the parser as its oracle, so none of
these were fixed in-place — they are queued for a dedicated parser phase
**after the Score→ABC writer epic closes** (i.e. alongside / after the
MusicXML→Score reader sub-project, per the tracker).

The round-trip harness (`tools/prove_abc_roundtrip.py`, local-only) is the
regression net for every fix below: each one should flip its excluded tunes to
in-scope with 0 structural diffs, and must not regress the proven set
(92.5% as of the coverage-2 branch).

## Coverage-capping (the remaining ~7.5% of the corpus)

1. **Mid-tune key/meter changes are not modeled** (~5.5% of tunes — the
   single biggest unlock). `[K:...]`/`[M:...]` inline fields and standalone
   body `K:`/`M:` lines change parser state, but the lowered `Score` keeps no
   key/meter-change event — key effects are baked into note alters, meter
   changes only affect measure validation. No writer can reproduce them.
   Fix: add key/meter-change events (or per-measure attributes) to the model,
   emit them in MusicXML `<attributes>`, and re-emit them in `write_abc`.
   Harness gate: `has_mid_tune_field_change`.

2. **65 tunes fail to lower entirely** (`lower_fail`, 0.7%). Not triaged
   individually yet; first step is a categorized error report over those 65.

3. **Rest-led tuplets drop the group start** (`(3z B A`, incl. behind a
   slur-open `(6:4:6(z/G/...`). The leading rest gets no `TupletAttachment`,
   so the group's first index is unrecoverable from the `Score` (the writer
   cannot place the `(p:q:r` marker). Fix: attach Start (or an explicit
   group-span) to rest members of a tuplet. Harness gate:
   `_REST_LED_TUPLET_RE`.

4. **`[I:tuplets ...]` inline directives** (4 tunes) change how later tuplets
   parse; not represented in the `Score`. Harness gate: `_INLINE_INFO_RE`.

5. **Bare-grace slurs `({Bc})`** (a slur wrapping only a grace group, no main
   note): slur halves land somewhere unreproducible. Degenerate; 7 tunes.
   Harness gate: `_BARE_GRACE_SLUR_RE`.

## Silent data loss (worked around in the writer; fix = real correctness wins)

6. **Quoted text before a grace group or slur-open is silently dropped.**
   `"F"{AB}c` loses the chord symbol entirely (no diagnostic); `"G7"(DE)`
   likewise. `{AB}"F"c` / `("G7"DE)` keep it. The writer dodges this by
   emission order (grace → slur-opens → quoted), but user-written ABC hits it
   silently. Fix in the parser: bind pending quoted text across grace/slur
   tokens (or at least warn).

7. **Ties to non-adjacent same-pitch notes are dropped, but their accidental
   carry survives.** `^a- | b a a`: the tie disappears from the `Score`
   (target not adjacent), yet measure-2 `a`s keep `alter=1` from the dropped
   tie's carry — leaving alters unexplainable by `written_accidental` + key +
   measure state. The writer papers over this with an accidental safety-net
   (`note_accidental` in `to_abc.rs`). Fix: keep the tie (MusicXML supports
   it) or drop its accidental carry consistently.

8. **Orphan lyric hyphens.** A `w:` line that overflows mid-hyphenated-word
   attaches a lone `{text:"-", control:Hyphen}` to the last in-block note;
   XML-invisible and unencodable in any clean `w:` emission (a bare `--`
   token would re-parse as two skips). Low value; consider not storing
   orphan hyphens.

## Exporter (MusicXML writer) issues observed

9. **Overlay `<voice>` number collision**: two voices in one merged part that
   both carry an overlay in the same measure get the same `<voice>` number
   (`musicxml/score.rs` base_count+overlay_index+1 restarts per voice).
   Currently invisible to the projection only because such tunes don't
   co-occur in the corpus subset.

10. **`<syllabic>` is always `single`** (`musicxml/lyric.rs`): Hyphen lyric
    attachments never become `begin`/`middle`/`end`, so hyphenation is
    dropped from MusicXML. If fixed, the writer's hyphen emission is already
    faithful and the projection's lyric tuple should grow a syllabic field.

## Notes

- The octave=/clef±8/middle= written→stored pitch shift is *not* a bug — the
  writer compensates (`voice_octave_shift` replica in `to_abc.rs`); just keep
  the two implementations in sync if the parser's shift rules change.
- `barline_lowering_kinds` splits `||:`→[Double,RepeatStart] and
  `[|:`→[Initial,RepeatStart] sharing one span; the writer's re-join detects
  span equality. A refactor that rewrites barline spans breaks that detection
  (unit tests cover both directions).
