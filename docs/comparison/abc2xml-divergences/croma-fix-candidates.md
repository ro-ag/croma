# croma fix candidates (surfaced by divergence triage)

Real **croma** bugs found while triaging the raw-comparator worklist. A file is
**kept** in the worklist (not dropped) until its bug is fixed, then graduates into
`whitelist.csv` on the next run. Each was confirmed by the
`abc-divergence-investigator` reasoning from the ABC 2.1 spec, with an adversarial
"find a croma error" pass where noted. Investigated 2026-06-13.

## Resolved

### Bug 1 — accidental dropped on the misplaced-length token (`^/c`) — FIXED 2026-06-13

When an accidental (`^` `=` `_`) was immediately followed by a `/` (or digit)
length operator **before** the note (e.g. `^/c`, `a^/ge`), croma discarded the
accidental and emitted the note natural. abc2xml leniently recovers the
accidental.

- **Spec:** ABC 2.1 §4.2 (KB raw line 855) — `^`/`=`/`_` notate sharp/natural/flat;
  §4.20 construct order is `<accidental><note><octave><length>`. The token is
  malformed, but the author's intent (a sharp) is unambiguous (cf. parallel
  well-formed `^c` bars).
- **Fix:** `parse_accidental_or_malformed` now recovers — when a misplaced length
  run sits between the accidental and a note, it flags the length as
  `malformed_length` (still not applied to the note's duration) but attaches the
  accidental to the following note instead of dropping it.
  (`crates/croma-core/src/parse/music.rs`, test in
  `crates/croma-core/src/lower/mod_tests.rs`).
- **Graduated:** `tune_001009`, `tune_002562`, `tune_001875`, `tune_003353`.
- **Note:** registering this fix required upgrading the comparator to compare the
  sounding `pitch.alter` rather than the display-accidental name — abc2xml emits a
  self-contradictory `<alter>1>` + `<accidental>natural>` glyph at these tokens, so
  a name-based comparison could not see that the corrected croma sharp now sounds
  identical. That comparator change also graduated 8 contradictory-glyph files that
  were previously parked in `dropped.csv` as `equivalence`.

## Open

### Bug 2 — explicit/non-traditional key signature `K:<tonic> exp <accidentals>`

croma does not apply explicit key signatures. For `[K:D exp _B^g]` croma emits
`<key><fifths>0</fifths>` paired with an **incomplete** `<key-step>`/`<key-alter>`
list (only 1 of the 2–3 declared accidentals). music21 reads `<fifths>0>` +
`<key-step>` as `altered=[]`, so **none** of the explicit accidentals apply and
every affected note comes out natural.

- **Spec:** ABC 2.1 §3.1.14 (KB raw line 688) — "`K:<tonic> exp <accidentals>` …
  explicitly define all the accidentals of a key signature. Thus `K:D Phr ^f`
  could also be notated as `K:D exp _b _e ^f`." The `exp` list defines the full
  key signature and applies like one.
- **Fix direction:** emit all the `exp` accidentals as the key signature (a full
  `<key-step>`/`<key-alter>` list music21 reads as a non-traditional key, or the
  correct `<fifths>` when expressible) so they apply to every matching note.
- **Manifest files:** `tune_003838` (65 rows), `tune_003836` (65 rows, sibling).
  ~50/65 rows per file are F♯/G♯/E♭ dropped; the remaining C♯ rows are abc2xml
  over-reaching but do not exonerate croma on the dominant failure.
