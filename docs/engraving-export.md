# Engraving export (`croma xml --engrave`)

By default `croma xml` emits MusicXML tuned for a **byte-for-byte round trip** — the
exact inverse of the reader, with no derived engraving detail. That output carries no
`<beam>` elements and a bare `<tuplet>`, so a renderer (Verovio, MuseScore, …) has to
re-derive beaming and tuplet display from its own heuristics, often engraving worse
than the source editor would.

`--engrave` opts into a **render-oriented** profile that computes those hints from
information croma already holds (meter + note durations) and writes them explicitly.
It is strictly additive: the default output is unchanged, so the round-trip, reader
self-loop, and formatter gates are untouched.

```sh
croma xml --engrave tune.abc > tune.musicxml
```

## What it emits

- **Beam grouping** — `<beam number="N">begin|continue|end|forward hook|backward hook</beam>`
  computed per voice/measure from the time signature and note durations. Secondary
  (16th, 32nd, …) sub-beams and partial-beam hooks are included.
- **Default tuplet display** — for a tuplet whose ABC source carried no explicit
  display directive, `<tuplet>` gains a convention-default `bracket`: **hidden**
  (number only) for a self-contained fully-beamed tuplet, **shown** otherwise. An
  explicit source directive is still honoured verbatim.
- **Stem direction** — `<stem>up|down</stem>` from the note/chord position relative to
  the clef's middle line (the note furthest from the line drives a chord; a beam shares
  one direction), with voice parity on multi-voice staves. Whole notes and longer get
  no stem.
- **Multi-voice rest placement** — a rest on a staff shared by several voices gains
  `<display-step>`/`<display-octave>` so the upper voice's rest sits above the middle
  line and the lower voice's below, instead of colliding on it.
- **Slur placement** — `<slur placement="above|below">` opposite the note's stem, or
  by voice on multi-voice staves.
- **Tie orientation** — `<tied orientation="over|under">` opposite the stem, or on the
  stem side in multiple voices.

Every hint is individually toggleable via [`MusicXmlWriteOptions`]; `--engrave` turns
them all on.

## Algorithm

The rules are croma's own, written from published engraving references (mainly
Elaine Gould, *Behind Bars*) and the MusicXML 4.0 reference. They are specified,
with a citation for each rule, in [`engraving/rules.md`](engraving/rules.md), and
the module boundary is fixed by [`engraving/contract.md`](engraving/contract.md).
The module was written clean-room from that specification, without reference to
any notation program's code.

- **Beam grouping** follows the beat: the meter's beat (a dotted value in compound
  meters, groups of two and three in irregular x/8 meters) decides where beams
  break; eighths may join across beats in 2/4 and 3/4 bars and in 4/4 half-bars,
  never across the middle of a 4/4 bar; rests and quarter-or-longer notes are never
  beamed; secondary beams break at the beat's subdivisions; a lone shorter note in
  a group gets a hook pointing to the note it belongs with.
- **Tuplet bracket:** number only when the tuplet is exactly one beam group,
  bracketed otherwise.
- **Stem direction:** below the middle line up, on or above it down; for a chord or
  a beam group the note farthest from the middle line decides; on a staff with
  several voices, the first voice's stems go up and the second's down.
- **Rests, slurs and ties:** on multi-voice staves each voice's rests move to its
  side of the staff and slurs/ties follow the voice; in a single voice, slurs and
  ties go on the notehead side, opposite the stem.

## Library API

```rust
use croma_core::{export_musicxml_with_options, ExportOptions, MusicXmlWriteOptions};

// Umbrella: every engraving hint on.
let opts = ExportOptions::default().engrave();

// Or per-hint control:
let opts = ExportOptions {
    write: MusicXmlWriteOptions { beams: true, ..Default::default() },
    ..Default::default()
};

let xml = export_musicxml_with_options(source, opts)?.musicxml;
```

`MusicXmlWriteOptions::default()` (all hints off) reproduces `write_musicxml` /
`croma xml` byte-for-byte.

## Scope

Shipped: beam grouping, default tuplet display, stem direction, multi-voice rest
placement, and slur/tie orientation. Accidentals are already handled by the writer's
explicit-accidental preservation (croma emits the source's accidentals and adds no
courtesy accidentals), so they are out of scope for this mode.
