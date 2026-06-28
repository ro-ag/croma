//! Experimental MusicXML -> [`Score`] reader: the inverse of croma's own
//! writer ([`super::write_score_partwise`]).
//!
//! **Status: gated + experimental** (behind `musicxml-reader`), exactly like the
//! LSP, until it has corpus round-trip evidence comparable to the formatter's.
//! The writer is the spec; this reader inverts *croma's* dialect only and never
//! mirrors an abc2xml-ism (see `docs/musicxml-reader.md` and the design doc
//! `docs/superpowers/specs/2026-06-15-musicxml-reader-design.md`).
//!
//! # Totality
//! [`read_musicxml`] is **total and non-panicking**: an unparseable document
//! yields a minimal [`Score`] plus a diagnostic, and unknown elements are
//! ignored (optionally with a diagnostic). There is no `unwrap`/`expect`/
//! `panic`/`todo` and no index that can panic anywhere in this module tree.
//!
//! # Stages S1–S2 (this module)
//! **S1:** `<score-partwise>` -> parts -> measures -> `<note>`
//! (`<pitch>`/`<rest>`, `<duration>`/`<type>`/`<dot>`), `<backup>`/`<forward>`,
//! plus the work-title/composer/credit metadata the writer reads back.
//! `<divisions>` is read because it is needed to map `<duration>` to a
//! [`Fraction`].
//!
//! **S2:** the header `<attributes>` block — `<key>`/`<fifths>` (+ explicit
//! `<key-step>`/`<key-alter>`/`<key-accidental>`) -> [`KeySignatureModel`],
//! `<time>` -> [`MeterModel`], `<clef>` -> the staff voice's
//! `initial_properties.clef`, and `<transpose><chromatic>` -> `midi_transpose`.
//! **S3:** the `<part-list>` MIDI instruments — `<score-instrument>` /
//! `<midi-instrument>` (`<midi-channel>`, 1-based `<midi-program>`, `<volume>`,
//! `<pan>`, `<midi-unpitched>`) -> [`MidiInstrumentModel`] on the owning voice.
//! This closes the forward/reverse `%%MIDI` loop (the epic's motivator): a
//! `%%MIDI program` / `channel` / `control` / `midi-unpitched` directive
//! (line-start or inline `[I:MIDI=...]`) now survives ABC -> XML -> [`Score`] ->
//! XML byte-for-byte.
//!
//! **S4:** the per-`<note>` `<notations>` block and `<time-modification>` —
//! `<tied>`/`<tie>` -> [`TieAttachment`], `<slur>` -> [`SlurAttachment`]
//! (`pair_id` chosen so the writer's `SlurNumbers` re-derives the same
//! `number`), `<tuplet>` + `<time-modification>` -> [`TupletAttachment`]
//! (`actual`/`normal` from the time-modification ratio, `role` from the
//! start/stop/continue position), and the `<notations>` decoration groups
//! (`<articulations>`/`<ornaments>`/`<technical>`/`<fermata>`/`<arpeggiate>`)
//! -> [`DecorationAttachment`], inverting the writer's `decoration_notation`
//! name map exactly. **Beams are derived, not stored**: croma's writer emits no
//! `<beam>` element (beaming is recomputed from durations), so the S1 note
//! reconstruction already round-trips them with no beam-specific reader code.
//!
//! **S5a:** the `<direction>` block — the inverse of
//! [`MusicXmlWriter::write_initial_directions`],
//! [`MusicXmlWriter::write_tempo_direction`] and
//! [`MusicXmlWriter::write_harmony_and_directions`]. A voice-less tempo
//! `<direction>` (`<metronome>` and/or tempo `<words>` + `<sound>`) before the
//! first note of part 1 -> the header [`TempoModel`] (`metadata.tempo_model`);
//! any other tempo direction -> a mid-tune [`TimedEventKind::TempoChange`] event
//! at its onset. A voice-bearing `<direction>` attaches to the FOLLOWING note's
//! [`EventAttachments`] (the writer emits an event's directions before the
//! event): `<words>` -> [`TextAttachment`] annotations (placement inverted from
//! the `placement` attribute), `<dynamics>` / `<coda>` / `<segno>` / `<wedge>` ->
//! the [`DecorationAttachment`]s whose names re-emit the identical direction.
//!
//! **S6b (mid-measure attributes):** a NON-leading `<attributes>` block (a second
//! block after notes in a measure, or the first block of a non-leading measure)
//! is reconstructed into the zero-duration change events the lowering places at a
//! voice's current onset: `<key>` -> [`TimedEventKind::KeyChange`], `<time>` ->
//! [`TimedEventKind::MeterChange`], `<clef>` -> [`TimedEventKind::ClefChange`]
//! (reusing the S2 sub-element parsers). The leading header `<attributes>` is
//! still read only by S2 ([`Reader::read_header_key_meter`] +
//! `read_clef`/`read_transpose`); see [`Reader::read_mid_measure_attributes`].
//!
//! **S6c (grace notes + chords):** a run of consecutive `<grace>` `<note>`s is
//! collected ([`Reader::push_grace_note`] + [`GraceGroupBuilder`]) into one
//! [`GraceGroupAttachment`] bound to the FOLLOWING main note (before-grace) — or,
//! when the run reaches the measure end with no following note, the PRECEDING note
//! ([`GraceGroupAttachment`] in `after_grace_groups`). It reconstructs the slash
//! (`slash="yes"`), grace rests/notes/chords (a grace chord is grace notes joined
//! by `<chord/>`), grace slurs, and each grace note's `length_multiplier`
//! (recovered from `<type>`/`<dots>` divided by the count-based base unit). A
//! `<chord/>` member note folds into the previous main event
//! ([`Reader::fold_chord_member`]) as a [`TimedEventKind::Chord`] carrying one
//! [`crate::model::ChordMemberEvent`] per member (pitch/duration/written-accidental
//! /per-member attachments); each chord gets a distinct synthetic `source_span` so
//! the writer's first-member attachment lookup resolves correctly.
//!
//! **S6d (multi-voice + multiple-rest):** the writer interleaves a part's
//! multiple voices with `<backup>` and a per-sequence `<voice>` number. The
//! reader partitions each measure's `<note>`s (and the directions/harmony/
//! notations emitted just before them) by `<voice>` ([`note_voice_number`] /
//! [`voice_state`]) and reconstructs each as a separate `Part.voices` entry — a
//! `TimedEvent` voice reusing the full S1–S6c per-note machinery — so the
//! writer's `measure_sequences` re-emits the identical `<voice>1` .. `<backup>`
//! .. `<voice>2` interleaving (`part.voices[i]` is numbered `i + 1`). Extra
//! voices share the single staff (so no `<staff>` is emitted) and carry their own
//! independent onset cursor (each `<backup>` returns to onset 0 for the next
//! voice). `<measure-style><multiple-rest>N` ([`read_multiple_rest`]) is the
//! inverse of `write_multiple_rest_measure_style`, reconstructed into
//! `Measure.multiple_rest` (attached to voice 1, the voice the writer reads it
//! from). See `docs/musicxml-reader.md` for the small documented residual.

use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::model::{
    Accidental, AccidentalMark, AccidentalPolicy, AccidentalScope, AlignedLyric,
    AnnotationPlacementModel, BarlineKind, ChordEvent, ChordMemberEvent, ClefChangeModel,
    DecorationAttachment, DecorationSourceKind, EventAttachments, Fraction, GraceEvent,
    GraceEventKind, GraceGroupAttachment, GraceNoteEvent, KeyAccidentalModel, KeySignatureModel,
    LyricControl, Measure, MeasureBarline, MeasureId, MeterModel, MidiInstrumentModel,
    MusicXmlInstrumentRef, MusicXmlPartInstrumentModel, NoteEvent, Part, PartId, Pitch,
    RepeatEndingModel, RepeatEndingPartModel, RestEvent, RestVisibility, Score,
    ScoreDirectiveModel, ScoreDirectiveTokenKindModel, ScoreDirectiveTokenModel, ScoreMetadata,
    SlurAttachment, SlurRole, Staff, StaffId, TempoBeat, TempoBeatRole, TempoModel, TextAttachment,
    TextLine, TieAttachment, TieRole, TimedEvent, TimedEventKind, TupletAttachment, TupletRole,
    Voice, VoiceId, VoicePropertiesModel,
};
use crate::parse::ParseReport;

use roxmltree::{Document, Node, ParsingOptions};

/// Sentinel span for every model field the reader cannot reconstruct from the
/// XML (the writer never emits source spans, so the idempotence gate is
/// span-agnostic). Equivalent to `Span::new(0, 0)`, written as a struct literal
/// because `Span::new` is not `const`.
const READER_SPAN: Span = Span { start: 0, end: 0 };

/// Parse croma-emitted MusicXML back into a [`Score`].
///
/// Total and non-panicking. On a malformed document the returned report carries
/// a single error [`Diagnostic`] and a minimal empty [`Score`]; reader warnings
/// (unknown/malformed elements) are surfaced in `report.diagnostics`. The
/// reconstructed `Score.diagnostics` mirrors the report diagnostics so callers
/// that inspect either see the same warnings.
pub fn read_musicxml(xml: &str) -> ParseReport<Score> {
    // DTD tolerance (R2b): every real-world MusicXML file (abc2xml, MuseScore,
    // Finale, Sibelius) opens with a `<!DOCTYPE score-partwise PUBLIC ...>`
    // declaration. roxmltree's default `allow_dtd: false` rejects all of them
    // before any reader logic runs, so the reverse direction never reaches a
    // real document. `allow_dtd: true` permits the declaration; roxmltree still
    // never fetches the external DTD subset, keeps its billion-laughs guard, and
    // raises `UnknownEntityReference` for unsafe entity expansion — so this is
    // the standard, safe way to read real MusicXML. Genuine malformation still
    // returns `Err`, handled by the graceful empty-Score path below. READ-only:
    // croma's writer emits no doctype, so forward output is byte-untouched.
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    let document = match Document::parse_with_options(xml, options) {
        Ok(document) => document,
        Err(error) => {
            let diagnostic = Diagnostic::new(
                Severity::Error,
                "musicxml.read.parse_error",
                format!("MusicXML is not well-formed XML: {error}"),
                READER_SPAN,
            );
            return ParseReport::new(empty_score(vec![diagnostic.clone()]), vec![diagnostic]);
        }
    };

    let mut reader = Reader::default();
    let score = reader.read_document(&document);
    ParseReport::new(score, reader.diagnostics)
}

/// A minimal [`Score`] with documented defaults for every field the writer
/// reads. Used for the empty/error case and as the base the reader fills in.
fn empty_score(diagnostics: Vec<Diagnostic>) -> Score {
    Score {
        metadata: empty_metadata(),
        parts: Vec::new(),
        diagnostics,
        divisions: 1,
        source_span: READER_SPAN,
        accidental_policy: AccidentalPolicy {
            // Matches the lowering's default so a reconstructed score's writer
            // behaviour (which consults `preserve_explicit_accidentals`) agrees
            // with a freshly lowered one.
            preserve_explicit_accidentals: true,
            reset_at_barlines: true,
            scope: AccidentalScope::PitchAndOctave,
            source_span: READER_SPAN,
        },
    }
}

fn empty_metadata() -> ScoreMetadata {
    ScoreMetadata {
        // `reference` (ABC `X:`) is never emitted by the writer, so it is
        // invisible to the idempotence gate; a blank line is the documented
        // default.
        reference: TextLine {
            text: String::new(),
            span: READER_SPAN,
        },
        title: None,
        composers: Vec::new(),
        tempo: None,
        tempo_model: None,
        meter: None,
        key: None,
        directives: Vec::new(),
        preserved_directives: Vec::new(),
        post_tune_lyrics: Vec::new(),
        source_span: READER_SPAN,
    }
}

#[derive(Default)]
struct Reader {
    diagnostics: Vec<Diagnostic>,
}

