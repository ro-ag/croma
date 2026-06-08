# 08 — Tuplet: bracket start/stop markers

**Affected:** 95 files (7 single-category).

## Cause

The tuplet **time-modification ratio** (e.g. actual 3 / normal 2) is identical
between the two engines in every row. The difference is only in the tuplet
**bracket start/stop notation markers**: Croma emits a tuplet with no visible
bracket marker (`type: null`) where abc2xml marks `start`/`stop`, or the two
place the start/stop on different members of a rest-containing tuplet.

## ABC 2.1 basis

§4.3 (tuplet notation) — the tuplet's *timing* (actual-in-the-time-of-normal) is
what the spec defines; bracket display is a typesetting detail. Durations remain
exact (§4.3 line 845). In every mismatch row the ratio (`actual=3, normal=2`)
agrees, so there is no timing error.

## abc2xml vs Croma

- **`tune_005185`** measure 21, `(3z/A/E/ (3z/B/E/ …` (rest-leading triplets):
  Croma `tuplets:[{actual:3, normal:2, type:null}]`; abc2xml marks
  `start`/`stop` per group. Ratio identical.
- **`tune_010459`** measure 14, `(3G,G,G,`: Croma marks `start` across events
  0–2; abc2xml marks `null/null/stop`. Bracket-placement edge only.

## Verdict

**ABC2XML_ARTIFACT / typesetting edge.** Tuplet ratios and note durations agree;
only bracket-marker placement differs. No genuine timing bug in Croma.
