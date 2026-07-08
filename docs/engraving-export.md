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

The rules are ported from [MuseScore](https://github.com/musescore/MuseScore)'s
engraving core so the output matches a mainstream editor's defaults:

- Beam grouping follows MuseScore's per-meter break table (`noteGroups[]` in
  `groups.cpp`) plus the `baseBeamMode`/`actualBeamMode` decision: a note breaks the
  beam at a strong beat, rests and quarter-or-longer notes are never beamed, and x/4
  meters adapt the break to the shortest note in the beat. Meters not in the table
  fall back to a full break at every denominator beat.
- Per-level `<beam>` text is the MuseScore `writeBeam` mapping from each note's
  previous/next beam counts, with the table's secondary-break codes fed in (so
  sub-beat 16th/32nd breaks are correct — an improvement over MuseScore's own
  MusicXML exporter, which leaves that as a TODO).
- Tuplet bracket follows `Tuplet::calcHasBracket` (the `AUTO_BRACKET` default).
- Stem direction is `Chord::computeUp` / `computeAutoStemDirection` (outermost-pair
  rule) with the `track % 2` voice-parity override; beam direction is the same rule
  over the whole beam's notes.
- Rest placement, slur placement, and tie orientation follow MuseScore's rest/slur/tie
  layout. Note MuseScore's *own* MusicXML exporter emits these only for hand-edited
  positions; croma emits the computed defaults, so a renderer that would otherwise
  guess gets a fully-specified document.

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
explicit-accidental preservation (croma emits the source's accidentals and MuseScore
has no automatic courtesy-accidental rule), so they are out of scope for this mode.
