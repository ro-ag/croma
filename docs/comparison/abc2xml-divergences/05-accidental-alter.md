# 05 — Accidental: redundant `<alter>0>` serialization

**Affected:** 963 files (575 single-category). ~92% of single-category accidental
rows are this benign serialization difference; a small minority (~115 rows) are
grace-note accidental-scope edge cases.

## Cause

For a note whose effective pitch is **natural by carry-through** within a bar,
`abc2xml` serializes `<alter>0</alter>` while Croma omits `<alter>` entirely.
In MusicXML an absent `<alter>` defaults to 0 — the two are **semantically
identical** and render the same (a natural is printed once, where the source
marked it). The comparison keys on `pitch/alter`, so `0.0` (abc2xml) vs `None`
(Croma) surfaces as an "accidental" row even though nothing visible differs.

## ABC 2.1 basis

§11.3 (line 2050): the default `%%propagate-accidentals` value is `pitch`;
§6 (lines 2047–2049): "accidentals … apply to all the notes of the same pitch …
up to the end of the bar." So a repeated same-pitch note in the bar **inherits**
the accidental and needs no second printed symbol — which is exactly what both
tools do. §4.2 (line 823) defines `^`/`=`/`_`.

## abc2xml vs Croma

- **`tune_000127`** P0 measure 12, `c c "Bb7"=d3/d/`: the first `d` is marked
  natural; the second same-bar `d` carries it. abc2xml writes a redundant
  `<alter>0>` on the carried `d`; Croma omits it. Both print the natural symbol
  only once.
- **`tune_000424`** P2 (V:2) measure 1, `=AABc`: both print the natural on the
  first `A` only; on the second `A` abc2xml has `alter=0`, Croma has no `<alter>`
  — identical pitch, no second symbol either way.
- **`tune_007903`** measure 2, `({^c}d4) edcd`: genuine edge — abc2xml propagates
  the grace-note `{^c}` sharp onto the following bare `c`; Croma treats the bare
  `c` as natural. Grace-note accidental scope is unspecified by §4.12.

## Verdict

**ABC2XML_ARTIFACT (benign serialization).** Croma is correct — the outputs are
semantically equal. (Croma *could* emit `<alter>0>` on natural notes for closer
byte parity and to satisfy consumers that derive the accidental from `<alter>`;
this would erase ~575 files of false-positive rows but changes no rendering. It
is a cosmetic option, not a correctness fix.) The grace-accidental-scope cases
are a separate, spec-unspecified edge.
