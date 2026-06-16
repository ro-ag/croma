# MusicXML → Score reader (coverage & policy)

The reader inverts **croma's own writer** (`crates/croma-core/src/musicxml/`).
It is **feature-gated** behind `musicxml-reader` and **experimental** (like the
LSP) until it has corpus round-trip evidence comparable to the formatter's. The
default build never compiles it nor its sole optional dependency (`roxmltree`).

- Entry point: `croma_core::read_musicxml(xml: &str) -> ParseReport<Score>`
  (`#[cfg(feature = "musicxml-reader")]`).
- Design: [`superpowers/specs/2026-06-15-musicxml-reader-design.md`](superpowers/specs/2026-06-15-musicxml-reader-design.md).
- The writer is the spec. The reader inverts croma's dialect **exactly** and
  never mirrors an abc2xml-ism.

## Verification gate

The primary gate is **XML re-emission idempotence**:
`write(read(write(score))) == write(score)` as exact strings. The writer is
deterministic and lossy, so byte-equality of the re-emission is what proves the
reader recovered every writer-emitted fact (drops nothing, invents nothing) with
no second-spec projection to rot. Each stage adds elements; any element the
writer emits that the reader does not yet read makes the gate red, which *drives*
the next stage's work list.

- **Per-element unit tests** (`crates/croma-core/src/musicxml/read/mod_tests.rs`)
  assert idempotence on the supported subset AND a reconstructed model field
  directly.
- **Corpus measurement** (env-gated by `ABC_ROOT`, mirrors
  `croma-fmt/src/corpus_proof.rs`): walks the 10k corpus, runs the
  write→read→write loop per file, reports the idempotent count (strict full-byte
  and the S1-supported-subset view) and a histogram of the first diverging XML
  tag. It asserts no hard count yet — it reports, because most files use
  later-stage elements.

## Totality

`read_musicxml` is **total and non-panicking**. A malformed document yields a
minimal `Score` plus an error diagnostic; unknown elements are ignored (with an
optional warning). There is no `unwrap`/`expect`/`panic`/`todo` and no index that
can panic in the reader module tree. Unreconstructable `Span`s use the documented
sentinel `READER_SPAN` (= `Span::new(0, 0)`); ABC-only model state not present in
the XML (`preserved_directives`, voice clef/transpose text, `reference`) is left
at documented defaults and is invisible to the gate.

## Staging

| Stage | Scope | Status |
|---|---|---|
| **S1** | `<score-partwise>` → parts → measures → `<note>` (`<pitch>` step/octave/alter, `<rest>`, `<duration>`/`<type>`/`<dot>`, `<accidental>`), `<backup>`/`<forward>`, `<divisions>`, work-title/composer/credit metadata | **done** |
| **S2** | header `<attributes>`: `<divisions>`, `<key>`/`<fifths>` (+ explicit `<key-step>`/`<key-alter>`/`<key-accidental>`), `<time>` (incl. `symbol="common"`/`"cut"`, compound, free), `<clef>`, `<transpose>` → `midi_transpose`. Mid-measure `<attributes>` changes (key/meter/clef) deferred. | **done** |
| **S3** | `<part-list>` MIDI: `<midi-instrument>` (`<midi-channel>`, 1-based `<midi-program>`, `<volume>`, `<pan>`) → `MidiInstrumentModel` on the owning voice. **Closes the forward/reverse `%%MIDI` loop.** | **done** |
| **S4** | per-`<note>` `<notations>` + `<time-modification>`: `<tied>`/`<tie>` → ties, `<slur>` → slurs, `<tuplet>`+`<time-modification>` → tuplets, and the `<articulations>`/`<ornaments>`/`<technical>`/`<fermata>`/`<arpeggiate>` decoration groups → `DecorationAttachment`. Beams are derived (no model field, no `<beam>` emitted) and round-trip via S1. | **done** |
| **S5a** | `<direction>`: tempo `<metronome>`/`<words>`+`<sound>` → header `tempo_model` or mid-tune `TempoChange`; `<dynamics>` (`p`..`fff`,`mp`/`mf`/`sfz`), `<wedge>` (crescendo/diminuendo/stop), `<coda>`/`<segno>` → `DecorationAttachment`; annotation `<words>` (+placement) → `TextAttachment`. Trailing/pre-barline directions reconstruct on a zero-duration `Spacer`. | **done** |
| **S5b** | `<harmony>` (chord symbols) → `chord_symbols`; `<lyric>` (`<syllabic>`+`<text>`+`<extend>`) → `lyrics` (`Syllable`/`Hyphen`/`Extender`), all verses | **done** |
| S6 | multi-voice (`<voice>`/`<backup>`), repeats/endings/barlines, grace, chords (`<chord/>`) | planned |