impl Reader {
    fn warn(&mut self, code: &'static str, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::new(
            Severity::Warning,
            code,
            message,
            READER_SPAN,
        ));
    }

    fn read_document(&mut self, document: &Document<'_>) -> Score {
        let root = document.root_element();
        if root.tag_name().name() != "score-partwise" {
            self.warn(
                "musicxml.read.unsupported_root",
                format!(
                    "expected a <score-partwise> root, found <{}>; only partwise scores are read",
                    root.tag_name().name()
                ),
            );
            return empty_score(self.diagnostics.clone());
        }

        let mut score = empty_score(Vec::new());
        score.divisions = self.read_divisions(root).unwrap_or(score.divisions);
        self.read_metadata(root, &mut score.metadata);
        // `<key>`/`<time>` are score-level: the writer emits them from
        // `score.metadata.{key,meter}` into the FIRST measure's `<attributes>`
        // of every part, identically. Read them from the first part's header
        // attributes. (Per-part `<clef>`/`<transpose>` are reconstructed in
        // `read_part`, scoped to the owning voice.)
        self.read_header_key_meter(root, &mut score.metadata);

        // Part names AND part-list MIDI instruments (S3) come from the
        // <part-list>; the music comes from the sibling <part> elements. The
        // writer keys them by matching `id`. P1a: `read_part_list` also
        // recovers any `<part-group>` spans for `%%score` synthesis.
        let part_list_result = self.read_part_list(root);
        // The header tempo direction (the writer's `write_initial_directions`)
        // belongs to the score once, emitted only in part 1; reconstruct it into
        // `metadata.tempo_model`. `read_part` reports the captured header tempo so
        // a voice-less tempo direction before part 1's first note becomes the
        // header model rather than a mid-tune `TempoChange`.
        for (part_index, part_node) in children_named(root, "part").enumerate() {
            let outcome = self.read_part(
                part_node,
                score.divisions,
                &part_list_result.entries,
                part_index == 0,
            );
            if let Some(tempo) = outcome.header_tempo {
                score.metadata.tempo_model.get_or_insert(tempo);
            }
            score.parts.push(outcome.part);
        }
        project_part_names_to_voice_properties(&mut score);

        // P1a: synthesize `%%score` directives from recovered `<part-group>`
        // spans and from multi-voice MusicXML parts. This is ABC-path only:
        // `write_score_partwise` (the `--format xml` pure-inverse path) does not
        // consult `metadata.directives`, so the directive is inert for the
        // self-loop and reverse-parity gates.
        // Voice-id alignment: the first (index 0) voice of each `<score-part
        // id="P1">` gets `voice_id_value("P1", "1", 0) = "P1"`, which is exactly
        // the part id. `write_abc` emits `V:P1` for that voice, so the synthesised
        // `%%score [P1 P2 …]` references the same ids that `V:` headers use.
        // Fix 3: pass the full ordered part-id list so ungrouped parts are
        // included as bare voice-id tokens in their document-order positions.
        let part_score_blocks = part_score_blocks(&score);
        let all_part_ids: Vec<&str> = score
            .parts
            .iter()
            .map(|part| part.id.value.as_str())
            .collect();
        if let Some(directive) =
            synthesize_score_directive(&part_list_result.groups, &all_part_ids, &part_score_blocks)
        {
            score.metadata.directives.push(directive);
        }

        score.diagnostics = self.diagnostics.clone();
        score
    }

    /// `<divisions>` lives in the first measure's `<attributes>`; it is what
    /// maps `<duration>` integers back to rational [`Fraction`]s.
    fn read_divisions(&mut self, root: Node<'_, '_>) -> Option<u32> {
        descendants_named(root, "divisions")
            .next()
            .and_then(|node| parse_u32(self, node, "divisions"))
            .filter(|value| *value > 0)
    }

    /// Read the score-level `<key>`/`<time>` from the first part's first
    /// measure's `<attributes>` into `metadata.{key,meter}`. The writer emits an
    /// identical copy in every part's header, so reading the first is sufficient
    /// and re-emitting it for all parts reproduces the input.
    fn read_header_key_meter(&mut self, root: Node<'_, '_>, metadata: &mut ScoreMetadata) {
        let Some(attributes) = children_named(root, "part")
            .next()
            .and_then(|part| children_named(part, "measure").next())
            .and_then(|measure| child_element(measure, "attributes"))
        else {
            return;
        };
        if let Some(key_node) = child_element(attributes, "key") {
            metadata.key = Some(self.read_key(key_node));
        }
        if let Some(time_node) = child_element(attributes, "time") {
            metadata.meter = self.read_meter(time_node);
        }
        // An absent `<time>` means either `M:none` (free meter) or no meter at
        // all; both re-emit nothing, so leaving `meter` as `None` is idempotent.
    }

    /// Invert [`MusicXmlWriter::write_key_element`]: `<fifths>` -> `fifths`, and
    /// each consecutive `<key-step>`/`<key-alter>`/`<key-accidental>` triple ->
    /// one [`KeyAccidentalModel`]. The writer never emits the `KeySignatureModel`
    /// `display` string, so it is left empty (the idempotence gate confirms
    /// `<key>` is fully driven by `fifths` + `explicit_accidentals`).
    fn read_key(&mut self, key_node: Node<'_, '_>) -> KeySignatureModel {
        let fifths = child_text(key_node, "fifths")
            .and_then(|text| self.parse_i8(text, "fifths"))
            .unwrap_or(0);

        // The writer interleaves the triple in document order; pair each
        // `<key-step>` with the `<key-accidental>` that follows it (preferred, as
        // the exact inverse of `musicxml_name`), falling back to `<key-alter>`.
        let mut explicit_accidentals = Vec::new();
        let mut pending_step: Option<char> = None;
        let mut pending_alter: Option<i8> = None;
        for child in element_children(key_node) {
            match child.tag_name().name() {
                "key-step" => {
                    pending_step = node_text(child).and_then(|text| text.chars().next());
                    pending_alter = None;
                }
                "key-alter" => {
                    pending_alter =
                        node_text(child).and_then(|text| self.parse_i8(text, "key-alter"));
                }
                "key-accidental" => {
                    if let Some(step) = pending_step {
                        let accidental = node_text(child)
                            .and_then(|name| self.accidental_from_name(name))
                            .or_else(|| pending_alter.and_then(accidental_from_alter))
                            .unwrap_or(Accidental::Natural);
                        explicit_accidentals.push(KeyAccidentalModel {
                            step,
                            accidental,
                            source_span: READER_SPAN,
                        });
                    }
                    pending_step = None;
                    pending_alter = None;
                }
                _ => {}
            }
        }

        KeySignatureModel {
            display: String::new(),
            fifths,
            explicit_accidentals,
            source_span: READER_SPAN,
        }
    }

    /// Invert [`MusicXmlWriter::write_time_element`]. The writer maps
    /// `meter.display` -> `<time>` via `meter_parts`; the reader reconstructs a
    /// `display` that maps back to the same element. `symbol="common"` -> `"C"`,
    /// `symbol="cut"` -> `"C|"`; otherwise reassemble `beats/beat-type` pairs
    /// (joined with `+` when compound). `MeterModel.display` is the only field the
    /// writer reads, so `duration`/`free_meter` get documented defaults.
    fn read_meter(&mut self, time_node: Node<'_, '_>) -> Option<MeterModel> {
        let display = match time_node.attribute("symbol") {
            Some("common") => "C".to_owned(),
            Some("cut") => "C|".to_owned(),
            _ => {
                let mut parts: Vec<String> = Vec::new();
                let mut pending_beats: Option<String> = None;
                for child in element_children(time_node) {
                    match child.tag_name().name() {
                        "beats" => pending_beats = node_text(child).map(str::to_owned),
                        "beat-type" => {
                            if let (Some(beats), Some(beat_type)) =
                                (pending_beats.take(), node_text(child))
                            {
                                parts.push(format!("{beats}/{beat_type}"));
                            }
                        }
                        _ => {}
                    }
                }
                if parts.is_empty() {
                    self.warn(
                        "musicxml.read.empty_time",
                        "<time> has no beats/beat-type pairs; meter not reconstructed",
                    );
                    return None;
                }
                parts.join("+")
            }
        };
        Some(MeterModel {
            display,
            duration: None,
            free_meter: false,
            source_span: READER_SPAN,
        })
    }

    /// Invert the `<clef>` the writer emits from a voice's
    /// `initial_properties.clef`: read `<sign>`/`<line>`/`<clef-octave-change>`
    /// and rebuild a canonical ABC clef text that `clef_model` re-maps to the
    /// same element. (The original ABC clef text is unrecoverable — many strings
    /// map to one `<clef>` — but a canonical representative is idempotent.) S2
    /// reconstructs a single voice per part, so this is the staff voice's clef.
    /// Returns `None` for the plain default treble clef, matching a freshly
    /// lowered score (the writer emits the same `<clef>` either way).
    fn read_clef(&mut self, attributes: Node<'_, '_>) -> Option<TextLine> {
        let clef_node = child_element(attributes, "clef")?;
        let sign = child_text(clef_node, "sign").unwrap_or("G");
        let line = child_text(clef_node, "line").unwrap_or("2");
        let octave_change = child_text(clef_node, "clef-octave-change")
            .and_then(|text| self.parse_i8(text, "clef-octave-change"))
            .unwrap_or(0);
        clef_text_from(sign, line, octave_change).map(text_line)
    }

    /// Invert `<transpose><chromatic>n` -> `midi_transpose = Some(n)`. The writer
    /// emits one `<transpose>` per part from the first voice that has either a
    /// `transpose=` property (ABC text, not in the XML) or `midi_transpose`;
    /// reconstructing `midi_transpose` reproduces the element on re-write.
    fn read_transpose(&mut self, attributes: Node<'_, '_>) -> Option<i16> {
        let transpose_node = child_element(attributes, "transpose")?;
        let chromatic = child_text(transpose_node, "chromatic")?;
        match chromatic.parse::<i16>() {
            Ok(value) => Some(value),
            Err(_) => {
                self.warn(
                    "musicxml.read.invalid_chromatic",
                    format!("<chromatic> `{chromatic}` is not an i16; transpose ignored"),
                );
                None
            }
        }
    }

    /// Parse a signed integer that fits in `i8` (`<fifths>`, `<key-alter>`,
    /// `<clef-octave-change>`), warning and yielding `None` otherwise.
    fn parse_i8(&mut self, text: &str, label: &str) -> Option<i8> {
        match text.trim().parse::<i8>() {
            Ok(value) => Some(value),
            Err(_) => {
                self.warn(
                    "musicxml.read.invalid_integer",
                    format!("<{label}> `{}` is not a valid signed integer", text.trim()),
                );
                None
            }
        }
    }

    /// Parse a `<alter>`-family field. The MusicXML spec types `<alter>` (and the
    /// `<root-alter>`/`<bass-alter>`/`<degree-alter>` chord-symbol variants) as
    /// `decimal`, and abc2xml / music21 / MuseScore / Finale all emit it as a
    /// FLOAT (`<alter>1.0</alter>`, `<alter>-1.0</alter>`). A bare-integer parse of
    /// `"1.0"` fails, silently dropping the accidental and corrupting the sounding
    /// pitch — so parse an `f64` and round to the nearest whole semitone (croma's
    /// model has no sub-semitone alter). A genuine quarter-tone (non-zero
    /// fractional part, e.g. `0.5`) is unrepresentable: keep the rounded value but
    /// emit a diagnostic rather than panic or drop. The rounded magnitude is
    /// clamped into `i8` range defensively (real accidentals are tiny).
    fn parse_alter(&mut self, text: &str, label: &str) -> Option<i8> {
        let trimmed = text.trim();
        let value = match trimmed.parse::<f64>() {
            Ok(value) if value.is_finite() => value,
            _ => {
                self.warn(
                    "musicxml.read.invalid_alter",
                    format!("<{label}> `{trimmed}` is not a finite decimal alter; ignored"),
                );
                return None;
            }
        };
        let rounded = value.round();
        if (value - rounded).abs() > f64::EPSILON {
            self.warn(
                "musicxml.read.fractional_alter",
                format!(
                    "<{label}> `{trimmed}` is a non-integer (microtonal) alter; \
                     rounded to {rounded} semitone(s) (croma has no sub-semitone model)"
                ),
            );
        }
        Some(rounded.clamp(f64::from(i8::MIN), f64::from(i8::MAX)) as i8)
    }

    /// Parse an unsigned integer that fits in `u8` (`<midi-channel>`), warning
    /// and yielding `None` otherwise.
    fn parse_u8(&mut self, text: &str, label: &str) -> Option<u8> {
        match text.trim().parse::<u8>() {
            Ok(value) => Some(value),
            Err(_) => {
                self.warn(
                    "musicxml.read.invalid_integer",
                    format!("<{label}> `{}` is not a valid 0-255 integer", text.trim()),
                );
                None
            }
        }
    }

    /// Parse an unsigned integer that fits in `u16` (raw 1-based
    /// `<midi-program>` before the `- 1` inverse), warning otherwise.
    fn parse_u16(&mut self, text: &str, label: &str) -> Option<u16> {
        match text.trim().parse::<u16>() {
            Ok(value) => Some(value),
            Err(_) => {
                self.warn(
                    "musicxml.read.invalid_integer",
                    format!(
                        "<{label}> `{}` is not a valid non-negative integer",
                        text.trim()
                    ),
                );
                None
            }
        }
    }

    fn read_metadata(&mut self, root: Node<'_, '_>, metadata: &mut ScoreMetadata) {
        // <work><work-title> -> title. Use the RAW (untrimmed) text so the
        // re-emitted title is byte-identical to the writer's input.
        if let Some(title) = children_named(root, "work")
            .next()
            .and_then(|work| child_element(work, "work-title"))
        {
            metadata.title = Some(text_line(raw_text(title)));
        }

        // <identification><creator type="composer"> -> composers. The writer
        // emits ONE <creator> per composer using the raw field text, including a
        // present-but-empty composer as `<creator type="composer"></creator>`;
        // reconstruct every such element (do not drop empty ones) so the whole
        // <identification> block round-trips.
        if let Some(identification) = children_named(root, "identification").next() {
            for creator in children_named(identification, "creator") {
                if creator.attribute("type") == Some("composer") {
                    metadata.composers.push(text_line(raw_text(creator)));
                }
            }
        }

        // <credit><credit-words> -> post-tune words (W:). The writer emits one
        // <credit page="1"> per non-blank post-tune lyric line.
        for credit in children_named(root, "credit") {
            if let Some(words) = child_element(credit, "credit-words") {
                metadata.post_tune_lyrics.push(text_line(raw_text(words)));
            }
        }
    }

    /// Read each `<score-part>` (into its id, `<part-name>`, and MIDI
    /// projections — S3) AND each `<part-group>` (P1a) from the `<part-list>`.
    ///
    /// **P1a — `<part-group>` reading.** `<part-group>` elements interleave with
    /// `<score-part>` elements in document order. A `type="start"` element opens
    /// a group keyed by its `number` attribute; a `type="stop"` with the same
    /// `number` closes it. The closed group's `<group-symbol>` determines the ABC
    /// delimiter (`bracket`/`square` → `'['`, `brace` → `'{'`, `line`/absent →
    /// `'\0'`). Each part id that appeared BETWEEN the start and stop is recorded
    /// in `PartGroupEntry.part_ids`. Nesting is supported via the active-group
    /// stack: an inner group's parts are added to both the inner AND every outer
    /// group so each level's delimiters span its correct range.
    ///
    /// **Forward byte-identity.** croma's own writer never emits `<part-group>`,
    /// so the groups list is always empty for croma-produced XML; the synthesised
    /// `%%score` directive is therefore never added for self-loop files.
    fn read_part_list(&mut self, root: Node<'_, '_>) -> PartListResult {
        let Some(part_list) = children_named(root, "part-list").next() else {
            return PartListResult {
                entries: Vec::new(),
                groups: Vec::new(),
            };
        };

        let mut entries: Vec<PartListEntry> = Vec::new();
        // Each active group: (number, symbol, accumulated part_ids).
        let mut open_groups: Vec<(String, Option<String>, Vec<String>)> = Vec::new();
        // Completed groups, in close order.
        let mut groups: Vec<PartGroupEntry> = Vec::new();

        for child in element_children(part_list) {
            match child.tag_name().name() {
                "score-part" => {
                    let id = child.attribute("id").unwrap_or_default().to_owned();
                    let name = child_text(child, "part-name").map(str::to_owned);
                    let instruments = self.read_part_instruments(child);
                    // Add this part id to every open group (outer → inner).
                    for (_, _, ids) in &mut open_groups {
                        ids.push(id.clone());
                    }
                    entries.push(PartListEntry {
                        id,
                        name,
                        instruments,
                    });
                }
                "part-group" => {
                    let type_attr = child.attribute("type").unwrap_or_default();
                    let number = child.attribute("number").unwrap_or("1").to_owned();
                    match type_attr {
                        "start" => {
                            let symbol = child_text(child, "group-symbol").map(str::to_owned);
                            open_groups.push((number, symbol, Vec::new()));
                        }
                        "stop" => {
                            // Find the matching open group by number (innermost
                            // match, per the MusicXML nesting model).
                            if let Some(pos) =
                                open_groups.iter().rposition(|(n, _, _)| n == &number)
                            {
                                let (_, symbol_opt, part_ids) = open_groups.remove(pos);
                                let symbol = match symbol_opt.as_deref().unwrap_or_default() {
                                    "brace" => '{',
                                    "bracket" | "square" => '[',
                                    _ => '\0', // "line" or absent → no delimiter
                                };
                                if !part_ids.is_empty() {
                                    groups.push(PartGroupEntry { symbol, part_ids });
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Fix 2: any group still open (no matching `stop`) is unbalanced.
        // Emit a warning for each rather than silently dropping it.
        for (number, _, _) in &open_groups {
            self.warn(
                "musicxml.read.unbalanced_part_group",
                format!(
                    "<part-group number=\"{number}\" type=\"start\"> has no matching stop; \
                     the group is dropped"
                ),
            );
        }

        PartListResult { entries, groups }
    }

    /// Invert [`MusicXmlWriter::write_part_instruments`]: read every
    /// `<score-instrument>` / `<midi-instrument>` child of a `<score-part>` into
    /// explicit part instrument metadata. The exact inverses of the writer's
    /// MIDI emission are:
    ///
    /// - `<midi-channel>n` -> `channel = n`,
    /// - `<midi-program>N` -> `program = N - 1` (forward is `program + 1`),
    /// - `<volume>v` -> `volume_cc = round(v * 1.27)` (forward `{:.2}` of `cc/1.27`),
    /// - `<pan>p` -> `pan_cc = round((p + 90) * 127 / 180)` (forward `{:.2}` of
    ///   `cc/127*180 - 90`),
    /// - `<midi-unpitched>n` -> `midi_unpitched = n`.
    ///
    /// Score-instrument-only entries are kept too, because their human names
    /// carry sheet/playback identity even when no MIDI payload is present.
    fn read_part_instruments(
        &mut self,
        score_part: Node<'_, '_>,
    ) -> Vec<MusicXmlPartInstrumentModel> {
        let mut instruments: Vec<MusicXmlPartInstrumentModel> = Vec::new();
        for node in children_named(score_part, "score-instrument") {
            let Some(id) = node
                .attribute("id")
                .map(str::trim)
                .filter(|id| !id.is_empty())
            else {
                continue;
            };
            let name = child_element(node, "instrument-name").map(|name| TextLine {
                text: raw_text(name).to_owned(),
                span: READER_SPAN,
            });
            instruments.push(MusicXmlPartInstrumentModel {
                id: id.to_owned(),
                name,
                midi: None,
                span: READER_SPAN,
            });
        }

        for node in children_named(score_part, "midi-instrument") {
            let Some(id) = node
                .attribute("id")
                .map(str::trim)
                .filter(|id| !id.is_empty())
            else {
                continue;
            };
            let Some(midi) = self.read_midi_instrument(node) else {
                continue;
            };
            if let Some(instrument) = instruments
                .iter_mut()
                .find(|instrument| instrument.id == id)
            {
                instrument.midi = Some(midi);
            } else {
                instruments.push(MusicXmlPartInstrumentModel {
                    id: id.to_owned(),
                    name: None,
                    midi: Some(midi),
                    span: READER_SPAN,
                });
            }
        }
        instruments
    }

    fn read_midi_instrument(&mut self, node: Node<'_, '_>) -> Option<MidiInstrumentModel> {
        let channel =
            child_text(node, "midi-channel").and_then(|text| self.parse_u8(text, "midi-channel"));
        // `<midi-program>` is 1-based (forward emits `program + 1`); subtract one
        // to recover the 0-based GM program. A value of 0 is out of range for the
        // writer's 1-based emission, so it warns and is dropped rather than
        // wrapping to 255.
        let program = child_text(node, "midi-program").and_then(|text| {
            let raw = self.parse_u16(text, "midi-program")?;
            match raw.checked_sub(1) {
                Some(value) if value <= u16::from(u8::MAX) => u8::try_from(value).ok(),
                _ => {
                    self.warn(
                        "musicxml.read.invalid_midi_program",
                        format!("<midi-program> `{raw}` is out of the 1-based GM range; ignored"),
                    );
                    None
                }
            }
        });
        let volume_cc = child_text(node, "volume")
            .and_then(|text| self.cc_from_float(text, "volume", |v| v * 1.27));
        let pan_cc = child_text(node, "pan")
            .and_then(|text| self.cc_from_float(text, "pan", |p| (p + 90.0) * 127.0 / 180.0));
        let midi_unpitched = child_text(node, "midi-unpitched").and_then(|text| {
            let raw = self.parse_u16(text, "midi-unpitched")?;
            if (1..=128).contains(&raw) {
                u8::try_from(raw).ok()
            } else {
                self.warn(
                    "musicxml.read.invalid_midi_unpitched",
                    format!("<midi-unpitched> `{raw}` is outside the valid 1-128 range; ignored"),
                );
                None
            }
        });

        let model = MidiInstrumentModel {
            program,
            channel,
            volume_cc,
            pan_cc,
            midi_unpitched,
            span: READER_SPAN,
        };
        // Drop an instrument that recovered nothing the writer would emit (so it
        // does not re-write a spurious empty <midi-instrument>).
        model.has_content().then_some(model)
    }

    /// Invert one of the writer's `{:.2}` float CCs back to the exact integer it
    /// started from. `to_cc` maps the parsed float to the (real-valued) CC, which
    /// is rounded to the nearest integer. The exhaustive `0..=127` stability unit
    /// test proves this recovers the original CC for every value the writer can
    /// emit; a value that rounds outside the valid `0..=127` MIDI CC range can
    /// only come from a hand-edited file, so it warns and is dropped (never a
    /// panicking cast).
    fn cc_from_float(&mut self, text: &str, label: &str, to_cc: impl Fn(f64) -> f64) -> Option<u8> {
        let value: f64 = match text.trim().parse() {
            Ok(value) => value,
            Err(_) => {
                self.warn(
                    "musicxml.read.invalid_float",
                    format!("<{label}> `{}` is not a number; ignored", text.trim()),
                );
                return None;
            }
        };
        let cc = to_cc(value).round();
        if (0.0..=127.0).contains(&cc) {
            Some(cc as u8)
        } else {
            self.warn(
                "musicxml.read.cc_out_of_range",
                format!(
                    "<{label}> `{}` maps to CC {cc}, outside the valid 0-127 range; ignored",
                    text.trim()
                ),
            );
            None
        }
    }

    fn read_part(
        &mut self,
        part_node: Node<'_, '_>,
        divisions: u32,
        part_list: &[PartListEntry],
        capture_header_tempo: bool,
    ) -> PartOutcome {
        let id = part_node.attribute("id").unwrap_or_default().to_owned();
        let entry = part_list.iter().find(|entry| entry.id == id);
        let name = entry
            .and_then(|entry| entry.name.clone())
            .filter(|name| !name.is_empty())
            .map(text_line);
        // S3/S6d: the writer emits one `<midi-instrument>` per voice that carried
        // `%%MIDI` sound metadata, in voice order; each is attached to the voice
        // at the same index during voice assembly below.
        let staff_id = StaffId {
            value: 1,
            span: READER_SPAN,
        };

        // S6d: the writer interleaves a part's voices with `<backup>` and a
        // per-sequence `<voice>` number. The reader reconstructs each `<voice>`
        // as a separate `Part.voices` entry (a `TimedEvent` voice, reusing the
        // full per-note machinery) so the writer's `measure_sequences` re-emits
        // the identical `<voice>1` .. `<backup>` .. `<voice>2` interleaving:
        // `part.voices[i]` is numbered `i + 1`, matching the `<voice>` we read.
        // `voice_accumulators` is ordered by first appearance and sorted by voice
        // number at the end so `<voice>1` becomes `part.voices[0]`.
        let mut voice_accumulators: Vec<VoiceAccumulator> = Vec::new();
        // The header tempo direction (if any) is captured from the FIRST measure,
        // before its first note, and only in part 1. Once captured it is never
        // overwritten, and later tempo directions become `TempoChange` events.
        let mut header_tempo: Option<TempoModel> = None;

        for (position, measure_node) in children_named(part_node, "measure").enumerate() {
            let measure_id = self.read_measure_id(measure_node, position);
            // Only part 1's first measure may yield the score header tempo; in
            // every other measure a voice-less tempo direction is a mid-tune
            // change. (`capture_header_tempo` is the part-1 flag; the first
            // measure is position 0.)
            let is_part_first_measure = position == 0;
            let capture_header = capture_header_tempo && is_part_first_measure;
            let outcome = self.read_measure(
                measure_node,
                divisions,
                measure_id,
                capture_header,
                is_part_first_measure,
            );
            if header_tempo.is_none() {
                header_tempo = outcome.header_tempo;
            }

            // The PRIMARY voice (`"1"`) is canonical: it gets a [`Measure`] for
            // EVERY `<measure>` element — including a content-less trailing bar —
            // so the part's measure list (and thus the writer's measure count) is
            // preserved exactly. It also carries the measure skeleton
            // (barlines/endings/multiple-rest), which the writer reads from any
            // voice's measure via a deduping union; attaching it to one voice
            // avoids double-emission. Extra voices get a minimal measure only in
            // the measures where they actually have content.
            const PRIMARY_VOICE: &str = "1";
            let mut primary_measure: Option<VoiceMeasure> = None;
            for (voice, voice_measure) in outcome.voices {
                if voice == PRIMARY_VOICE {
                    primary_measure = Some(voice_measure);
                    continue;
                }
                let accumulator = voice_accumulator(&mut voice_accumulators, &voice);
                accumulator.events.extend(voice_measure.events);
                accumulator.measures.push(Measure {
                    id: measure_id,
                    source_span: READER_SPAN,
                    expected_duration: voice_measure.expected_duration,
                    actual_duration: voice_measure.actual_duration,
                    multiple_rest: None,
                    pickup: false,
                    complete: true,
                    barlines: Vec::new(),
                    repeat_endings: Vec::new(),
                    overlays: Vec::new(),
                });
            }
            // Primary voice: push its events (if any) and the skeleton measure
            // (empty when the bar had no voice-1 content, e.g. a trailing barline
            // bar that the writer still emits as `<measure></measure>`).
            let (events, expected, actual) = match primary_measure {
                Some(vm) => (vm.events, vm.expected_duration, vm.actual_duration),
                None => (Vec::new(), None, Fraction::zero()),
            };
            let primary = voice_accumulator(&mut voice_accumulators, PRIMARY_VOICE);
            primary.events.extend(events);
            primary.measures.push(Measure {
                expected_duration: expected,
                actual_duration: actual,
                ..outcome.measure.clone()
            });
        }

        // S2: reconstruct the part's header `<clef>` and `<transpose>` from the
        // first measure's `<attributes>`. The writer reads the staff voice's
        // `initial_properties.clef` (so it must be populated for re-emission) and
        // either `properties.transpose` (ABC text, unreconstructable from XML) or
        // `midi_transpose`; setting `midi_transpose` reproduces `<transpose>`.
        let header_attributes = children_named(part_node, "measure")
            .next()
            .and_then(|measure| child_element(measure, "attributes"));
        let mut initial_properties = VoicePropertiesModel::default();
        let mut midi_transpose = None;
        if let Some(attributes) = header_attributes {
            initial_properties.clef = self.read_clef(attributes);
            midi_transpose = self.read_transpose(attributes);
        }

        // Order voices by their numeric `<voice>` value so `<voice>1` ->
        // `part.voices[0]` (numbered `0 + 1` on re-write). A part with no notes at
        // all still reconstructs a single empty voice so the writer emits a part.
        voice_accumulators.sort_by_key(|accumulator| parse_voice_number(&accumulator.voice));
        if voice_accumulators.is_empty() {
            voice_accumulators.push(VoiceAccumulator {
                voice: "1".to_owned(),
                events: Vec::new(),
                measures: Vec::new(),
            });
        }

        let voices: Vec<Voice> = voice_accumulators
            .into_iter()
            .enumerate()
            .map(|(index, accumulator)| {
                // S3: the writer emits one `<midi-instrument>` per voice that
                // carried `%%MIDI` sound metadata, in voice order; attach each
                // recovered instrument to the voice at the same index. A voice
                // with no instrument keeps `None` (no extra `<midi-instrument>`).
                let part_list_instrument = entry.and_then(|entry| entry.instruments.get(index));
                let midi_instrument = part_list_instrument.and_then(|instrument| instrument.midi);
                let instrument_name =
                    part_list_instrument.and_then(|instrument| instrument.name.clone());
                // Only the first (staff) voice carries the header clef/transpose
                // and its ABC `transpose=` property; extra voices share the staff
                // but the writer reads clef/transpose from the FIRST qualifying
                // voice per part, so leaving extras default reproduces the single
                // `<clef>`/`<transpose>` emission.
                let (mut init_props, transpose) = if index == 0 {
                    (initial_properties.clone(), midi_transpose)
                } else {
                    (VoicePropertiesModel::default(), None)
                };
                if let Some(instrument_name) = instrument_name {
                    init_props.nm.get_or_insert(instrument_name);
                }
                let voice_id = VoiceId {
                    value: voice_id_value(&id, &accumulator.voice, index),
                    span: READER_SPAN,
                };
                Voice {
                    id: voice_id,
                    staff: staff_id,
                    initial_properties: init_props.clone(),
                    properties: init_props,
                    measures: accumulator.measures,
                    events: accumulator.events,
                    midi_instrument,
                    midi_transpose: transpose,
                    source_span: READER_SPAN,
                }
            })
            .collect();

        let staff_voice_ids: Vec<VoiceId> = voices.iter().map(|voice| voice.id.clone()).collect();

        PartOutcome {
            part: Part {
                id: PartId {
                    value: id,
                    span: READER_SPAN,
                },
                name,
                instruments: entry
                    .map(|entry| entry.instruments.clone())
                    .unwrap_or_default(),
                staves: vec![Staff {
                    id: staff_id,
                    voices: staff_voice_ids,
                    source_span: READER_SPAN,
                }],
                voices,
                source_span: READER_SPAN,
            },
            header_tempo,
        }
    }

    /// `MeasureId.number` is the 1-based `<measure number>`; `index` is the
    /// 0-based position. The writer sorts measures by `(index, number)` and uses
    /// `number` for the emitted attribute, so both must be reconstructed.
    fn read_measure_id(&mut self, measure_node: Node<'_, '_>, position: usize) -> MeasureId {
        let number = measure_node
            .attribute("number")
            .and_then(|raw| raw.trim().parse::<u32>().ok())
            .unwrap_or_else(|| u32::try_from(position + 1).unwrap_or(u32::MAX));
        MeasureId {
            index: u32::try_from(position).unwrap_or(u32::MAX),
            number,
        }
    }

    fn read_measure(
        &mut self,
        measure_node: Node<'_, '_>,
        divisions: u32,
        measure_id: MeasureId,
        capture_header_tempo: bool,
        is_part_first_measure: bool,
    ) -> MeasureOutcome {
        // S6d: the writer interleaves a part's multiple voices with `<backup>`
        // and a per-sequence `<voice>` number. Each voice's notes (and the
        // directions/harmony/notations emitted just before them) form a
        // contiguous region; the reader partitions the measure by `<voice>` into
        // independent [`VoiceMeasureState`]s, each replicating the single-voice
        // reconstruction (its own onset cursor, buffered directions, grace run,
        // chord head, open tuplets, and measure-rest bookkeeping). `voices`
        // preserves first-seen order; it is sorted numerically at assembly so
        // `<voice>1` maps to `part.voices[0]`. `current_voice` selects the active
        // region's state, switched by each note's `<voice>` (a direction/harmony
        // uses its own `<voice>` when present, else the active one).
        let mut voices: Vec<(String, VoiceMeasureState)> = Vec::new();
        let mut current_voice: String = "1".to_owned();
        // The header tempo (`write_initial_directions`) is the FIRST voice-less
        // tempo direction before the first note of part 1's first measure; once
        // captured, later tempo directions are mid-tune `TempoChange` events.
        let mut header_tempo: Option<TempoModel> = None;
        let mut seen_note = false;
        // S6b: the part's first measure opens with the header `<attributes>`
        // (`write_attributes`: `<divisions>` + score `<key>`/`<time>`/`<clef>` +
        // `<transpose>`), already read into `metadata`/`initial_properties`. Skip
        // exactly that ONE leading block; every other `<attributes>` (a second
        // block after notes, or the first block of a non-leading measure) is a
        // mid-tune `KeyChange`/`MeterChange`/`ClefChange` (or, S6d, a
        // `<measure-style><multiple-rest>`). Off the first measure there is no
        // header block, so this stays `true` and every `<attributes>` is treated
        // as a mid-tune change.
        let mut header_attributes_consumed = !is_part_first_measure;
        // S6a: the `<barline>` blocks reconstructed into the measure's
        // `barlines`/`repeat_endings` (measure-level — they belong to voice 1).
        // `next_right_span_start` gives every *trailing* (right) barline a
        // distinct, non-zero `span.start` so the writer's `is_leading_barline`
        // classifies it as a right barline (and so `unique_barlines` keeps
        // document order via the span sort); a *leading* (left) barline keeps
        // `span.start == 0` to match the measure's `READER_SPAN` source span.
        let mut barlines: Vec<MeasureBarline> = Vec::new();
        let mut repeat_endings: Vec<RepeatEndingModel> = Vec::new();
        let mut next_right_span_start: usize = 1;
        // S6d: `<measure-style><multiple-rest>N` (the writer's
        // `write_multiple_rest_measure_style`) is its own `<attributes>` wrapper.
        // It is a measure-level glyph hint (`Measure.multiple_rest`), not a
        // key/meter/clef change.
        let mut multiple_rest: Option<u32> = None;

        for child in element_children(measure_node) {
            match child.tag_name().name() {
                "note" => {
                    // The note's `<voice>` selects (and switches to) its region's
                    // state. The writer emits `<voice>` on every note, so this is
                    // present for croma output; missing → the active voice.
                    let voice = note_voice_number(child).unwrap_or_else(|| current_voice.clone());
                    current_voice = voice.clone();
                    let state = voice_state(&mut voices, &voice);
                    // S6c: a `<grace>` note joins the open grace run (starting one
                    // if needed) and produces NO timed event. It is finalised when
                    // the following main note or the measure end is reached.
                    if child_element(child, "grace").is_some() {
                        let builder = state
                            .grace_builder
                            .get_or_insert_with(GraceGroupBuilder::default);
                        self.push_grace_note(builder, child);
                        seen_note = true;
                        continue;
                    }
                    // A main note terminates any open grace run as a BEFORE-grace
                    // group bound to THIS note (the writer emits a note's grace
                    // groups immediately before it, with nothing in between).
                    if let Some(builder) = state.grace_builder.take() {
                        state.pending_graces.push(builder.finish());
                    }

                    let Some(parsed) = self.read_note(child, divisions) else {
                        continue;
                    };
                    seen_note = true;

                    if parsed.measure_rest {
                        state.measure_rest_duration = Some(parsed.duration);
                    }

                    // S4: reconstruct this note's `<notations>` +
                    // `<time-modification>` into its `EventAttachments`.
                    let mut attachments = self.read_note_attachments(
                        child,
                        &mut state.open_tuplets,
                        &mut state.events,
                    );

                    if parsed.chord_member {
                        // S6c: a `<chord/>` member folds into the previous main
                        // event at the SAME onset, turning a Note into a Chord (or
                        // extending an existing Chord). It carries its own
                        // per-member attachments and never advances the cursor.
                        self.fold_chord_member(
                            &mut state.events,
                            state.last_main_event,
                            parsed,
                            attachments,
                            &mut state.next_chord_span_start,
                        );
                        continue;
                    }

                    // S5a/S5b: prepend any buffered harmony/direction attachments
                    // (the writer emitted them just before this note), then attach
                    // the finalised before-grace groups (emitted between the
                    // directions and the note).
                    prepend_attachments(&mut attachments, std::mem::take(&mut state.pending));
                    // S6e: the pending run bound to this note, so it no longer needs
                    // a trailing Spacer anchor.
                    state.pending_insert_index = None;
                    attachments
                        .grace_groups
                        .extend(std::mem::take(&mut state.pending_graces));

                    state.last_main_event = Some(state.events.len());
                    state.events.push(TimedEvent {
                        measure: measure_id,
                        onset: state.cursor,
                        duration: parsed.duration,
                        source: READER_SPAN,
                        kind: parsed.kind,
                        attachments,
                    });

                    state.cursor = state.cursor.checked_add(parsed.duration);
                    state.max_cursor = max_fraction(state.max_cursor, state.cursor);
                }
                "harmony" => {
                    // S5b: the writer emits a chord symbol's `<harmony>` from
                    // `write_harmony_and_directions`, BEFORE the event's
                    // `<direction>`s and `<note>`. Buffer it onto the owning
                    // voice's `pending` so it flushes (chord_symbols first) onto
                    // the next event. A `<harmony>` carries no `<voice>`; it
                    // belongs to the active region's voice.
                    if let Some(symbol) = self.read_harmony(child) {
                        let state = voice_state(&mut voices, &current_voice);
                        // S6e: anchor a new pending run at the current event index
                        // (document order) before buffering, so a surviving run
                        // becomes a Spacer in the right place vs same-onset changes.
                        state.mark_pending_position();
                        state.pending.chord_symbols.push(symbol);
                    }
                }
                "direction" => {
                    match self.read_direction(child) {
                        // A voice-less tempo direction: the header tempo when it
                        // precedes part 1's first note, else a mid-tune change.
                        ParsedDirection::Tempo(tempo) => {
                            if capture_header_tempo && !seen_note && header_tempo.is_none() {
                                header_tempo = Some(tempo);
                            } else {
                                // The writer emits a TempoChange's own attachments
                                // (any directions right before it) before the
                                // metronome, so the buffered directions belong to
                                // this zero-duration event. A tempo is voice-less
                                // but lowers within a voice's sequence; route it to
                                // the active region's voice.
                                let state = voice_state(&mut voices, &current_voice);
                                state.events.push(TimedEvent {
                                    measure: measure_id,
                                    onset: state.cursor,
                                    duration: Fraction::zero(),
                                    source: READER_SPAN,
                                    kind: TimedEventKind::TempoChange(tempo),
                                    attachments: std::mem::take(&mut state.pending),
                                });
                                // S6e: this tempo change consumed the pending run.
                                state.pending_insert_index = None;
                            }
                        }
                        // A `<rehearsal>` section label: push a zero-duration
                        // SectionLabel at the active region's voice/cursor,
                        // mirroring the TempoChange arm. It carries no `<voice>`
                        // but lowers within a voice's sequence, exactly like the
                        // writer emitted it.
                        ParsedDirection::SectionLabel(label) => {
                            // A positioned section label occupies the leading slot
                            // just like a note: a `<metronome>` AFTER it is a
                            // mid-tune `TempoChange`, not the header tempo. Without
                            // this, a body `P:A` written before an in-sequence
                            // `Q:` (the writer orders both zero-duration events by
                            // source position) would re-import the tempo as the
                            // header tempo, which always re-writes FIRST — swapping
                            // the rehearsal and metronome and breaking the
                            // round-trip. Setting `seen_note` keeps the tempo a
                            // mid-tune event so re-write preserves their order.
                            seen_note = true;
                            let state = voice_state(&mut voices, &current_voice);
                            state.events.push(TimedEvent {
                                measure: measure_id,
                                onset: state.cursor,
                                duration: Fraction::zero(),
                                source: READER_SPAN,
                                kind: TimedEventKind::SectionLabel(label),
                                attachments: std::mem::take(&mut state.pending),
                            });
                            // This label consumed any pending run, like the tempo
                            // change above.
                            state.pending_insert_index = None;
                        }
                        // A voice-bearing direction (annotation words, dynamics,
                        // coda/segno, wedge): buffer it for the next event of its
                        // `<voice>` (falling back to the active region's voice).
                        ParsedDirection::Event(attachments) => {
                            let voice = direction_voice_number(child)
                                .unwrap_or_else(|| current_voice.clone());
                            current_voice = voice.clone();
                            let state = voice_state(&mut voices, &voice);
                            // S6e: anchor a new pending run at the current event
                            // index before buffering (see the harmony arm).
                            state.mark_pending_position();
                            state.pending.extend(*attachments);
                        }
                        ParsedDirection::Ignored => {}
                    }
                }
                "forward" => {
                    if let Some(duration) = self.read_duration(child, divisions) {
                        let state = voice_state(&mut voices, &current_voice);
                        state.cursor = state.cursor.checked_add(duration);
                        state.max_cursor = max_fraction(state.max_cursor, state.cursor);
                    }
                }
                "backup" => {
                    // A `<backup>` rewinds the active region's cursor; the writer
                    // emits it to return to onset 0 before the next voice. The next
                    // note's `<voice>` switches state, and that voice's cursor
                    // already starts at 0, so the rewind only affects the region it
                    // closes (harmless), keeping each voice's onsets relative to 0.
                    if let Some(duration) = self.read_duration(child, divisions) {
                        let state = voice_state(&mut voices, &current_voice);
                        state.cursor = subtract_fraction(state.cursor, duration);
                    }
                }
                "barline" => {
                    self.read_barline(
                        child,
                        &mut barlines,
                        &mut repeat_endings,
                        &mut next_right_span_start,
                    );
                    // S6a×S5a: a leading barline (e.g. an opening `|:`) precedes
                    // the measure's content. The writer emits a true HEADER tempo
                    // (`write_initial_directions`) BEFORE any left barline, so a
                    // tempo direction seen *after* a barline cannot be the header
                    // tempo — it is a body-position `Q:`/`[Q:..]` TempoChange at the
                    // measure start. Mark content as started so the header-tempo
                    // capture below does not wrongly promote it. (Idempotence-only:
                    // a tempo with no preceding barline is unaffected.)
                    seen_note = true;
                }
                "attributes" => {
                    // S6d: a `<measure-style><multiple-rest>N>` is a measure-level
                    // glyph hint regardless of header status; capture it first.
                    if let Some(count) = read_multiple_rest(child) {
                        multiple_rest = Some(count);
                    }
                    // The header `<attributes>` (the part's first measure's first
                    // block) is read elsewhere into `metadata`/`initial_properties`;
                    // skip it once. Any OTHER `<attributes>` is a mid-tune change
                    // (its key/meter/clef children) routed to the active voice.
                    if !header_attributes_consumed {
                        header_attributes_consumed = true;
                    } else {
                        let cursor = voice_state(&mut voices, &current_voice).cursor;
                        let state = voice_state(&mut voices, &current_voice);
                        self.read_mid_measure_attributes(
                            child,
                            measure_id,
                            cursor,
                            &mut state.events,
                            &mut state.pending,
                        );
                        // S6b×S5a: the writer emits a true HEADER tempo
                        // (`write_initial_directions`) BEFORE any measure-sequence
                        // event, so a mid-measure `<attributes>` change always
                        // precedes it. A tempo direction seen *after* a mid-tune
                        // change is therefore a body-position `Q:` `TempoChange`
                        // (sorted by onset behind this change), NOT the header
                        // tempo. Mark content started so the header-tempo capture
                        // does not wrongly promote it (cf. the barline arm above).
                        seen_note = true;
                    }
                }
                // <lyric> etc. are read in their own arms / later stages.
                _ => {}
            }
        }

        // Finalise each voice's region: drain an open after-grace run and a
        // trailing direction/harmony Spacer, then compute the measure durations.
        let mut voice_measures: Vec<(String, VoiceMeasure)> = Vec::new();
        for (voice, mut state) in voices {
            // S6c: a grace run still open at the END of the measure had no
            // following main note, so it is an AFTER-grace group bound to the most
            // recent main (note/chord) event — the writer emits after-grace notes
            // right after the owner note, landing them at the measure tail. With no
            // preceding main event the group is dropped with a diagnostic rather
            // than fabricated onto nothing.
            if let Some(builder) = state.grace_builder.take() {
                let group = builder.finish();
                match state.last_main_event.and_then(|index| state.events.get_mut(index)) {
                    Some(event) => event.attachments.after_grace_groups.push(group),
                    None => self.warn(
                        "musicxml.read.orphan_grace",
                        "a trailing <grace> run has no preceding note to bind as an after-grace; dropped",
                    ),
                }
            }
            // `pending_graces` always drains onto the first following main note
            // for clean croma output, so it is normally empty here. It can only be
            // non-empty for input the writer never emits — e.g. a `<grace>` run
            // immediately followed by a `<chord/>` member note (the member folds
            // into the previous event and `continue`s before the buffered before-
            // grace groups flush). Totality (design §2.2/§6) forbids a panic even in
            // debug/test builds, so instead of a `debug_assert!` the reader degrades
            // gracefully: re-bind the orphaned groups to the most recent main event
            // as BEFORE-grace groups (preserving their content and order — they did
            // precede that event's chord), or drop them with a diagnostic when there
            // is no main event to host them. Either way it never panics and never
            // changes behaviour on clean input (where the loop below is a no-op).
            if !state.pending_graces.is_empty() {
                let groups = std::mem::take(&mut state.pending_graces);
                match state
                    .last_main_event
                    .and_then(|index| state.events.get_mut(index))
                {
                    Some(event) => {
                        // Prepend so the orphaned before-grace groups keep their
                        // original position ahead of any already on the event.
                        let mut merged = groups;
                        merged.append(&mut event.attachments.grace_groups);
                        event.attachments.grace_groups = merged;
                    }
                    None => self.warn(
                        "musicxml.read.orphan_before_grace",
                        "a <grace> run had no following note to bind as a before-grace group; dropped",
                    ),
                }
            }

            // S5a/S5b: directions or chord symbols with no following note in this
            // voice's region (a pre-barline `!segno!`, a trailing `"C"` chord, or a
            // note-less measure carrying only an annotation) are emitted by the
            // writer on a zero-duration `Spacer` whose `write_event` emits its
            // harmony/directions then nothing. Reconstruct that Spacer so the
            // trailing attachments re-emit at the region's end.
            if !state.pending_is_empty() {
                let spacer = TimedEvent {
                    measure: measure_id,
                    onset: state.cursor,
                    duration: Fraction::zero(),
                    source: READER_SPAN,
                    kind: TimedEventKind::Spacer,
                    attachments: std::mem::take(&mut state.pending),
                };
                // S6e ordering fix. The Spacer must keep its DOCUMENT-ORDER position
                // relative to any same-onset mid-tune change events. The writer's
                // `write_event` emits an event's attachments before its body, and
                // `measure_sequences` stable-sorts by `(onset, source.start)`; with
                // both the Spacer and the change events at the same READER_SPAN
                // onset/start, insertion order is the only tiebreaker the sort sees.
                // `pending_insert_index` recorded `events.len()` at the moment this
                // run's first attachment was buffered, so it sits exactly where the
                // direction/harmony appeared in the XML walk:
                //   - `"Trio"[K:F]|` (direction THEN change, no note) → index 0,
                //     before the KeyChange pushed afterwards → `<direction>` then
                //     `<attributes>` (matches the writer).
                //   - `[K:..]"E"|` (change THEN harmony, no note) → index after the
                //     KeyChange → `<attributes>` then `<harmony>` (also matches).
                //   - a pre-barline `!segno!` after notes → index past the notes
                //     (= append), unchanged.
                // The `"^hi"[K:G] E` case (direction before change THEN a note)
                // never reaches here: the run drained onto that note during the walk
                // (clearing the index), so the annotation stays on the note.
                match state.pending_insert_index {
                    Some(index) if index <= state.events.len() => {
                        state.events.insert(index, spacer);
                    }
                    _ => state.events.push(spacer),
                }
            }

            // A measure rest forces `expected == actual == rest.duration` at onset
            // 0; otherwise leave `expected` unset so ordinary rests stay plain (the
            // writer's measure-rest predicate is `expected.is_some_and(...)`).
            // `actual` is the furthest cursor reached.
            let (expected_duration, actual_duration) = match state.measure_rest_duration {
                Some(duration) => (Some(duration), duration),
                None => (None, state.max_cursor),
            };
            voice_measures.push((
                voice,
                VoiceMeasure {
                    events: state.events,
                    expected_duration,
                    actual_duration,
                },
            ));
        }
        // Order voices by their numeric `<voice>` value so `"1"` maps to
        // `part.voices[0]` (the writer numbers `part.voices[i]` as `i + 1`).
        voice_measures.sort_by_key(|(voice, _)| parse_voice_number(voice));

        MeasureOutcome {
            voices: voice_measures,
            header_tempo,
            measure: Measure {
                id: measure_id,
                source_span: READER_SPAN,
                // The measure skeleton's own duration belongs to voice 1; the
                // per-voice durations are carried in `VoiceMeasure` and applied at
                // assembly (`read_part`). Defaults here are overwritten there.
                expected_duration: None,
                actual_duration: Fraction::zero(),
                multiple_rest,
                pickup: false,
                complete: true,
                barlines,
                repeat_endings,
                overlays: Vec::new(),
            },
        }
    }

    /// S6b: invert a NON-leading `<attributes>` block — a mid-tune
    /// `KeyChange`/`MeterChange`/`ClefChange`. The writer emits each such change
    /// as its OWN minimal `<attributes>` wrapper at the event's cursor position
    /// ([`MusicXmlWriter::write_mid_tune_key`] / `write_mid_tune_meter` /
    /// `write_mid_tune_clef`, dispatched from `write_event` for the
    /// [`TimedEventKind::KeyChange`]/`MeterChange`/`ClefChange` variants). A single
    /// croma-emitted mid-tune block therefore holds exactly one of `<key>`/
    /// `<time>`/`<clef>`; to stay robust the reader walks every child in document
    /// order and emits one zero-duration event per recognised sub-element.
    ///
    /// **Onset and ordering.** Each event is placed at the current `cursor` (the
    /// position the preceding notes advanced to) with `Fraction::zero()` duration,
    /// exactly as the lowering creates these events (`lower::timeline`). The event
    /// is pushed onto `events` in document order; `measure_sequences` sorts by
    /// `(onset, source.start)` with a STABLE sort, and the reconstructed event's
    /// `source` is [`READER_SPAN`] (`start == 0`) like the surrounding notes, so a
    /// change at onset N re-sorts after the notes already emitted at earlier onsets
    /// and before the following note at onset N — reproducing the writer's
    /// interleaving. The change is zero-duration, so it never advances the cursor.
    ///
    /// A buffered direction/harmony in `pending` is left untouched (it flushes onto
    /// the next note): croma's lowering gives these change events empty
    /// attachments, so a direction preceding an inline change attaches to the
    /// following note, not the change.
    fn read_mid_measure_attributes(
        &mut self,
        attributes: Node<'_, '_>,
        measure_id: MeasureId,
        cursor: Fraction,
        events: &mut Vec<TimedEvent>,
        pending: &mut EventAttachments,
    ) {
        for child in element_children(attributes) {
            let kind = match child.tag_name().name() {
                "key" => Some(TimedEventKind::KeyChange(self.read_key(child))),
                "time" => self.read_meter(child).map(TimedEventKind::MeterChange),
                "clef" => Some(TimedEventKind::ClefChange(self.read_clef_change(child))),
                // `<divisions>` never appears in a croma mid-tune block (the writer
                // only emits it in the header `write_attributes`); other children
                // are not croma's output. Ignore with a diagnostic so a hand-edited
                // file does not silently lose data.
                "divisions" => {
                    self.warn(
                        "musicxml.read.mid_measure_divisions",
                        "<divisions> inside a mid-measure <attributes> is not reconstructed",
                    );
                    None
                }
                // S6d: `<measure-style><multiple-rest>` is read in the caller's
                // `<attributes>` arm into `Measure.multiple_rest`; it is not a
                // key/meter/clef change, so ignore it silently here.
                "measure-style" => None,
                other => {
                    self.warn(
                        "musicxml.read.unsupported_mid_measure_attribute",
                        format!(
                            "<attributes> child <{other}> mid-measure has no Score event inverse; skipped"
                        ),
                    );
                    None
                }
            };
            if let Some(kind) = kind {
                events.push(TimedEvent {
                    measure: measure_id,
                    onset: cursor,
                    duration: Fraction::zero(),
                    source: READER_SPAN,
                    kind,
                    attachments: EventAttachments::default(),
                });
            }
        }
        // `pending` is intentionally not consumed here; it flushes onto the next
        // note (the change events carry no attachments, matching the lowering).
        let _ = pending;
    }

    /// S6b: reconstruct a [`ClefChangeModel`] from a mid-tune `<clef>` element.
    /// Unlike the header clef ([`Reader::read_clef`]), a mid-tune clef ALWAYS
    /// re-emits a `<clef>` (the writer's `write_mid_tune_clef` is unconditional),
    /// so a default treble clef maps to the explicit canonical `"treble"` text
    /// rather than `None` — `clef_model` re-maps `"treble"` to the same G/2 element.
    fn read_clef_change(&mut self, clef_node: Node<'_, '_>) -> ClefChangeModel {
        let sign = child_text(clef_node, "sign").unwrap_or("G");
        let line = child_text(clef_node, "line").unwrap_or("2");
        let octave_change = child_text(clef_node, "clef-octave-change")
            .and_then(|text| self.parse_i8(text, "clef-octave-change"))
            .unwrap_or(0);
        let text = clef_text_from(sign, line, octave_change).unwrap_or_else(|| "treble".to_owned());
        ClefChangeModel {
            clef: text_line(text),
            source_span: READER_SPAN,
        }
    }

    /// S6a: invert one `<barline>` block — the inverse of
    /// [`MusicXmlWriter::write_barline`] / [`MusicXmlWriter::write_ending_barline`]
    /// and the left/right placement in [`MusicXmlWriter::write_part`].
    ///
    /// A `<barline>` carries up to three things the writer emits, in order:
    /// `<bar-style>`, `<ending>`(s), `<repeat>`. The reader reconstructs:
    ///
    /// - the **bar-style + repeat** into a [`MeasureBarline`] whose `kind` is the
    ///   exact inverse of the writer's forward map *disambiguated by `location` and
    ///   the `<repeat>` direction* (see [`barline_kind_from`]). The combined
    ///   `RepeatBoth` is **decomposed**: a `light-heavy` + `repeat="backward"`
    ///   right barline is read as a plain [`BarlineKind::RepeatEnd`] and the
    ///   matching `heavy-light` + `repeat="forward"` left barline of the next
    ///   measure as a leading [`BarlineKind::RepeatStart`]. The writer re-emits a
    ///   `RepeatEnd`-then-`RepeatStart` pair byte-identically to a `RepeatBoth`
    ///   (verified: `::` and `:|` `|:` produce the same XML), so the reader never
    ///   needs to materialise `RepeatBoth` and avoids the trailing/deferred-repeat
    ///   ambiguity entirely.
    /// - an `<ending type="start">` into a [`RepeatEndingModel`] on this measure.
    ///   The `<ending type="stop">` / `"discontinue">` closers are **not** stored:
    ///   the writer regenerates them from the open-bracket positions plus barline
    ///   kinds (`ending_stop_schedule`), both of which the reader reconstructs from
    ///   the same XML, so re-emission reproduces the identical stop placement.
    ///
    /// **Span discipline (placement, idempotence-invisible).** The writer chooses a
    /// barline's side via `is_leading_barline` = `measure.source_span.start ==
    /// barline.span.start`. The measure's reconstructed `source_span` is
    /// [`READER_SPAN`] (`start == 0`), so a `location="left"` barline is given
    /// `span.start == 0` (leading → emitted left) and each `location="right"`
    /// barline a distinct non-zero `span.start` (trailing → emitted right, in
    /// document order via the writer's span sort). The writer never emits spans, so
    /// these synthetic spans are invisible to the idempotence gate; they exist only
    /// to drive the left/right placement the gate then verifies.
    fn read_barline(
        &mut self,
        barline: Node<'_, '_>,
        barlines: &mut Vec<MeasureBarline>,
        repeat_endings: &mut Vec<RepeatEndingModel>,
        next_right_span_start: &mut usize,
    ) {
        let is_left = barline.attribute("location") == Some("left");
        let bar_style = child_text(barline, "bar-style");
        let repeat_direction = child_element(barline, "repeat")
            .and_then(|repeat| repeat.attribute("direction"))
            .map(str::to_owned);

        // An `<ending type="start">` opens a volta bracket on this measure. The
        // stop/discontinue closers are schedule-regenerated, so only starts are
        // reconstructed.
        for ending in children_named(barline, "ending") {
            if ending.attribute("type") == Some("start")
                && let Some(model) = self.read_ending(ending)
            {
                repeat_endings.push(model);
            }
        }

        // The bar-style + repeat together name the `MeasureBarline` kind. A
        // barline with neither (an ending-only `<barline>`) contributes no
        // `MeasureBarline` — the ending alone was reconstructed above.
        if bar_style.is_none() && repeat_direction.is_none() {
            return;
        }
        let Some(kind) = barline_kind_from(bar_style, repeat_direction.as_deref(), is_left) else {
            self.warn(
                "musicxml.read.unsupported_barline",
                format!(
                    "<barline location={:?}> bar-style {:?} / repeat {:?} has no BarlineKind inverse; ignored",
                    barline.attribute("location").unwrap_or(""),
                    bar_style.unwrap_or(""),
                    repeat_direction.as_deref().unwrap_or(""),
                ),
            );
            return;
        };

        // Leading (left) barlines keep span.start == 0 to match the measure's
        // READER_SPAN; trailing (right) barlines get a distinct non-zero start so
        // `is_leading_barline` is false and the writer's span sort preserves order.
        let span = if is_left {
            READER_SPAN
        } else {
            let start = *next_right_span_start;
            *next_right_span_start = next_right_span_start.saturating_add(1);
            Span { start, end: start }
        };
        barlines.push(MeasureBarline { kind, span });
    }

    /// S6a: invert one `<ending type="start" number="...">` into a
    /// [`RepeatEndingModel`]. Inverse of the writer's `ending_display` +
    /// `unique_endings`:
    ///
    /// - a text-bearing `<ending>` (the writer emits the source label as element
    ///   text, with `number="33"`) → a single [`RepeatEndingPartModel::Text`]; the
    ///   `number` is regenerated from the `Text` variant on re-write, so it is not
    ///   read.
    /// - otherwise the `number` attribute is a comma-separated pass list: each
    ///   `"s-e"` token → [`RepeatEndingPartModel::Range`], each plain `"n"` token →
    ///   [`RepeatEndingPartModel::Single`]. An unparsable list warns and yields
    ///   `None` (no bracket is fabricated).
    fn read_ending(&mut self, ending: Node<'_, '_>) -> Option<RepeatEndingModel> {
        // A non-empty text body is a `["label"` ending; the label is the source.
        if let Some(text) = node_text(ending) {
            return Some(RepeatEndingModel {
                span: READER_SPAN,
                endings: vec![RepeatEndingPartModel::Text(text.to_owned())],
            });
        }

        let number = match ending.attribute("number") {
            Some(number) => number.trim(),
            None => {
                self.warn(
                    "musicxml.read.ending_without_number",
                    "<ending type=\"start\"> has no number and no text; not reconstructed",
                );
                return None;
            }
        };
        let mut parts = Vec::new();
        for token in number.split(',') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            if let Some((start, end)) = token.split_once('-') {
                match (start.trim().parse::<u32>(), end.trim().parse::<u32>()) {
                    (Ok(start), Ok(end)) => {
                        parts.push(RepeatEndingPartModel::Range { start, end });
                    }
                    _ => {
                        self.warn(
                            "musicxml.read.invalid_ending_range",
                            format!("<ending number> range `{token}` is not `start-end`; skipped"),
                        );
                    }
                }
            } else {
                match token.parse::<u32>() {
                    Ok(value) => parts.push(RepeatEndingPartModel::Single(value)),
                    Err(_) => self.warn(
                        "musicxml.read.invalid_ending_number",
                        format!("<ending number> token `{token}` is not a u32; skipped"),
                    ),
                }
            }
        }
        if parts.is_empty() {
            self.warn(
                "musicxml.read.empty_ending_number",
                format!("<ending number=\"{number}\"> yielded no pass numbers; not reconstructed"),
            );
            return None;
        }
        Some(RepeatEndingModel {
            span: READER_SPAN,
            endings: parts,
        })
    }

    /// Read one **main** (non-grace) `<note>` into the data a [`TimedEvent`]
    /// needs. Grace notes are collected separately ([`Reader::push_grace_note`])
    /// before this is called, so the `<grace>` guard here is only defensive;
    /// returning `None` skips a note the reader cannot turn into a timed event.
    fn read_note(&mut self, note_node: Node<'_, '_>, divisions: u32) -> Option<ParsedNote> {
        let chord_member = child_element(note_node, "chord").is_some();
        if child_element(note_node, "grace").is_some() {
            // Grace notes are handled by the grace-run collector; a grace note
            // reaching here would carry no <duration> and is skipped defensively.
            return None;
        }

        let duration = self.read_duration(note_node, divisions).unwrap_or_else(|| {
            self.warn(
                "musicxml.read.note_missing_duration",
                "<note> has no <duration>; treated as zero-length",
            );
            Fraction::zero()
        });

        if let Some(rest_node) = child_element(note_node, "rest") {
            let visibility = if note_node.attribute("print-object") == Some("no") {
                RestVisibility::Invisible
            } else {
                RestVisibility::Visible
            };
            let measure_rest = rest_node.attribute("measure") == Some("yes");
            return Some(ParsedNote {
                kind: TimedEventKind::Rest(RestEvent { visibility }),
                duration,
                chord_member: false,
                measure_rest,
            });
        }

        let pitch = self.read_pitch(note_node)?;
        // The writer emits a `<accidental>` element only for an explicit written
        // accidental (when `preserve_explicit_accidentals` is set, which the
        // reconstructed score's policy is). Inverting it keeps simple accidental
        // notes idempotent; absent `<accidental>` means no written accidental.
        let written_accidental = child_text(note_node, "accidental")
            .and_then(|name| self.accidental_from_name(name))
            .map(|kind| AccidentalMark {
                kind,
                explicit: true,
                courtesy: false,
                source: READER_SPAN,
            });
        Some(ParsedNote {
            kind: TimedEventKind::Note(NoteEvent {
                pitch,
                written_accidental,
                chord_member,
            }),
            duration,
            chord_member,
            measure_rest: false,
        })
    }

    /// S6c: add one `<grace>` `<note>` to the open grace run, inverting
    /// [`MusicXmlWriter::write_grace_group`]. The element is one of:
    ///
    /// - a grace **rest** (`<rest>`) -> a [`GraceEventKind::Rest`] event;
    /// - a grace **chord member** (carries `<chord/>`) -> appended to the previous
    ///   grace event's [`GraceEventKind::Chord`] (promoting a lone grace `Note`
    ///   into a `Chord` on the first member);
    /// - a plain grace **note** -> a new [`GraceEventKind::Note`] event.
    ///
    /// `note_count` (the writer's grace base-unit selector) counts grace
    /// *elements*, so a grace chord increments it by ONE — handled by only
    /// counting non-`<chord/>` notes and rests. Each note's display duration
    /// (`<type>`/`<dots>`) is recorded raw; the `length_multiplier` is recovered at
    /// [`GraceGroupBuilder::finish`] once the base unit (1/8 for a single-element
    /// group, else 1/16) is known. A grace's `<slur>` (and the slur that opened
    /// before the brace, which the writer also emits on the first grace note) is
    /// reconstructed onto its [`GraceEvent`]; the writer re-emits the first note's
    /// `group.slurs ++ event.slurs`, so folding everything into `event.slurs`
    /// (leaving `group.slurs` empty) is byte-identical.
    fn push_grace_note(&mut self, builder: &mut GraceGroupBuilder, note_node: Node<'_, '_>) {
        // A slashed grace (`<grace slash="yes"/>`) marks the whole group as an
        // acciaccatura; the writer emits `slash="yes"` on every grace note of a
        // slashed group, so recording it from the first is sufficient.
        if builder.slash.is_none()
            && let Some(grace) = child_element(note_node, "grace")
            && grace.attribute("slash") == Some("yes")
        {
            builder.slash = Some(READER_SPAN);
        }
        let is_chord_member = child_element(note_node, "chord").is_some();
        let slurs = self.read_grace_slurs(note_node);

        // A grace rest: a `<rest>` with no pitch (never a chord member in croma's
        // output). Recorded as its own grace event.
        if child_element(note_node, "rest").is_some() {
            let visibility = if note_node.attribute("print-object") == Some("no") {
                RestVisibility::Invisible
            } else {
                RestVisibility::Visible
            };
            builder.events.push(GraceEvent {
                source_span: READER_SPAN,
                kind: GraceEventKind::Rest(RestEvent { visibility }),
                slurs,
            });
            return;
        }

        let Some(grace_note) = self.read_grace_note(note_node) else {
            return;
        };

        if is_chord_member {
            // Fold into the previous grace event, turning a lone Note into a Chord
            // (or extending an existing Chord). A leading `<chord/>` with no
            // previous grace event is not croma's output; start a fresh note so
            // nothing is lost.
            if let Some(previous) = builder.events.last_mut() {
                match &mut previous.kind {
                    GraceEventKind::Note(first) => {
                        previous.kind = GraceEventKind::Chord(vec![first.clone(), grace_note]);
                    }
                    GraceEventKind::Chord(members) => members.push(grace_note),
                    GraceEventKind::Rest(_) => {
                        self.warn(
                            "musicxml.read.grace_chord_on_rest",
                            "a grace <chord/> follows a grace rest; treated as a separate note",
                        );
                        builder.events.push(GraceEvent {
                            source_span: READER_SPAN,
                            kind: GraceEventKind::Note(grace_note),
                            slurs,
                        });
                    }
                }
                return;
            }
        }

        builder.events.push(GraceEvent {
            source_span: READER_SPAN,
            kind: GraceEventKind::Note(grace_note),
            slurs,
        });
    }

    /// S6c: reconstruct one grace [`GraceNoteEvent`] (pitch + written accidental +
    /// the raw display-duration fraction the writer spelled into `<type>`/`<dots>`).
    /// The `length_multiplier` is deferred to [`GraceGroupBuilder::finish`]; here
    /// the display duration is stashed in `length_multiplier` verbatim as a
    /// placeholder and rescaled by the base unit once the group size is known.
    fn read_grace_note(&mut self, note_node: Node<'_, '_>) -> Option<GraceNoteEvent> {
        let pitch = self.read_pitch(note_node)?;
        let written_accidental = child_text(note_node, "accidental")
            .and_then(|name| self.accidental_from_name(name))
            .map(|kind| AccidentalMark {
                kind,
                explicit: true,
                courtesy: false,
                source: READER_SPAN,
            });
        let display_duration = self.read_note_type_fraction(note_node);
        Some(GraceNoteEvent {
            pitch,
            written_accidental,
            // Placeholder: holds the raw display duration until `finish` divides it
            // by the group's base unit to recover the true length multiplier.
            length_multiplier: display_duration,
        })
    }

    /// S6c: reconstruct a grace note's slurs (`<notations><slur>`), inverting the
    /// `<slur>` half of [`MusicXmlWriter::write_notations`]. Grace notes carry no
    /// ties/tuplets/decorations in croma's output, so only `<slur>` is read.
    fn read_grace_slurs(&mut self, note_node: Node<'_, '_>) -> Vec<SlurAttachment> {
        let Some(notations) = child_element(note_node, "notations") else {
            return Vec::new();
        };
        children_named(notations, "slur")
            .filter_map(|slur| self.read_slur(slur))
            .collect()
    }

    /// S6c: map a `<note>`'s `<type>` (+ `<dots>`) to its written duration
    /// fraction, the inverse of the writer's `note_spelling` for the no-tuplet
    /// case (grace notes carry no `<time-modification>`). An unrecognised or
    /// absent `<type>` falls back to an eighth (the writer's zero-duration
    /// spelling), so a grace note always yields a usable fraction.
    fn read_note_type_fraction(&mut self, note_node: Node<'_, '_>) -> Fraction {
        let base = child_text(note_node, "type")
            .and_then(note_type_fraction)
            .unwrap_or_else(|| {
                self.warn(
                    "musicxml.read.grace_missing_type",
                    "a grace <note> has no recognised <type>; assuming eighth",
                );
                Fraction::new(1, 8)
            });
        let dots = children_named(note_node, "dot").count();
        dotted_fraction(base, dots)
    }

    /// S6c: fold a `<chord/>` member note into the previous main event, inverting
    /// the writer's chord emission ([`MusicXmlWriter::write_chord`] / the
    /// `is_chord_member` fast-path in `write_sequence`). The first pitched note of
    /// a chord was already pushed as a [`TimedEventKind::Note`]; the first
    /// `<chord/>` member promotes it to a [`TimedEventKind::Chord`] (carrying both
    /// the first note and this member), and each later `<chord/>` member is
    /// appended.
    ///
    /// **Attachment placement.** The chord's `TimedEvent.attachments` stay equal to
    /// the first member's attachments (what the writer reads for the index-0 note
    /// via the `source`-keyed lookup and for `write_harmony_and_directions` /
    /// `write_grace_groups`), and each member also keeps its own attachments
    /// ([`ChordMemberEvent::attachments`]) so per-member ties/decorations re-emit.
    ///
    /// **Span discipline (idempotence-invisible).** The chord is given a DISTINCT
    /// `source_span`, mirrored on its `TimedEvent.source`, so the writer's
    /// `write_chord` first-member lookup (`timed.source == chord.source_span`)
    /// resolves to THIS chord rather than another same-`READER_SPAN` event. The
    /// writer never emits spans, so the synthetic span is invisible to the
    /// idempotence gate.
    fn fold_chord_member(
        &mut self,
        events: &mut [TimedEvent],
        last_main_event: Option<usize>,
        parsed: ParsedNote,
        attachments: EventAttachments,
        next_chord_span_start: &mut usize,
    ) {
        let member_pitch = match parsed.kind {
            TimedEventKind::Note(note) => note,
            // A `<chord/>` on a rest is not croma's output; drop it defensively.
            _ => {
                self.warn(
                    "musicxml.read.chord_member_not_note",
                    "a <chord/> member is not a pitched note; ignored",
                );
                return;
            }
        };
        let member = ChordMemberEvent {
            pitch: member_pitch.pitch,
            duration: parsed.duration,
            written_accidental: member_pitch.written_accidental,
            source_span: READER_SPAN,
            attachments,
        };
        let Some(event) = last_main_event.and_then(|index| events.get_mut(index)) else {
            self.warn(
                "musicxml.read.chord_member_without_head",
                "a <chord/> member has no preceding note to attach to; ignored",
            );
            return;
        };
        match &mut event.kind {
            TimedEventKind::Note(first) => {
                // Promote the lone note to a chord: the first member inherits the
                // event's attachments (the writer's index-0 source-keyed lookup
                // returns exactly those), and the event's source becomes the
                // chord's distinct span so that lookup resolves here.
                let span_start = *next_chord_span_start;
                *next_chord_span_start = next_chord_span_start.saturating_add(1);
                let source_span = Span {
                    start: span_start,
                    end: span_start,
                };
                let first_member = ChordMemberEvent {
                    pitch: first.pitch,
                    duration: event.duration,
                    written_accidental: first.written_accidental,
                    source_span: READER_SPAN,
                    attachments: event.attachments.clone(),
                };
                event.source = source_span;
                event.kind = TimedEventKind::Chord(ChordEvent {
                    members: vec![first_member, member],
                    source_span,
                });
            }
            TimedEventKind::Chord(chord) => chord.members.push(member),
            _ => self.warn(
                "musicxml.read.chord_member_without_head",
                "a <chord/> member follows a non-note event; ignored",
            ),
        }
    }

    /// S4: reconstruct a note's [`EventAttachments`] from its `<notations>` block
    /// and `<time-modification>`. Inverts [`MusicXmlWriter::write_notations`] and
    /// [`MusicXmlWriter::write_time_modification`] (plus the `<note>`-level
    /// `<tie>` the writer emits alongside `<tied>`). Only the four model-driven
    /// notation classes are reconstructed (ties/slurs/tuplets/decorations); the
    /// remaining attachment fields stay at their defaults. Grace/lyric/direction
    /// attachments belong to later stages.
    fn read_note_attachments(
        &mut self,
        note_node: Node<'_, '_>,
        open_tuplets: &mut OpenTuplets,
        events: &mut [TimedEvent],
    ) -> EventAttachments {
        let mut attachments = EventAttachments {
            instrument: self.read_note_instrument_ref(note_node),
            ..EventAttachments::default()
        };
        let notations = child_element(note_node, "notations");

        // Ties: the writer emits BOTH a `<note>`-level `<tie>` (no number) and a
        // `<notations>/<tied>` (with `number` = `pair_id` and optional dotted
        // `line-type`). `<tied>` is the richer source, so reconstruct the single
        // `ties` list from it; that one list re-emits both elements identically.
        // A file with `<tie>` but no `<tied>` (not croma's own output) falls back
        // to the `<note>`-level element.
        if let Some(notations) = notations {
            for tied in children_named(notations, "tied") {
                if let Some(tie) = self.read_tie(tied) {
                    attachments.ties.push(tie);
                }
            }
        }
        if attachments.ties.is_empty() {
            for tie in children_named(note_node, "tie") {
                if let Some(tie) = self.read_tie(tie) {
                    attachments.ties.push(tie);
                }
            }
        }

        if let Some(notations) = notations {
            // Slurs: `pair_id = number` so the writer's `SlurNumbers::number_for`
            // re-derives the same `number` (its `preferred = pair_id`). Distinct
            // numbers for overlapping/nested slurs therefore become distinct
            // pair_ids, exactly reproduced on re-write.
            for slur in children_named(notations, "slur") {
                if let Some(slur) = self.read_slur(slur) {
                    attachments.slurs.push(slur);
                }
            }
        }

        // Tuplets need the note's composite `<time-modification>` ratio plus the
        // open-tuplet state across the measure.
        let time_modification = self.read_time_modification_ratio(note_node);
        let tuplet_elements: Vec<(TupletRole, u32)> = notations
            .map(|notations| {
                children_named(notations, "tuplet")
                    .filter_map(|tuplet| self.read_tuplet_marker(tuplet))
                    .collect()
            })
            .unwrap_or_default();
        attachments.tuplets =
            open_tuplets.resolve(self, &tuplet_elements, time_modification, events);

        // Decorations: invert the writer's `decoration_notation` name map from
        // the grouped `<ornaments>`/`<technical>`/`<articulations>` blocks and
        // the bare `<fermata>`/`<arpeggiate>` elements.
        if let Some(notations) = notations {
            self.read_decorations(notations, &mut attachments.decorations);
        }

        // S5b: the per-`<note>` `<lyric>` block (emitted last in `write_note`).
        self.read_lyrics(note_node, &mut attachments.lyrics);

        attachments
    }

    fn read_note_instrument_ref(
        &mut self,
        note_node: Node<'_, '_>,
    ) -> Option<MusicXmlInstrumentRef> {
        children_named(note_node, "instrument").find_map(|instrument| {
            let id = instrument.attribute("id")?.trim();
            (!id.is_empty()).then(|| MusicXmlInstrumentRef {
                id: id.to_owned(),
                span: READER_SPAN,
            })
        })
    }

    /// S5a: classify one `<direction>` and reconstruct its model contribution,
    /// inverting [`MusicXmlWriter::write_tempo_direction`],
    /// [`MusicXmlWriter::write_direction_words`], [`MusicXmlWriter::write_dynamic`],
    /// [`MusicXmlWriter::write_direction_type`] (coda/segno) and
    /// [`MusicXmlWriter::write_wedge`].
    ///
    /// A **tempo** direction carries a `<metronome>`, a playback `<sound tempo>`,
    /// or voice-less tempo `<words>`; it becomes a [`TempoModel`] the caller
    /// routes to the header or to a `TempoChange`. Every other direction is
    /// voice-bearing and reconstructs an [`EventAttachments`] fragment
    /// (annotation words, or a dynamics/coda/segno/wedge decoration) for the
    /// following event.
    fn read_direction(&mut self, direction: Node<'_, '_>) -> ParsedDirection {
        // Tempo directions are voice-less. Voice-bearing words fall through to
        // annotations/decorations below.
        if let Some(tempo) = self.read_tempo_direction(direction) {
            return ParsedDirection::Tempo(tempo);
        }

        let mut attachments = EventAttachments::default();
        let placement = direction.attribute("placement");
        for direction_type in children_named(direction, "direction-type") {
            for element in element_children(direction_type) {
                match element.tag_name().name() {
                    "words" => {
                        // R3: a placement-LESS `<direction><words>` in croma's own
                        // output is a chord-symbol that `write_chord_symbol`
                        // *demoted* (a non-chord string like `tr`/`Trio` that
                        // `parse_chord_symbol` rejected, emitted as a `<direction>`
                        // via the SAME `chord_symbols` channel — never the
                        // `annotations` channel, which always carries a placement
                        // prefix and thus emits a `placement` attribute). Reading it
                        // back into `chord_symbols` (not `annotations`) preserves its
                        // DOCUMENT-ORDER position relative to any real `<harmony>` in
                        // the same buffered run: the writer emits the whole
                        // `chord_symbols` vec in order before any `annotations`, so a
                        // `"tr""G7"note` run (`<direction>tr` THEN `<harmony>G7`)
                        // round-trips only when `tr` and `G7` share the ordered
                        // `chord_symbols` vec. Re-emission is byte-identical: a
                        // placement-less, trim-stable word emits the same
                        // `<direction><words>` whether it travels the chord-symbol
                        // (demoted) path or the annotation path — only the relative
                        // order vs `<harmony>` differs, which is exactly the fix.
                        // A placement-BEARING word stays an annotation (the writer's
                        // annotation channel), and a word with surrounding whitespace
                        // (not trim-stable) also stays an annotation, since the
                        // chord-symbol path would `trim()` it and change the bytes.
                        match demoted_chord_symbol_from_words(element, placement) {
                            Some(symbol) => attachments.chord_symbols.push(symbol),
                            None => attachments
                                .annotations
                                .push(annotation_from_words(element, placement)),
                        }
                    }
                    "dynamics" => {
                        for dynamic in element_children(element) {
                            match dynamic_decoration_name(dynamic.tag_name().name()) {
                                Some(name) => {
                                    attachments.decorations.push(named_decoration(name));
                                }
                                None => self.warn(
                                    "musicxml.read.unsupported_dynamic",
                                    format!(
                                        "<dynamics> child <{}> has no ABC decoration inverse; skipped",
                                        dynamic.tag_name().name()
                                    ),
                                ),
                            }
                        }
                    }
                    "wedge" => {
                        if let Some(name) = wedge_decoration_name(element.attribute("type")) {
                            attachments.decorations.push(named_decoration(name));
                        } else {
                            self.warn(
                                "musicxml.read.unsupported_wedge",
                                format!(
                                    "<wedge type={:?}> has no ABC decoration inverse; skipped",
                                    element.attribute("type").unwrap_or("")
                                ),
                            );
                        }
                    }
                    "coda" => attachments.decorations.push(named_decoration("coda")),
                    "segno" => attachments.decorations.push(named_decoration("segno")),
                    // A `<rehearsal>` is the writer's (and abc2xml's) encoding of a
                    // body/inline `P:` section label. Reconstruct it as a distinct
                    // SectionLabel; `raw_text` XML-unescapes the label, inverting
                    // the writer's escape byte-for-byte. A `<rehearsal>` is emitted
                    // as its own `<direction>` (no words/dynamics alongside it), so
                    // returning early here cannot drop a co-located attachment.
                    "rehearsal" => {
                        return ParsedDirection::SectionLabel(raw_text(element).to_owned());
                    }
                    // Other direction-type children (pedal, …) have no model-backed
                    // inverse the writer emits; left for later work.
                    other => self.warn(
                        "musicxml.read.unsupported_direction_type",
                        format!("<direction-type> child <{other}> is not reconstructed; skipped"),
                    ),
                }
            }
        }

        if attachments.annotations.is_empty()
            && attachments.decorations.is_empty()
            && attachments.chord_symbols.is_empty()
        {
            ParsedDirection::Ignored
        } else {
            ParsedDirection::Event(Box::new(attachments))
        }
    }

    /// Invert [`MusicXmlWriter::write_tempo_direction`]: reconstruct a
    /// [`TempoModel`] from a tempo `<direction>`. Returns `None` when the
    /// direction is not a tempo (so the caller treats it as a plain words /
    /// decoration direction).
    ///
    /// The writer emits two tempo shapes, both **voice-less**:
    /// - a **numeric** tempo: an optional tempo `<words>` (`tempo.text`) then a
    ///   `<metronome>` (`<beat-unit>` + optional `<beat-unit-dot/>` +
    ///   `<per-minute>`) plus `<sound tempo=...>`. The reader recovers `text`
    ///   from the words and `beat` from the metronome.
    /// - a **text-only** tempo (no numeric beat): just a tempo `<words>` + the
    ///   `tempo.text`-only `TempoModel`, beat `None`. The reader recovers `text`
    ///   and leaves `beat = None`.
    ///
    /// Voice-bearing `<words>` are regular annotations. A `<metronome>` whose
    /// `<beat-unit>`/`<per-minute>` cannot be parsed yields `None`.
    fn read_tempo_direction(&mut self, direction: Node<'_, '_>) -> Option<TempoModel> {
        // A voice-bearing direction is never a tempo (tempo directions carry no
        // `<voice>`); bail so it is reconstructed as an annotation/decoration.
        if child_element(direction, "voice").is_some() {
            return None;
        }
        // The tempo words (if any) are the `<words>` of direction-types that do
        // NOT contain the metronome. Foreign MusicXML can split one visible
        // tempo label across multiple `<words>` siblings, including whitespace
        // placeholders; normalize that to the single ABC Q: text croma can carry.
        let words = || tempo_words(direction);

        let sound_beat =
            child_element(direction, "sound").and_then(|sound| self.read_sound_tempo_beat(sound));

        if let Some(metronome) = descendants_named(direction, "metronome").next() {
            if let Some(beat) = self.read_tempo_beat(metronome) {
                return Some(TempoModel {
                    text: words(),
                    beat: Some(beat),
                    beat_role: TempoBeatRole::PrintedMetronome,
                    source_span: READER_SPAN,
                });
            }
            if let Some(beat) = sound_beat {
                return Some(TempoModel {
                    text: words(),
                    beat: Some(beat),
                    beat_role: TempoBeatRole::PlaybackSoundOnly,
                    source_span: READER_SPAN,
                });
            }
            return None;
        }

        // No metronome: foreign MusicXML often carries playback tempo only as
        // `<sound tempo="...">`. Project that to ABC's quarter-note `Q:` form.
        if child_element(direction, "sound").is_some() {
            let text = words();
            let beat = sound_beat;
            if beat.is_none() && text.is_none() {
                return None;
            }
            return Some(TempoModel {
                text,
                beat,
                beat_role: TempoBeatRole::PlaybackSoundOnly,
                source_span: READER_SPAN,
            });
        }
        if let Some(text) = words().filter(|text| !text.trim().is_empty()) {
            return Some(TempoModel {
                text: Some(text),
                beat: None,
                beat_role: TempoBeatRole::PrintedMetronome,
                source_span: READER_SPAN,
            });
        }
        None
    }

    /// Invert the writer's `beat_unit_model`: reconstruct a [`TempoBeat`] from a
    /// `<metronome>`'s `<beat-unit>` (+ optional `<beat-unit-dot/>`) and
    /// `<per-minute>`. A plain unit maps to `1/denominator`; a dotted unit to
    /// `3/(2*denominator)` (the exact inverse of `3/(2^k)` -> dotted). Returns
    /// `None` for an unrecognised beat-unit name or a non-numeric per-minute.
    fn read_tempo_beat(&mut self, metronome: Node<'_, '_>) -> Option<TempoBeat> {
        let unit = child_text(metronome, "beat-unit")?;
        let base_denominator: u32 = match unit {
            "whole" => 1,
            "half" => 2,
            "quarter" => 4,
            "eighth" => 8,
            "16th" => 16,
            "32nd" => 32,
            "64th" => 64,
            other => {
                self.warn(
                    "musicxml.read.unknown_beat_unit",
                    format!("<beat-unit> `{other}` is not a recognised note type; tempo ignored"),
                );
                return None;
            }
        };
        let dotted = child_element(metronome, "beat-unit-dot").is_some();
        let (beat_numerator, beat_denominator) = if dotted {
            // A dotted unit is 3/(2*base): e.g. dotted quarter = 3/8.
            (3, base_denominator.saturating_mul(2))
        } else {
            (1, base_denominator)
        };
        let bpm = match child_text(metronome, "per-minute")
            .and_then(|text| self.parse_tempo_bpm(text, "per-minute"))
        {
            Some(bpm) => bpm,
            _ => {
                self.warn(
                    "musicxml.read.invalid_per_minute",
                    "<per-minute> is missing or not a non-negative finite number; tempo ignored",
                );
                return None;
            }
        };
        Some(TempoBeat {
            beat_numerator,
            beat_denominator,
            bpm,
        })
    }

    /// Read `<sound tempo>` as quarter-notes per minute. ABC's `Q:` model stores
    /// integer BPM, so a decimal value is rounded with a diagnostic.
    fn read_sound_tempo_beat(&mut self, sound: Node<'_, '_>) -> Option<TempoBeat> {
        let bpm = self.parse_tempo_bpm(sound.attribute("tempo")?, "sound tempo")?;
        Some(TempoBeat {
            beat_numerator: 1,
            beat_denominator: 4,
            bpm,
        })
    }

    fn parse_tempo_bpm(&mut self, text: &str, label: &str) -> Option<u32> {
        let trimmed = text.trim();
        let value = match trimmed.parse::<f64>() {
            Ok(value) if value.is_finite() && value >= 0.0 => value,
            _ => {
                self.warn(
                    "musicxml.read.invalid_tempo",
                    format!("<{label}> `{trimmed}` is not a non-negative finite number"),
                );
                return None;
            }
        };
        let rounded = value.round();
        if rounded > f64::from(u32::MAX) {
            self.warn(
                "musicxml.read.invalid_tempo",
                format!("<{label}> `{trimmed}` is too large for ABC tempo BPM"),
            );
            return None;
        }
        if (value - rounded).abs() > f64::EPSILON {
            self.warn(
                "musicxml.read.fractional_tempo",
                format!(
                    "<{label}> `{trimmed}` has fractional BPM; rounded to {rounded} for ABC Q:"
                ),
            );
        }
        Some(rounded as u32)
    }

    /// S5b: invert [`MusicXmlWriter::write_harmony`]. The writer emits a chord
    /// symbol's `<harmony>` (`<root>`, `<kind text="…">`, optional `<bass>`,
    /// `<degree>`s) from the ABC chord-symbol *string*, and crucially preserves
    /// that exact original string as the `<kind text="…">` attribute. The reader
    /// reconstructs croma-owned `<kind text>` directly when it starts with a
    /// complete ABC chord root. Foreign MusicXML also uses `text` as a quality
    /// suffix (`text="dim"`, `text="7"`) while the root lives in `<root>`; those
    /// must be synthesised from the tree so ABC receives a complete playable chord
    /// symbol. (The writer only ever emits `<harmony>` when the string parses as a
    /// chord; a non-chord string is emitted as a `<direction><words>` instead, which
    /// the S5a direction reader already round-trips as an annotation.)
    ///
    /// A `<kind>` with no `text` attribute is **foreign functional harmony**
    /// (R2c): abc2xml / music21 emit the chord as a structured `<root>`/`<kind>`/
    /// `<bass>`/`<degree>` tree with no source string. croma's model carries chord
    /// symbols, so this is legitimate foreign-dialect reading (not writer-mimicry):
    /// synthesise an ABC chord-symbol string from the tree via [`synthesise_chord_symbol`]
    /// and reconstruct the SAME `chord_symbols` [`TextAttachment`] the `text=` path
    /// produces. The synthesised string is chosen so croma's own re-parse
    /// (`parse_chord_symbol`) reproduces the identical `<root>`/`<kind>`, keeping
    /// re-export stable. A `<kind>` value croma cannot model and that carries no
    /// usable text content is skipped with a diagnostic — never invented.
    fn read_harmony(&mut self, harmony: Node<'_, '_>) -> Option<TextAttachment> {
        let kind_text = child_element(harmony, "kind").and_then(|kind| kind.attribute("text"));
        let (text, musicxml_harmony_text) = match kind_text {
            Some(text)
                if starts_with_abc_chord_root(text) || child_element(harmony, "root").is_none() =>
            {
                (text.to_owned(), None)
            }
            Some(text) => (
                self.synthesise_chord_symbol(harmony)?,
                Some(text.to_owned()),
            ),
            None => (self.synthesise_chord_symbol(harmony)?, Some(String::new())),
        };
        Some(TextAttachment {
            text,
            span: READER_SPAN,
            // A chord symbol carries no placement (the writer's `<harmony>` has no
            // placement attribute); the lowering's chord_symbols are placement-less.
            placement: None,
            musicxml_harmony_text,
        })
    }

    /// R2c: synthesise an ABC chord-symbol string from a textless functional
    /// `<harmony>` tree (`<root>`, `<kind>`, optional `<bass>`, `<degree>`s),
    /// returning `None` (with a diagnostic) when the chord cannot be modelled.
    ///
    /// The pieces are assembled to mirror croma's OWN forward `parse_chord_symbol`
    /// grammar (`root accidental? quality? degree* ("/" bass)?`) so the result
    /// round-trips: re-parsing it reproduces the same `<root>`/`<kind>`. The
    /// kind→suffix map ([`chord_kind_suffix`]) is the inverse of the writer's
    /// `CHORD_QUALITY_TABLE`/`SUSPENDED_TABLE` for every value croma can emit, plus
    /// the common General-MusicXML kinds abc2xml/music21 use. An unknown kind falls
    /// back to the `<kind>` element's own text content if present, else the chord is
    /// skipped (never fabricated).
    fn synthesise_chord_symbol(&mut self, harmony: Node<'_, '_>) -> Option<String> {
        let Some(root_node) = child_element(harmony, "root") else {
            self.warn(
                "musicxml.read.harmony_without_root",
                "textless <harmony> has no <root>; chord symbol not reconstructed",
            );
            return None;
        };
        let Some(root_step) = child_text(root_node, "root-step").and_then(first_upper_letter)
        else {
            self.warn(
                "musicxml.read.harmony_without_root",
                "textless <harmony> <root> has no <root-step> letter; chord symbol not reconstructed",
            );
            return None;
        };
        let root_alter = child_text(root_node, "root-alter")
            .and_then(|text| self.parse_alter(text, "root-alter"))
            .unwrap_or(0);

        let mut symbol = String::new();
        symbol.push(root_step);
        symbol.push_str(accidental_suffix(root_alter));

        // The quality suffix. A textless `<kind>` is required; without one there is
        // nothing to model. A known kind maps to a round-trip-stable suffix; an
        // unknown kind falls back to the element's own text content, else skips.
        let Some(kind_node) = child_element(harmony, "kind") else {
            self.warn(
                "musicxml.read.harmony_unmodellable_kind",
                "textless <harmony> has no <kind>; chord symbol not reconstructed",
            );
            return None;
        };
        let kind_value = node_text(kind_node).unwrap_or("");
        match chord_kind_suffix(kind_value) {
            Some(suffix) => symbol.push_str(suffix),
            None => {
                // Unknown kind: append the `<kind>`'s own text content (an
                // already-human-readable quality like "Tristan"), or skip when it is
                // empty — never invent a spelling from an unknown enum value.
                if kind_value.is_empty() {
                    self.warn(
                        "musicxml.read.harmony_unmodellable_kind",
                        "textless <harmony> <kind> is empty and unmodellable; chord symbol skipped",
                    );
                    return None;
                }
                self.warn(
                    "musicxml.read.harmony_unknown_kind",
                    format!(
                        "textless <harmony> <kind> `{kind_value}` is not a known quality; \
                         using its text content verbatim"
                    ),
                );
                symbol.push_str(kind_value);
            }
        }

        // `<degree>`s follow the quality, before the bass. The writer's
        // `parse_chord_degree` accepts `[#=b]?(2|4|5|6|7|9|11|13)`; emit `add`
        // degrees as the bare (optionally accidentalled) digit, mirror an explicit
        // alter as the accidental, and skip a `subtract` (croma's forward grammar
        // has no removal token — dropping it keeps the string parseable and stable).
        for degree in element_children(harmony).filter(|n| n.tag_name().name() == "degree") {
            self.append_chord_degree(&mut symbol, degree);
        }

        // The slash bass, appended last so `parse_chord_symbol` splits it off the
        // tail: `<bass><bass-step>/<bass-alter>` -> "/<Bass>".
        if let Some(bass_node) = child_element(harmony, "bass")
            && let Some(bass_step) = child_text(bass_node, "bass-step").and_then(first_upper_letter)
        {
            let bass_alter = child_text(bass_node, "bass-alter")
                .and_then(|text| self.parse_alter(text, "bass-alter"))
                .unwrap_or(0);
            symbol.push('/');
            symbol.push(bass_step);
            symbol.push_str(accidental_suffix(bass_alter));
        }

        Some(symbol)
    }

    /// R2c: append one `<degree>` to a synthesised chord-symbol string, mirroring
    /// the writer's `parse_chord_degree` token (`[#=b]?digit`). `add`/`alter` emit
    /// the (optionally accidentalled) value; `subtract` is dropped (no forward
    /// removal syntax) so the string stays round-trip-stable.
    fn append_chord_degree(&mut self, symbol: &mut String, degree: Node<'_, '_>) {
        let degree_type = child_text(degree, "degree-type").unwrap_or("add");
        if degree_type == "subtract" {
            return;
        }
        let Some(value) = child_text(degree, "degree-value") else {
            return;
        };
        let alter = child_text(degree, "degree-alter")
            .and_then(|text| self.parse_alter(text, "degree-alter"))
            .unwrap_or(0);
        symbol.push_str(degree_accidental(alter));
        symbol.push_str(value.trim());
    }

    /// S5b: invert [`MusicXmlWriter::write_lyrics`] + `syllabic_for_lyric` for one
    /// `<note>`. Each `<lyric number=N>` reconstructs one (or two) [`AlignedLyric`]:
    ///
    /// - `<syllabic>` + `<text>` -> an [`LyricControl::Syllable`] carrying the text,
    ///   plus — when the syllabic is `begin` or `middle` — a trailing
    ///   [`LyricControl::Hyphen`] on the SAME note. This is the exact inverse of the
    ///   writer's state machine: it emits `begin`/`middle` precisely when the note's
    ///   model lyrics contain a `Hyphen` after the syllable (`continues`), and
    ///   `single`/`end` when they do not, so a trailing model `Hyphen` ⇔ a
    ///   begin/middle syllabic. (`single`/`end` therefore reconstruct a lone
    ///   `Syllable`; the begin/middle "open hyphen" cross-note state the writer
    ///   tracks is fully re-derivable from these per-note encodings, so no reader
    ///   state is needed.)
    /// - `<extend/>` (no syllabic/text) -> an [`LyricControl::Extender`] with empty
    ///   text (the writer emits `<extend/>` only and reads back empty-text Extenders).
    ///
    /// `number` -> `verse`. Verses are pushed in document order, reproducing the
    /// writer's per-note `verse` emission order. The writer never emits a `<lyric>`
    /// for a `Skip` or a (standalone) `Hyphen` control, so neither is reconstructed
    /// here; the `Hyphen` is recovered only as the trailing companion of a
    /// begin/middle syllable above.
    fn read_lyrics(&mut self, note_node: Node<'_, '_>, out: &mut Vec<AlignedLyric>) {
        for lyric in children_named(note_node, "lyric") {
            let verse = lyric
                .attribute("number")
                .and_then(|raw| raw.trim().parse::<u32>().ok())
                .unwrap_or(1);

            let text_node = child_element(lyric, "text");
            if text_node.is_none() && child_element(lyric, "extend").is_some() {
                out.push(AlignedLyric {
                    verse,
                    text: String::new(),
                    span: READER_SPAN,
                    control: LyricControl::Extender,
                });
                continue;
            }

            let Some(text_node) = text_node else {
                // No <text> and no <extend>: nothing the writer emits maps here.
                self.warn(
                    "musicxml.read.lyric_without_text",
                    "<lyric> has neither <text> nor <extend>; not reconstructed",
                );
                continue;
            };
            // Use the RAW (untrimmed) text: the writer emits `lyric.text` verbatim,
            // so a syllable with a trailing/leading space (common in the corpus)
            // must round-trip byte-for-byte; trimming would drop it.
            out.push(AlignedLyric {
                verse,
                text: raw_text(text_node).to_owned(),
                span: READER_SPAN,
                control: LyricControl::Syllable,
            });
            // A begin/middle syllabic is the writer's signal that the note's model
            // lyrics carry a trailing Hyphen; reconstruct it so the same syllabic
            // is re-derived. The Hyphen's text is never emitted (the writer skips
            // Hyphen controls), so the canonical "-" is idempotence-invisible.
            if matches!(
                child_text(lyric, "syllabic"),
                Some("begin") | Some("middle")
            ) {
                out.push(AlignedLyric {
                    verse,
                    text: "-".to_owned(),
                    span: READER_SPAN,
                    control: LyricControl::Hyphen,
                });
            }
        }
    }

    /// Invert one `<tied>` (or `<note>`-level `<tie>`): `type` -> [`TieRole`],
    /// `number` -> `pair_id` (default 1 when absent, as on the `<note>`-level
    /// `<tie>`), `line-type="dotted"` -> `dotted`.
    fn read_tie(&mut self, node: Node<'_, '_>) -> Option<TieAttachment> {
        let role = match node.attribute("type") {
            Some("start") => TieRole::Start,
            Some("stop") => TieRole::Stop,
            other => {
                self.warn(
                    "musicxml.read.unknown_tie_type",
                    format!(
                        "<{}> type `{}` is neither start nor stop; ignored",
                        node.tag_name().name(),
                        other.unwrap_or("")
                    ),
                );
                return None;
            }
        };
        let pair_id = node
            .attribute("number")
            .and_then(|raw| raw.trim().parse::<u32>().ok())
            .unwrap_or(1);
        Some(TieAttachment {
            pair_id,
            role,
            span: READER_SPAN,
            dotted: node.attribute("line-type") == Some("dotted"),
        })
    }

    /// Invert one `<slur>`: `type` -> [`SlurRole`], `number` -> `pair_id` (so the
    /// writer re-derives the same number), `line-type="dotted"` -> `dotted`.
    fn read_slur(&mut self, node: Node<'_, '_>) -> Option<SlurAttachment> {
        let role = match node.attribute("type") {
            Some("start") => SlurRole::Start,
            Some("stop") => SlurRole::Stop,
            // `<slur type="continue">` is valid MusicXML but croma's writer never
            // emits it (the model has only Start/Stop), so it is ignored here.
            other => {
                self.warn(
                    "musicxml.read.unsupported_slur_type",
                    format!(
                        "<slur> type `{}` is not start/stop; ignored",
                        other.unwrap_or("")
                    ),
                );
                return None;
            }
        };
        let pair_id = node
            .attribute("number")
            .and_then(|raw| raw.trim().parse::<u32>().ok())
            .unwrap_or(1);
        Some(SlurAttachment {
            pair_id,
            role,
            span: READER_SPAN,
            dotted: node.attribute("line-type") == Some("dotted"),
        })
    }

    /// Read a `<tuplet>` marker's `(role, number)`. Continue is never emitted by
    /// the writer (middle notes carry only `<time-modification>`), so only
    /// start/stop are recognised; the `number` pairs a stop with its start.
    fn read_tuplet_marker(&mut self, node: Node<'_, '_>) -> Option<(TupletRole, u32)> {
        let role = match node.attribute("type") {
            Some("start") => TupletRole::Start,
            Some("stop") => TupletRole::Stop,
            other => {
                self.warn(
                    "musicxml.read.unknown_tuplet_type",
                    format!(
                        "<tuplet> type `{}` is not start/stop; ignored",
                        other.unwrap_or("")
                    ),
                );
                return None;
            }
        };
        let number = node
            .attribute("number")
            .and_then(|raw| raw.trim().parse::<u32>().ok())
            .unwrap_or(1);
        Some((role, number))
    }

    /// Read a note's `<time-modification>` as an `(actual_notes, normal_notes)`
    /// ratio, or `None` when absent. This is the COMPOSITE ratio the writer
    /// emitted (`TimeModification::composite`); for a single tuplet it equals
    /// that tuplet's own ratio, which is what makes the common case exact.
    fn read_time_modification_ratio(&mut self, note_node: Node<'_, '_>) -> Option<(u32, u32)> {
        let node = child_element(note_node, "time-modification")?;
        let actual = child_text(node, "actual-notes").and_then(|t| t.parse::<u32>().ok());
        let normal = child_text(node, "normal-notes").and_then(|t| t.parse::<u32>().ok());
        match (actual, normal) {
            (Some(actual), Some(normal)) if actual > 0 && normal > 0 => Some((actual, normal)),
            _ => {
                self.warn(
                    "musicxml.read.invalid_time_modification",
                    "<time-modification> lacks positive actual-notes/normal-notes; ignored",
                );
                None
            }
        }
    }

    /// Invert the writer's grouped `<notations>` decoration blocks
    /// (`<ornaments>`/`<technical>`/`<articulations>`) and the bare `<fermata>`/
    /// `<arpeggiate>` elements into [`DecorationAttachment`]s. Each MusicXML
    /// element name is mapped back through [`decoration_for_notation_element`] to
    /// the ABC decoration name (and [`DecorationSourceKind`]) that re-emits the
    /// identical element via the writer's `decoration_notation`. An element with
    /// no clean inverse warns and is skipped (never invents a mapping).
    fn read_decorations(&mut self, notations: Node<'_, '_>, out: &mut Vec<DecorationAttachment>) {
        for child in element_children(notations) {
            match child.tag_name().name() {
                "ornaments" | "technical" | "articulations" => {
                    for element in element_children(child) {
                        self.push_decoration(element, out);
                    }
                }
                "fermata" => {
                    // `<fermata type="upright">` <- `fermata`,
                    // `<fermata type="inverted">` <- `invertedfermata`. An absent
                    // type defaults to upright in MusicXML; the writer always
                    // emits one, so map the type explicitly.
                    let name = match child.attribute("type") {
                        Some("inverted") => "invertedfermata",
                        _ => "fermata",
                    };
                    out.push(named_decoration(name));
                }
                "arpeggiate" => out.push(named_decoration("arpeggio")),
                // `<tied>`/`<slur>`/`<tuplet>` are handled above; anything else
                // is a later-stage or unsupported notation.
                _ => {}
            }
        }
    }

    /// Map a single grouped notation child element (inside `<ornaments>` /
    /// `<technical>` / `<articulations>`) to its [`DecorationAttachment`].
    fn push_decoration(&mut self, element: Node<'_, '_>, out: &mut Vec<DecorationAttachment>) {
        let tag = element.tag_name().name();
        // `<fingering>N` is the one text-bearing technical: its inverse is the
        // ABC decoration whose name is the digit text (`!0!`..`!5!`).
        if tag == "fingering" {
            if let Some(text) = node_text(element)
                && matches!(text, "0" | "1" | "2" | "3" | "4" | "5")
            {
                out.push(named_decoration(text));
                return;
            }
            self.warn(
                "musicxml.read.unsupported_fingering",
                "<fingering> text is not 0-5; no ABC decoration inverse, skipped",
            );
            return;
        }
        match decoration_for_notation_element(tag) {
            Some(name) => out.push(named_decoration(name)),
            None => self.warn(
                "musicxml.read.unsupported_notation",
                format!("<{tag}> has no ABC decoration inverse; skipped"),
            ),
        }
    }

    /// Inverse of [`Accidental::musicxml_name`]. An unrecognised name warns and
    /// yields `None` (the note keeps its `<alter>`-derived sounding pitch but no
    /// written accidental).
    fn accidental_from_name(&mut self, name: &str) -> Option<Accidental> {
        match name {
            "flat-flat" => Some(Accidental::DoubleFlat),
            "flat" => Some(Accidental::Flat),
            "natural" => Some(Accidental::Natural),
            "sharp" => Some(Accidental::Sharp),
            "double-sharp" => Some(Accidental::DoubleSharp),
            other => {
                self.warn(
                    "musicxml.read.unknown_accidental",
                    format!("<accidental> `{other}` is not a recognised MusicXML accidental"),
                );
                None
            }
        }
    }

    fn read_pitch(&mut self, note_node: Node<'_, '_>) -> Option<Pitch> {
        if let Some(pitch_node) = child_element(note_node, "pitch") {
            return self.read_pitched_note(pitch_node);
        }
        if let Some(unpitched_node) = child_element(note_node, "unpitched") {
            return self.read_unpitched_note(unpitched_node);
        }
        None
    }

    fn read_pitched_note(&mut self, pitch_node: Node<'_, '_>) -> Option<Pitch> {
        let step = child_text(pitch_node, "step")
            .and_then(|text| text.trim().chars().next())
            .or_else(|| {
                self.warn(
                    "musicxml.read.pitch_missing_step",
                    "<pitch> has no usable <step>; note skipped",
                );
                None
            })?;
        let octave = child_text(pitch_node, "octave")
            .and_then(|text| text.trim().parse::<i8>().ok())
            .or_else(|| {
                self.warn(
                    "musicxml.read.pitch_missing_octave",
                    "<pitch> has no usable <octave>; note skipped",
                );
                None
            })?;
        // `<alter>` is optional; the writer omits it when zero. The spec types it
        // as `decimal` (`<alter>1.0</alter>`), so parse via `parse_alter`.
        let alter = child_text(pitch_node, "alter")
            .and_then(|text| self.parse_alter(text, "alter"))
            .unwrap_or(0);
        Some(Pitch {
            step,
            alter,
            octave,
            spelling_source: READER_SPAN,
        })
    }

    fn read_unpitched_note(&mut self, unpitched_node: Node<'_, '_>) -> Option<Pitch> {
        let step = child_text(unpitched_node, "display-step")
            .and_then(|text| text.trim().chars().next())
            .or_else(|| {
                self.warn(
                    "musicxml.read.unpitched_missing_display_step",
                    "<unpitched> has no usable <display-step>; note skipped",
                );
                None
            })?;
        let octave = child_text(unpitched_node, "display-octave")
            .and_then(|text| text.trim().parse::<i8>().ok())
            .or_else(|| {
                self.warn(
                    "musicxml.read.unpitched_missing_display_octave",
                    "<unpitched> has no usable <display-octave>; note skipped",
                );
                None
            })?;
        Some(Pitch {
            step,
            alter: 0,
            octave,
            spelling_source: READER_SPAN,
        })
    }

    /// `<duration>` is an integer count of divisions. The writer's forward map
    /// is `duration = numerator * 4 * divisions / denominator`; the exact
    /// inverse of a clean, integral croma duration is
    /// `Fraction::new(duration, 4 * divisions)` (reduced), which `note_spelling`
    /// and `duration_to_divisions` both depend on only by value.
    fn read_duration(&mut self, node: Node<'_, '_>, divisions: u32) -> Option<Fraction> {
        let raw = child_text(node, "duration")?;
        let value = match raw.trim().parse::<u32>() {
            Ok(value) => value,
            Err(_) => {
                self.warn(
                    "musicxml.read.invalid_duration",
                    format!("<duration> `{}` is not a non-negative integer", raw.trim()),
                );
                return None;
            }
        };
        let denominator = divisions.max(1).saturating_mul(4);
        Some(Fraction::new(value, denominator))
    }
}

/// One `<score-part>` reconstructed from the `<part-list>`: its `id`, optional
/// `<part-name>`, and the MIDI instruments (one per voice that carried `%%MIDI`
/// sound metadata) recovered from its `<midi-instrument>` children (S3).
struct PartListEntry {
    id: String,
    name: Option<String>,
    instruments: Vec<MusicXmlPartInstrumentModel>,
}

/// P1a: one `<part-group>` span recovered from the `<part-list>`.
///
/// `symbol` drives the ABC grouping delimiters:
/// - `'['` → `bracket` or `square` → `[ … ]`
/// - `'{'` → `brace` → `{ … }`
/// - `'\0'` → `line` or absent → no delimiters (bare voice list)
///
/// `part_ids` is the ordered list of `<score-part id>` values inside the group.
/// Nested groups: the outer group's `part_ids` also contains the inner group's
/// parts (interleaved via the stack mechanism); a separate inner `PartGroupEntry`
/// captures the inner group's subset so it emits its own delimiters.
struct PartGroupEntry {
    symbol: char,
    part_ids: Vec<String>,
}

/// The full `<part-list>` read result: the ordered `<score-part>` entries
/// (forwarded to `read_part`) plus any `<part-group>` spans (P1a).
struct PartListResult {
    entries: Vec<PartListEntry>,
    groups: Vec<PartGroupEntry>,
}

/// S6d: accumulates one voice's reconstruction across a part's measures — its
/// `<voice>` string (for ordering and `slur_voice_key` derivation), its full
/// `TimedEvent` stream, and one [`Measure`] per measure where it has content.
struct VoiceAccumulator {
    voice: String,
    events: Vec<TimedEvent>,
    measures: Vec<Measure>,
}

/// S6d: borrow the [`VoiceAccumulator`] for `voice`, creating it (preserving
/// first-seen order) when absent.
fn voice_accumulator<'a>(
    accumulators: &'a mut Vec<VoiceAccumulator>,
    voice: &str,
) -> &'a mut VoiceAccumulator {
    let index = match accumulators
        .iter()
        .position(|accumulator| accumulator.voice == voice)
    {
        Some(index) => index,
        None => {
            accumulators.push(VoiceAccumulator {
                voice: voice.to_owned(),
                events: Vec::new(),
                measures: Vec::new(),
            });
            accumulators.len() - 1
        }
    };
    &mut accumulators[index]
}

/// S6d: the `VoiceId.value` for a reconstructed voice. The first (index 0) voice
/// keeps the part id (matching the single-voice reconstruction and preserving its
/// `slur_voice_key`); each additional voice gets a distinct key derived from the
/// part id and its `<voice>` number, so the writer's per-voice `SlurNumbers`
/// (keyed on `slur_voice_key`) numbers each voice's slurs independently — exactly
/// as the original multi-voice lowering did.
fn voice_id_value(part_id: &str, voice: &str, index: usize) -> String {
    if index == 0 {
        part_id.to_owned()
    } else {
        format!("{part_id}#{}", voice.trim())
    }
}

fn project_part_names_to_voice_properties(score: &mut Score) {
    let single_part = score.parts.len() == 1;
    let title = score.metadata.title.as_ref().map(|title| title.text.trim());
    for part in &mut score.parts {
        let Some(name) = part
            .name
            .clone()
            .filter(|line| !line.text.trim().is_empty())
        else {
            continue;
        };
        if single_part && title.is_some_and(|title| title == name.text.trim()) {
            continue;
        }
        let Some(voice) = part.voices.first_mut() else {
            continue;
        };
        if voice.initial_properties.name.is_none() {
            voice.initial_properties.name = Some(name.clone());
        }
        if voice.properties.name.is_none() {
            voice.properties.name = Some(name);
        }
    }
}

struct PartScoreBlock {
    part_id: String,
    text: String,
    multi_voice: bool,
}

fn part_score_blocks(score: &Score) -> Vec<PartScoreBlock> {
    score
        .parts
        .iter()
        .map(|part| {
            let voice_ids = part
                .voices
                .iter()
                .map(|voice| voice.id.value.as_str())
                .collect::<Vec<_>>();
            let text = match voice_ids.as_slice() {
                [] => part.id.value.clone(),
                [id] => (*id).to_owned(),
                ids => format!("({})", ids.join(" ")),
            };
            PartScoreBlock {
                part_id: part.id.value.clone(),
                text,
                multi_voice: voice_ids.len() > 1,
            }
        })
        .collect()
}

fn part_score_block_text(part_id: &str, part_score_blocks: &[PartScoreBlock]) -> String {
    part_score_blocks
        .iter()
        .find(|block| block.part_id == part_id)
        .map(|block| block.text.clone())
        .unwrap_or_else(|| part_id.to_owned())
}

/// P1a: build one `%%score` [`ScoreDirectiveModel`] from the list of recovered
/// `<part-group>` spans, or `None` when there are no groups.
///
/// **Voice-id alignment.** The first (index 0) voice of a part with id `"P1"` is
/// named `voice_id_value("P1", "1", 0) = "P1"`. `write_abc` emits `V:P1` for
/// that voice, so `%%score [P1 P2 …]` references the exact same id. For
/// single-voice foreign parts this is always the case.
///
/// **Text form.** The emitted text (stored in `directive.value.text` and re-emitted
/// verbatim by `write_abc`) follows croma's `%%score` grammar:
/// - `bracket`/`square` → `[P1 P2 P3]`
/// - `brace` → `{P1 P2}`
/// - `line`/absent → `P1 P2` (no delimiters)
///
/// **Nested groups.** When multiple groups are present (nested or sequential), the
/// directive text is built by rendering each group with its delimiters in the order
/// they were encountered, then deduplicating consecutive ids to avoid repeating a
/// part that the outer group already emitted. The nesting logic:
/// - Groups are sorted by decreasing `part_ids.len()` so that enclosing groups are
///   rendered before the inner groups they contain.
/// - Parts already emitted by a sub-group are NOT repeated at the enclosing level;
///   instead the sub-group's bracketed token block is inserted where those ids were.
///
/// **Single-group fast path.** When there is exactly one group, we emit the simple
/// `[id1 id2 …]` or `{id1 id2}` form directly, which covers the vast majority of
/// corpus files.
///
/// **Fix 3 — ungrouped parts.** `all_part_ids` is the full `<score-part>` list
/// in document order. When ≥1 group exists AND ≥1 part is outside every group, the
/// ungrouped parts are emitted as bare voice-id tokens interleaved with the group
/// blocks at their document-order positions, so no voice is hidden by `%%score`.
fn synthesize_score_directive(
    groups: &[PartGroupEntry],
    all_part_ids: &[&str],
    part_score_blocks: &[PartScoreBlock],
) -> Option<ScoreDirectiveModel> {
    let has_multi_voice_part = part_score_blocks.iter().any(|block| block.multi_voice);
    if groups.is_empty() && !has_multi_voice_part {
        return None;
    }

    // Build the directive text with proper nesting.
    // Strategy: find the outermost group (largest part_ids set), then for each
    // position in its part_ids list, check if there is a sub-group starting at
    // that position and substitute the sub-group's bracketed form.
    let text = build_score_text(groups, all_part_ids, part_score_blocks);
    if text.is_empty() {
        return None;
    }

    // Build the token list to match the text (for the model's structured tokens
    // field). The `tokens` are used by the lowering layer's `part_voice_groups` to
    // decide which voices share a part; bracket/brace groups keep one part per
    // voice (visual-only grouping), so the tokens only need to be structurally
    // correct, not semantically load-bearing for the self-loop.
    let tokens = parse_score_tokens(&text);

    Some(ScoreDirectiveModel {
        span: READER_SPAN,
        value: TextLine {
            text,
            span: READER_SPAN,
        },
        tokens,
    })
}

/// Build the `%%score` text body (the part after `%%score `) from the recovered
/// groups.  Handles flat, nested, sibling, and mixed layouts.
///
/// `all_part_ids` is the full `<score-part>` id list in document order.  Parts
/// not covered by any group are emitted as bare voice-id tokens at their
/// document-order positions (Fix 3 — ungrouped-part fidelity).
///
/// **Algorithm.**
///
/// 1. Identify *top-level* groups — groups whose `part_ids` are NOT a strict
///    subset of any other group in the list.  Sibling groups are both top-level;
///    an enclosing wrapper is top-level while its inner groups are not.
///
/// 2. Sort top-level groups by the position of their first `part_id` in the
///    global ordered part list (a union of all part ids in document order).
///    This preserves document order for sibling groups.
///
/// 3. For each top-level group, render it using `render_group_with_subs`, which
///    substitutes any inner sub-group blocks inline.
///
/// 4. Walk `all_part_ids` in document order.  For each id that is the first id
///    of a top-level group, emit that group's rendered block and skip the
///    remaining ids of the group.  For each id that belongs to no group at all,
///    emit it as a bare token.  Skip ids that are non-first members of a
///    top-level group (they were consumed by the group block in step 4).
///
/// 5. Join all collected tokens with `" "`.
fn build_score_text(
    groups: &[PartGroupEntry],
    all_part_ids: &[&str],
    part_score_blocks: &[PartScoreBlock],
) -> String {
    if groups.is_empty() {
        return all_part_ids
            .iter()
            .map(|id| part_score_block_text(id, part_score_blocks))
            .collect::<Vec<_>>()
            .join(" ");
    }

    // Step 1: find top-level groups — not a strict subset of any other group.
    // A group G is top-level iff there is no other group H such that every
    // part_id in G is also in H (i.e. G ⊆ H strictly).
    let top_level_indices: Vec<usize> = (0..groups.len())
        .filter(|&i| {
            let g = &groups[i];
            // G is NOT contained in any OTHER group H.
            !groups.iter().enumerate().any(|(j, h)| {
                j != i
                    && !h.part_ids.is_empty()
                    && g.part_ids.iter().all(|id| h.part_ids.contains(id))
            })
        })
        .collect();

    // Fast path: single top-level group AND no ungrouped parts → the pre-fix
    // simple form; avoids rebuilding the document-order walk for the common case.
    let all_grouped: bool = all_part_ids
        .iter()
        .all(|&id| groups.iter().any(|g| g.part_ids.iter().any(|p| p == id)));
    if top_level_indices.len() == 1 && all_grouped {
        let idx = top_level_indices[0];
        if groups.len() == 1 {
            return render_group(&groups[idx], part_score_blocks);
        }
        return render_group_with_subs(&groups[idx], groups, idx, part_score_blocks);
    }

    // Step 2: stable document order for top-level groups.  Build a global part
    // order from all part_ids across all groups (union, first-seen), augmented
    // with any ungrouped ids from `all_part_ids`.
    let mut global_order: Vec<&str> = Vec::new();
    for &id in all_part_ids {
        if !global_order.contains(&id) {
            global_order.push(id);
        }
    }
    for g in groups {
        for id in &g.part_ids {
            if !global_order.contains(&id.as_str()) {
                global_order.push(id.as_str());
            }
        }
    }
    let position_of = |id: &str| -> usize {
        global_order
            .iter()
            .position(|&s| s == id)
            .unwrap_or(usize::MAX)
    };

    let mut sorted_top: Vec<usize> = top_level_indices;
    sorted_top.sort_by_key(|&i| {
        groups[i]
            .part_ids
            .first()
            .map_or(usize::MAX, |id| position_of(id))
    });

    // Step 3: render each top-level group with its sub-group substitutions.
    // Build a map: first_id → (rendered_block, set of all ids consumed by that group).
    let mut top_by_first: std::collections::HashMap<&str, (String, &[String])> = Default::default();
    for &idx in &sorted_top {
        let g = &groups[idx];
        if let Some(first) = g.part_ids.first() {
            let block = if groups.len() == 1 {
                render_group(g, part_score_blocks)
            } else {
                render_group_with_subs(g, groups, idx, part_score_blocks)
            };
            top_by_first.insert(first.as_str(), (block, &g.part_ids));
        }
    }

    // Step 4: walk all_part_ids in document order, emitting group blocks or bare tokens.
    let mut result_tokens: Vec<String> = Vec::new();
    let mut skip_ids: std::collections::HashSet<&str> = Default::default();
    for &id in all_part_ids {
        if skip_ids.contains(id) {
            continue;
        }
        if let Some((block, consumed_ids)) = top_by_first.get(id) {
            result_tokens.push(block.clone());
            // Mark all ids in this top-level group as consumed so we don't
            // emit them again as bare tokens.
            for cid in *consumed_ids {
                skip_ids.insert(cid.as_str());
            }
        } else {
            // Ungrouped part: emit as bare voice-id token.
            result_tokens.push(part_score_block_text(id, part_score_blocks));
        }
    }

    // Step 5: join all collected tokens.
    result_tokens.join(" ")
}

/// Render one group, substituting any inner sub-groups (groups whose `part_ids`
/// are a strict subset of this group) inline at the position of their first part.
fn render_group_with_subs(
    group: &PartGroupEntry,
    all_groups: &[PartGroupEntry],
    self_idx: usize,
    part_score_blocks: &[PartScoreBlock],
) -> String {
    // Build a map: first part_id of each sub-group → (rendered block, all sub ids).
    let mut sub_blocks: std::collections::HashMap<&str, (String, &[String])> = Default::default();
    for (i, g) in all_groups.iter().enumerate() {
        if i == self_idx || g.part_ids.is_empty() {
            continue;
        }
        // Sub-group: all of g's parts are in `group`, AND g is not the group itself.
        let is_sub = g.part_ids.iter().all(|id| group.part_ids.contains(id));
        if is_sub {
            sub_blocks.insert(
                g.part_ids[0].as_str(),
                (render_group(g, part_score_blocks), &g.part_ids),
            );
        }
    }

    // Walk this group's part_ids, substituting sub-group blocks in place.
    let open = open_char(group.symbol);
    let close = close_char(group.symbol);
    let mut tokens: Vec<String> = Vec::new();
    let mut skip_remaining: usize = 0;
    for id in &group.part_ids {
        if skip_remaining > 0 {
            skip_remaining -= 1;
            continue;
        }
        if let Some((block, sub_ids)) = sub_blocks.get(id.as_str()) {
            skip_remaining = sub_ids.len().saturating_sub(1);
            tokens.push(block.clone());
        } else {
            tokens.push(part_score_block_text(id, part_score_blocks));
        }
    }
    let inner = tokens.join(" ");
    if open == '\0' {
        inner
    } else {
        format!("{open}{inner}{close}")
    }
}

/// Render one group as its bracketed string, e.g. `[P1 P2 P3]` or `{P1 P2}`.
fn render_group(group: &PartGroupEntry, part_score_blocks: &[PartScoreBlock]) -> String {
    let open = open_char(group.symbol);
    let close = close_char(group.symbol);
    let ids = group
        .part_ids
        .iter()
        .map(|id| part_score_block_text(id, part_score_blocks))
        .collect::<Vec<_>>()
        .join(" ");
    if open == '\0' {
        ids
    } else {
        format!("{open}{ids}{close}")
    }
}

fn open_char(symbol: char) -> char {
    match symbol {
        '[' => '[',
        '{' => '{',
        _ => '\0',
    }
}

fn close_char(symbol: char) -> char {
    match symbol {
        '[' => ']',
        '{' => '}',
        _ => '\0',
    }
}

/// Parse the score text into `ScoreDirectiveTokenModel`s. Mirrors
/// `parse::field::voice::parse_score_directive` but works on a plain `&str`
/// and produces `ScoreDirectiveTokenModel` directly (without the parse-layer
/// `Spanned` wrapper). All spans are `READER_SPAN` (idempotence-invisible).
fn parse_score_tokens(text: &str) -> Vec<ScoreDirectiveTokenModel> {
    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((_, ch)) = chars.next() {
        if ch.is_whitespace() {
            continue;
        }
        let kind = match ch {
            '(' | '[' | '{' => ScoreDirectiveTokenKindModel::GroupStart(ch),
            ')' | ']' | '}' => ScoreDirectiveTokenKindModel::GroupEnd(ch),
            '|' => ScoreDirectiveTokenKindModel::StaffSeparator,
            ',' => ScoreDirectiveTokenKindModel::MeasureSeparator,
            '*' => ScoreDirectiveTokenKindModel::FloatingVoiceMarker,
            _ => {
                // Collect the full voice id (until whitespace or delimiter).
                let mut id = String::new();
                id.push(ch);
                while let Some(&(_, next_ch)) = chars.peek() {
                    if next_ch.is_whitespace()
                        || matches!(next_ch, '(' | ')' | '[' | ']' | '{' | '}' | '|')
                    {
                        break;
                    }
                    id.push(next_ch);
                    chars.next();
                }
                ScoreDirectiveTokenKindModel::Voice(id)
            }
        };
        tokens.push(ScoreDirectiveTokenModel {
            span: READER_SPAN,
            kind,
        });
    }
    tokens
}

/// One `<part>` reconstructed, plus the header [`TempoModel`] (S5a) captured from
/// a voice-less tempo direction before its first note. Only part 1 yields a
/// header tempo; for every other part it is `None`.
struct PartOutcome {
    part: Part,
    header_tempo: Option<TempoModel>,
}

/// S6d: the reconstruction of one `<measure>` across ALL its `<voice>`s. The
/// writer interleaves multiple voices of one part with `<backup>` and a
/// per-sequence `<voice>` number; the reader inverts that by partitioning the
/// measure's `<note>`s (and their directions/harmony/notations) by `<voice>`,
/// reconstructing each voice's [`TimedEvent`] stream independently.
///
/// - `voices` is ordered by the numeric `<voice>` value (so `"1"` comes first).
///   Each entry is one voice's [`VoiceMeasure`] for THIS measure.
/// - `measure` is the structural skeleton (id, barlines, endings, multiple_rest)
///   that belongs to the **measure** (voice 1). Extra voices reuse the id but
///   carry empty structure; the writer reads barlines/endings/multiple-rest from
///   any voice's measure via a deduping union, so attaching them to voice 1 is
///   sufficient and avoids double-emission.
struct MeasureOutcome {
    voices: Vec<(String, VoiceMeasure)>,
    measure: Measure,
    /// The header tempo (S5a) when this measure was the header-eligible first
    /// measure of part 1 and a voice-less tempo direction preceded its first
    /// note. `None` otherwise.
    header_tempo: Option<TempoModel>,
}

/// One voice's reconstruction within a single measure: its [`TimedEvent`] stream
/// plus the `expected`/`actual` measure durations that drive the writer's
/// measure-rest predicate ([`MeasureSequence::is_full_measure_rest`]).
struct VoiceMeasure {
    events: Vec<TimedEvent>,
    expected_duration: Option<Fraction>,
    actual_duration: Fraction,
}

/// S6d: the per-voice transient state threaded through one measure's
/// document-order walk. Each `<voice>` region (the writer emits them
/// contiguously, separated by `<backup>`) accumulates into its own state: an
/// independent onset `cursor`, the buffered `pending` directions/harmony, the
/// open grace run, the most recent main event (for chord folding / after-grace),
/// the open-tuplet set, and the measure-rest / furthest-cursor bookkeeping. This
/// mirrors the single-voice reader exactly, replicated per voice.
struct VoiceMeasureState {
    events: Vec<TimedEvent>,
    cursor: Fraction,
    max_cursor: Fraction,
    measure_rest_duration: Option<Fraction>,
    pending: EventAttachments,
    /// S6e: the index in `events` at which a surviving `pending` run (one with no
    /// following note to bind to) must materialise as a trailing `Spacer`, so the
    /// Spacer keeps its DOCUMENT-ORDER position relative to the same-onset mid-tune
    /// change events pushed during the walk. Set to `Some(events.len())` the moment
    /// a direction/harmony is buffered into an *empty* `pending` (the Spacer belongs
    /// right there, ahead of any change pushed afterwards); cleared whenever
    /// `pending` is drained onto a note or a tempo change. `None` ⇒ append (no
    /// surviving pending, or the empty→non-empty transition was never recorded).
    pending_insert_index: Option<usize>,
    open_tuplets: OpenTuplets,
    grace_builder: Option<GraceGroupBuilder>,
    pending_graces: Vec<GraceGroupAttachment>,
    last_main_event: Option<usize>,
    next_chord_span_start: usize,
}

impl Default for VoiceMeasureState {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            cursor: Fraction::zero(),
            max_cursor: Fraction::zero(),
            measure_rest_duration: None,
            pending: EventAttachments::default(),
            pending_insert_index: None,
            open_tuplets: OpenTuplets::default(),
            grace_builder: None,
            pending_graces: Vec::new(),
            last_main_event: None,
            // S6c: each reconstructed chord needs a DISTINCT `source_span` so the
            // writer's `write_chord` first-member lookup resolves to it; a high
            // monotonic base keeps these clear of the zero-duration mid-tune
            // change events (which keep `start == 0` and must sort BEFORE a note
            // at the same onset). Voices never share a span comparison, so each
            // voice's chord spans starting at the same base is safe.
            next_chord_span_start: 1_000_000,
        }
    }
}

impl VoiceMeasureState {
    /// S5a/S5b: whether the buffered `pending` attachments carry any
    /// writer-emitted channel (chord symbols, annotations, or decorations). The
    /// other `EventAttachments` channels are never buffered into `pending`.
    fn pending_is_empty(&self) -> bool {
        self.pending.chord_symbols.is_empty()
            && self.pending.annotations.is_empty()
            && self.pending.decorations.is_empty()
    }

    /// S6e: record where a NEW `pending` run begins in the event stream, so a
    /// surviving run (no following note) re-materialises as a trailing `Spacer` at
    /// its document-order position relative to the same-onset mid-tune change
    /// events. Call this immediately BEFORE buffering a direction/harmony; it only
    /// captures the index on the empty→non-empty transition (the first attachment
    /// of the run), leaving an in-progress run's anchor intact.
    fn mark_pending_position(&mut self) {
        if self.pending_is_empty() {
            self.pending_insert_index = Some(self.events.len());
        }
    }
}

/// S5a: the classification of one `<direction>` by the reader.
enum ParsedDirection {
    /// A voice-less tempo direction (`<metronome>`); the caller routes it to the
    /// header `tempo_model` or a mid-tune `TempoChange`.
    Tempo(TempoModel),
    /// A voice-bearing direction reconstructed into attachments (annotation
    /// words, dynamics, coda/segno, wedge) for the following event.
    Event(Box<EventAttachments>),
    /// A `<rehearsal>` section label (abc2xml's encoding of a body/inline `P:`);
    /// the caller pushes it as a zero-duration [`TimedEventKind::SectionLabel`].
    SectionLabel(String),
    /// A direction with no model-backed inverse the writer emits.
    Ignored,
}

struct ParsedNote {
    kind: TimedEventKind,
    duration: Fraction,
    chord_member: bool,
    measure_rest: bool,
}

/// S6c: accumulates one run of consecutive `<grace>` notes into a
/// [`GraceGroupAttachment`]. While the run is open, each grace note's
/// `length_multiplier` field holds its RAW display duration (`<type>`/`<dots>`);
/// [`GraceGroupBuilder::finish`] rescales every multiplier by the group's base
/// unit (1/8 for a single-element group, else 1/16) once `note_count` is known,
/// the exact inverse of the writer's `grace_display_duration`.
#[derive(Default)]
struct GraceGroupBuilder {
    slash: Option<Span>,
    events: Vec<GraceEvent>,
}

impl GraceGroupBuilder {
    /// Finalise the run into a [`GraceGroupAttachment`], recovering each grace
    /// note's `length_multiplier` from its stashed display duration and the
    /// count-based base unit. `note_count` counts grace *elements* (a chord is
    /// one), matching the writer's `grace_base_unit` selector.
    fn finish(mut self) -> GraceGroupAttachment {
        let note_count = u32::try_from(self.events.len()).unwrap_or(u32::MAX);
        let base_unit = grace_base_unit(note_count);
        for event in &mut self.events {
            match &mut event.kind {
                GraceEventKind::Note(note) => {
                    note.length_multiplier = divide_fraction(note.length_multiplier, base_unit);
                }
                GraceEventKind::Chord(members) => {
                    for member in members {
                        member.length_multiplier =
                            divide_fraction(member.length_multiplier, base_unit);
                    }
                }
                GraceEventKind::Rest(_) => {}
            }
        }
        GraceGroupAttachment {
            span: READER_SPAN,
            slash: self.slash,
            note_count,
            events: self.events,
            // The writer re-emits the first grace note's `group.slurs ++
            // event.slurs`; folding every reconstructed slur into the per-event
            // `slurs` (above) makes `group.slurs` redundant, so it stays empty.
            slurs: Vec::new(),
        }
    }
}

/// One tuplet currently open across a measure's notes, with the `pair_id` and
/// ratio reconstructed at its `<tuplet type="start">` and the MusicXML `number`
/// used to pair its `type="stop"`.
#[derive(Debug, Clone, Copy)]
struct OpenTuplet {
    pair_id: u32,
    number: u32,
    actual_notes: u32,
    normal_notes: u32,
}

/// Tracks the tuplets open across a measure so each note's
/// [`TupletAttachment`]s can be reconstructed in inverse of the writer:
/// `type="start"` opens one (with the note's `<time-modification>` ratio), a
/// middle note with only a `<time-modification>` continues every open tuplet,
/// and `type="stop"` closes the matching one.
///
/// `next_pair_id` is a per-measure monotonic counter giving every tuplet a
/// distinct `pair_id`. Tuplet `pair_id`s share no namespace with slur `pair_id`s
/// (separate model vectors), and the writer's `sequence_tuplet_numbers`
/// re-derives the MusicXML `number` from the active-set discipline (not from the
/// `pair_id` value), so distinct sequential ids reproduce the emitted numbers —
/// including two separate tuplets that both re-emit as `number="1"`.
#[derive(Default)]
struct OpenTuplets {
    open: Vec<OpenTuplet>,
    next_pair_id: u32,
}

impl OpenTuplets {
    /// Resolve one note's tuplet attachments from its `<tuplet>` start/stop
    /// markers and composite `<time-modification>` ratio, mutating the open set.
    ///
    /// Cases, in inverse of the writer:
    /// - a `type="start"` marker opens a tuplet (ratio = this note's
    ///   time-modification) and yields a `Start` attachment;
    /// - a `type="stop"` marker closes the matching open tuplet (by `number`,
    ///   else the most recently opened) and yields a `Stop` attachment;
    /// - a note with a `<time-modification>` but NO `<tuplet>` marker is either a
    ///   middle note of the open tuplet(s) -> a `Continue` for each, OR (when no
    ///   tuplet is open) a *derived* time-modification the writer synthesised from
    ///   an odd duration (e.g. `C2/3`) -> no attachment, since S1's duration
    ///   reconstruction already re-emits the identical `<time-modification>`.
    fn resolve(
        &mut self,
        reader: &mut Reader,
        markers: &[(TupletRole, u32)],
        time_modification: Option<(u32, u32)>,
        events: &mut [TimedEvent],
    ) -> Vec<TupletAttachment> {
        let mut out = Vec::new();
        let has_start = markers.iter().any(|(role, _)| *role == TupletRole::Start);
        let has_stop = markers.iter().any(|(role, _)| *role == TupletRole::Stop);

        // Middle (continue) note: a time-modification, an open tuplet, and no
        // start/stop marker on this note.
        if !has_start && !has_stop {
            if let Some((tm_actual, tm_normal)) = time_modification
                && !self.open.is_empty()
            {
                // Nested-tuplet recovery (P2): when exactly one tuplet is
                // still open and this tail note's `<time-modification>`
                // differs from the stored ratio, the stored ratio is the
                // composite that was emitted during the inner-open phase.
                // The tail note's ratio is the true outer ratio. Recover it
                // and retroactively patch previously emitted attachments.
                // Guard: only when exactly one tuplet is open (the inner
                // already closed) and the ratios disagree.
                if self.open.len() == 1 {
                    let outer = &self.open[0];
                    if outer.actual_notes != tm_actual || outer.normal_notes != tm_normal {
                        // tm_actual/tm_normal IS the correct outer ratio; the
                        // stored ratio is the composite (outer × inner).
                        // inner = composite ÷ outer.
                        let composite_actual = outer.actual_notes;
                        let composite_normal = outer.normal_notes;
                        let outer_pair_id = outer.pair_id;
                        // Patch outer: every event bearing the outer pair_id
                        // had the composite stored; correct it to the true
                        // outer ratio.
                        patch_tuplet_ratio(events, outer_pair_id, tm_actual, tm_normal);
                        // Patch inner: every event bearing the composite
                        // ratio (but NOT the outer pair_id) was the inner
                        // tuplet emitted with the wrong composite. Rewrite
                        // those to inner = composite / outer.
                        patch_inner_tuplet_ratio(
                            events,
                            outer_pair_id,
                            composite_actual,
                            composite_normal,
                            tm_actual,
                            tm_normal,
                        );
                        // Update the live open-tuplet entry.
                        self.open[0].actual_notes = tm_actual;
                        self.open[0].normal_notes = tm_normal;
                    }
                }
                for open in &self.open {
                    out.push(TupletAttachment {
                        pair_id: open.pair_id,
                        actual_notes: open.actual_notes,
                        normal_notes: open.normal_notes,
                        role: TupletRole::Continue,
                        span: READER_SPAN,
                    });
                }
            }
            return out;
        }

        for (role, number) in markers {
            match role {
                TupletRole::Start => {
                    let (actual_notes, normal_notes) = time_modification.unwrap_or_else(|| {
                        reader.warn(
                            "musicxml.read.tuplet_without_time_modification",
                            "<tuplet type=\"start\"> has no <time-modification>; assuming 3:2",
                        );
                        (3, 2)
                    });
                    let pair_id = self.next_pair_id;
                    self.next_pair_id = self.next_pair_id.saturating_add(1);
                    self.open.push(OpenTuplet {
                        pair_id,
                        number: *number,
                        actual_notes,
                        normal_notes,
                    });
                    out.push(TupletAttachment {
                        pair_id,
                        actual_notes,
                        normal_notes,
                        role: TupletRole::Start,
                        span: READER_SPAN,
                    });
                }
                TupletRole::Stop => {
                    // Pair with the open tuplet of the same `number`; fall back to
                    // the most recently opened (LIFO) if none matches.
                    let index = self
                        .open
                        .iter()
                        .rposition(|open| open.number == *number)
                        .or_else(|| self.open.len().checked_sub(1));
                    let closed = index.map(|index| self.open.remove(index));
                    let closed = match closed {
                        Some(open) => open,
                        None => {
                            reader.warn(
                                "musicxml.read.tuplet_stop_without_start",
                                "<tuplet type=\"stop\"> has no open tuplet to close; ignored",
                            );
                            continue;
                        }
                    };
                    out.push(TupletAttachment {
                        pair_id: closed.pair_id,
                        actual_notes: closed.actual_notes,
                        normal_notes: closed.normal_notes,
                        role: TupletRole::Stop,
                        span: READER_SPAN,
                    });
                }
                TupletRole::Continue => {}
            }
        }
        // If a Stop was processed and there are still-open outer tuplets
        // (the outer remains while the inner just closed), emit Continues for
        // those outer tuplets. The writer stores these explicitly in the model
        // (e.g. an inner-stop note carries both an inner Stop and an outer
        // Continue), and `TimeModification::composite` over all of them
        // produces the correct composite for re-emission.
        //
        // Guard: `!has_start` — a foreign note may carry both a `stop` for one
        // tuplet AND a `start` for another (Sibelius/Finale dialect). In that
        // case, the newly-opened tuplet is now in `self.open`; emitting a
        // spurious Continue for it would corrupt its ratio. Only emit outer
        // Continues when this note opens no new tuplet.
        if has_stop && !has_start && time_modification.is_some() {
            for open in &self.open {
                out.push(TupletAttachment {
                    pair_id: open.pair_id,
                    actual_notes: open.actual_notes,
                    normal_notes: open.normal_notes,
                    role: TupletRole::Continue,
                    span: READER_SPAN,
                });
            }
        }
        out
    }
}

/// Retroactively patch the `actual_notes`/`normal_notes` on every
/// [`TupletAttachment`] in `events` whose `pair_id` matches `target_pair_id`.
/// Used by the nested-tuplet recovery path (P2): when an outer tuplet was
/// opened with the composite ratio (e.g. 21/16) and a subsequent tail note
/// reveals the true outer ratio (e.g. 7/8), this rewrites every previously
/// emitted Start/Continue for that tuplet so the re-write produces the
/// original bytes.
fn patch_tuplet_ratio(
    events: &mut [TimedEvent],
    target_pair_id: u32,
    actual_notes: u32,
    normal_notes: u32,
) {
    for event in events.iter_mut().rev() {
        for tuplet in event.attachments.tuplets.iter_mut() {
            if tuplet.pair_id == target_pair_id {
                tuplet.actual_notes = actual_notes;
                tuplet.normal_notes = normal_notes;
            }
        }
    }
}

/// Retroactively patch inner tuplet attachments after the outer ratio is
/// recovered. `composite_actual/composite_normal` is the ratio that was stored
/// on both outer and inner when they were opened together (the writer emits the
/// composite for all inner notes). `outer_actual/outer_normal` is the true outer
/// ratio now known from the tail note. The inner ratio is `composite ÷ outer`.
/// All events with pair_id ≠ `outer_pair_id` that currently carry the composite
/// ratio are the inner-tuplet attachments; rewrite them to the inner ratio.
fn patch_inner_tuplet_ratio(
    events: &mut [TimedEvent],
    outer_pair_id: u32,
    composite_actual: u32,
    composite_normal: u32,
    outer_actual: u32,
    outer_normal: u32,
) {
    // inner = composite / outer = (composite_actual / composite_normal) /
    // (outer_actual / outer_normal) = (composite_actual * outer_normal) /
    // (composite_normal * outer_actual). Reduce by GCD.
    let num = u64::from(composite_actual) * u64::from(outer_normal);
    let den = u64::from(composite_normal) * u64::from(outer_actual);
    if den == 0 {
        return;
    }
    let g = super::gcd_u64(num, den);
    let inner_actual = num / g;
    let inner_normal = den / g;
    // Only patch if the result fits in u32.
    let (Ok(inner_actual), Ok(inner_normal)) =
        (u32::try_from(inner_actual), u32::try_from(inner_normal))
    else {
        return;
    };
    for event in events.iter_mut().rev() {
        for tuplet in event.attachments.tuplets.iter_mut() {
            if tuplet.pair_id != outer_pair_id
                && tuplet.actual_notes == composite_actual
                && tuplet.normal_notes == composite_normal
            {
                tuplet.actual_notes = inner_actual;
                tuplet.normal_notes = inner_normal;
            }
        }
    }
}

// Note: `super::gcd_u64` (in musicxml/mod.rs) is reachable from this child
// module. Use it directly rather than duplicating the algorithm.

/// S6c: the writer's count-based grace base unit ([`MusicXmlWriter`]'s
/// `grace_base_unit`): 1/8 for a single-element grace group, 1/16 otherwise. A
/// grace note's display duration is this scaled by its `length_multiplier`.
fn grace_base_unit(note_count: u32) -> Fraction {
    if note_count <= 1 {
        Fraction {
            numerator: 1,
            denominator: 8,
        }
    } else {
        Fraction {
            numerator: 1,
            denominator: 16,
        }
    }
}

/// `left / right` (exact rational division). Used to recover a grace note's
/// `length_multiplier = display_duration / base_unit`. A zero/degenerate divisor
/// yields zero (never a panic); the writer never emits a zero base unit.
fn divide_fraction(left: Fraction, right: Fraction) -> Fraction {
    if right.numerator == 0 {
        return Fraction::zero();
    }
    Fraction::new(
        left.numerator.saturating_mul(right.denominator),
        left.denominator.saturating_mul(right.numerator),
    )
}

/// S6c: map a MusicXML `<type>` name to its base note-value fraction, the inverse
/// of the writer's `note_type_candidates` table. `None` for an unrecognised name.
fn note_type_fraction(name: &str) -> Option<Fraction> {
    Some(match name.trim() {
        "maxima" => Fraction {
            numerator: 8,
            denominator: 1,
        },
        "long" => Fraction {
            numerator: 4,
            denominator: 1,
        },
        "breve" => Fraction {
            numerator: 2,
            denominator: 1,
        },
        "whole" => Fraction {
            numerator: 1,
            denominator: 1,
        },
        "half" => Fraction {
            numerator: 1,
            denominator: 2,
        },
        "quarter" => Fraction {
            numerator: 1,
            denominator: 4,
        },
        "eighth" => Fraction {
            numerator: 1,
            denominator: 8,
        },
        "16th" => Fraction {
            numerator: 1,
            denominator: 16,
        },
        "32nd" => Fraction {
            numerator: 1,
            denominator: 32,
        },
        "64th" => Fraction {
            numerator: 1,
            denominator: 64,
        },
        "128th" => Fraction {
            numerator: 1,
            denominator: 128,
        },
        _ => return None,
    })
}

/// S6c: a base note value plus `dots` augmentation dots, the inverse of the
/// writer's `dotted_fraction` (each dot adds half the previous increment).
fn dotted_fraction(base: Fraction, dots: usize) -> Fraction {
    let mut duration = base;
    let mut dot = base;
    for _ in 0..dots {
        dot = Fraction::new(dot.numerator, dot.denominator.saturating_mul(2));
        duration = duration.checked_add(dot);
    }
    duration
}

/// A [`DecorationAttachment`] with the canonical [`DecorationSourceKind::Named`]
/// source. The reconstructed `name` is chosen so the writer's
/// `decoration_notation` re-emits the identical MusicXML element; `Named` (the
/// `!name!` form) re-emits exactly the same notation element regardless of which
/// `source_kind` the original ABC used (the writer's notation map keys only on
/// the decoration *name*), so it is the byte-stable inverse.
fn named_decoration(name: &str) -> DecorationAttachment {
    DecorationAttachment {
        name: name.to_owned(),
        span: READER_SPAN,
        source_kind: DecorationSourceKind::Named,
    }
}

/// Inverse of the writer's `decoration_notation` for the grouped notation
/// elements (`<ornaments>`/`<technical>`/`<articulations>` children, excluding
/// the text-bearing `<fingering>` handled separately). Each MusicXML element maps
/// back to ONE canonical ABC decoration name that `decoration_notation` re-emits
/// to the same element. Where the forward map is many-to-one (e.g. both `.` and
/// `staccato` -> `<staccato/>`), the canonical full name is chosen so the
/// `!name!` form round-trips. Returns `None` for an element croma's writer never
/// emits as a notation (so the caller can warn rather than invent a mapping).
fn decoration_for_notation_element(element: &str) -> Option<&'static str> {
    Some(match element {
        // Articulations.
        "staccato" => "staccato",
        "accent" => "accent",
        "tenuto" => "tenuto",
        "staccatissimo" => "wedge",
        "strong-accent" => "marcato",
        "breath-mark" => "breath",
        "scoop" => "slide",
        // Ornaments.
        "trill-mark" => "trill",
        "mordent" => "mordent",
        "inverted-mordent" => "uppermordent",
        "turn" => "turn",
        "inverted-turn" => "invertedturn",
        // Technical (non-text).
        "up-bow" => "upbow",
        "down-bow" => "downbow",
        "open-string" => "open",
        "thumb-position" => "thumb",
        "snap-pizzicato" => "snap",
        "stopped" => "plus",
        _ => return None,
    })
}

