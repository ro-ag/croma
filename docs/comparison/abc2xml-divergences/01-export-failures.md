# 01 — Export failures: tunes with no body music

**Affected:** 65 files (Croma returns a non-zero exit and emits no MusicXML).
62 carry the diagnostic `abc.file.no_music`; the rest fail for closely related
reasons. All audited examples are **header-only** tunes.

## Cause

The ABC source ends at the `K:` line (or contains only information fields,
stylesheet directives, and comments) with **no music-code line** afterwards.
There is no music to export, so Croma raises `abc.file.no_music` and produces no
score rather than fabricating an empty one.

## ABC 2.1 basis

§2.2.1 (lines 261–263):

> "It is legal to write an abc tune without a tune body. This feature can be used
> to document tunes without transcribing them."

Music code is (§2.2.1, lines 264–266) "any line which is not an information
field, stylesheet directive or comment line." The 65 tunes contain none.

## abc2xml vs Croma

- **abc2xml** emits a `<part>` containing a single empty `<measure number="1">`
  with only `<attributes>` and **zero `<note>`** elements — a part representing
  no music. In `tune_000906` it additionally misparses a stray trailing line
  (`T:18+4`, `7/24/2020`) into a spurious `<ending number="7">` plus bogus key
  and time changes inside that empty measure.
- **Croma** declines to emit a part with no music.

## Examples

| Tune | Header ends | Reference | Croma |
|---|---|---|---|
| `tune_000906` | `K:G`, no body | 1 empty measure (+ phantom `ending=7`) | export failure |
| `tune_001095` | `K:` then only `%` comments | 1 empty measure | export failure |
| `tune_002341` (Cage, *4'11''*) | `K:Abm`, no body | 1 empty measure (tempo+key only) | export failure |

## Verdict

**MALFORMED_INPUT.** A tune with no body has nothing to render; Croma's refusal
is spec-defensible and abc2xml's "empty measure of attributes" is a fabricated
measure of nothing. Not a Croma bug.
