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
//! # Stage S1 (this module)
//! `<score-partwise>` -> parts -> measures -> `<note>` (`<pitch>`/`<rest>`,
//! `<duration>`/`<type>`/`<dot>`), `<backup>`/`<forward>`, plus the
//! work-title/composer/credit metadata the writer reads back. `<divisions>` is
//! read because it is needed to map `<duration>` to a [`Fraction`]. Attributes
//! (`<key>`/`<time>`/`<clef>`/`<transpose>`), part-list MIDI, notations,
//! directions, multi-voice, repeats, grace and chords are **later stages** and
//! are intentionally not reconstructed yet — files that use them simply do not
//! round-trip idempotently yet, which the corpus gate measures.

use crate::diagnostic::{Diagnostic, Severity, Span};
use crate::model::{
    Accidental, AccidentalMark, AccidentalPolicy, AccidentalScope, Fraction, Measure, MeasureId,
    NoteEvent, Part, PartId, Pitch, RestEvent, RestVisibility, Score, ScoreMetadata, Staff,
    StaffId, TextLine, TimedEvent, TimedEventKind, Voice, VoiceId, VoicePropertiesModel,
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

        // Part names come from the <part-list>; the music comes from the
        // sibling <part> elements. The writer keys them by matching `id`.
        let part_names = self.read_part_list(root);
        for part_node in children_named(root, "part") {
            let part = self.read_part(part_node, score.divisions, &part_names);
            score.parts.push(part);
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

    /// Map `score-part` id -> part name. (Full `<score-instrument>` /
    /// `<midi-instrument>` reconstruction is stage S3 and is deferred.)
    fn read_part_list(&mut self, root: Node<'_, '_>) -> Vec<(String, Option<String>)> {
        let Some(part_list) = children_named(root, "part-list").next() else {
            return Vec::new();
        };
        children_named(part_list, "score-part")
            .map(|score_part| {
                let id = score_part.attribute("id").unwrap_or_default().to_owned();
                let name = child_text(score_part, "part-name").map(str::to_owned);
                (id, name)
            })
            .collect()
    }

    fn read_part(
        &mut self,
        part_node: Node<'_, '_>,
        divisions: u32,
        part_names: &[(String, Option<String>)],
    ) -> Part {
        let id = part_node.attribute("id").unwrap_or_default().to_owned();
        let name = part_names
            .iter()
            .find(|(part_id, _)| *part_id == id)
            .and_then(|(_, name)| name.clone())
            .filter(|name| !name.is_empty())
            .map(text_line);

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

        for measure_node in children_named(part_node, "measure") {
            let measure_id = self.read_measure_id(measure_node, measures.len());
            let outcome = self.read_measure(measure_node, divisions, measure_id);
            events.extend(outcome.events);
            measures.push(outcome.measure);
        }

        let voice = Voice {
            id: voice_id.clone(),
            staff: staff_id,
            initial_properties: VoicePropertiesModel::default(),
            properties: VoicePropertiesModel::default(),
            measures,
            events,
            midi_instrument: None,
            midi_transpose: None,
            source_span: READER_SPAN,
        };

        Part {
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

        for child in element_children(measure_node) {
            match child.tag_name().name() {
                "note" => {
                    let Some(parsed) = self.read_note(child, divisions) else {
                        continue;
                    };
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

                    events.push(TimedEvent {
                        measure: measure_id,
                        onset,
                        duration: parsed.duration,
                        source: READER_SPAN,
                        kind: parsed.kind,
                        attachments: Default::default(),
                    });

                    if !parsed.chord_member {
                        last_onset = onset;
                        cursor = cursor.checked_add(parsed.duration);
                        max_cursor = max_fraction(max_cursor, cursor);
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
                // <direction>, <harmony> etc. are read in later stages.
                _ => {}
            }
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

struct MeasureOutcome {
    events: Vec<TimedEvent>,
    measure: Measure,
}

struct ParsedNote {
    kind: TimedEventKind,
    duration: Fraction,
    chord_member: bool,
    measure_rest: bool,
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