/// S5a: reconstruct a [`TextAttachment`] from a `<direction-type><words>` plus the
/// direction's `placement` attribute, inverting
/// [`MusicXmlWriter::write_direction_words`] + `annotation_text`.
///
/// The writer strips a leading placement prefix (`^`/`_`/`<`/`>`/`@`) from the
/// model text whenever the annotation has a placement, then emits the bare text
/// and a `placement="above"|"below"` attribute. The exact inverse rebuilds the
/// model text by re-attaching the **canonical** prefix for that placement (`^`
/// for above, `_` for below), so `annotation_text` strips it back to the emitted
/// words and `placement_name` re-emits the same attribute — byte-identical even
/// when the words themselves start with a prefix character. A direction with no
/// `placement` attribute reconstructs a placement-less annotation whose text is
/// the words verbatim (the writer does not strip a prefix in that case).
///
/// `placement="above"` is the canonical inverse for the writer's collapse of
/// left/right/free placements onto `above`; re-emission is byte-identical because
/// those placements all print as `above` with the prefix stripped.
fn annotation_from_words(words: Node<'_, '_>, placement: Option<&str>) -> TextAttachment {
    let text = raw_text(words);
    let (placement_model, prefix) = match placement {
        Some("below") => (Some(AnnotationPlacementModel::Below), "_"),
        Some(_) => (Some(AnnotationPlacementModel::Above), "^"),
        None => (None, ""),
    };
    TextAttachment {
        text: format!("{prefix}{text}"),
        span: READER_SPAN,
        placement: placement_model,
        musicxml_harmony_text: None,
    }
}

