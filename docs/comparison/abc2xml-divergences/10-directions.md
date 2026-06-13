# 10 — Direction residuals (tempo / annotation text)

**Affected:** 291 files (24 single-category). Mixed residual after the `Q:`-tempo,
decoration, and `%%MIDI` direction fixes (see `docs/progress/`).

## Sub-causes

1. **Text-only tempo, fabricated BPM.** `Q:"Allegretto"` carries no numeric
   tempo, so a playback BPM must be invented. `tune_003833`: Croma emits
   `MetronomeMark Quarter=120`, abc2xml `Quarter=112`. ABC 2.1 §3.1.8 gives no
   BPM here, so neither value is "wrong".
2. **Malformed `Q:` field.** `tune_006535`, `Q:1/4=1/4=160` (two `=` tokens):
   Croma preserves the literal text; abc2xml parses a partial `"1"`. Malformed
   input.
3. **Annotation text with punctuation.** `tune_007910`, `" >"G2`: the annotation
   is a space + `>`. Croma preserves the text `>` (a `"..."` annotation per
   §4.19); abc2xml emits an empty `TextExpression`. Croma is correct.

## ABC 2.1 basis

§3.1.8 (`Q:` tempo — may be text, numeric, or both); §4.19 (annotations: a
quoted string is printed text). Where the field is text-only or malformed the
spec fixes no playback tempo, so a default-BPM difference is not a correctness
issue.

## Verdict

Mixed: **ABC2XML_ARTIFACT** (drops annotation text — `tune_007910`) /
**MALFORMED_INPUT** (`tune_006535`) / **default-tempo difference** (neither wrong
— `tune_003833`). No genuine Croma direction bug.

## Phase 43 comparator status

Phase 43 stopped counting the default-tempo sub-cause as a structural mismatch
by normalizing music21 `MetronomeMark` facts whose text is explicitly marked
`(playback only)`. This removes the 26 residual playback-only BPM rows from the
phase-42 full 10k table without changing Croma MusicXML export behavior.

After that comparison-policy change, the full residual table has 408
direction-component rows across 126 files, with 258 rows categorized as
`direction`. Remaining rows still need case-by-case triage against malformed
input recovery, source-text preservation, harmony/chord-symbol placement policy,
and positional cascades.
