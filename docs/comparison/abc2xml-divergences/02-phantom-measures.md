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

## Phase 47 comparator status

Phase 47 normalized the confirmed abc2xml harmony-leading phantom-measure
artifact. When the reference MusicXML starts a part with a zero-note measure
that carries harmony, and Croma's corresponding first measure already contains
musical notes, the comparator drops the reference phantom before extracting
structural facts. Retained measures are numbered canonically, and harmony,
direction, and repeat-ending context measure values are remapped to the retained
measure numbers.

This is deliberately pair-aware. A leading Croma direction-only measure, or a
case where both sides start with an empty measure, is not dropped. The full 10k
residual table dropped from 7,232 to 6,433 `measure_alignment` rows and from
1,211 to 901 `harmony` rows, resolving 28 files with no file-level regressions.