/// R3: reconstruct a placement-LESS `<direction><words>` as the chord-symbol
/// [`TextAttachment`] (placement-less, like a real chord symbol) that
/// `write_chord_symbol` *demoted* — the writer routes a non-chord string through
/// the `chord_symbols` channel and emits it as a `<direction><words>` only when
/// `parse_chord_symbol` rejects it. Returns `None` when the words must instead be
/// a genuine annotation, in which case the caller falls back to
/// [`annotation_from_words`]:
///
/// - a direction WITH a `placement` attribute is the writer's *annotation*
///   channel (the model text carried a `^`/`_`/`<`/`>`/`@` prefix), not a
///   demoted chord symbol; and
/// - a word whose raw text is NOT trim-stable (has surrounding whitespace) would
///   be `trim()`med by `write_chord_symbol`, changing the re-emitted bytes, so it
///   stays an annotation (whose path emits the text verbatim).
///
/// For a placement-less, trim-stable word the two channels emit the IDENTICAL
/// `<direction><words>` element; routing it through the chord-symbol channel only
/// fixes its document-order position relative to a real `<harmony>` in the same
/// buffered run (the writer emits the ordered `chord_symbols` vec before any
/// `annotations`). croma's own writer always emits the words trimmed, so this is
/// the demoted-chord path for every corpus file and a no-op for foreign,
/// whitespace-padded words.
fn demoted_chord_symbol_from_words(
    words: Node<'_, '_>,
    placement: Option<&str>,
) -> Option<TextAttachment> {
    if placement.is_some() {
        return None;
    }
    let text = raw_text(words);
    if text != text.trim() {
        return None;
    }
    Some(TextAttachment {
        text: text.to_owned(),
        span: READER_SPAN,
        placement: None,
        musicxml_harmony_text: None,
    })
}

