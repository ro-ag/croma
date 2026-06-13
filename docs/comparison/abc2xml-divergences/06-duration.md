# 06 — Duration: default unit length and rounding

**Affected:** 368 files (10 single-category). Two distinct sub-causes.

## Sub-cause (a) — meter-derived default unit length (abc2xml ignores §4.6)

When a tune has **no `L:` field**, ABC 2.1 derives the default unit note length
from the meter; `abc2xml` instead hard-codes `L:1/8`.

**ABC 2.1 §4.6 (lines 554–558):**

> "if [meter as a decimal] is less than 0.75 the default unit note length is a
> sixteenth note; if it is 0.75 or greater, it is an eighth note. For example,
> 2/4 = 0.5, so the default unit note length is a sixteenth note."

- **`tune_000399`** (`M:2/4`, no `L:`): a bare `F` is `1/16` (2/4 = 0.5 < 0.75).
  Croma = 1/16; abc2xml = 1/8. Every note differs by exactly 2×. **Croma follows
  the spec.**

~40 of the 368 files have no `L:` field.

## Sub-cause (b) — exact rational vs grid rounding

`abc2xml` caps/rounds long or sub-grid durations to its `<divisions>` lattice;
Croma keeps exact rationals.

**ABC 2.1 §4.3 (line 845):** "All compliant software should be able to handle
note lengths down to a 128th note." (And `F////` = 1/128 — four halvings.)

- **`tune_001029`** (`L:1/2`, `M:C|`): opening `z4` = 4 × 1/2 = 2 whole notes →
  Croma `breve` (8.0 quarter-lengths); abc2xml `whole` (4.0) — abc2xml caps the
  duration.
- Tuplet rounding: `(4c/B/A/G/` is exactly `3/64`; abc2xml's `divisions=315`
  gives `3/64 × 1260 = 59.06 → 59/1260`.

## Verdict

(a) **ABC2XML_DEVIATION** — abc2xml violates the §4.6 default-length rule; Croma
is correct. (b) **ABC2XML_ARTIFACT** — grid rounding; Croma is exact. Both are
reference artifacts, not Croma bugs.

## Phase 45 comparator status

Phase 45 removed the full-measure-rest subset from the residual duration table.
The affected rows were not Croma export changes: abc2xml's raw MusicXML duration
for the leading rest still spans a breve, but music21 rewrites the reference rest
to a whole-note full-measure rest while leaving the next event offset at the
longer breve position. The comparator now uses that next-event offset span for
rest facts only when music21's full-measure-rest rewrite shortened the reported
duration.

This resolved the eight remaining duration-only files in that family and removed
16 rows from the full 10k residual table. Remaining duration rows are not this
full-measure-rest extraction artifact and need separate evidence before any
comparator or exporter change.
