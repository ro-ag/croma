# croma fix candidates (surfaced by divergence triage)

Real **croma** bugs found while triaging the raw-comparator worklist. These files
are **kept** in the worklist (not dropped) — they are genuine croma defects and
graduate into `whitelist.csv` once the parser fix lands. Each was confirmed by the
`abc-divergence-investigator` reasoning from the ABC 2.1 spec, with an adversarial
"find a croma error" pass where noted.

Investigated 2026-06-13 (accidental content category, single-category files).

## Bug 1 — accidental dropped on the malformed `^/` token (`^/c`)

When an accidental symbol (`^` `=` `_`) is immediately followed by a `/`
length-divider **before** the note (e.g. `^/c`, `a^/ge`), croma discards the
accidental entirely and emits `warning[abc.music.malformed_accidental]`
("Accidentals must appear immediately before a note") + `warning[abc.music.malformed_length]`.
The note then sounds natural. abc2xml leniently recovers: it applies the
accidental to the following note (treating the `/` as a length) and propagates it
in-bar per §11.3.

- **Spec:** ABC 2.1 §4.2 (KB raw line 855) — `^`/`=`/`_` notate sharp/natural/flat;
  §4.20 construct order is `<accidental><note><octave><length>`, so `/` belongs
  *after* the note. The token is malformed, but the author's intent (and the
  parallel well-formed `^c` bars in the same tunes) is unambiguously a sharp.
- **Fix direction:** recover the accidental on `^/`-style tokens (apply it to the
  following note, treat the stray `/` as the length divider) instead of dropping it.
- **Manifest files (sounding pitch wrong — `croma 0 / abc2xml 1`):**
  `tune_001009.abc` (`^/c`), `tune_003353.abc` (`^/c2`, also see Bug 3),
  `tune_002562.abc` (`a^/ge`), `tune_001875.abc` (8 notes).
- **Latent (masked by key signature — pitch right, only abc2xml's display glyph
  differs; those files are dropped as equivalence):** `tune_002913.abc` (`^/F`),
  `tune_003294.abc`, `tune_003427.abc`, etc.

## Bug 2 — explicit/non-traditional key signature `K:<tonic> exp <accidentals>`

croma does not apply explicit key signatures. For `[K:D exp _B^g]` croma emits
`<key><fifths>0</fifths>` paired with an **incomplete** `<key-step>`/`<key-alter>`
list (only 1 of the 2–3 declared accidentals). music21 reads `<fifths>0>` +
`<key-step>` as `altered=[]`, so **none** of the explicit accidentals apply and
every affected note comes out natural.

- **Spec:** ABC 2.1 §3.1.14 (KB raw line 688) — "`K:<tonic> exp <accidentals>` …
  explicitly define all the accidentals of a key signature. Thus `K:D Phr ^f`
  could also be notated as `K:D exp _b _e ^f`." The `exp` list defines the full
  key signature and applies like one.
- **Fix direction:** emit all the `exp` accidentals as the key signature (full
  `<key-step>`/`<key-alter>` list that music21 reads as a non-traditional key, or
  the correct `<fifths>` when expressible) so they apply to every matching note.
- **Manifest files:** `tune_003838.abc` (65 rows), `tune_003836.abc` (65 rows,
  sibling). ~50/65 rows per file are F♯/G♯/E♭ dropped; the remaining C♯ rows are
  abc2xml over-reaching but do not exonerate croma on the dominant failure.

## Bug 3 — `N:` header lines balloon the measure structure (secondary)

In `tune_003353.abc`, 18 `N:` annotation lines (indented free text, one blank
`N:`) cause croma to balloon from 18 to 19 garbled measures with wrong notes.
Stripping the `N:` lines restores correct measure alignment. Needs its own
investigation; surfaced here because it co-occurs with Bug 1 in that file.
