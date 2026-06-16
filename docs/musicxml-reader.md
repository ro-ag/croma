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

## CLI surface (R1, gated)

Built with `cargo build -p croma-cli --features musicxml-reader`, the reader is
reachable from two subcommands (both `#[cfg(feature = "musicxml-reader")]`;
absent from the default zero-dep build):

- `croma read <file.musicxml> [-o out] [--format xml|abc|dump]` — read XML →
  `Score`, print reader diagnostics to stderr, project per `--format` (default
  `xml` = `write_musicxml`, `abc` = `write_abc`, `dump` = the `Score` debug).
  `--format xml` is the **pure inverse** `write_musicxml(read_musicxml(xml))`
  (used by the reverse music21 comparator, R2).
- `croma musicxml2abc <file.musicxml> [-o out.abc]` — read XML → `Score` → ABC
  (`= read --format abc`), the headline conversion.

The croma-cli feature `musicxml-reader = ["croma-core/musicxml-reader"]` pulls
`roxmltree` only when enabled; the default `cargo build -p croma-cli` stays
dep-free and exposes neither subcommand.

### ABC projection completion (`complete_score_for_abc`)

The reader was built+proven as the inverse of **`write_musicxml`**, so it
populates that writer's view (per-`<measure>` boundaries + `Measure.barlines`).
`write_abc` consumes a *different* slice of `Score` (`TimedEventKind::Barline`
events in `voice.events`, `KeySignatureModel.display`). The `--format abc` /
`musicxml2abc` paths therefore run a gated, **ABC-path-only** completion pass
(`read::complete_score_for_abc`) that synthesizes the `voice.events`
barline/ending events from the reconstructed measure structure, fills a
canonical-major `K:` display from `<fifths>`, and renumbers tuplet `pair_id`s
globally-unique-per-voice (the reader numbers them per-measure for
`write_musicxml`, but `write_abc`'s `tuplet_layout` groups globally). The pass is
**never** applied on `--format xml` nor in the XML idempotence gate, so it
cannot perturb the write_musicxml inverse.

**Structural round-trip evidence** (`tools/prove_reader_abc_roundtrip.py`,
LOCAL-ONLY: `croma xml`→X1, `croma read X1 --format abc`→ABC', `croma xml ABC'`
→X2, compare the normalized musical projection X1≡X2): **9,514 / 9,933 in-scope
round-trip structurally (95.8%)**. The 419 residual is categorized (empty-measure
/`y`-spacer length, multi-voice `V:`-vs-`&`, key/barline ordering, slur-drop) and
logged for the consolidated residual pass — a valid-but-different Score that the
lossy XML intermediate cannot always render back to byte-faithful ABC.

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

`read_musicxml` is **total and non-panicking** (design §2.2 / §6). A malformed
document yields a minimal `Score` plus an error diagnostic; unknown elements are
ignored (with an optional warning). There is no
`unwrap`/`expect`/`panic`/`todo`/`unreachable`/`debug_assert` and no index that
can panic anywhere in the reader module tree — **not even in debug/test builds**
(S6e removed the last `debug_assert!`, the grace-drain invariant guard, in favour
of a graceful degrade: an orphaned before-grace run is re-bound to the most recent
main event, or dropped with a diagnostic when there is no host — never a panic).
Unreconstructable `Span`s use the documented sentinel `READER_SPAN`
(= `Span::new(0, 0)`); ABC-only model state not present in the XML
(`preserved_directives`, voice clef/transpose text, `reference`) is left at
documented defaults and is invisible to the gate.

### Totality fuzz evidence (S6e)

A feature-gated fuzz test (`totality_fuzz_read_musicxml_never_panics`) asserts the
reader **never panics** across three arms, each `read_musicxml` call wrapped in
`std::panic::catch_unwind` so a panic becomes a named test failure, not an abort:

- **croma's own 10k exports** (via `ABC_ROOT`): every corpus ABC that exports
  cleanly is fed back through the reader. **9,935 files exercised, 0 panics.**
- **abc2xml reference set** (via `REF_ROOT`): the raw bytes of every reference
  `.musicxml`/`.xml` — the abc2xml/music21 dialect, full of elements croma's
  writer never emits — are read directly. **10,000 files exercised, 0 panics.**
  (The arm reports "UNAVAILABLE" and is skipped if `REF_ROOT`/the directory is
  absent.)
- **hand-crafted malformed inputs** (always run): truncated/ill-formed XML, a
  non-MusicXML or timewise root, out-of-range numbers everywhere the reader parses
  one, an empty-but-valid score, unknown elements, and the S6e grace fallback.
  **11 cases, 0 panics**; the genuinely-malformed subset also each surfaces a
  diagnostic.

Run it with both corpora:

```sh
ABC_ROOT=/abs/path/to/abc REF_ROOT=/abs/path/to/musicxml \
  cargo test -p croma-core --release --features musicxml-reader \
  totality_fuzz_read_musicxml_never_panics -- --nocapture
```

## Reference-dialect reading & parity (R2)

The self-loop (XML re-emission idempotence) proves the reader inverts croma's
**own** writer, but never exercises **foreign** MusicXML. R2 measures reading the
abc2xml/music21 dialect: `tools/musicxml_reverse_corpus_compare.py` runs each
abc2xml-reference file through `croma read --format xml` (the **pure inverse**)
and feeds the re-export + the original into the existing **music21 semantic
comparator** (`tools/music21_polars_corpus_compare.py`, unchanged) — cross-writer
semantic parity, not byte idempotence. (LOCAL-ONLY; corpus is external.)

**Three clean reader bugs the self-loop could never surface** (each gated,
read-side only, forward + self-loop idempotence untouched at 9915/9935):

1. **DTD-tolerant parse.** Every real-world MusicXML file (abc2xml, MuseScore,
   Finale, Sibelius) carries a `<!DOCTYPE … MusicXML … Partwise>`. roxmltree's
   default `allow_dtd:false` rejected **all 10,000** at the door → empty Score.
   croma's own writer emits no doctype, so the self-loop never saw it.
   `parse_with_options(allow_dtd:true)` unlocked it: parity **0 → 77.1%**.
2. **Textless functional `<harmony>`.** abc2xml emits *functional* harmony
   (`<root>`+`<kind>major`, **no** `<kind text=…>`); croma's writer always emits
   `text=`, so S5b only read the text attribute and dropped foreign harmony
   (~48k facts / ~1948 files). The reader now **synthesises** a chord-symbol
   string from `<root>/<kind>/<bass>/<degree>` (inverting croma's own
   kind↔suffix table, round-trip-stable) when `text=` is absent: parity
   **77.1 → 98.27%**. This is foreign-dialect *reading* into croma's existing
   chord-symbol model — never writer-mimicry.
3. **Decimal `<alter>`.** The MusicXML spec types `<alter>` as `decimal`;
   abc2xml/music21 emit `1.0`. croma parsed it as `i8` → `"1.0"` failed → the
   accidental was silently dropped (a note read as natural). The `<alter>`
   family now parses as f64 rounded to the nearest semitone (quarter-tones
   degrade with a diagnostic): the `accidental` mismatch category **80 → 2**,
   parity **98.27 → 98.50%**.

**Result: reverse music21 parity 9,850 / 10,000 (98.50%)** — above the forward
raw-comparator floor (93.9%). Residual (adjudicated to verdicts, none a clean
reader bug):

| Category (count) | Verdict |
|---|---|
| `duration` 293 (~35 files) | **comparator/music21 artifact** — the breve note is byte-identical in croma's re-export and the ref; music21 reports a context-dependent ql, croma is not wrong. |
| `extra_in_croma` 181 (~81 files) | **croma writer default** — croma always emits a playback `<sound tempo="120">` that abc2xml omits; semantically neutral, not changeable without touching the (out-of-scope) forward writer. |
| `missing_in_croma` 398 + `measure_alignment` 73 + `voice` 38 | **single outlier file** `tune_011134.xml` (a pathological 2-part score; reference `<dot>`s croma lacks). 1 file. |
| `tuplet` 47 | **reader gap on complex/nested tuplets**, shared with the documented self-loop nested-tuplet residual — R3-adjacent, not chased here. |
| `direction` 34, `barline` 7, `accidental` 2 | uncommon directions croma leaves unread (with a diagnostic) / minor structural & spelling tails. |

**Foreign-engraver stretch (Decision 4).** A totality probe over 40 real
non-abc2xml engravings from music21 10.3.0's bundled corpus (MuseScore/Finale-origin
Bach chorales etc., `.mxl`): croma reads **40/40 with 0 panics, 0 empty** — every
file produces notes (one chorale → 503). Semantic parity on full multi-staff SATB
scores is lower (they exceed croma's ABC-oriented model), so it is a documented
totality + substantive-recovery probe, not a parity claim. abc2xml is the
parity-measured foreign dialect; other engravers are totality-proven.

## Staging

| Stage | Scope | Status |
|---|---|---|
| **S1** | `<score-partwise>` → parts → measures → `<note>` (`<pitch>` step/octave/alter, `<rest>`, `<duration>`/`<type>`/`<dot>`, `<accidental>`), `<backup>`/`<forward>`, `<divisions>`, work-title/composer/credit metadata | **done** |
| **S2** | header `<attributes>`: `<divisions>`, `<key>`/`<fifths>` (+ explicit `<key-step>`/`<key-alter>`/`<key-accidental>`), `<time>` (incl. `symbol="common"`/`"cut"`, compound, free), `<clef>`, `<transpose>` → `midi_transpose`. Mid-measure `<attributes>` changes (key/meter/clef) deferred. | **done** |
| **S3** | `<part-list>` MIDI: `<midi-instrument>` (`<midi-channel>`, 1-based `<midi-program>`, `<volume>`, `<pan>`) → `MidiInstrumentModel` on the owning voice. **Closes the forward/reverse `%%MIDI` loop.** | **done** |
| **S4** | per-`<note>` `<notations>` + `<time-modification>`: `<tied>`/`<tie>` → ties, `<slur>` → slurs, `<tuplet>`+`<time-modification>` → tuplets, and the `<articulations>`/`<ornaments>`/`<technical>`/`<fermata>`/`<arpeggiate>` decoration groups → `DecorationAttachment`. Beams are derived (no model field, no `<beam>` emitted) and round-trip via S1. | **done** |
| **S5a** | `<direction>`: tempo `<metronome>`/`<words>`+`<sound>` → header `tempo_model` or mid-tune `TempoChange`; `<dynamics>` (`p`..`fff`,`mp`/`mf`/`sfz`), `<wedge>` (crescendo/diminuendo/stop), `<coda>`/`<segno>` → `DecorationAttachment`; annotation `<words>` (+placement) → `TextAttachment`. Trailing/pre-barline directions reconstruct on a zero-duration `Spacer`. | **done** |
| **S5b** | `<harmony>` (chord symbols) → `chord_symbols`; `<lyric>` (`<syllabic>`+`<text>`+`<extend>`) → `lyrics` (`Syllable`/`Hyphen`/`Extender`), all verses | **done** |
| **S6a** | `<barline>`: `<bar-style>`+`<repeat>` → `Measure.barlines` (`BarlineKind` inverted by bar-style × `location` × repeat direction; `RepeatBoth` decomposed into `RepeatEnd`+`RepeatStart`); `<ending type="start">` → `Measure.repeat_endings` (`Single`/`Range`/`Text`), the `stop` closers regenerated by the writer's schedule. | **done** |
| **S6b (mid-measure attrs)** | mid-measure `<attributes>` changes: a NON-leading `<attributes>` block → zero-duration `KeyChange`/`MeterChange`/`ClefChange` events at the current onset (`<key>`→`KeyChange`, `<time>`→`MeterChange`, `<clef>`→`ClefChange`), reusing the S2 sub-element parsers. | **done** |
| **S6c (grace + chords)** | `<grace>` notes → `GraceGroupAttachment` on the following note (before-grace) or the preceding note (after-grace at measure end): slash (`slash="yes"`), grace rests, grace chords (`<chord/>`-joined grace notes), grace slurs, and per-note `length_multiplier` recovered from `<type>`/`<dots>` ÷ count-based base unit. `<chord/>` members → one `TimedEventKind::Chord` per onset, with per-member pitch/duration/written-accidental/attachments. | **done** |
| **S6d (multi-voice + multiple-rest)** | multi-voice: a measure's `<note>`s (and the directions/harmony/notations emitted just before them) are partitioned by `<voice>` and each reconstructed as a separate `Part.voices` entry (a `TimedEvent` voice reusing the full S1–S6c machinery), so the writer's `measure_sequences` re-emits the identical `<voice>1` .. `<backup>` .. `<voice>2`. `<measure-style><multiple-rest>N` → `Measure.multiple_rest`. | **done** |
| **S6e (hardening + totality)** | closeout: removed the last `debug_assert!` (grace-drain) in favour of a non-panicking graceful degrade; added the totality fuzz over own-exports + abc2xml-reference + malformed inputs (above); closed the clean subset of the annotation-before-inline-change ordering edge (trailing `Spacer` placed at its document-order position vs same-onset mid-tune changes via `pending_insert_index`). Additive only — no forward behaviour change. | **done** |

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
- Mid-measure `<attributes>` changes (a *second* `<attributes>` block from a
  `KeyChange`/`MeterChange`/`ClefChange` event) are reconstructed in **S6b**, not
  S2: S2 reads only the **leading** header `<attributes>`. See the S6b notes.

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

### S6a reconstruction notes — `<barline>` + `<repeat>` + `<ending>`

S6a inverts `write_barline` / `write_ending_barline` (`barline.rs`) and the
left/right barline placement in `write_part` (`score.rs`). The writer sources
barlines from **`Measure.barlines`** and endings from **`Measure.repeat_endings`**
(the `TimedEventKind::Barline`/`RepeatEnding` variants exist but the writer does not
read them), so the reader populates those two `Measure` fields.

- **Bar-style → `BarlineKind`** is inverted by the triple `(bar-style, location,
  repeat direction)`, because the forward map is many-to-one on `bar-style` alone:
  `heavy-light` ← both `Initial` and `RepeatStart`; `light-heavy` ← `Final`,
  `RepeatEnd`, and `RepeatBoth`. The disambiguating table (`barline_kind_from`):

  | location | bar-style   | repeat   | → kind        |
  |----------|-------------|----------|---------------|
  | left     | heavy-light | forward  | `RepeatStart` |
  | right    | heavy-light | (none)   | `Initial`     |
  | right    | light-heavy | backward | `RepeatEnd`   |
  | right    | light-heavy | (none)   | `Final`       |
  | right    | light-light | —        | `Double`      |
  | right    | dotted      | —        | `Dotted`      |
  | right    | none        | —        | `Invisible`   |

  `Regular`/`Liberal` are never emitted (the writer early-returns on them when there
  is no ending), so an ordinary `|` measure boundary reconstructs **no**
  `MeasureBarline` — the boundary itself is the implicit bar. An unknown combination
  warns and is skipped (never invents a kind).
- **`RepeatBoth` is decomposed, never materialised.** A `::` (combined back-then-
  forward repeat) emits **byte-identical** XML to a `RepeatEnd` immediately followed
  by a leading `RepeatStart`: a `light-heavy`+`backward` right barline on one measure
  and a `heavy-light`+`forward` left barline on the next (verified against the writer).
  The reader reads each half independently — `RepeatEnd` on the closing measure,
  leading `RepeatStart` on the opening one — so it never needs to reconstruct
  `RepeatBoth` and sidesteps the writer's trailing/deferred-`|:` placement machinery
  entirely. Re-emission is identical because the writer's `pending_left_repeat`
  deferral produces exactly this pair.
- **Left/right placement via synthetic spans (idempotence-invisible).** The writer
  picks a barline's side with `is_leading_barline` = `measure.source_span.start ==
  barline.span.start`. The reconstructed measure's `source_span` is `READER_SPAN`
  (`start == 0`), so a `location="left"` barline is given `span.start == 0` (leading
  → re-emitted left) and each `location="right"` barline a distinct non-zero
  `span.start` (trailing → re-emitted right, in document order via the writer's span
  sort). The writer never emits spans, so these synthetic spans are invisible to the
  idempotence gate; they exist only to drive the placement the gate then verifies.
- **Endings.** An `<ending type="start" number="…">` opens a volta bracket on its
  measure → one `RepeatEndingModel`. The `number` is a comma list inverted to
  `RepeatEndingPartModel`s: `"1"`→`Single(1)`, `"1-2"`→`Range{1,2}`, `"1,2"`→two
  `Single`s; a text-bearing `<ending>` (the writer emits the `["label"` source as
  element text with `number="33"`) → a single `Text` part (the `33` is regenerated
  from the `Text` variant, so it is not read). The **`type="stop"`/`"discontinue"`
  closers are not stored**: the writer regenerates them from the open-bracket
  positions plus the closing barline kinds (`ending_stop_schedule`), both of which the
  reader reconstructs from the same XML, so the schedule reproduces the identical stop
  placement (including the implicit-regular-barline force-close at a section end).
- **Header-tempo × leading barline (an S5a interaction the barline read resolves).**
  The writer emits a true header tempo (`write_initial_directions`) **before** any
  left barline, but a body-position `Q:`/`[Q:..]` lowers to a mid-tune `TempoChange`
  emitted **after** the measure's leading `|:`. Now that the reader sees the leading
  barline, a tempo direction encountered *after* it is correctly classified as a
  `TempoChange` rather than promoted to the header `tempo_model` — clearing the last 9
  files whose first divergence was a leading repeat ahead of a body tempo.

### S6b reconstruction notes — mid-measure `<attributes>` (key/meter/clef changes)

S6b inverts the writer's mid-tune `<attributes>` emission
([`MusicXmlWriter::write_mid_tune_key`] / `write_mid_tune_meter` /
`write_mid_tune_clef`, dispatched from `write_event` for the
`TimedEventKind::KeyChange`/`MeterChange`/`ClefChange` variants). S2 reads only
the **leading** header `<attributes>`; S6b reconstructs every **non-leading** one
into the zero-duration change events that the lowering (`lower::timeline`) places
at a voice's current onset.

- **Leading vs. mid-tune.** The writer emits the header `<attributes>`
  (`write_attributes`: `<divisions>` + score `<key>`/`<time>`/`<clef>` +
  `<transpose>`) **only** in a part's first measure, as its first block. The
  reader skips exactly that one block (already read into
  `metadata`/`initial_properties`); **every other** `<attributes>` — a *second*
  block after notes in any measure, **or** the first block of a non-leading
  measure (a body-field `K:`/`M:`/clef change at onset 0) — is a mid-tune change.
- **One element per event.** Each mid-tune change is its own minimal
  `<attributes>` wrapper holding exactly one of `<key>`/`<time>`/`<clef>`
  (`[K:G][M:2/4]` emits *two* blocks, key then time). The reader walks every child
  in document order and emits one zero-duration event per recognised sub-element,
  reusing the S2 parsers: `<key>` → `KeyChange(KeySignatureModel)` (incl. explicit
  `<key-step>`/`<key-alter>`/`<key-accidental>`), `<time>` →
  `MeterChange(MeterModel)`, `<clef>` → `ClefChange(ClefChangeModel)`.
- **Onset & ordering.** Each event is placed at the current `cursor` (the onset
  the preceding notes advanced to) with `Fraction::zero()` duration — exactly as
  the lowering creates them. The event is pushed onto `voice.events` in document
  order; `measure_sequences` sorts by `(onset, source.start)` with a **stable**
  sort and the reconstructed `source` is `READER_SPAN` (`start == 0`) like the
  surrounding notes, so a change at onset N re-sorts after earlier-onset notes and
  before the following note at onset N — reproducing the writer's interleaving.
  Zero duration means it never advances the cursor.
- **Mid-tune clef back to treble.** Unlike the header clef (which leaves a plain
  G/2 treble as `None`), `write_mid_tune_clef` is **unconditional**, so a mid-tune
  clef change *to* treble still emits a `<clef>`; the reader reconstructs the
  explicit canonical `"treble"` text (not `None`) so the G/2 element re-emits.
- **Tempo classification (S6b × S5a).** The writer emits a true header tempo
  (`write_initial_directions`) **before** any measure-sequence event, so a tempo
  `<direction>` seen *after* a mid-measure `<attributes>` is a body-position `Q:`
  `TempoChange` (sorted by onset behind the change), **not** the header tempo. The
  reader marks content as started when it consumes a mid-tune change, so such a
  tempo is correctly kept as a `TempoChange` rather than promoted to
  `metadata.tempo_model` (cf. the identical leading-barline interaction in S6a).
- **Buffered directions/harmony.** A `<direction>`/`<harmony>` *preceding* an
  inline change stays buffered onto the **following note** (the lowering gives
  change events empty attachments, so the writer emits the change's `<attributes>`
  first, then the buffered `<direction>`/`<harmony>`, then the note — verified to
  round-trip).
- **Not S6b scope:** `<divisions>` never appears mid-tune in croma's output (warns
  if hand-edited in); `<measure-style><multiple-rest>` is a *separate* feature
  (`Measure.multiple_rest`), tracked as remaining, not a key/meter/clef change.

[`MusicXmlWriter::write_mid_tune_key`]: ../crates/croma-core/src/musicxml/attributes.rs

### S6d reconstruction notes — multi-voice (`<voice>`/`<backup>`) + multiple-rest

S6d inverts the writer's multi-voice interleaving (`measure_sequences` +
`write_part`'s per-sequence `<backup>` in `score.rs`, and the always-emitted
`<voice>` in `write_note`/`write_harmony_and_directions`) and
`write_multiple_rest_measure_style` (`attributes.rs`). It is the final structural
sub-stage; with it the corpus idempotent count flattens (all remaining
divergences are unrelated single-voice issues — see the metric below).

- **Which writer representation the reader populates — extra `Part.voices`, not
  `Measure.overlays`.** The writer emits multiple voices of one part as
  contiguous, `<backup>`-separated sequences each tagged with its own `<voice>N`;
  `measure_sequences` builds these from BOTH `voice.events` (numbered
  `voice_index + 1`) AND `measure.overlays` (numbered after `part.voices.len()`).
  A two-voice part therefore re-emits **byte-identically** whether the second
  voice lives in `part.voices[1]` or in an `OverlaySegment`. The reader
  reconstructs the second voice as an additional **`Part.voices` entry** (a
  `TimedEvent` voice), because that path **reuses 100 % of the S1–S6c per-note
  machinery** — `read_note`, `read_note_attachments` (ties/slurs/tuplets/
  decorations/lyrics), directions, harmony, grace, and chord folding — whereas
  the overlay path uses the parallel `VoiceTimedEvent`/`TimelineEventKind`
  representation and would need all of that re-implemented. (ABC `&` overlays and
  `%%staves`-grouped body voices both lower to the overlay form on the *forward*
  side; the reader's extra-voices form is a valid-but-different `Score` that
  re-writes identically, accepted by the gate per design §9.)
- **Per-voice partition (`read_measure`).** The writer emits each voice's notes
  contiguously, so the reader walks the measure once and routes every `<note>`
  (and the `<direction>`/`<harmony>`/mid-tune `<attributes>` emitted just before
  it) to a per-voice [`VoiceMeasureState`] keyed on the note's `<voice>` text
  (a direction/harmony uses its own `<voice>`, else the active region's). Each
  state replicates the single-voice reconstruction independently: its own onset
  `cursor`, buffered `pending` directions, open grace run, chord head, open
  tuplets, and measure-rest / furthest-cursor bookkeeping. A `<backup>` rewinds
  the **active** region's cursor; since the next voice's cursor already starts at
  0, the rewind only affects the region it closes, keeping every voice's onsets
  relative to 0 — exactly what the writer's per-sequence cursor produces.
- **Voice numbering & ordering.** Voices are sorted by their numeric `<voice>`
  value so `<voice>1` → `part.voices[0]` (the writer re-numbers `part.voices[i]`
  as `i + 1`). croma's output numbers voices contiguously from 1, so this is
  exact. The **primary voice (`"1"`)** is canonical: it gets a `Measure` for
  **every** `<measure>` element (including a content-less trailing
  `<measure></measure>`, so the measure count is preserved) and carries the
  measure **skeleton** — `barlines`/`repeat_endings`/`multiple_rest` — which the
  writer reads from any voice's measure via a deduping union; extra voices carry
  a minimal `Measure` (their own `expected`/`actual` duration, empty structure)
  only in the measures where they have content. Each extra voice gets a distinct
  `VoiceId.value` (`{part}#{n}`) so the writer's per-voice `SlurNumbers` numbers
  each voice's slurs independently. A single staff is reconstructed, so the
  `<staff>`-emission guard (`part.staves.len() > 1`) stays off and no `<staff>`
  is emitted — matching the writer's single-staff output.
- **Per-voice MIDI.** The writer emits one `<midi-instrument>` per voice that
  carried `%%MIDI` sound metadata, in voice order; the reader attaches each
  recovered instrument to the voice at the **same index** (the S3 multi-voice
  residual is thereby closed — see the S3 note's "deferred to S6").
- **Multiple-rest.** `<measure-style><multiple-rest>N</multiple-rest>` (the
  writer's own `<attributes>` wrapper) → `Measure.multiple_rest = Some(N)` for
  `N > 1`, attached to the primary voice. It is read in the `<attributes>` arm
  regardless of header/mid-tune status (it is a measure-level glyph hint, not a
  key/meter/clef change), so the mid-tune attributes reader skips `measure-style`
  silently. The expanded bars of the run stay individual measures (the second bar
  is an ordinary `<rest measure="yes">`); only the first carries the glyph count.

#### Unsupported residual (documented per design "stop where coverage flattens")

**Final state (after S6e): the corpus idempotent count is `9,915 / 9,935`** — the
20 remaining non-idempotent files are all single-voice (`<backup>`-free) and are
documented as the reader's residual. (S6d reached `9,912 / 9,935`; S6e's clean
ordering fix added **+3 with 0 regressions** — verified by a corpus set-diff of the
strictly-idempotent file set, HEAD vs the fix: 0 previously-passing files break, 3
new wins. See the corpus metric below.) The residual:

- **19 files, first-diverging tag `direction`:** a **note-less** measure carrying
  *both* a standalone annotation (`<direction><words>`) **and** a chord symbol
  (`<harmony>`) with no note to host them. The writer emits them in their original
  ABC document order, but the reader buffers both onto one trailing `Spacer` whose
  attachment channels re-emit in a fixed order (chord symbols before annotations),
  reversing the pair. Recovering the cross-channel document order within a single
  Spacer would need per-attachment source tracking through the `pending` buffer —
  a deeper change with its own regression surface — so it is **left documented**,
  not forced. (S6e's `pending_insert_index` fix addresses the *adjacent* sub-case,
  annotation-or-harmony **before an inline key/meter/clef change** in a note-less
  measure — e.g. `"Trio"[K:F]` — by placing the trailing Spacer at its
  document-order position relative to the same-onset change event; that subset, 3
  files, now round-trips.)
- **1 file, first-diverging tag `type`:** a deeply nested tuplet (a 21:16
  composite from two stacked `<tuplet type="start">`) whose first note's `<type>`
  spelling the reader does not reproduce (the reader recovers the composite
  `21/16` ratio as the writer's reduced `441/256`, an S4 tuplet/note-spelling edge
  case). Single-voice, **left documented**.

Every multi-voice (`<backup>`/`<voice>`) and every `<multiple-rest>` file in the
corpus round-trips byte-for-byte; **zero** files first-diverge on `note`,
`backup`, `voice`, `multiple-rest`, `score-instrument`, `pitch`, `step`, or (after
S6e) on `attributes` (no mid-tune key/meter/clef change ordering remains).

### S1/S2/S3/S4/S5a/S5b/S6a/S6b/S6c/S6d corpus metric (10k zenodo set)

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
- **S6a** strict full-byte idempotence: **8,008 / 9,935** (**+7,373** vs S5b's 635) —
  by far the largest single-stage jump, as predicted: `barline` was the dominant
  blocker (8,573 files), and S6a reads back every `<barline>`/`<repeat>`/`<ending>`.
  The **`barline` first-diverging tag collapses 8,573 → 0** — eliminated entirely from
  the histogram. The new top first-diverging tags are all later-stage structure the
  design defers: `attributes` (977, **deferred mid-measure key/meter/clef changes**),
  `grace` (911, grace notes), `note` (21, multi-voice), `direction` (14, multi-voice
  `<backup>` shift), `score-instrument` (2) and `voice` (2) — the remaining **S6b**
  work (multi-voice, grace, chords, mid-measure attributes). Barlines, repeats and
  endings round-trip across the corpus (9,246 files use them; **0** now first-diverge
  on a barline).
- **S6b (mid-measure attributes)** strict full-byte idempotence: **8,934 / 9,935**
  (**+926** vs S6a's 8,008). The self-policing signal fires exactly as designed:
  the **`attributes` first-diverging tag collapses 977 → 6** — S6b reads back every
  mid-measure key/meter/clef `<attributes>` change wherever it was the first
  divergence. (The fix also resolves the S6b × S5a tempo interaction: a body `Q:`
  after a mid-tune meter change is now classified as a `TempoChange`, not promoted
  to the header tempo — cf. the S6a leading-barline case.) The **6 residual
  `attributes` files are all `<measure-style><multiple-rest>`** — a *separate*
  feature (`Measure.multiple_rest`), **not** a key/meter/clef change; **zero**
  mid-measure key/meter/clef changes remain. The new top first-diverging tags are
  `grace` (946, grace notes), `note` (23, multi-voice), `direction` (22,
  multi-voice `<backup>` shift), `attributes` (6, the multiple-rest feature),
  `score-instrument` (2) and `voice` (2) — the remaining S6 multi-voice / grace /
  chord / multiple-rest work. Mid-measure key/meter/clef changes round-trip across
  the corpus.
- **S6c (grace + chords)** strict full-byte idempotence: **9,869 / 9,935**
  (**+935** vs S6b's 8,934). The self-policing signal fires as designed: the
  **`grace` first-diverging tag collapses 946 → 0** — eliminated entirely from the
  histogram (the reader now reads every `<grace>` note, slash/before/after-grace,
  grace chord and grace slur, and folds every `<chord/>` member into a
  `TimedEventKind::Chord`). The new top first-diverging tags are all the deferred
  multi-voice / multiple-rest work: `note` (27, multi-voice `<note>` vs `<backup>`
  ordering), `direction` (25, multi-voice `<backup>` shift), `attributes` (6, the
  `<measure-style><multiple-rest>` feature — unchanged), `voice` (3),
  `score-instrument` (2, multi-voice-per-part), and `pitch`/`step`/`type` (1 each,
  all in `%%staves` multi-voice files where only the first voice is reconstructed).
  **No grace or chord shape first-diverges anywhere in the corpus.** The remaining
  ~66 non-idempotent files are dominated by multi-voice (`<voice>`/`<backup>`) and
  `<multiple-rest>`, the final sub-stage.
- **S6d (multi-voice + multiple-rest)** strict full-byte idempotence:
  **9,912 / 9,935** (**+43** vs S6c's 9,869), the final structural sub-stage.
  The self-policing signal fires exactly as designed and the corpus coverage
  **flattens**: the multi-voice / multiple-rest first-diverging tags collapse
  entirely — `note` (27 → 0), `direction`'s multi-voice share (25 → 22, the 3
  `<backup>` files cleared), `attributes`/multiple-rest (6 → 0), `voice` (3 → 0),
  `score-instrument` (2 → 0), and `pitch`/`step` (1 each → 0). A set-diff against
  the pre-S6d HEAD confirms **0 regressions and 43 new wins** (every `<backup>`
  file and every `<multiple-rest>` file now round-trips). The **23 residual
  non-idempotent files are all single-voice** (`<backup>`-free) and **unrelated to
  S6d**: `direction` (22, a standalone annotation before an inline mid-tune change
  in a note-less measure — an S5a/S6b ordering interaction) and `type` (1, a
  deeply nested 21:16 tuplet note-spelling edge case). Both are documented as the
  reader's residual under "Unsupported residual" above; they are out of S6d's
  multi-voice / multiple-rest scope and left for a future single-voice pass.
  **Coverage has flattened — this closes the structural staging (S1–S6d).**
- **S6e (hardening + totality)** strict full-byte idempotence: **9,915 / 9,935**
  (**+3** vs S6d's 9,912). S6e is a closeout, not a structural stage: its corpus
  delta is the small, clean ordering subset only. The `pending_insert_index` fix
  places a note-less measure's trailing direction/harmony `Spacer` at its
  **document-order position** relative to same-onset mid-tune changes, so an
  annotation/chord-symbol **before** an inline `[K:]`/`[M:]`/clef change (e.g.
  `"Trio"[K:F]`) re-emits the `<direction>`/`<harmony>` ahead of the
  `<attributes>` — clearing `direction` from 22 → 19 (the `attributes`-adjacency
  cases) and removing every mid-tune-change ordering divergence. A **corpus
  set-diff of the strictly-idempotent file set (HEAD vs the fix) confirms 0
  regressions and 3 new wins**, satisfying the closeout's "byte-idempotent, zero
  regressions" gate. The 20 residual files (19 `direction` annotation/harmony
  co-ordering on one Spacer; 1 `type` nested 21:16 tuplet) are documented under
  "Unsupported residual" above — not forced, per the design's "stop where coverage
  flattens." The totality work (debug_assert removal + the fuzz over 9,935 own
  exports + 10,000 abc2xml-reference files + 11 malformed cases, all 0 panics) is
  the stage's primary deliverable; see **Totality** above.

Re-run the measurement with (note: pass an **absolute** `ABC_ROOT` — the test
runs with the crate dir as its working directory):

```sh
ABC_ROOT=/abs/path/to/docs/untracked/corpus/zenodo-10k/abc \
  cargo test -p croma-core --release --features musicxml-reader \
  corpus_idempotence_measurement -- --nocapture
```

Set `READER_IDEMPOTENT_LIST=/tmp/idem.txt` to additionally dump the sorted list of
strictly-idempotent file names; two runs (baseline vs a change) can then be
`comm`-diffed to prove a fix has zero regressions, as S6e did.
