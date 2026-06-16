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
//! `<pan>`) -> [`MidiInstrumentModel`] on the owning voice. This closes the
//! forward/reverse `%%MIDI` loop (the epic's motivator): a `%%MIDI program` /
//! `channel` / `control` directive (line-start or inline `[I:MIDI=...]`) now
//! survives ABC -> XML -> [`Score`] -> XML byte-for-byte.
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
//! Mid-measure `<attributes>` changes (`KeyChange`/`MeterChange`/`ClefChange`),
//! `<harmony>`, `<lyric>`, multi-voice, repeats, grace and chords are **later
//! stages** and are intentionally not reconstructed yet — files that use them
//! simply do not round-trip idempotently yet, which the corpus gate measures.

use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::model::{
    Accidental, AccidentalMark, AccidentalPolicy, AccidentalScope, AlignedLyric,
    AnnotationPlacementModel, DecorationAttachment, DecorationSourceKind, EventAttachments,
    Fraction, KeyAccidentalModel, KeySignatureModel, LyricControl, Measure, MeasureId, MeterModel,
    MidiInstrumentModel, NoteEvent, Part, PartId, Pitch, RestEvent, RestVisibility, Score,
    ScoreMetadata, SlurAttachment, SlurRole, Staff, StaffId, TempoBeat, TempoModel, TextAttachment,
    TextLine, TieAttachment, TieRole, TimedEvent, TimedEventKind, TupletAttachment, TupletRole,
    Voice, VoiceId, VoicePropertiesModel,
};
use crate::parse::ParseReport;

