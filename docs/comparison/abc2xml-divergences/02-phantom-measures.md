# 02 — Phantom empty measures at annotation / section boundaries

**Affected:** 238 files where Croma emits **fewer measures** than the reference.
This is the dominant driver of the `missing_in_croma`, `extra_in_croma`, and
`measure_alignment` categories: a one-measure offset cascades into thousands of
positionally-misaligned rows downstream.

## Cause

`abc2xml` inserts a spurious **empty (zero-note) `<measure>`** whenever a
standalone annotation, part label, or inline key/clef change sits at a bar
boundary — e.g. a line that is only `"Trio"[K:G]|:` or a part label `"A"\`.
The annotation/key gets its own measure and the real music begins in the next
one. Croma attaches such a boundary to the **following** real measure and emits
no empty measure.

## ABC 2.1 basis

§2.2.1 (lines 264–266) defines music code as notes, bar lines and symbols; an
annotation string (§4.19) and an inline information field (§4.16) are **not**
music. §4.8 lists bar-line tokens — a bar line marks a boundary, it does not by
itself constitute a measure of music. Nothing in the spec creates a measure from
an annotation or a key change.

## abc2xml vs Croma

- **`tune_003837`** (parts A–H, each introduced by `"A"\` … `"H"\`): reference
  **78** measures, Croma **70** (Δ = 8). The 8 surplus reference measures are
  exactly the zero-note measures at the part boundaries — reference measures
  **1, 10, 19, 35, 45, 51, 57, 70** each have `<note>` count 0 (measure 45 also
  carries the `[K:D _B ^g]` inline key, measure 70 the `"H"` words). Every
  note-bearing measure matches one-to-one.
- **`tune_000289`** (`"Trio"[K:G]|:` mid-tune): reference **33** measures, Croma
  **32** (Δ = 1). Reference **measure 17** has 0 notes and carries
  `<words>Trio</words>`; Croma's measure 17 is the real first Trio bar
  (`b>b bb bb` → five `B` notes with a forward-repeat left barline).

## Verdict

**ABC2XML_ARTIFACT.** A bar line or annotation does not create a measure of
music; Croma's folded output is the more correct MusicXML. Justify, do not fix.

> Related but separate: in `tune_000289` Croma also does not apply the inline
> `[K:G]` key change at that boundary (keeps `fifths=2`). That is a distinct
> key-change-handling item, not the phantom-measure artifact.