### S1 reconstruction notes

- `<duration>` (an integer count of divisions) inverts to a reduced
  `Fraction::new(duration, 4 * divisions)`; `<type>`/`<dot>` are re-derived by
  the writer from that fraction, so reconstructing the rational value is exact
  and sufficient. `<divisions>` is read first because it scales every duration.
- Onsets are rebuilt with the same cursor the writer uses: `<forward>` advances
  it, `<backup>` rewinds it, and each non-chord note sits at the current cursor
  then advances it by its duration.
- A full-measure rest (`<rest measure="yes">`) is reconstructed by setting the
  measure's `expected_duration == actual_duration == rest.duration` at onset 0,
  which is exactly the writer's measure-rest predicate. Ordinary measures leave
  `expected_duration` unset so plain rests stay plain.
- Composer metadata reconstructs **every** `<creator type="composer">`,
  including a present-but-empty one (`<creator type="composer"></creator>`), and
  uses raw (untrimmed) text so the `<identification>` block round-trips byte for
  byte.
- An explicit `<accidental>` reconstructs the note's written accidental
  (`explicit = true`); absent `<accidental>` means none. `<alter>` always sets
  the sounding pitch.

### S2 reconstruction notes

- `<key>` is **score-level**: the writer emits an identical `<key>` in every
  part's first-measure `<attributes>` from `score.metadata.key`, so the reader
  reads the first part's header into `metadata.key` and re-emits it everywhere.
  `<fifths>` → `KeySignatureModel.fifths`; each consecutive
  `<key-step>`/`<key-alter>`/`<key-accidental>` triple → one
  `KeyAccidentalModel` (the `<key-accidental>` name is the exact inverse of
  `Accidental::musicxml_name`, with `<key-alter>` as a fallback). The
  `KeySignatureModel.display` string is **never emitted** by the writer, so it is
  left empty — the idempotence gate confirms `<key>` is fully driven by
  `fifths` + `explicit_accidentals`.
- `<time>` is score-level too. `symbol="common"` → `display = "C"`,
  `symbol="cut"` → `"C|"`; otherwise the `<beats>`/`<beat-type>` pairs are
  reassembled into `display` (joined with `+` when compound, e.g. `"3/8+2/8"`).
  An **absent** `<time>` (free meter `M:none`, or no meter) leaves
  `metadata.meter = None`; both `None` and a free meter re-emit nothing, so this
  is idempotent. `MeterModel.display` is the only field the writer reads;
  `duration`/`free_meter` get documented defaults.
- `<clef>` is **per-staff**: the writer reads the staff voice's
  `initial_properties.clef` (ABC text). The reader rebuilds a *canonical* clef
  text from `<sign>`/`<line>`/`<clef-octave-change>` that `clef_model` re-maps to
  the same element (`clef_model` is many-to-one, so a representative per
  `(sign,line)` plus an octave suffix `+8`/`-8`/`+15`/`-15` is sufficient).
  Plain treble (G/2, no octave change) reconstructs as `None`, matching a freshly
  lowered score (the writer emits the default `<clef>` either way). The original
  ABC clef string is unrecoverable but irrelevant to the gate.
