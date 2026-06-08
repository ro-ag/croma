# 09 — Tie and slur residuals

After the chord-tie and grace-slur fixes (see `docs/progress/`), the residual
tie (45 files, 8 single-cat) and slur (31 files, 8 single-cat) differences are
edge cases and abc2xml artifacts.

## Ties

**ABC 2.1 §4.11 (lines 992–1006):** "You can tie two notes of the same pitch
together, within or between bars, with a `-` symbol." "The tie symbol must always
be adjacent to the first note of the pair … `abc|-cba` are not legal." Ties
connect successive notes **of the same pitch**.

- **abc2xml drops a legal tie — Croma correct.** `tune_005286` measure 9,
  `E2)-E2`: a slur-close `)` then a tie `-` between two same-pitch E4s. Croma
  renders the tie (`tie`/`tied` start+stop); abc2xml emits **no** tie on either
  E4. Same pitch+octave ⇒ legal per §4.11.
- **Malformed (detached tie).** `tune_008162` measure 15, `… ! -{c}d2 …`: the `-`
  is detached from its first note (preceded by a decoration, before a grace) —
  illegal per §4.11 line 994. The engines recover differently.
- **Tie across a different-pitch chord** (`[ec]-[d2B2]`) is not a valid tie
  (different pitches); abc2xml renders per-member slurs, Croma drops it — both
  spec-defensible recoveries of non-tie input.

## Slurs

**ABC 2.1 §4.11 (lines 995–1000):** "(DEFG) puts a slur over the four notes …
they may also start and end on the same note: `(c d (e) f g a)`."

- **Single-note slur.** `tune_012447`, `(e6)`: a slur over one note. Croma
  count=1 (start=stop on E5 — spec-supported, line 999–1000); abc2xml count=2.
- **Endpoint off-by-one at a barline/chord.** `tune_006724`, `(A |Bcd[eG])`: the
  slur spans a barline ending on a chord; Croma stops on B3, abc2xml on D4 — a
  one-note endpoint difference at the chord boundary.

## Verdict

**ABC2XML_ARTIFACT** (drops a legal tie; single-note-slur count) / **MALFORMED**
(detached tie) / **edge** (slur endpoint at chord boundary). No clean spec
violation by Croma; in the clearest case (`tune_005286`) Croma is the correct
one.
