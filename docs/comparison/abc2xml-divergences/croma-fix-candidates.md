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

### Bug 2 — explicit key signature `K:<tonic> exp <accidentals>` — FIXED 2026-06-13

A space-less explicit accidental list (`K:D exp _B^g`, `K:D exp ^f_B_e`) arrives as
a single token, and the key parser read only the **first** accidental per token
(`_B`), dropping the rest (`^g`). The dropped pitches then resolved to natural.
(Space-separated lists like `K:D exp _b _e ^f` already worked, and `key_fifths`
already returns 0 for `exp`, so the per-note resolution and key-step emission were
correct — the bug was purely the parser dropping accidentals.)

- **Spec:** ABC 2.1 §3.1.14 (KB raw line 688) — `K:<tonic> exp <accidentals>`
  explicitly defines **all** the accidentals; `K:D Phr ^f` ≡ `K:D exp _b _e ^f`,
  so the tonic contributes nothing beyond the explicit list.
- **Fix:** `parse_key_accidentals` (in `crates/croma-core/src/parse/field/key.rs`)
  now walks the whole token, capturing every `<sign><note>` pair. Test in
  `crates/croma-core/src/parse/field/mod_tests.rs`.
- **Outcome:** croma is now spec-correct for `exp` keys (corrected ~56 corpus rows
  — the G♯/E♭/F♯ that were dropped). `tune_003838`/`tune_003836` do **not** fully
  graduate because abc2xml **over-reaches**, injecting the tonic's D-major F♯/C♯ on
  top of the explicit list; per §3.1.14 those bare F/C are natural, so croma is
  correct and the files are dropped as `abc2xml-accidental` (residual abc2xml bug).

### Bug 3 — illegal post-barline tie `a|-a` — FIXED 2026-06-13

A tie `-` placed immediately **after** a barline (`d4|-{c}d2`) was bound backward
across the barline to the pre-barline note, fabricating an illegal cross-bar tie.
ABC 2.1 §4.11: a tie must be adjacent to the **first** note of the pair — the legal
cross-bar form is `a-|a` (`-` before the bar); `abc|-cba` (`-` after the bar) is
"not legal".

- **Spec:** ABC 2.1 §4.11 (KB raw line 1048) — "The tie symbol must always be
  adjacent to the first note of the pair ... `abc|-cba` ... not legal".
- **Fix:** `apply_tie` (in `crates/croma-core/src/lower/tie.rs`) now rejects a tie
  marker when no timed note exists in the current measure
  (`broken_left_available` is false — the same §4.4 anti-cross-bar state used for
  broken rhythm, reset at every barline but surviving line breaks), emitting
  `unmatched_tie` instead of binding backward. Legal `a-|a` cross-bar and
  cross-line ties are unaffected. Test in `crates/croma-core/src/lower/mod_tests.rs`.
- **Graduated:** `tune_008162`, `tune_008163`, `tune_008166`, `tune_008168`,
  `tune_011106`, `tune_014796`.

## Undetermined / co-fault (kept in worklist, flagged for human — NOT a clean fix)

### empty-bar collapse in multi-voice — spec-ambiguous, do not "fix" lightly

croma collapses runs of consecutive **bare** empty bars (`... | | | | ...`):
`tune_011865.abc` lower voices come out 10 measures vs the upper voice's 17, a
real multi-voice misalignment. **But this is not a clean croma bug** and a fix was
deliberately deferred (root-caused 2026-06-13):

- The collapse is **intentional and §4.8-grounded**: a *run of bar lines*
  (`||`, `|]`, `]|`, split barlines) is **one boundary**, not multiple measures.
  croma's `is_empty_measure`/barline coalescing (`crates/croma-core/src/lower/timeline.rs`)
  implements exactly this and has tests for `||`/`|]`/pickup measures. The bare
  `| | | |` runs are literally consecutive barlines → §4.8 = one boundary.
- The **source is internally inconsistent**: the upper voice fills empty bars with
  `z` rests (→ kept, 17 measures) while the lower voices use bare `| |` (→ collapsed).
  ABC 2.1 §7's silent-voice-alignment example uses **explicit `x` rests**
  (`[V:B2] x8 | x8`), not bare barlines — so the lower voices are non-standard.
- A gate fix (skip coalescing in multi-voice) would **break legitimate `||`/`|]`
  handling**; distinguishing `||` (adjacent = one boundary) from `| |` (intended
  empty measure) needs source-adjacency info the timeline no longer carries.
- It **won't graduate the file**: abc2xml drops the empty bars to 6 measures, so
  even croma→17 stays a mismatch.

Disposition: `tune_011865` kept in the worklist (not dropped, not fixed),
flagged `undetermined`. If revisited, a real fix must preserve §4.8 barline-run
coalescing while distinguishing intended empty measures — likely needs the parser
to carry barline-adjacency into the timeline.

### Bug 5 — whitespace-surrounded lone `:` treated as malformed, not a barline (DEBATABLE — deliberate behavior)

croma rejects a `:` with **whitespace on both sides** as a barline: `parse_colon`
(`crates/croma-core/src/parse/barline.rs`) already accepts a `:` *adjacent* to
notes as a Liberal barline (case 4, fixed 71 files), but a free-floating `: `
deliberately becomes an `invalid_barline` malformed stray dot. In
`tune_005539.abc` (`... d3/2e/ : d/B/c/d/ ... :|`) croma drops the three
space-surrounded `:` and collapses 4 bars into one 32-quarter measure (4× the
`M:8/4` meter); abc2xml segments into 4.

- **Not a clean fix — deliberate & tested:** the case-5 behavior is asserted by
  `recovers_invalid_barline_fragments_as_skipped_malformed_items` (`C : D` →
  `invalid_barline` + `InvalidBarline`). "Fixing" it = relaxing case 5 so a
  whitespace-surrounded `:` becomes a Liberal barline, **reversing a tested
  decision**.
- **Spec-debatable:** §4.8 line 1001 ("liberal ... sequence of `|` and `:`") and
  abc2xml favour treating it as a barline; croma's stance is that a free-floating
  `:` is an ambiguous stray dot. The investigator rated croma at-fault high (the
  4×-overfull measure is clearly bad output), but croma's choice is defensible.
- **Disposition:** kept in worklist, flagged `undetermined`. A real fix is a
  one-site change (case 5 → `parse_barline`) but must be regression-tested across
  the corpus and the `C : D` test updated; it's a deliberate policy reversal, so
  it needs an explicit decision, not a drive-by fix.

## Open

None clean. The two cascade-surfaced "croma" candidates (Bug 4 empty-bar collapse,
Bug 5 free-floating `:`) are both **deliberate, tested behaviors on non-standard
input** — spec-debatable policy calls, not clear defects. All clear croma bugs
(`^/c`, `K:exp`, post-barline tie) were in the content categories and are fixed.