- `<transpose><chromatic>n` → `voice.midi_transpose = Some(n)`. The ABC
  `transpose=` voice property is not in the XML; on re-write the writer falls
  through to `midi_transpose`, reproducing the element. Scoped per part (the
  writer emits one `<transpose>` per part from the first qualifying voice; S2
  reconstructs a single voice per part).
- **Deferred:** mid-measure `<attributes>` (a second `<attributes>` block from a
  `KeyChange`/`MeterChange`/`ClefChange` event) is **not** reconstructed in S2.
  Doing so requires synthesising the right `TimedEvent` at the exact onset routed
  through the writer's measure-sequence/overlay timeline; it is tracked as
  remaining (≈100 corpus files; first-diverging tag `attributes`).

### S3 reconstruction notes — the closed `%%MIDI` loop

S3 inverts `write_part_instruments`, recovering the `<part-list>` MIDI
projection that the forward `%%MIDI` translation (PRs #122–#125,
[`midi-directives.md`](midi-directives.md)) emits. This **closes the
forward/reverse loop**: a `%%MIDI program` / `program <chan> <prog>` /
`channel` / `control 7` / `control 10` directive — line-start **or** inline
`[I:MIDI=...]` — now survives `ABC → XML → Score → XML` byte-for-byte.

- The reader reads each `<score-part>`'s `<midi-instrument>` children into
  `MidiInstrumentModel`s and attaches them to the part's voices in order. The
  exact inverses (each the byte-for-byte inverse of the writer):
  - `<midi-channel>n` → `channel = n`;
  - `<midi-program>N` → `program = N − 1` (forward emits the **1-based**
    `program + 1`; a `<midi-program>0`, out of the writer's range, warns and is
    dropped);
  - `<volume>v` → `volume_cc = round(v × 1.27)` (forward wrote `{:.2}` of
    `cc / 1.27`);
  - `<pan>p` → `pan_cc = round((p + 90) × 127 / 180)` (forward wrote `{:.2}` of
    `cc / 127 × 180 − 90`).
- `<score-instrument>` / `<instrument-name>` is **not read back**: the name is
  *derived* on the forward side (the General MIDI name when a program exists,
  else the part name), so recovering `program` (or leaving it `None` for a
  standalone channel/volume/pan) regenerates the **identical** `<instrument-name>`
  on re-write. Storing the name would risk a second, drifting spec.
- **Float CC stability is proven exhaustively** (design §9): a unit test asserts
  `round(parse(format!("{:.2}", cc/1.27)) × 1.27) == cc` and the pan analogue for
  **every** `cc ∈ 0..=127`, so `<volume>`/`<pan>` are idempotent. The reader also
  clamps a hand-edited out-of-range float to `0..=127` with a diagnostic rather
  than panicking.
- **Multi-voice-per-part is deferred to S6.** When the writer emits more than one
  `<midi-instrument>` in a single `<score-part>` (two ABC voices in one part each
  carrying `%%MIDI`), S3 attaches the **first** to the single reconstructed voice
  and leaves the rest for the multi-voice stage. Such files do not round-trip yet
  (2 corpus files; first-diverging tag `score-instrument`).

### S4 reconstruction notes — `<notations>` + `<time-modification>`

S4 inverts `write_notations`, `write_time_modification`, the `<note>`-level
`<tie>`, and the `decoration_notation` name map. Only the four **model-driven**
notation classes are reconstructed; everything the writer *derives* needs no
reader code.

- **Ties.** The writer emits both a `<note>`-level `<tie type=...>` (no number)
  and a `<notations>/<tied type=... number=pair_id [line-type="dotted"]>` from the
  same `EventAttachments.ties` list. The reader rebuilds that single list from the
  richer `<tied>` (role ← `type`, `pair_id` ← `number`, `dotted` ←
  `line-type="dotted"`), which re-emits **both** elements identically; it falls
  back to the bare `<tie>` only for non-croma input that omits `<tied>`.