/// Inverse of the writer's `dynamic_decoration`: map a `<dynamics>` child element
/// name back to the ABC decoration name that re-emits the identical element.
/// Returns `None` for an element croma's writer never emits as a dynamic.
fn dynamic_decoration_name(element: &str) -> Option<&'static str> {
    Some(match element {
        "p" => "p",
        "pp" => "pp",
        "ppp" => "ppp",
        "f" => "f",
        "ff" => "ff",
        "fff" => "fff",
        "mp" => "mp",
        "mf" => "mf",
        "sfz" => "sfz",
        _ => return None,
    })
}

/// Inverse of the writer's `wedge_decoration`: map a `<wedge type=...>` back to a
/// canonical ABC hairpin decoration name. `crescendo`/`diminuendo` map to the
/// long-form open names (`crescendo(` / `diminuendo(`); `stop` maps to
/// `crescendo)` (the writer's `wedge_decoration` emits `stop` for every close
/// form, so the canonical close name re-emits the identical `type="stop"`).
fn wedge_decoration_name(wedge_type: Option<&str>) -> Option<&'static str> {
    Some(match wedge_type {
        Some("crescendo") => "crescendo(",
        Some("diminuendo") => "diminuendo(",
        Some("stop") => "crescendo)",
        _ => return None,
    })
}

