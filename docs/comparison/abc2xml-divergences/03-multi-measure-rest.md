# 03 — Multi-measure rest `Z`/`X` expansion

**Affected:** every tune containing a standalone `Zn`/`Xn` (n > 1), e.g.
`tune_010751`, `tune_009310`, `tune_013477`. Contributes to the measure-count /
missing / extra cascade in those files.

## Cause

`abc2xml` **expands** `Zn` into *n* separate whole-measure rest measures; Croma
keeps **one** measure holding an *n*-bar rest.

## ABC 2.1 basis

§4.5 (lines 866–873):

> "Multi-measure rests are notated using Z (upper case) followed by the number of
> measures." … and the collapsed and expanded forms are *"musically equivalent
> (although they are typeset differently)"* — `Z4|CD EF` ≡ `z4|z4|z4|z4|CD EF`.

The spec explicitly calls the two forms equivalent, so this is a representation
choice, not a correctness difference.

## abc2xml vs Croma

- **`tune_010751`** (M:2/4, body `… Z8 :| …`): reference **41** measures, Croma
  **34** (Δ = 7 = `Z8` → 8 measures vs 1). Reference measures 13–20 are each a
  separate `<rest measure="yes"/>` (and abc2xml does **not** use the proper
  MusicXML `<multiple-rest>` element). Croma keeps a single measure 13 with one
  `<rest>` of `<duration>128</duration>` (`<type>breve</type>`).

## Verdict

**ABC2XML_ARTIFACT / representation choice.** Both forms are spec-permitted
("musically equivalent"); Croma's single-measure form is the more compact and
matches the `Z` semantics directly. Not a Croma bug.