- **Slurs.** `<slur number=N type=...>` → `SlurAttachment` with **`pair_id = N`**,
  so the writer's `SlurNumbers::number_for` (whose `preferred = pair_id`)
  re-derives the same `number`. Overlapping/nested slurs carry distinct numbers,
  hence distinct `pair_id`s, and reproduce exactly (the nested-slur test pins
  outer = 1 / inner = 2).
- **Tuplets.** `<tuplet type="start|stop" number=N>` plus the note's
  `<time-modification>` ratio → `TupletAttachment`. Tuplet state is tracked across
  the measure (`OpenTuplets`): a `start` opens a tuplet whose `actual`/`normal`
  come from that note's `<time-modification>`; a middle note carrying **only** a
  `<time-modification>` (no `<tuplet>`) while a tuplet is open is a `Continue`; a
  `stop` closes the matching open tuplet (by `number`, LIFO fallback). Each tuplet
  gets a fresh per-measure `pair_id`; the writer's `sequence_tuplet_numbers`
  re-derives the MusicXML `number` from its active-set discipline (not from the
  `pair_id` value), so two separate tuplets that both re-emit as `number="1"`
  round-trip. For a single (non-nested) tuplet the note's `<time-modification>`
  equals that tuplet's own ratio, making the common case exact.
- **Derived time-modifications (no tuplet).** An odd duration like `C2/3` makes the
  writer **synthesise** a `<time-modification>` (e.g. 3:2) from the duration alone,
  with no `<tuplet>` and no `<notations>`. The reader creates **no** tuplet
  attachment here (no open tuplet) — S1's duration reconstruction already re-emits
  the identical `<time-modification>`. This is the same "derived, not stored"
  principle as beams.
- **Beams are derived, not stored.** The model has no beam field and croma's writer
  emits **no `<beam>` element at all** (beaming is implicit/recomputed from
  durations). Reading the S1 notes correctly therefore round-trips beaming with
  zero beam-specific reader code; a unit test pins that the writer emits no
  `<beam>` and consecutive eighths round-trip.
- **Decorations.** The grouped `<ornaments>`/`<technical>`/`<articulations>`
  children and the bare `<fermata>`/`<arpeggiate>` invert through
  `decoration_for_notation_element` (and `<fingering>N` → the `!0!`..`!5!`
  decoration) to **one canonical ABC decoration name** that re-emits the identical
  element via the writer's `decoration_notation`. Where the forward map is
  many-to-one (e.g. both `.` and `staccato` → `<staccato/>`), the reader picks the
  full `!name!` form with `DecorationSourceKind::Named` — the writer's notation map
  keys only on the decoration *name*, so `Named` re-emits byte-identically
  regardless of the original ABC shorthand. The writer re-groups decorations by
  category on output, so reconstructing the correct **set** suffices.
  - **Round-tripping decoration classes (S4):** articulations (`staccato`,
    `accent`, `tenuto`, `staccatissimo`, `strong-accent`, `breath-mark`, `scoop`),
    ornaments (`trill-mark`, `mordent`, `inverted-mordent`, `turn`,
    `inverted-turn`), technical (`up-bow`, `down-bow`, `open-string`,
    `thumb-position`, `snap-pizzicato`, `stopped`, `fingering` 0–5), `fermata`
    (upright/inverted), and `arpeggiate`.
  - **Handled by the `<direction>` writer (S5a, not S4):** dynamics (`p`..`fff`,
    `mp`/`mf`/`sfz`), hairpin wedges (`crescendo`/`diminuendo`), and `coda`/`segno`
    — the writer emits these as `<direction>` elements, **not** inside
    `<notations>`; the reader reconstructs them in S5a (see the S5a notes). The
    suppressed-decoration set (e.g. the Irish roll `~`) emits nothing on either
    side and needs no inverse.

### S5a reconstruction notes — `<direction>`