/// Prepend `head`'s chord-symbol/annotation/decoration channels before
/// `target`'s existing ones (S5a/S5b buffered harmony + directions precede the
/// note's own notations, and the writer emits chord symbols before annotations
/// before decorations). Only the channels the harmony/direction readers populate
/// are merged; the rest of `head` is always empty.
fn prepend_attachments(target: &mut EventAttachments, head: EventAttachments) {
    if head.chord_symbols.is_empty() && head.annotations.is_empty() && head.decorations.is_empty() {
        return;
    }
    let mut chord_symbols = head.chord_symbols;
    chord_symbols.append(&mut target.chord_symbols);
    target.chord_symbols = chord_symbols;
    let mut annotations = head.annotations;
    annotations.append(&mut target.annotations);
    target.annotations = annotations;
    let mut decorations = head.decorations;
    decorations.append(&mut target.decorations);
    target.decorations = decorations;
}

fn text_line(text: impl Into<String>) -> TextLine {
    TextLine {
        text: text.into(),
        span: READER_SPAN,
    }
}

/// Largest of two non-negative fractions.
fn max_fraction(left: Fraction, right: Fraction) -> Fraction {
    if left.less_than(right) { right } else { left }
}

/// `left - right`, clamped at zero (durations are non-negative).
fn subtract_fraction(left: Fraction, right: Fraction) -> Fraction {
    let numerator = u64::from(left.numerator)
        .saturating_mul(u64::from(right.denominator))
        .saturating_sub(u64::from(right.numerator).saturating_mul(u64::from(left.denominator)));
    let denominator = u64::from(left.denominator).saturating_mul(u64::from(right.denominator));
    Fraction::new(
        u32::try_from(numerator).unwrap_or(u32::MAX),
        u32::try_from(denominator).unwrap_or(u32::MAX),
    )
}

