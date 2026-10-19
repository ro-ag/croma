# croma `--engrave` module — interface contract

This contract fixes only the Rust boundary between croma's MusicXML writer and the
engraving-rules module, so the writer does not change. It deliberately says
nothing about which rules to apply: those come from the separate engraving spec.

Location: `crates/croma-core/src/musicxml/engrave/` with `mod.rs` declaring
`pub(crate) mod beam;` and `pub(crate) mod stem;`. Everything below is
`pub(crate)`. Pure functions, no I/O, no panics on any input (inputs can be
extreme: huge meter numerators, empty slices, unusual durations). Rust 1.96,
edition per the workspace; `croma-core` must stay zero-dependency.

## Units

- Time is in **ticks**: a quarter note = 480 ticks, a whole note = 1920.
- Staff positions are **diatonic steps**: `abs_step(step, octave)` =
  `7 * (octave + 1) + index`, with `index` C=0, D=1, E=2, F=3, G=4, A=5, B=6.
  Middle C (C4) = 35. `step` may be lowercase; treat it case-insensitively.
- A clef is given as its ABC name text (`Option<&str>`, `None` = treble). The
  staff it implies must be taken from
  `crate::musicxml::attributes::clef_model(clef) -> ClefModel` (fields
  `sign: &'static str` — `"G"`, `"F"`, `"C"`, `"percussion"` — and
  `line: &'static str` — the staff line `"1"`..`"5"` the clef sits on, counted
  from the bottom). Do not parse clef names yourself.

## `beam` module

```rust
pub(crate) struct BeamInput {
    pub rtick: i64,          // onset within the measure, ticks from the barline
    pub dur: i64,            // written duration, ticks
    pub beams: u8,           // flag count of the WRITTEN note type: eighth = 1,
                             // 16th = 2, 32nd = 3, ...; quarter or longer = 0
    pub is_rest: bool,
    pub tuplet_id: Option<u32>, // innermost tuplet this event belongs to, if any
}
// Derive Debug, Clone, Copy, PartialEq, Eq on all of these types.

pub(crate) struct BeamSegment { pub level: u8, pub text: BeamText }
// level 1 = the primary (eighth) beam, 2 = 16th beam, 3 = 32nd, ...

pub(crate) enum BeamText { Begin, Continue, End, ForwardHook, BackwardHook }
impl BeamText { pub(crate) fn as_str(self) -> &'static str }
// "begin", "continue", "end", "forward hook", "backward hook" (MusicXML <beam> text)

pub(crate) struct Meter { pub numerator: u32, pub denominator: u32 }
// The literal time signature (6/8 stays 6/8). `C` arrives as 4/4, `C|` as 2/2.
// Called only when the measure has a known meter.

pub(crate) struct BeamPlan {
    pub segments: Vec<Vec<BeamSegment>>, // one entry per input, same order;
                                         // empty = no <beam> on that note
    pub groups: Vec<Vec<usize>>,         // each beam group: indices into the
                                         // input slice, ascending, >= 2 members
}

pub(crate) fn plan(notes: &[BeamInput], meter: Meter) -> BeamPlan;
pub(crate) fn tuplet_shows_bracket(
    notes: &[BeamInput], groups: &[Vec<usize>], pair_id: u32,
) -> bool;
```

- `plan` gets one voice of one measure, in time order; chord members are not
  listed (a chord is one input). It decides which notes are beamed together and
  the `<beam>` elements for every level on every beamed note, MusicXML-valid:
  per level, a `begin` … `continue` … `end` run, or a single-note hook.
  `segments.len() == notes.len()` always; `groups` must be exactly the groups the
  segments describe.
- `tuplet_shows_bracket` says whether the tuplet `pair_id` (members = inputs with
  that `tuplet_id`) should print its bracket (`true`) or only its number
  (`false`), given the beam groups `plan` returned.

## `stem` module

```rust
pub(crate) fn abs_step(step: char, octave: i8) -> i32;
pub(crate) fn middle_distance(clef_text: Option<&str>, step: char, octave: i8) -> i32;
pub(crate) fn note_type_has_stem(note_type: &str) -> bool;
pub(crate) fn stem_up(
    own_distances: &[i32],
    beam_distances: Option<&[i32]>,
    multivoice: bool,
    voice_slot: usize,
) -> bool;
pub(crate) fn rest_display(clef_text: Option<&str>, voice_slot: usize) -> (char, i8);
pub(crate) fn slur_placement(stem_up: Option<bool>, multivoice: bool, voice_slot: usize) -> &'static str;
pub(crate) fn tie_orientation(stem_up: Option<bool>, multivoice: bool) -> &'static str;
```

- `middle_distance`: signed diatonic distance from the staff's middle line to the
  note; **positive = the note is below the middle line**, negative = above.
- `note_type_has_stem`: whether a MusicXML note type (`"whole"`, `"half"`,
  `"quarter"`, `"eighth"`, `"16th"` … `"1024th"`, `"breve"`, `"long"`, `"maxima"`)
  carries a stem.
- `stem_up`: the stem direction for one note or chord (`true` = up).
  `own_distances` = its notes' middle distances (one per chord member,
  non-empty). `beam_distances` = the middle distances of every note in the beam
  group it belongs to, or `None` when it is not beamed. `multivoice` = the staff
  carries more than one voice; `voice_slot` = this voice's 0-based index on the
  staff (0 = first/top voice).
- `rest_display`: for a rest in a multi-voice staff, the MusicXML
  `<display-step>`/`<display-octave>` (step letter uppercase, octave) where the
  rest is drawn.
- `slur_placement`: MusicXML `placement` for a slur starting on a note:
  `"above"` or `"below"`. `stem_up` is `None` when the note has no stem.
- `tie_orientation`: MusicXML tie `orientation`: `"over"` or `"under"`.

## Callers (for reference; do not change them)

`crates/croma-core/src/musicxml/note.rs` builds the inputs per voice/measure,
calls `plan`, writes `segments` as `<beam number=level>text</beam>`, uses
`groups` for `stem_up`'s `beam_distances` and for `tuplet_shows_bracket`, and
calls `rest_display` only on multi-voice staves.
`crates/croma-core/src/musicxml/notation.rs` calls `tie_orientation` and
`slur_placement`.