S5a inverts `write_initial_directions`, `write_tempo_direction`,
`write_harmony_and_directions`, `write_direction_words`, `write_dynamic`,
`write_direction_type` (coda/segno) and `write_wedge`. The writer emits a (timed)
event's `<direction>`s **immediately before** the event (`write_event` calls
`write_harmony_and_directions` first), so the reader buffers each voice-bearing
direction and flushes the buffer onto the **next** timed event (note, rest, or
`TempoChange`).

- **Tempo** directions are **voice-less** and always carry a `<sound tempo=...>`,
  which is exactly what distinguishes them from a voice-bearing annotation
  `<words>` direction:
  - *numeric* (`<metronome>`): `<beat-unit>` (+ optional `<beat-unit-dot/>`) →
    `TempoBeat` (plain unit `1/d`; dotted `3/(2d)`, the exact inverse of the
    writer's `3/(2^k)` → dotted), `<per-minute>` → `bpm`, the non-metronome
    `<words>` → `tempo.text`.
  - *text-only* (no `<metronome>`, just `<words>` + `<sound>`): reconstructs a
    `TempoModel { text: Some(..), beat: None }` so the writer re-emits the words
    plus the default `<sound tempo="120.00"/>`.
  - **Header vs mid-tune:** the first voice-less tempo direction **before part 1's
    first note** is the score header `tempo_model` (`write_initial_directions`);
    every other tempo direction becomes a mid-tune `TimedEventKind::TempoChange`
    at its onset. A `TempoChange` carries any directions buffered just before it
    (the writer emits an event's attachments before its metronome).
- **Dynamics / coda / segno / wedge** → `DecorationAttachment`, inverting each
  element-name map exactly. The reconstructed **canonical** name re-emits the
  identical element regardless of the original ABC shorthand: `crescendo`/
  `diminuendo` → `crescendo(`/`diminuendo(`, `<wedge type="stop">` → `crescendo)`
  (the writer maps every close form to `stop`), each `<dynamics>` child → its
  same-named ABC dynamic, `<coda>`/`<segno>` → `coda`/`segno`.
- **Annotation `<words>`** → `TextAttachment`. The writer strips a leading
  placement prefix (`^`/`_`/`<`/`>`/`@`) from the model text when the annotation
  has a placement, then prints the bare words + a `placement="above"|"below"`
  attribute. The inverse rebuilds the model text by re-attaching the **canonical**
  prefix for that placement (`^` above, `_` below) so the writer's `annotation_text`
  strips it back to the same words — byte-identical even when the words themselves
  begin with a prefix character. `placement="above"` is the canonical inverse for
  the writer's collapse of left/right/free onto `above` (all print as `above` with
  the prefix stripped). A direction with no `placement` attribute reconstructs a
  placement-less annotation whose text is the words verbatim.
- **Trailing / pre-barline directions** (a `!segno!` before a bar line, or a
  note-less measure carrying only an annotation) have no following note; the
  writer flushes them onto a zero-duration `Spacer` whose `write_event` emits the
  directions then nothing. The reader reconstructs that `Spacer` at the end of the
  measure so the directions re-emit in place.
- **Writer-derived, no reader code:** the `<sound>` tempo value, the `<voice>`/
  `<staff>` routing, and the per-category re-grouping are all derived by the
  writer from the reconstructed fields, so recovering the model field reproduces
  them. Anything with no model-backed inverse (rehearsal marks, pedal, …) is left
  unread with a diagnostic, never an invented mapping.

### S5b reconstruction notes — `<harmony>` + `<lyric>`

S5b inverts `write_harmony` (`harmony.rs`) and `write_lyrics` + `syllabic_for_lyric`
(`lyric.rs`). Like S5a directions, a chord symbol's `<harmony>` is emitted **before**
its event (in `write_harmony_and_directions`, chord symbols first, then annotations,
then decorations), so the reader buffers `<harmony>` onto the same `pending`
attachments that S5a uses and flushes them — `chord_symbols` first — onto the next
event. `<lyric>` lives **inside** the `<note>` (emitted last by `write_note`), so it
is read in `read_note_attachments`.

- **Harmony.** The writer derives the entire `<harmony>` tree (`<root>`/`<root-alter>`,
  `<kind text="…">value</kind>`, optional `<bass>`, `<degree>`s) from the ABC
  chord-symbol *string* — and crucially preserves that exact original string as the
  **`<kind text="…">` attribute**. The reader therefore reconstructs the
  `TextAttachment.text` **directly from `text`**, with **no** kind-value→suffix
  inversion: re-parsing the same string through the writer's `parse_chord_symbol`
  reproduces the identical `<root>`/`<kind>`/`<bass>`/`<degree>` tree byte-for-byte.
  This round-trips every chord shape the writer emits — majors (`C`), minors (`Dm`),
  sevenths (`Cmaj7`), slash/bass (`G7/B`), altered roots (`F#m7b5`), and added
  degrees (`C7b9`) — by construction. A non-chord string (e.g. `Cadd9`, `NC`) is
  **not** emitted as `<harmony>` at all; the writer demotes it to a
  `<direction><words>`, which the S5a direction reader already round-trips (as an
  `annotation` rather than a `chord_symbol` — a valid-but-different `Score` that
  re-writes identically, which the gate accepts by design §9). A `<kind>` lacking a
  `text` attribute is not croma's output and has no recoverable ABC source, so it is
  skipped with a diagnostic rather than inventing a spelling from the kind value.
  **No remaining harmony shapes** — `harmony` is fully cleared from the corpus
  first-divergence histogram.
- **Lyrics.** Each `<lyric number=N>` → `verse = N`. The writer's syllabic state
  machine emits `single`/`begin`/`middle`/`end` from the note's **model** lyrics:
  it emits a `begin`/`middle` exactly when the note's lyrics contain a trailing
  `Hyphen` (its `continues` test) and `single`/`end` when they do not, tracking an
  "open hyphen" across notes. The reader inverts this **locally, with no cross-note
  state**, because the per-note encoding fully determines the writer's output:
  - `<syllabic>` + `<text>` → a `LyricControl::Syllable` carrying the text, **plus**
    a trailing `LyricControl::Hyphen` on the **same note** iff the syllabic is
    `begin` or `middle`. (`single`/`end` reconstruct a lone `Syllable`.) The
    re-derived open-hyphen state then reproduces the exact syllabic on re-write.
  - `<extend/>` (no syllabic/text) → a `LyricControl::Extender` with empty text.
  - The `<text>` content is read **raw (untrimmed)**: the writer emits `lyric.text`
    verbatim, and the corpus has syllables with a trailing space (e.g. the `~` lyric
    space lowers to `<text>la </text>`), so trimming would break the round-trip.
  - Verses are pushed in document order, reproducing the writer's per-note
    `verse` emission order. The writer never emits a `<lyric>` for a `Skip` or a
    standalone `Hyphen`, so neither is reconstructed except as the trailing companion
    of a begin/middle syllable above. **No remaining lyric cases** — `lyric` is fully
    cleared from the corpus histogram.

### S1/S2/S3/S4/S5a/S5b corpus metric (10k zenodo set)

- **S1** strict full-byte idempotence was **0 / 9,935** exported files (every ABC
  tune carries a `K:`, so a `<key>` block S1 did not read always diverged first);
  the S1-supported-subset view (deferred `<key>`/`<time>`/`<score-instrument>`/
  `<midi-instrument>` stripped) was **483 / 9,935**.
- **S2** strict full-byte idempotence: **483 / 9,935** — S2 clears the `key`
  divergence entirely (the strict count now equals the old S1-supported-subset
  number, confirming S2 reconstructs the whole header `<attributes>` for every
  file previously blocked only by key/time).
- **S3** strict full-byte idempotence: **483 / 9,935** — **unchanged**, and this
  is expected and honest. The strict count is monotonic but a file is counted
  only when the reader handles *every* element its export uses; nearly every file
  with a part-list instrument also uses `barline`/`direction` (S5/S6), so closing
  the MIDI loop moves their first-divergence forward without making them
  idempotent. S3's value is the **closed loop**, not the count: the
  `score-instrument` first-diverging tag collapses from **426 (S2) → 2 (S3)** —
  S3 eliminates the part-list MIDI as a first divergence for 424 files. The 2
  residual `score-instrument` files are multi-voice-per-part (two
  `<midi-instrument>` in one `<score-part>`), deferred to S6 (see the S3 notes).
  The new top first-diverging tags are `barline` (~3,419, S6), `direction`
  (~3,408, S5), `notations` (~1,257, S4), `harmony` (~796, S5), then `lyric`,
  `tie`, `grace` (~112), `attributes` (~101, deferred mid-measure changes), and
  `note` (6). These name the next stages' work lists.
- **S4** strict full-byte idempotence: **541 / 9,935** (**+58** vs S3's 483). More
  importantly, the gate's self-policing signal fires exactly as the design
  predicts: the **`notations` first-diverging tag collapses from ~1,257 (S3) → 0**
  — S4 fully reads back every `<notations>`/`<time-modification>` wherever it was
  the first divergence. The modest strict delta is honest: most files that use
  notations also use `barline`/`direction`, which diverge earlier and mask the
  fix. The new top first-diverging tags are `barline` (4,609, S6), `direction`
  (3,467, S5), `harmony` (809, S5), `grace` (203, S6), `lyric` (190, S5),
  `attributes` (107, deferred mid-measure changes), and `note` (6). The 6 `note`
  files are heavily multi-voice (`<note>` vs `<backup>` ordering) — pre-existing
  multi-voice incompleteness now surfaced as the first divergence; it is **S6
  scope** (multi-voice `<voice>`/`<backup>`), unrelated to S4.
- **S5a** strict full-byte idempotence: **606 / 9,935** (**+65** vs S4's 541). The
  self-policing signal fires exactly as designed: the **`direction` first-diverging
  tag collapses from 3,467 (S4) → 0**, and the text-only-tempo `sound` tag (190,
  which appeared transiently mid-stage) is also fully cleared — S5a reads back
  every `<direction>` wherever it was the first divergence. The strict delta is
  honest: most files using directions also use `barline`/`harmony`/`lyric`, which
  diverge earlier and mask the fix. The new top first-diverging tags are `barline`
  (7,051, S6), `harmony` (1,137, S5b), `lyric` (505, S5b), `grace` (440, S6),
  `attributes` (181, deferred mid-measure changes), `note` (12, S6 multi-voice),
  `score-instrument` (2, S6 multi-voice) and `voice` (1, S6). These name S5b/S6's
  work lists.
- **S5b** strict full-byte idempotence: **635 / 9,935** (**+29** vs S5a's 606). The
  self-policing signal fires exactly as designed: the **`harmony` (1,137 → 0) and
  `lyric` (505 → 0) first-diverging tags collapse entirely** — S5b reads back every
  `<harmony>` and `<lyric>` wherever it was the first divergence. The strict delta is
  honest and the same masking effect as prior stages: most files using chord symbols
  or lyrics also contain a `barline` that diverges earlier (an S6 concern), so
  clearing harmony/lyric pushes their first divergence forward to `barline` rather
  than making them fully idempotent — visible as `barline` rising from 7,051 → 8,573
  as it absorbs those files. The new top first-diverging tags are `barline` (8,573,
  S6), `grace` (493, S6), `attributes` (207, deferred mid-measure changes), `note`
  (13, S6 multi-voice), `direction` (11, **S6 multi-voice** — these are `<backup>`
  files whose second voice shifts the measure, surfacing at a `<direction>` line, not
  an S5 regression), `score-instrument` (2, S6) and `voice` (1, S6). With harmony and
  lyric cleared, **`barline`/structure is the dominant remaining work, owned by S6.**

Re-run the measurement with:

```sh
ABC_ROOT=docs/untracked/corpus/zenodo-10k/abc \
  cargo test -p croma-core --release --features musicxml-reader \
  corpus_idempotence_measurement -- --nocapture
```