/// Map a numeric `<key-alter>` back to an [`Accidental`] (the fallback inverse
/// when `<key-accidental>` is absent or unrecognised). Mirrors
/// [`Accidental::alter`].
fn accidental_from_alter(alter: i8) -> Option<Accidental> {
    match alter {
        -2 => Some(Accidental::DoubleFlat),
        -1 => Some(Accidental::Flat),
        0 => Some(Accidental::Natural),
        1 => Some(Accidental::Sharp),
        2 => Some(Accidental::DoubleSharp),
        _ => None,
    }
}

/// S6a: the inverse of [`MusicXmlWriter::write_barline`]'s `bar-style` map,
/// disambiguated by `location` and the `<repeat>` direction. The forward map is
/// many-to-one on `bar-style` alone (`heavy-light` ← both `Initial` and
/// `RepeatStart`; `light-heavy` ← `Final`, `RepeatEnd`, and `RepeatBoth`), so the
/// inverse must consult the repeat element and the side:
///
/// | location | bar-style     | repeat   | → kind         |
/// |----------|---------------|----------|----------------|
/// | left     | heavy-light   | forward  | `RepeatStart`  |
/// | right    | heavy-light   | (none)   | `Initial`      |
/// | right    | light-heavy   | backward | `RepeatEnd`    |
/// | right    | light-heavy   | (none)   | `Final`        |
/// | right    | light-light   | —        | `Double`       |
/// | right    | dotted        | —        | `Dotted`       |
/// | right    | none          | —        | `Invisible`    |
///
/// `RepeatBoth` is intentionally never produced: it is decomposed into a
/// `RepeatEnd` (the `light-heavy` + backward right barline) plus a `RepeatStart`
/// (the next measure's leading `heavy-light` + forward left barline), which the
/// writer re-emits byte-identically. A combination the writer never emits (e.g. a
/// `light-light` with a repeat, or a `forward` repeat on the right) yields `None`
/// so the caller can warn rather than invent a kind.
fn barline_kind_from(
    bar_style: Option<&str>,
    repeat_direction: Option<&str>,
    is_left: bool,
) -> Option<BarlineKind> {
    Some(match (bar_style, repeat_direction, is_left) {
        // Forward repeat: a leading `|:` (always a left barline in the writer).
        (Some("heavy-light"), Some("forward"), true) => BarlineKind::RepeatStart,
        // Backward repeat: a `:|` close (right). RepeatBoth is decomposed to this.
        (Some("light-heavy"), Some("backward"), _) => BarlineKind::RepeatEnd,
        // Plain bar-styles with no repeat (all right barlines).
        (Some("light-light"), None, _) => BarlineKind::Double,
        (Some("light-heavy"), None, _) => BarlineKind::Final,
        (Some("heavy-light"), None, _) => BarlineKind::Initial,
        (Some("dotted"), None, _) => BarlineKind::Dotted,
        (Some("none"), None, _) => BarlineKind::Invisible,
        _ => return None,
    })
}

/// Inverse of the writer's `clef_model`: build a canonical ABC clef text from the
/// emitted `<sign>`/`<line>`/`<clef-octave-change>` that `clef_model` re-maps to
/// the SAME element. `clef_model` is many-to-one (e.g. "treble"/"g" both -> G/2),
/// so a single representative per `(sign, line)` plus the octave suffix
/// (`+8`/`-8`/`+15`/`-15`) suffices for byte-identical re-emission. Returns
/// `None` for the plain default treble clef (G/2, no octave change), which a
/// freshly lowered score also leaves as `None` (the writer emits an identical
/// default `<clef>` either way) — keeping the reconstructed model close to the
/// lowering's and avoiding a spurious clef property.
fn clef_text_from(sign: &str, line: &str, octave_change: i8) -> Option<String> {
    let base = match (sign.trim(), line.trim()) {
        ("F", "4") => "bass",
        ("C", "3") => "alto",
        ("C", "4") => "tenor",
        ("percussion", _) => "perc",
        // Anything else (notably the default G/2) maps through `clef_model`'s
        // final `else` arm to a treble G clef.
        _ => "treble",
    };
    let suffix = match octave_change {
        -2 => "-15",
        -1 => "-8",
        1 => "+8",
        2 => "+15",
        _ => "",
    };
    if base == "treble" && suffix.is_empty() {
        // Plain treble is the writer's default; emit no clef property so the
        // reconstructed score matches a freshly lowered one.
        return None;
    }
    Some(format!("{base}{suffix}"))
}

/// R2c: the round-trip-stable ABC chord-quality suffix for a MusicXML `<kind>`
/// value, or `None` for a kind croma cannot model.
///
/// This is the **inverse of the writer's `CHORD_QUALITY_TABLE` / `SUSPENDED_TABLE`
/// `<kind>` mapping** (`crates/croma-core/src/musicxml/harmony.rs`): for every kind
/// croma's own writer can emit, the chosen suffix is one whose first quality token
/// re-parses (via `parse_chord_symbol`'s greedy longest-prefix match) back to the
/// SAME kind, so a synthesised string survives re-export unchanged. The remaining
/// entries are the common General-MusicXML kinds abc2xml / music21 emit that
/// croma's own writer never produces (`power`, `dominant-seventh`, the spelled-out
/// `*-fifth`/`*-ninth` aliases, …), mapped to their standard ABC suffix.
///
/// Crucially these suffixes round-trip through croma's forward grammar:
/// - `""`→major, `"m"`→minor, `"7"`→dominant, `"maj7"`→major-seventh,
///   `"m7"`→minor-seventh, `"dim"`→diminished, `"dim7"`→diminished-seventh,
///   `"m7b5"`→half-diminished, `"+"`→augmented, `"6"`/`"m6"`→sixths,
///   `"9"`/`"maj9"`/`"m9"`→ninths, the `11`/`13` families, `"sus4"`/`"sus2"`,
///   `"5"`→power (`parse_chord_degree` reads the lone `5` as a degree, re-emitting
///   the same chord). Each was verified against the forward table's match order.
fn chord_kind_suffix(kind: &str) -> Option<&'static str> {
    Some(match kind.trim() {
        // --- Triads (kinds croma's writer emits) ---
        "major" => "",
        "minor" => "m",
        "augmented" => "+",
        "diminished" => "dim",
        // --- Sevenths ---
        "dominant" | "dominant-seventh" => "7",
        "major-seventh" => "maj7",
        "minor-seventh" => "m7",
        "diminished-seventh" => "dim7",
        "augmented-seventh" => "aug7",
        "half-diminished" | "half-diminished-seventh" => "m7b5",
        // --- Sixths ---
        "major-sixth" => "6",
        "minor-sixth" => "m6",
        // --- Ninths ---
        "dominant-ninth" => "9",
        "major-ninth" => "maj9",
        "minor-ninth" => "m9",
        // --- Elevenths ---
        "dominant-11th" => "11",
        "major-11th" => "maj11",
        "minor-11th" => "m11",
        // --- Thirteenths ---
        "dominant-13th" => "13",
        "major-13th" => "maj13",
        "minor-13th" => "m13",
        // --- Suspended ---
        "suspended-fourth" => "sus4",
        "suspended-second" => "sus2",
        // --- Common General-MusicXML kinds croma's writer never emits but
        //     abc2xml / music21 do. Mapped to the standard ABC suffix; each
        //     re-parses to a stable chord (power's "5" reads as a degree). ---
        "power" => "5",
        // `major-minor` (a minor triad + major-7th) has no croma chord-grammar
        // spelling that re-parses to the same chord ("mmaj7" demotes to <words>),
        // so it is deliberately unmapped: it falls back to the `<kind>`'s own text
        // content if present, else is skipped — never an unstable string. The
        // remaining exotic kinds (`Neapolitan`, `Tristan`, `pedal`, …) likewise
        // have no chord-symbol spelling and take the same text-content fallback.
        _ => return None,
    })
}

/// R2c: the chord ROOT/BASS accidental suffix for a MusicXML `*-alter` integer,
/// the inverse of `parse_chord_tone` (`#`→+1, `b`→-1). croma's chord-tone grammar
/// accepts only a SINGLE accidental, so ±2 (double accidentals) widen to two
/// characters — abc2xml never emits a double-altered root, but reading one as
/// `##`/`bb` is harmless (re-parse demotes it to a `<words>` direction, not a
/// crash). `0` / absent -> none.
fn accidental_suffix(alter: i8) -> &'static str {
    match alter {
        2 => "##",
        1 => "#",
        -1 => "b",
        -2 => "bb",
        _ => "",
    }
}

/// R2c: the chord-DEGREE accidental prefix for a `<degree-alter>` integer, the
/// inverse of `parse_chord_degree` (`#`→+1, `b`→-1, `=`→0). A non-zero alter on a
/// degree is spelled `#`/`b`; `0` is left bare (the forward grammar reads a bare
/// digit as a natural degree).
fn degree_accidental(alter: i8) -> &'static str {
    match alter {
        a if a > 0 => "#",
        a if a < 0 => "b",
        _ => "",
    }
}

/// R2c: the first character of `text` as an uppercase ASCII letter, or `None`.
/// Used for `<root-step>` / `<bass-step>`, which are single uppercase letters.
fn first_upper_letter(text: &str) -> Option<char> {
    text.trim()
        .chars()
        .next()
        .map(|ch| ch.to_ascii_uppercase())
        .filter(char::is_ascii_uppercase)
}

// --- roxmltree element helpers (all non-panicking) -------------------------

fn element_children<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(Node::is_element)
}

fn children_named<'a, 'input>(
    node: Node<'a, 'input>,
    name: &'a str,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children()
        .filter(move |child| child.is_element() && child.tag_name().name() == name)
}

fn descendants_named<'a, 'input>(
    node: Node<'a, 'input>,
    name: &'a str,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.descendants()
        .filter(move |child| child.is_element() && child.tag_name().name() == name)
}

fn child_element<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
}

/// The trimmed text content of `node`, or `None` when it has none.
fn node_text<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    node.text().map(str::trim).filter(|text| !text.is_empty())
}

/// The RAW text content of `node` (untrimmed, empty string when absent). Used
/// for metadata text the writer emits verbatim, so the round-trip is exact even
/// for an empty element like `<creator type="composer"></creator>`.
fn raw_text<'a>(node: Node<'a, '_>) -> &'a str {
    node.text().unwrap_or("")
}

/// The trimmed text of the first `name` child element of `node`.
fn child_text<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    child_element(node, name).and_then(node_text)
}

fn parse_u32(reader: &mut Reader, node: Node<'_, '_>, label: &str) -> Option<u32> {
    let text = node_text(node)?;
    match text.parse::<u32>() {
        Ok(value) => Some(value),
        Err(_) => {
            reader.warn(
                "musicxml.read.invalid_integer",
                format!("<{label}> `{text}` is not a non-negative integer"),
            );
            None
        }
    }
}

/// S6d: the `<voice>` text of a `<note>` (the writer emits one on every note), or
/// `None` when absent. Used to partition a measure's notes by voice.
fn note_voice_number(note_node: Node<'_, '_>) -> Option<String> {
    child_text(note_node, "voice").map(str::to_owned)
}

/// S6d: the `<voice>` text of a `<direction>` (the writer emits one on every
/// voice-bearing direction), or `None` when absent (e.g. a voice-less tempo
/// direction, handled separately).
fn direction_voice_number(direction: Node<'_, '_>) -> Option<String> {
    child_text(direction, "voice").map(str::to_owned)
}

/// S5a: collect the tempo label text from a voice-less direction. MusicXML
/// producers sometimes split one label across multiple `<words>` siblings and
/// can include whitespace-only placeholders; ABC has one quoted Q: text slot, so
/// normalize to the nonempty text that can survive the ABC leg.
fn tempo_words(direction: Node<'_, '_>) -> Option<String> {
    let words: Vec<String> = children_named(direction, "direction-type")
        .filter(|dt| child_element(*dt, "metronome").is_none())
        .flat_map(|dt| children_named(dt, "words"))
        .filter_map(|word| {
            let text = raw_text(word)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            (!text.is_empty()).then_some(text)
        })
        .collect();
    (!words.is_empty()).then_some(words.join(" "))
}

fn starts_with_abc_chord_root(text: &str) -> bool {
    matches!(text.trim_start().chars().next(), Some('A'..='G'))
}

/// S6d: the numeric value of a `<voice>` string for ordering. A non-numeric
/// voice (never croma's output) sorts last so it does not displace `"1"`.
fn parse_voice_number(voice: &str) -> u32 {
    voice.trim().parse::<u32>().unwrap_or(u32::MAX)
}

/// S6d: borrow the [`VoiceMeasureState`] for `voice`, creating it (preserving
/// first-seen order) when absent. Computing the index before borrowing keeps the
/// borrow checker happy for the create-then-return path.
fn voice_state<'a>(
    voices: &'a mut Vec<(String, VoiceMeasureState)>,
    voice: &str,
) -> &'a mut VoiceMeasureState {
    let index = match voices.iter().position(|(name, _)| name == voice) {
        Some(index) => index,
        None => {
            voices.push((voice.to_owned(), VoiceMeasureState::default()));
            voices.len() - 1
        }
    };
    &mut voices[index].1
}

/// S6d: read a `<measure-style><multiple-rest>N</multiple-rest>` count from an
/// `<attributes>` block, the inverse of
/// [`MusicXmlWriter::write_multiple_rest_measure_style`]. Only `N > 1` is a real
/// multi-rest glyph (the writer's `unique_multiple_rest` re-emits only `> 1`), so
/// a `0`/`1`/unparsable value yields `None`.
fn read_multiple_rest(attributes: Node<'_, '_>) -> Option<u32> {
    let measure_style = child_element(attributes, "measure-style")?;
    let count = child_text(measure_style, "multiple-rest")?
        .trim()
        .parse::<u32>()
        .ok()?;
    (count > 1).then_some(count)
}

