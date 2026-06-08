# 07 — Cascade artifacts: pitch, octave, harmony, lyric

**Affected:** pitch 262 files, octave 258, harmony 381, lyric 9 — **0
single-category** files in all four. These categories do **not** represent real
pitch/octave/harmony/lyric errors; they are positional **cascades** of a
structural artifact (a phantom empty measure, a `Z`-expansion, or header/prose
mis-segmentation) that shifts the event streams so aligned positions no longer
correspond.

## Why these are cascades, not errors

The comparison aligns the two event streams positionally. When the reference has
one extra (phantom) measure or rest, every following event is offset by one slot,
so the comparator reports the now-misaligned notes as pitch/octave/harmony/lyric
differences even though the underlying values match once realigned. This is why
**every** file in these categories also carries `missing_in_croma` +
`measure_alignment` (+ `voice`) rows — the structural offset is the root cause.

## Evidence

- **Pitch — `tune_011263`** (`pitch:2, measure_alignment:17, missing_in_croma:12`):
  all pitch diffs collapse into measure 1, where the body contains prose
  (`his song "Silent O Moyle".`) and a bare `"Slow"` annotation before the first
  note. The two engines segment that header/prose differently, shifting m1 event
  indices; the notes match when realigned.
- **Octave — same population**: 254 of 258 octave files overlap the pitch files;
  each is dominated by alignment/missing rows. (Croma's `middle=` and `K:`-line
  clef-octave handling were fixed in earlier phases; no systematic octave error
  remains.)
- **Harmony — `tune_000163`** (`"G"…"C"…"Em"…"Am"…"D7"…`): chord figures are
  identical in both engines (post the table-driven chord-symbol rewrite); the one
  row is `"G"@m11 off0` (Croma) vs `"D"@m10 off2` (reference) — the same symbol
  stream shifted by one measure, with a companion `missing_in_croma:1`.
- **Lyric — every file is heavily structural** (e.g. `tune_001361`:
  `missing:271, alignment:47, lyric:7`); the 2–7 lyric rows sit on the misaligned
  events.

## Verdict

**CASCADE_ARTIFACT.** Fix the structural root (which is itself an abc2xml
phantom-measure/`Z`-expansion artifact — see docs 02 and 03, justify-don't-fix)
and these vanish. No genuine pitch/octave/harmony/lyric bug in Croma.