use roxmltree::{Document, Node};

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
    let document = match Document::parse(xml) {
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
        // writer keys them by matching `id`.
        let part_list = self.read_part_list(root);
        // The header tempo direction (the writer's `write_initial_directions`)
        // belongs to the score once, emitted only in part 1; reconstruct it into
        // `metadata.tempo_model`. `read_part` reports the captured header tempo so
        // a voice-less tempo direction before part 1's first note becomes the
        // header model rather than a mid-tune `TempoChange`.
        for (part_index, part_node) in children_named(root, "part").enumerate() {
            let outcome = self.read_part(part_node, score.divisions, &part_list, part_index == 0);
            if let Some(tempo) = outcome.header_tempo {
                score.metadata.tempo_model.get_or_insert(tempo);
            }
            score.parts.push(outcome.part);
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

    /// Read each `<score-part>` into its id, `<part-name>`, and the list of
    /// `<midi-instrument>` MIDI projections (S3). The writer emits one
    /// `<score-part>` per part, with all `<score-instrument>` before all
    /// `<midi-instrument>`; only the `<midi-instrument>` carries the
    /// score-translatable fields the model stores, so the `<score-instrument>`
    /// blocks (whose `<instrument-name>` is *derived* from the program / part
    /// name on the forward side) are not read back — recovering `program`
    /// regenerates the identical name on re-write.
    fn read_part_list(&mut self, root: Node<'_, '_>) -> Vec<PartListEntry> {
        let Some(part_list) = children_named(root, "part-list").next() else {
            return Vec::new();
        };
        children_named(part_list, "score-part")
            .map(|score_part| {
                let id = score_part.attribute("id").unwrap_or_default().to_owned();
                let name = child_text(score_part, "part-name").map(str::to_owned);
                let instruments = self.read_midi_instruments(score_part);
                PartListEntry {
                    id,
                    name,
                    instruments,
                }
            })
            .collect()
    }

    /// Invert [`MusicXmlWriter::write_part_instruments`]: read every
    /// `<midi-instrument>` child of a `<score-part>` into a
    /// [`MidiInstrumentModel`] (one per voice that carried `%%MIDI` sound
    /// metadata). The exact inverses of the writer's emission are:
    ///
    /// - `<midi-channel>n` -> `channel = n`,
    /// - `<midi-program>N` -> `program = N - 1` (forward is `program + 1`),
    /// - `<volume>v` -> `volume_cc = round(v * 1.27)` (forward `{:.2}` of `cc/1.27`),
    /// - `<pan>p` -> `pan_cc = round((p + 90) * 127 / 180)` (forward `{:.2}` of
    ///   `cc/127*180 - 90`).
    ///
    /// `<instrument-name>` is intentionally **not** read: on the forward side it
    /// is derived (the GM name when a program exists, else the part name), so
    /// recovering `program` (or leaving it `None`) regenerates the identical name
    /// on re-write. The `0..=127` float-stability test proves the CC inverses are
    /// idempotent. An instrument with no recovered field is dropped (the writer
    /// would not have emitted it).
    fn read_midi_instruments(&mut self, score_part: Node<'_, '_>) -> Vec<MidiInstrumentModel> {
        children_named(score_part, "midi-instrument")
            .filter_map(|node| self.read_midi_instrument(node))
            .collect()
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

        let model = MidiInstrumentModel {
            program,
            channel,
            volume_cc,
            pan_cc,
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
        // S3: the writer emits one `<midi-instrument>` per voice that carried
        // `%%MIDI` sound metadata, in voice order. S1–S3 reconstruct a single
        // voice per part, so the first recovered instrument is this voice's; any
        // further instruments belong to additional voices reconstructed in the
        // multi-voice stage (S6) and are left for then. The common corpus shape
        // (one voice = one part = one instrument) round-trips now.
        let midi_instrument = entry.and_then(|entry| entry.instruments.first().copied());

        let staff_id = StaffId {
            value: 1,
            span: READER_SPAN,
        };
        let voice_id = VoiceId {
            value: id.clone(),
            span: READER_SPAN,
        };

        // S1 reconstructs a single voice per part; the writer reads back both
        // `voice.events` (the timed sequence) and `voice.measures` (durations),
        // so both are populated consistently.
        let mut events: Vec<TimedEvent> = Vec::new();
        let mut measures: Vec<Measure> = Vec::new();
        // The header tempo direction (if any) is captured from the FIRST measure,
        // before its first note, and only in part 1. Once captured it is never
        // overwritten, and later tempo directions become `TempoChange` events.
        let mut header_tempo: Option<TempoModel> = None;

        for measure_node in children_named(part_node, "measure") {
            let measure_id = self.read_measure_id(measure_node, measures.len());
            // Only part 1's first measure may yield the score header tempo; in
            // every other measure a voice-less tempo direction is a mid-tune
            // change. (`capture_header_tempo` is the part-1 flag; `measures` is
            // empty only for this part's first measure.)
            let capture_header = capture_header_tempo && measures.is_empty();
            let outcome = self.read_measure(measure_node, divisions, measure_id, capture_header);
            if header_tempo.is_none() {
                header_tempo = outcome.header_tempo;
            }
            events.extend(outcome.events);
            measures.push(outcome.measure);
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

        let voice = Voice {
            id: voice_id.clone(),
            staff: staff_id,
            initial_properties: initial_properties.clone(),
            properties: initial_properties,
            measures,
            events,
            midi_instrument,
            midi_transpose,
            source_span: READER_SPAN,
        };

        PartOutcome {
            part: Part {
                id: PartId {
                    value: id,
                    span: READER_SPAN,
                },
                name,
                staves: vec![Staff {
                    id: staff_id,
                    voices: vec![voice_id],
                    source_span: READER_SPAN,
                }],
                voices: vec![voice],
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
    ) -> MeasureOutcome {
        let mut events = Vec::new();
        // The writer reconstructs onsets with a per-sequence cursor that
        // <forward> advances and <backup> rewinds; inverting it means tracking
        // the same cursor across the measure's children in document order.
        let mut cursor = Fraction::zero();
        let mut last_onset = Fraction::zero();
        let mut max_cursor = Fraction::zero();
        // Detect a full-measure rest (`<rest measure="yes">`) so re-emission
        // reproduces the `measure="yes"` attribute (the writer gates it on
        // `expected_duration == duration == actual_duration` at onset 0).
        let mut measure_rest_duration: Option<Fraction> = None;
        // S4 tuplet reconstruction tracks tuplets open across the measure's
        // notes: a `<tuplet type="start">` opens one, a middle note with only a
        // `<time-modification>` continues it, and `type="stop"` closes it.
        let mut open_tuplets = OpenTuplets::default();
        // S5a: the writer emits a (timed) event's `<direction>`s immediately
        // BEFORE the event (`write_harmony_and_directions` runs first in
        // `write_event`). Inverting it means buffering each voice-bearing
        // direction's reconstructed attachments and flushing them onto the next
        // timed event (note, rest, or `TempoChange`).
        let mut pending = EventAttachments::default();
        // The header tempo (`write_initial_directions`) is the FIRST voice-less
        // tempo direction before the first note of part 1's first measure; once
        // captured, later tempo directions are mid-tune `TempoChange` events.
        let mut header_tempo: Option<TempoModel> = None;
        let mut seen_note = false;

        for child in element_children(measure_node) {
            match child.tag_name().name() {
                "note" => {
                    let Some(parsed) = self.read_note(child, divisions) else {
                        continue;
                    };
                    seen_note = true;
                    let onset = if parsed.chord_member {
                        // A `<chord/>` member shares the previous note's onset
                        // and does not advance the cursor. (Chords are fully a
                        // stage-S6 concern; S1 keeps the cursor honest so files
                        // without chords stay idempotent.)
                        last_onset
                    } else {
                        cursor
                    };

                    if parsed.measure_rest {
                        measure_rest_duration = Some(parsed.duration);
                    }

                    // S4: reconstruct this note's `<notations>` +
                    // `<time-modification>` into its `EventAttachments`. A
                    // `<chord/>` member's notations belong to the chord member
                    // (S6); the writer reads its first member's attachments off
                    // the timed event, so attaching to every note here is the
                    // exact inverse for the single-note (non-chord) shapes S4
                    // proves, and is harmless for chord members until S6.
                    let mut attachments = self.read_note_attachments(child, &mut open_tuplets);
                    // S5a: prepend any buffered direction attachments (the writer
                    // emitted them just before this note). Prepending preserves
                    // the model field order the lowering uses.
                    prepend_attachments(&mut attachments, std::mem::take(&mut pending));

                    events.push(TimedEvent {
                        measure: measure_id,
                        onset,
                        duration: parsed.duration,
                        source: READER_SPAN,
                        kind: parsed.kind,
                        attachments,
                    });

                    if !parsed.chord_member {
                        last_onset = onset;
                        cursor = cursor.checked_add(parsed.duration);
                        max_cursor = max_fraction(max_cursor, cursor);
                    }
                }
                "harmony" => {
                    // S5b: the writer emits a chord symbol's `<harmony>` from
                    // `write_harmony_and_directions`, BEFORE the event's
                    // `<direction>`s and `<note>`. Buffer it onto `pending` so it
                    // flushes (chord_symbols first) onto the next event, exactly
                    // as the writer emits chord symbols before annotations.
                    if let Some(symbol) = self.read_harmony(child) {
                        pending.chord_symbols.push(symbol);
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
                                // this zero-duration event.
                                events.push(TimedEvent {
                                    measure: measure_id,
                                    onset: cursor,
                                    duration: Fraction::zero(),
                                    source: READER_SPAN,
                                    kind: TimedEventKind::TempoChange(tempo),
                                    attachments: std::mem::take(&mut pending),
                                });
                            }
                        }
                        // A voice-bearing direction (annotation words, dynamics,
                        // coda/segno, wedge): buffer it for the next event.
                        ParsedDirection::Event(attachments) => {
                            pending.extend(attachments);
                        }
                        ParsedDirection::Ignored => {}
                    }
                }
                "forward" => {
                    if let Some(duration) = self.read_duration(child, divisions) {
                        cursor = cursor.checked_add(duration);
                        max_cursor = max_fraction(max_cursor, cursor);
                    }
                }
                "backup" => {
                    if let Some(duration) = self.read_duration(child, divisions) {
                        cursor = subtract_fraction(cursor, duration);
                    }
                }
                // <attributes> (divisions/key/time/clef/transpose), <barline>,
                // <harmony>, <lyric> etc. are read in later stages.
                _ => {}
            }
        }

        // S5a/S5b: directions or chord symbols with no following note in this
        // measure (e.g. a pre-barline `!segno!` or a trailing `"C"` chord, or a
        // note-less measure carrying only an annotation) are emitted by the writer
        // on a zero-duration `Spacer` event whose `write_event` emits its
        // harmony/directions then nothing. Reconstruct that Spacer so the trailing
        // attachments re-emit at the end of the measure.
        if !pending.chord_symbols.is_empty()
            || !pending.annotations.is_empty()
            || !pending.decorations.is_empty()
        {
            events.push(TimedEvent {
                measure: measure_id,
                onset: cursor,
                duration: Fraction::zero(),
                source: READER_SPAN,
                kind: TimedEventKind::Spacer,
                attachments: std::mem::take(&mut pending),
            });
        }

        // A measure rest forces `expected_duration == actual_duration ==
        // rest.duration` at onset 0; otherwise leave `expected_duration` unset
        // so ordinary rests stay plain (the writer's measure-rest predicate is
        // `expected_duration.is_some_and(...)`). `actual_duration` is the
        // furthest cursor reached.
        let (expected_duration, actual_duration) = match measure_rest_duration {
            Some(duration) => (Some(duration), duration),
            None => (None, max_cursor),
        };

        MeasureOutcome {
            events,
            header_tempo,
            measure: Measure {
                id: measure_id,
                source_span: READER_SPAN,
                expected_duration,
                actual_duration,
                multiple_rest: None,
                pickup: false,
                complete: true,
                barlines: Vec::new(),
                repeat_endings: Vec::new(),
                overlays: Vec::new(),
            },
        }
    }

    /// Read one `<note>` into the data a [`TimedEvent`] needs. Returns `None`
    /// for a note the reader cannot turn into a timed event (e.g. a grace note,
    /// deferred to S6, which carries no `<duration>`).
    fn read_note(&mut self, note_node: Node<'_, '_>, divisions: u32) -> Option<ParsedNote> {
        let chord_member = child_element(note_node, "chord").is_some();
        let is_grace = child_element(note_node, "grace").is_some();
        if is_grace {
            // Grace notes have no <duration> and belong to stage S6.
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
    ) -> EventAttachments {
        let mut attachments = EventAttachments::default();
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
        attachments.tuplets = open_tuplets.resolve(self, &tuplet_elements, time_modification);

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

    /// S5a: classify one `<direction>` and reconstruct its model contribution,
    /// inverting [`MusicXmlWriter::write_tempo_direction`],
    /// [`MusicXmlWriter::write_direction_words`], [`MusicXmlWriter::write_dynamic`],
    /// [`MusicXmlWriter::write_direction_type`] (coda/segno) and
    /// [`MusicXmlWriter::write_wedge`].
    ///
    /// A **tempo** direction carries a `<metronome>` (and/or a tempo `<words>` +
    /// `<sound>`) and is voice-less; it becomes a [`TempoModel`] the caller routes
    /// to the header or to a `TempoChange`. Every other direction is voice-bearing
    /// and reconstructs an [`EventAttachments`] fragment (annotation words, or a
    /// dynamics/coda/segno/wedge decoration) for the following event.
    fn read_direction(&mut self, direction: Node<'_, '_>) -> ParsedDirection {
        // A `<metronome>` (or a tempo `<words>` accompanied by a `<sound>` and no
        // `<voice>`) is a tempo direction. Detect the metronome first; a bare
        // tempo-words direction is handled by the words branch below only when it
        // is NOT a tempo (i.e. it has a `<voice>`).
        if let Some(tempo) = self.read_tempo_direction(direction) {
            return ParsedDirection::Tempo(tempo);
        }

        let mut attachments = EventAttachments::default();
        let placement = direction.attribute("placement");
        for direction_type in children_named(direction, "direction-type") {
            for element in element_children(direction_type) {
                match element.tag_name().name() {
                    "words" => {
                        attachments
                            .annotations
                            .push(annotation_from_words(element, placement));
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
                    // Other direction-type children (rehearsal, pedal, …) have no
                    // model-backed inverse the writer emits; left for later work.
                    other => self.warn(
                        "musicxml.read.unsupported_direction_type",
                        format!("<direction-type> child <{other}> is not reconstructed; skipped"),
                    ),
                }
            }
        }

        if attachments.annotations.is_empty() && attachments.decorations.is_empty() {
            ParsedDirection::Ignored
        } else {
            ParsedDirection::Event(attachments)
        }
    }

    /// Invert [`MusicXmlWriter::write_tempo_direction`]: reconstruct a
    /// [`TempoModel`] from a tempo `<direction>`. Returns `None` when the
    /// direction is not a tempo (so the caller treats it as a plain words /
    /// decoration direction).
    ///
    /// The writer emits two tempo shapes, both **voice-less** and always carrying
    /// a `<sound tempo=...>`:
    /// - a **numeric** tempo: an optional tempo `<words>` (`tempo.text`) then a
    ///   `<metronome>` (`<beat-unit>` + optional `<beat-unit-dot/>` +
    ///   `<per-minute>`). The reader recovers `text` from the words and `beat`
    ///   from the metronome.
    /// - a **text-only** tempo (no numeric beat): just a tempo `<words>` + the
    ///   `<sound>` (the `tempo.text`-only `TempoModel`, beat `None`). The reader
    ///   recovers `text` and leaves `beat = None`.
    ///
    /// The voice-less + `<sound>` shape is what distinguishes a tempo direction
    /// from a regular annotation `<words>` direction (which is voice-bearing and
    /// has no `<sound>`). A `<metronome>` whose `<beat-unit>`/`<per-minute>`
    /// cannot be parsed yields `None`.
    fn read_tempo_direction(&mut self, direction: Node<'_, '_>) -> Option<TempoModel> {
        // A voice-bearing direction is never a tempo (tempo directions carry no
        // `<voice>`); bail so it is reconstructed as an annotation/decoration.
        if child_element(direction, "voice").is_some() {
            return None;
        }
        // The tempo words (if any) are the `<words>` of a direction-type that does
        // NOT contain the metronome. Shared by both shapes.
        let words = || {
            children_named(direction, "direction-type")
                .filter(|dt| child_element(*dt, "metronome").is_none())
                .find_map(|dt| child_element(dt, "words"))
                .map(|words| raw_text(words).to_owned())
        };

        if let Some(metronome) = descendants_named(direction, "metronome").next() {
            let beat = self.read_tempo_beat(metronome)?;
            return Some(TempoModel {
                text: words(),
                beat: Some(beat),
                source_span: READER_SPAN,
            });
        }

        // No metronome: a text-only tempo is a voice-less words direction WITH a
        // `<sound>` (the writer's `tempo.text`-only fallback). Without a `<sound>`
        // it is an ordinary words annotation, not a tempo.
        if child_element(direction, "sound").is_some()
            && let Some(text) = words()
        {
            return Some(TempoModel {
                text: Some(text),
                beat: None,
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
        let bpm = match child_text(metronome, "per-minute").map(|t| t.parse::<u32>()) {
            Some(Ok(bpm)) => bpm,
            _ => {
                self.warn(
                    "musicxml.read.invalid_per_minute",
                    "<per-minute> is missing or not a non-negative integer; tempo ignored",
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

    /// S5b: invert [`MusicXmlWriter::write_harmony`]. The writer emits a chord
    /// symbol's `<harmony>` (`<root>`, `<kind text="…">`, optional `<bass>`,
    /// `<degree>`s) from the ABC chord-symbol *string*, and crucially preserves
    /// that exact original string as the `<kind text="…">` attribute. The reader
    /// therefore reconstructs the [`TextAttachment`] directly from `text`: re-parsing
    /// it through `parse_chord_symbol` reproduces the identical `<root>`/`<kind>`/
    /// `<bass>`/`<degree>` tree byte-for-byte, so no kind-value→suffix inversion is
    /// needed. (The writer only ever emits `<harmony>` when the string parses as a
    /// chord; a non-chord string is emitted as a `<direction><words>` instead, which
    /// the S5a direction reader already round-trips as an annotation.)
    ///
    /// A `<kind>` with no `text` attribute is not croma's own output (the writer
    /// always sets it); it has no recoverable ABC source, so it warns and is
    /// skipped rather than inventing a chord spelling from the kind value.
    fn read_harmony(&mut self, harmony: Node<'_, '_>) -> Option<TextAttachment> {
        let text = match child_element(harmony, "kind").and_then(|kind| kind.attribute("text")) {
            Some(text) => text,
            None => {
                self.warn(
                    "musicxml.read.harmony_without_kind_text",
                    "<harmony> has no <kind text=...>; chord symbol not reconstructed",
                );
                return None;
            }
        };
        Some(TextAttachment {
            text: text.to_owned(),
            span: READER_SPAN,
            // A chord symbol carries no placement (the writer's `<harmony>` has no
            // placement attribute); the lowering's chord_symbols are placement-less.
            placement: None,
        })
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
        let pitch_node = child_element(note_node, "pitch")?;
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
        // `<alter>` is optional; the writer omits it when zero.
        let alter = child_text(pitch_node, "alter")
            .and_then(|text| text.trim().parse::<i8>().ok())
            .unwrap_or(0);
        Some(Pitch {
            step,
            alter,
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
    instruments: Vec<MidiInstrumentModel>,
}

/// One `<part>` reconstructed, plus the header [`TempoModel`] (S5a) captured from
/// a voice-less tempo direction before its first note. Only part 1 yields a
/// header tempo; for every other part it is `None`.
struct PartOutcome {
    part: Part,
    header_tempo: Option<TempoModel>,
}

struct MeasureOutcome {
    events: Vec<TimedEvent>,
    measure: Measure,
    /// The header tempo (S5a) when this measure was the header-eligible first
    /// measure of part 1 and a voice-less tempo direction preceded its first
    /// note. `None` otherwise.
    header_tempo: Option<TempoModel>,
}

/// S5a: the classification of one `<direction>` by the reader.
enum ParsedDirection {
    /// A voice-less tempo direction (`<metronome>`); the caller routes it to the
    /// header `tempo_model` or a mid-tune `TempoChange`.
    Tempo(TempoModel),
    /// A voice-bearing direction reconstructed into attachments (annotation
    /// words, dynamics, coda/segno, wedge) for the following event.
    Event(EventAttachments),
    /// A direction with no model-backed inverse the writer emits.
    Ignored,
}

struct ParsedNote {
    kind: TimedEventKind,
    duration: Fraction,
    chord_member: bool,
    measure_rest: bool,
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
    ) -> Vec<TupletAttachment> {
        let mut out = Vec::new();
        let has_start = markers.iter().any(|(role, _)| *role == TupletRole::Start);
        let has_stop = markers.iter().any(|(role, _)| *role == TupletRole::Stop);

        // Middle (continue) note: a time-modification, an open tuplet, and no
        // start/stop marker on this note.
        if !has_start && !has_stop {
            if time_modification.is_some() && !self.open.is_empty() {
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
        out
    }
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
    }
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

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