/// R1b: complete a Score reconstructed by [`read_musicxml`] for the **ABC
/// projection only** (`croma read --format abc` / `croma musicxml2abc`).
///
/// `read_musicxml` is the byte-exact inverse of [`write_musicxml`], so it
/// populates only the fields the *MusicXML* writer reads: per-measure barlines
/// live in [`Measure::barlines`] / [`Measure::repeat_endings`] (consumed by
/// `unique_barlines` / `unique_endings`), and the key is driven by
/// `fifths` + explicit accidentals (consumed by `write_key_element`). The **ABC**
/// writer ([`crate::write_abc`]) reads a different projection: it emits `|`
/// glyphs ONLY from [`TimedEventKind::Barline`] events in `voice.events` (and
/// volta brackets from [`TimedEventKind::RepeatEnding`] events), and it emits
/// `K:{key.display}` from the key's `display` string. A reader-built Score leaves
/// `voice.events` barline-free and `display` empty, so the ABC projection
/// collapses to one barline-less line with an empty `K:`.
///
/// This pass fills exactly those two ABC-only gaps, IN PLACE, mirroring the
/// canonical shapes forward lowering produces (`lower::semantic`):
///
/// 1. **Barline / ending events.** For each voice, splice synthesized
///    [`TimedEventKind::Barline`] / [`TimedEventKind::RepeatEnding`] events into
///    `voice.events` around each measure's existing note/rest events, reproducing
///    the order `semantic_events_for_measure` emits (a measure's leading barline,
///    then its repeat-ending opens, then its content, then its trailing
///    barlines). The split-token re-join (`||:`/`[|:`) and left/right placement in
///    `write_abc` then round-trip these events to the same ABC.
/// 2. **Key display.** When `metadata.key` is present with an empty `display`,
///    set the canonical circle-of-fifths MAJOR spelling for its `fifths` (e.g.
///    `-3 -> "Eb"`, `+2 -> "D"`, `0 -> "C"`). Mode and original spelling are
///    unrecoverable from XML and irrelevant to the structural gate: a canonical
///    major spelling re-parses to the same `fifths`, so the same key accidentals
///    are applied and the pitch sequence survives. `None` is left untouched
///    (`write_abc` defaults to `K:C`).
///
/// This is applied ONLY on the ABC path; the `--format xml` projection keeps the
/// pure-inverse `write_musicxml(read_musicxml(xml))` it depends on, and the XML
/// idempotence gate (which calls `write_musicxml` directly) never sees it. The
/// mutation is structurally invisible to MusicXML re-emission: `write_musicxml`
/// reads barlines from `Measure.barlines` (not the events) and the key from
/// `fifths` (not `display`), so the spliced events and the filled `display` are
/// inert there.
#[cfg(feature = "musicxml-reader")]
pub fn complete_score_for_abc(score: &mut Score) {
    if let Some(key) = score.metadata.key.as_mut()
        && key.display.is_empty()
    {
        key.display = key_display_for_abc(key);
    }
    move_header_playback_tempo_to_first_voice(score);
    for part in &mut score.parts {
        for voice in &mut part.voices {
            renumber_voice_tuplet_pair_ids(voice);
            reproject_grace_slurs_for_abc(voice);
            synthesize_voice_barline_events(voice);
            // A mid-tune `[K:..]` is reconstructed as a `KeyChange` event whose
            // `display` is empty for the same reason the header key's is (the
            // writer drives `<key>` from `fifths`, never `display`). `write_abc`
            // emits `[K:{display}]`, so an empty display re-parses to `fifths 0`
            // and drops the change. Fill the canonical major spelling — or, when
            // the change carries explicit accidentals, the explicit-accidental
            // spelling (P3) — so the mid-tune change survives.
            for event in &mut voice.events {
                if let TimedEventKind::KeyChange(key) = &mut event.kind
                    && key.display.is_empty()
                {
                    key.display = key_display_for_abc(key);
                }
            }
        }
    }
}

#[cfg(feature = "musicxml-reader")]
fn move_header_playback_tempo_to_first_voice(score: &mut Score) {
    let Some(tempo) = score.metadata.tempo_model.as_ref() else {
        return;
    };
    if tempo.beat_role != TempoBeatRole::PlaybackSoundOnly || tempo.beat.is_none() {
        return;
    }
    let Some(voice) = score
        .parts
        .first_mut()
        .and_then(|part| part.voices.first_mut())
    else {
        return;
    };
    let Some(measure) = voice
        .measures
        .first()
        .map(|measure| measure.id)
        .or_else(|| voice.events.first().map(|event| event.measure))
    else {
        return;
    };
    let tempo = score
        .metadata
        .tempo_model
        .take()
        .expect("tempo_model checked above");
    voice.events.insert(
        0,
        TimedEvent {
            measure,
            onset: Fraction::zero(),
            duration: Fraction::zero(),
            source: tempo.source_span,
            kind: TimedEventKind::TempoChange(tempo),
            attachments: EventAttachments::default(),
        },
    );
}

/// P2: re-project a voice's grace-group slurs into the channels `write_abc`
/// reads, mirroring forward lowering.
///
/// **The mismatch.** `read_musicxml` reconstructs every grace note's `<slur>`
/// into its per-[`GraceEvent`] `slurs`, leaving the group-level
/// [`GraceGroupAttachment::slurs`] empty (the comment on
/// [`GraceGroupBuilder::finish`] explains this is byte-identical for
/// `write_score_partwise`, which emits `group.slurs ++ first_event.slurs` on the
/// first grace note). But `write_abc` reads the two channels DIFFERENTLY:
///
/// - a slur in `group.slurs` opens its `(` BEFORE the `{` (`event_prefix`), giving
///   the canonical `({grace}note)` of a slur anchored on the first grace note;
/// - a slur in a per-grace-event `slurs` opens its `(` INSIDE the braces
///   (`grace_str`), and its `)` renders inside the braces only when
///   `stop.span.start < group.span.end`, else as a trailing `)` after `}`.
///
/// The reader's flat [`READER_SPAN`] therefore breaks BOTH grace-slur shapes:
/// a group-anchored start (`({Bc}B2)`) emits as `{(Bc}` (start swallowed into the
/// braces, lost on re-parse), and an internal grace slur (`{(ef)}`) emits as
/// `{(ef})` (stop pushed past the brace, also lost), since `0 >= 0` makes every
/// reconstructed stop look "trailing".
///
/// **The fix (ABC projection only).** For each grace group:
///
/// 1. A slur START on a grace event whose matching `pair_id` has NO STOP within
///    the same group is **group-anchored** (its STOP is on the following main
///    note, which the reader already reconstructed into the event's own `slurs`).
///    Move it into `group.slurs` so `write_abc` opens it before the `{`. This is
///    XML-invisible: `write_score_partwise` emits `group.slurs ++ first_event.slurs`
///    on the first grace note, and a group-anchored start is always the first
///    `<slur>` there, so the combined per-note list is byte-identical.
/// 2. When the group still carries any internal stop slur, give the group span a
///    non-zero `end` so each internal stop (kept at `READER_SPAN`, `start == 0`)
///    satisfies `start < group.span.end` and renders its `)` INSIDE the braces.
///    The writer never emits a grace group's span, so this is XML-invisible too.
///
/// Both edits live in the ABC projection; the `write_musicxml` view (which reads
/// the union of the slur channels and ignores spans) is untouched, so the
/// `--format xml` pure inverse holds.
#[cfg(feature = "musicxml-reader")]
fn reproject_grace_slurs_for_abc(voice: &mut Voice) {
    for event in &mut voice.events {
        for group in event
            .attachments
            .grace_groups
            .iter_mut()
            .chain(&mut event.attachments.after_grace_groups)
        {
            reproject_one_grace_group_slurs(group);
        }
    }
}

/// P2 helper: re-project one grace group's slurs (see
/// [`reproject_grace_slurs_for_abc`]).
#[cfg(feature = "musicxml-reader")]
fn reproject_one_grace_group_slurs(group: &mut GraceGroupAttachment) {
    use std::collections::HashSet;

    // pair_ids that have a STOP among the grace events, and those that have a
    // START among them. A slur is **fully internal** iff its pair_id is in BOTH:
    // it opens and closes within the group (`{(ef)}`). A START whose pair_id has
    // no internal STOP is **group-anchored** (its STOP is on the following main
    // note: `({Bc}B2)`). A STOP whose pair_id has no internal START closes a slur
    // that opened on the PRECEDING main note — the after-grace shape `(f4{ef})` —
    // and must stay a TRAILING `)` after `}`.
    let internal_stop_ids: HashSet<u32> = group
        .events
        .iter()
        .flat_map(|grace| &grace.slurs)
        .filter(|slur| slur.role == SlurRole::Stop)
        .map(|slur| slur.pair_id)
        .collect();
    let internal_start_ids: HashSet<u32> = group
        .events
        .iter()
        .flat_map(|grace| &grace.slurs)
        .filter(|slur| slur.role == SlurRole::Start)
        .map(|slur| slur.pair_id)
        .collect();

    // Hoist every group-anchored START (a START with no matching internal STOP)
    // out of the grace events and into `group.slurs`, preserving document order.
    // Track whether any FULLY-INTERNAL stop remains (a stop whose start is also
    // internal); only those need the span tweak that renders `)` inside the braces.
    let mut has_fully_internal_stop = false;
    for grace in &mut group.events {
        let mut kept = Vec::with_capacity(grace.slurs.len());
        for slur in grace.slurs.drain(..) {
            if slur.role == SlurRole::Start && !internal_stop_ids.contains(&slur.pair_id) {
                group.slurs.push(slur);
            } else {
                if slur.role == SlurRole::Stop && internal_start_ids.contains(&slur.pair_id) {
                    has_fully_internal_stop = true;
                }
                kept.push(slur);
            }
        }
        grace.slurs = kept;
    }

    // When a fully-internal stop remains, ensure the group span's `end` is non-zero
    // so each internal stop (kept at `READER_SPAN`, `start == 0`) renders its `)`
    // inside the braces (`start < group.span.end`). An after-grace trailing stop
    // (start outside the group) is left untouched: with `group.span.end == 0` and
    // the stop at `READER_SPAN`, `0 >= 0` keeps it a trailing `)`, matching forward.
    if has_fully_internal_stop && group.span.end == 0 {
        group.span = Span { start: 0, end: 1 };
    }
}

/// Renumber every tuplet attachment's `pair_id` to be globally unique across a
/// whole voice, preserving each tuplet's grouping.
///
/// **Why.** The reader assigns tuplet `pair_id`s FRESH per measure (its
/// [`OpenTuplets::next_pair_id`] resets each measure). That is correct for
/// [`super::write_score_partwise`], which re-derives the MusicXML `number` from
/// an active-set discipline, never from the `pair_id` *value*. But `write_abc`'s
/// `tuplet_layout` (and its overlay sibling `overlay_tuplet_layout`) group tuplet
/// attachments by `pair_id` GLOBALLY across the entire `voice.events`, taking the
/// group span as `min(index)..=max(index)`. So when the reader reuses e.g.
/// `pair_id = 0` for a triplet in measure 1 AND another in measure 2, `write_abc`
/// merges them into ONE group spanning the whole line and multiplies the tuplet
/// ratio across everything between — producing an absurd span count and
/// compounded (`9/4`, `27/4`, …) durations.
///
/// **The key.** The reader's `pair_id`s are unique WITHIN a measure and reset per
/// measure, so `(event.measure.index, old_pair_id)` uniquely identifies one
/// tuplet. We allocate a fresh monotonic global id per distinct key, in
/// first-occurrence order over `voice.events`, and rewrite every
/// [`TupletAttachment`] (a tuplet's Start/Continue/Stop attachments share one key,
/// so they keep one shared new id and their roles stay consistent). Chord-member
/// attachments ([`ChordMemberEvent::attachments`]) are renumbered with the same
/// map under the owning event's measure index, since a member can carry tuplet
/// attachments too.
///
/// **Isolation.** This runs ONLY on the ABC-projection Score (the
/// [`complete_score_for_abc`] pass, applied on the `read --format abc` /
/// `musicxml2abc` path). The `write_musicxml` view re-derives `number` from the
/// active set, not the `pair_id` value, so renumbering is invisible there — the
/// `--format xml` pure inverse is untouched.
#[cfg(feature = "musicxml-reader")]
fn renumber_voice_tuplet_pair_ids(voice: &mut Voice) {
    use std::collections::HashMap;

    // (measure_index, old_pair_id) -> fresh globally-unique pair_id, allocated in
    // first-occurrence order over the voice's events (and chord members).
    let mut remap: HashMap<(u32, u32), u32> = HashMap::new();
    let mut next_id: u32 = 0;

    // Rewrite one tuplet list's `pair_id`s through the shared map, allocating a
    // fresh global id the first time a `(measure, old_pair_id)` key is seen.
    fn renumber(
        tuplets: &mut [TupletAttachment],
        measure_index: u32,
        remap: &mut HashMap<(u32, u32), u32>,
        next_id: &mut u32,
    ) {
        for tuplet in tuplets {
            tuplet.pair_id = *remap
                .entry((measure_index, tuplet.pair_id))
                .or_insert_with(|| {
                    let id = *next_id;
                    *next_id = next_id.saturating_add(1);
                    id
                });
        }
    }

    for event in &mut voice.events {
        let measure_index = event.measure.index;
        renumber(
            &mut event.attachments.tuplets,
            measure_index,
            &mut remap,
            &mut next_id,
        );
        // A `<chord/>` member can carry its own tuplet attachments; renumber them
        // under the SAME (measure, old_pair_id) namespace so a tuplet straddling a
        // chord member keeps one shared new id.
        if let TimedEventKind::Chord(chord) = &mut event.kind {
            for member in &mut chord.members {
                renumber(
                    &mut member.attachments.tuplets,
                    measure_index,
                    &mut remap,
                    &mut next_id,
                );
            }
        }
    }
}

/// Rebuild `voice.events` with synthesized barline/ending events interleaved at
/// measure boundaries. The existing `voice.events` carry no barline/ending events
/// (the reader stored those in `Measure.barlines`/`repeat_endings`); they DO carry
/// the per-measure note/rest/chord/mid-tune-change events, already in document
/// order and tagged with `event.measure.index`. We group those by measure index
/// and, walking `voice.measures` in order, emit per measure:
///
/// 1. the **leading** barline (the one whose reader span is `start == 0`, the
///    `is_leading_barline` marker), as a `Barline` event at the measure start;
/// 2. each `RepeatEnding` (volta `[N` open) on the measure;
/// 3. the measure's existing content events, unchanged;
/// 4. each **trailing** barline (reader span `start >= 1`), in document order,
///    as a `Barline` event closing the measure.
///
/// **The plain-`|` problem.** A plain `Regular` barline emits NO `<barline>`
/// element in MusicXML, so the reader has no record of it in `Measure.barlines`.
/// Yet every internal measure boundary needs exactly one barline glyph in ABC, or
/// the two measures fuse into one (`write_abc` segments measures by these events).
/// We therefore synthesize a trailing `Regular` between measure `i` and `i+1`
/// **iff** the boundary carries no explicit barline from either side — i.e. `i`
/// has no explicit trailing barline AND `i+1` has no leading barline (a leading
/// `|:` on `i+1`, or a trailing `:|`/`|]` on `i`, already marks the boundary). The
/// LAST measure gets no synthesized trailing `|`: a tune may end without a bar, and
/// a plain trailing `|` there is structurally inert (the parser coalesces it).
/// This reproduces exactly the `Regular` events forward lowering emits (verified:
/// `C D | E F | G A` lowers to 2 barline events, one per internal boundary).
///
/// A measure with no `Measure` entry for this voice (extra voices only carry
/// content measures) keeps its events spliced in index order with no synthesized
/// barlines, exactly as forward lowering would for a content-only overlay voice.
#[cfg(feature = "musicxml-reader")]
fn synthesize_voice_barline_events(voice: &mut Voice) {
    use std::collections::BTreeMap;

    // Existing content events, grouped by measure index in stable document order.
    let mut events_by_measure: BTreeMap<u32, Vec<TimedEvent>> = BTreeMap::new();
    for event in std::mem::take(&mut voice.events) {
        events_by_measure
            .entry(event.measure.index)
            .or_default()
            .push(event);
    }

    let mut rebuilt: Vec<TimedEvent> = Vec::new();
    let mut emitted_measures: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();

    // Walk the authoritative measure list (carries the barlines/endings). The
    // primary voice has a Measure per index; extra voices only for content bars.
    let count = voice.measures.len();
    for (position, measure) in voice.measures.iter().enumerate() {
        let index = measure.id.index;
        emitted_measures.insert(index);
        // The boundary to the NEXT measure needs a synthesized plain `|` only
        // when neither this measure's trailing barlines nor the next measure's
        // leading barline already marks it.
        let next_has_leading_barline = voice
            .measures
            .get(position + 1)
            .is_some_and(|next| next.barlines.iter().any(is_leading_reader_barline));
        let synthesize_trailing_regular = position + 1 < count
            && !measure.barlines.iter().any(is_trailing_reader_barline)
            && !next_has_leading_barline;
        emit_measure_with_barlines(
            measure,
            measure.id,
            events_by_measure.remove(&index).unwrap_or_default(),
            synthesize_trailing_regular,
            &mut rebuilt,
        );
    }

    // Defensive: any content event whose measure index had no `Measure` entry
    // (should not happen for reader output, but keeps the pass total and never
    // drops an event). Emit them in index order with no synthesized barlines.
    for (index, events) in events_by_measure {
        if emitted_measures.contains(&index) {
            continue;
        }
        rebuilt.extend(events);
    }

    voice.events = rebuilt;
}

/// Emit one measure's events into `out`: leading barline, repeat-ending opens,
/// the measure's content events, then trailing barlines — the order
/// `lower::semantic::semantic_events_for_measure` produces so `write_abc`
/// round-trips them. `synthesize_trailing_regular` appends a plain `Regular`
/// closing barline (the internal-boundary glyph the XML never recorded) when the
/// caller determined the boundary to the next measure needs one.
///
/// **P1: measure that anchors no `write_abc` segment.** A measure that, after
/// lowering, carries no note/rest/chord/spacer emits an empty (or directive-only)
/// `<measure>` in MusicXML — an all-spacer measure (`y8 ...`, whose spacers produce
/// no XML element), an empty `||`-boundary slot, or a measure holding only a
/// redundant key/meter restatement (`<attributes>` with no notes). The reader keeps
/// a `Measure` for every `<measure>`, but only `Note`/`Chord`/`Rest`/`Spacer`
/// anchor a measure on re-parse; a measure emitting only barlines, repeat endings,
/// or zero-duration directive events (`[K:..]`/`[M:..]`/clef/tempo) is folded into a
/// neighbour, so the re-parsed ABC loses a measure (the dominant `measure_count`
/// divergence). When the measure's content anchors no segment, synthesize a single
/// zero-duration [`TimedEventKind::Spacer`] in the content slot so `write_abc`
/// renders a `y` (`y |`, or `[K:G] y |` for a directive-only measure), preserving
/// the boundary. This mirrors forward lowering, which represents such a slot with
/// glyphs that re-parse to the empty measure; the structural projection counts the
/// measure, not the spacer count, so one `y` suffices. The Spacer is inert in
/// `write_musicxml` (a spacer emits no XML element), keeping the `--format xml`
/// pure inverse intact.
#[cfg(feature = "musicxml-reader")]
fn emit_measure_with_barlines(
    measure: &Measure,
    measure_id: MeasureId,
    content: Vec<TimedEvent>,
    synthesize_trailing_regular: bool,
    out: &mut Vec<TimedEvent>,
) {
    // A barline "leads" its measure when its reader span starts at 0 (matching
    // the measure's READER_SPAN), exactly the writer's `is_leading_barline`
    // predicate. Leading barlines open the measure; the rest close it.
    for barline in &measure.barlines {
        if is_leading_reader_barline(barline) {
            out.push(barline_event(*barline, measure_id));
        }
    }
    for ending in &measure.repeat_endings {
        out.push(TimedEvent {
            measure: measure_id,
            onset: Fraction::zero(),
            duration: Fraction::zero(),
            source: ending.span,
            kind: TimedEventKind::RepeatEnding(ending.clone()),
            attachments: EventAttachments::default(),
        });
    }
    // P1: a measure that `write_abc` would render with NO segment-anchoring token
    // (no note/rest/chord/spacer) must carry a single zero-duration `Spacer` in the
    // CONTENT slot — after any leading barline/ending and any mid-tune change, but
    // before the trailing barline — so `write_abc` emits a `y` that keeps the
    // measure boundary. Only `Note`/`Chord`/`Rest`/`Spacer` anchor a measure on
    // re-parse; barlines, repeat endings, and zero-duration directive events
    // (`KeyChange`/`MeterChange`/`ClefChange`/`TempoChange`) are absorbed into a
    // neighbour. So two distinct slots collapse without this spacer and lose a
    // measure:
    //   * a fully-empty interior measure whose boundary to the next measure is a
    //     synthesized trailing `|` would emit only `|`, collapsing `... | | ...`
    //     (an empty `| |` the parser drops);
    //   * a directive-only measure (e.g. a redundant key restatement the forward
    //     path parked in its own empty `<measure>`) would emit `| [K:G] |`, which
    //     the parser folds into the adjacent measure.
    // With the spacer they emit `y |` / `[K:G] y |`, which re-parse to a counted
    // measure (mirroring the forward writer, whose bar glyphs re-parse to the empty
    // measure). The Spacer is inert in `write_musicxml` (a spacer emits no XML
    // element), so the `--format xml` pure inverse is unchanged.
    let content_anchors_measure = content.iter().any(|event| {
        matches!(
            event.kind,
            TimedEventKind::Note(_)
                | TimedEventKind::Chord(_)
                | TimedEventKind::Rest(_)
                | TimedEventKind::Spacer
        )
    });
    out.extend(content);
    if !content_anchors_measure {
        out.push(TimedEvent {
            measure: measure_id,
            onset: Fraction::zero(),
            duration: Fraction::zero(),
            source: READER_SPAN,
            kind: TimedEventKind::Spacer,
            attachments: EventAttachments::default(),
        });
    }
    for barline in &measure.barlines {
        if is_trailing_reader_barline(barline) {
            out.push(barline_event(*barline, measure_id));
        }
    }
    if synthesize_trailing_regular {
        out.push(barline_event(
            MeasureBarline {
                kind: BarlineKind::Regular,
                // A non-zero span keeps `is_leading_barline` false, matching a
                // real trailing barline; the writer never emits spans, so the
                // exact value is invisible.
                span: Span { start: 1, end: 1 },
            },
            measure_id,
        ));
    }
}

/// A reader-reconstructed barline trails its measure iff it is not leading.
#[cfg(feature = "musicxml-reader")]
fn is_trailing_reader_barline(barline: &MeasureBarline) -> bool {
    !is_leading_reader_barline(barline)
}

/// A reader-reconstructed barline leads its measure iff its synthetic span starts
/// at 0 (see [`Reader::read_barline`]: leading=left keeps `start == 0`, trailing=
/// right gets `start >= 1`). Mirrors `musicxml::score::is_leading_barline` against
/// the measure's `READER_SPAN` (`start == 0`).
#[cfg(feature = "musicxml-reader")]
fn is_leading_reader_barline(barline: &MeasureBarline) -> bool {
    barline.span.start == READER_SPAN.start
}

/// A `TimedEventKind::Barline` event mirroring `non_note_event_from_timeline`'s
/// barline shape (zero duration, the barline's own span as `source`).
#[cfg(feature = "musicxml-reader")]
fn barline_event(barline: MeasureBarline, measure_id: MeasureId) -> TimedEvent {
    TimedEvent {
        measure: measure_id,
        onset: Fraction::zero(),
        duration: Fraction::zero(),
        source: barline.span,
        kind: TimedEventKind::Barline(barline),
        attachments: EventAttachments::default(),
    }
}

/// P3: the ABC `K:`/`[K:]` display string for a reconstructed key, the inverse
/// spelling `write_abc` (re-parsed by the forward parser + lowering) maps back to
/// the same `fifths` + `explicit_accidentals`.
///
/// **Why the canonical major spelling alone is wrong here.** `read_musicxml`
/// reconstructs the explicit `<key-step>`/`<key-alter>`/`<key-accidental>` triples
/// into [`KeySignatureModel::explicit_accidentals`] (confirmed by the XML
/// idempotence gate), but the bare [`canonical_major_key_display`] spells only the
/// `<fifths>`, so `write_abc` emits e.g. `K:Eb` and drops the explicit accidentals
/// — losing a `K:<tonic> exp <accidentals>` signature, or a cancellation
/// accidental on a consecutive `[K:][K:]`.
///
/// **The spelling.** When the key carries explicit accidentals, append them to the
/// canonical major tonic as space-separated `<sign><step>` tokens (`Eb _b ^f`).
/// The forward parser ([`crate::parse::field::parse_key`]) reads a tonic followed
/// by space-separated accidental tokens into exactly `KeySignature.accidentals`
/// (ABC 2.1 §3.1.14), and lowering copies them into `explicit_accidentals`; the
/// canonical major tonic re-derives the same `fifths` via `key_fifths`. This
/// round-trips both the `K:C _B` tonic-plus-accidental form (its `fifths` is the
/// C-major 0) and the `K:D exp _B^g` form (whose `fifths` is forced to 0 by `exp`,
/// so the canonical-major tonic for 0 — `C` — re-derives the same 0; no `exp`
/// keyword is needed because the accidental tokens alone reconstruct the
/// accidentals and the major tonic fixes the fifths). A key with no explicit
/// accidentals falls back to the bare canonical major spelling.
#[cfg(feature = "musicxml-reader")]
fn key_display_for_abc(key: &KeySignatureModel) -> String {
    let tonic = canonical_major_key_display(key.fifths);
    if key.explicit_accidentals.is_empty() {
        return tonic;
    }
    let mut display = tonic;
    for accidental in &key.explicit_accidentals {
        display.push(' ');
        display.push_str(key_accidental_sign_token(accidental.accidental));
        // The step is stored uppercase; the parser uppercases the note letter, so
        // a lowercase token (the common ABC spelling) re-parses identically.
        display.push(accidental.step.to_ascii_lowercase());
    }
    display
}

/// P3: the ABC accidental-sign prefix for a key accidental, the inverse of the
/// parser's `<sign><note>` accidental tokens (ABC 2.1 §3.1.14): `^`/`_`/`=` and
/// the doubles `^^`/`__`.
#[cfg(feature = "musicxml-reader")]
fn key_accidental_sign_token(accidental: Accidental) -> &'static str {
    match accidental {
        Accidental::DoubleFlat => "__",
        Accidental::Flat => "_",
        Accidental::Natural => "=",
        Accidental::Sharp => "^",
        Accidental::DoubleSharp => "^^",
    }
}

/// Canonical ABC `K:` display for a major key with the given circle-of-fifths
/// value, the inverse spelling `write_abc` re-parses to the same `fifths`. Covers
/// the standard -7..=+7 range; an out-of-range value (never croma's output, since
/// the writer clamps `<fifths>` to a real key) falls back to `"C"`, which
/// re-parses to `fifths == 0` rather than panicking.
#[cfg(feature = "musicxml-reader")]
fn canonical_major_key_display(fifths: i8) -> String {
    let name = match fifths {
        -7 => "Cb",
        -6 => "Gb",
        -5 => "Db",
        -4 => "Ab",
        -3 => "Eb",
        -2 => "Bb",
        -1 => "F",
        0 => "C",
        1 => "G",
        2 => "D",
        3 => "A",
        4 => "E",
        5 => "B",
        6 => "F#",
        7 => "C#",
        _ => "C",
    };
    name.to_owned()
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
