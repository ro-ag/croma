use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
use crate::model::{
    AccidentalMark, DecorationAttachment, EventAttachments, Fraction, GraceNoteEvent, Pitch,
    RestEvent, Score, StaffId, TimedEvent, TimedEventKind, TimelineEventKind, TupletAttachment,
    VoiceTimedEvent,
};
use crate::parse::ParseReport;

mod attributes;
mod barline;
mod direction;
mod grace;
mod harmony;
mod lyric;
mod notation;
mod note;
mod score;

pub fn write_score_partwise(score: &Score) -> ParseReport<String> {
    let mut writer = MusicXmlWriter::new(score);
    writer.write();
    ParseReport::new(writer.xml.finish(), writer.diagnostics)
}

struct MusicXmlWriter<'score> {
    score: &'score Score,
    xml: XmlWriter,
    diagnostics: Vec<Diagnostic>,
}

impl<'score> MusicXmlWriter<'score> {
    fn new(score: &'score Score) -> Self {
        Self {
            score,
            xml: XmlWriter::new(),
            diagnostics: Vec::new(),
        }
    }

    fn write(&mut self) {
        self.xml.declaration();
        self.xml.start("score-partwise", &[("version", "4.0")]);
        self.write_metadata();
        self.write_credits();
        self.write_part_list();
        for (part_index, part) in self.score.parts.iter().enumerate() {
            self.write_part(part, part_index);
        }
        self.xml.end("score-partwise");
    }

    fn write_forward(&mut self, duration: Fraction) {
        if duration == Fraction::zero() {
            return;
        }
        let duration = self.duration_to_divisions(duration, self.score.source_span);
        self.xml.start("forward", &[]);
        self.xml.text_element("duration", &duration.to_string());
        self.xml.end("forward");
    }

    fn write_backup(&mut self, duration: Fraction) {
        if duration == Fraction::zero() {
            return;
        }
        let duration = self.duration_to_divisions(duration, self.score.source_span);
        self.xml.start("backup", &[]);
        self.xml.text_element("duration", &duration.to_string());
        self.xml.end("backup");
    }

    fn duration_to_divisions(&mut self, duration: Fraction, span: Span) -> u32 {
        let divisions = self.score.divisions.max(1);
        let numerator = u64::from(duration.numerator) * 4 * u64::from(divisions);
        let denominator = u64::from(duration.denominator.max(1));
        if numerator % denominator != 0 {
            self.diagnostics.push(non_integral_duration_warning(span));
        }
        u32::try_from((numerator / denominator).max(1)).unwrap_or(u32::MAX)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NoteWrite<'a> {
    pitch: Option<&'a Pitch>,
    rest: Option<&'a RestEvent>,
    duration: Fraction,
    source: Span,
    written_accidental: Option<&'a AccidentalMark>,
    attachments: &'a EventAttachments,
    chord_member: bool,
    grace: bool,
    grace_slash: bool,
}

#[derive(Debug, Clone, Copy)]
struct GraceNoteWrite<'a> {
    note: &'a GraceNoteEvent,
    source: Span,
    chord_member: bool,
    slash: bool,
    display_duration: Fraction,
}

#[derive(Debug, Clone)]
pub(crate) struct MeasureSequence<'score> {
    voice_number: String,
    staff: StaffId,
    events: Vec<SequenceEvent<'score>>,
}

#[derive(Debug, Clone)]
enum SequenceEvent<'score> {
    Timed(&'score TimedEvent),
    Overlay(&'score VoiceTimedEvent),
}

impl SequenceEvent<'_> {
    fn onset(&self) -> Fraction {
        match self {
            Self::Timed(event) => event.onset,
            Self::Overlay(event) => event.onset,
        }
    }

    fn duration(&self) -> Fraction {
        match self {
            Self::Timed(event) => event.duration,
            Self::Overlay(event) => event.duration,
        }
    }

    fn attachments(&self) -> &EventAttachments {
        match self {
            Self::Timed(event) => &event.attachments,
            Self::Overlay(event) => &event.attachments,
        }
    }

    fn advances_time(&self) -> bool {
        match self {
            Self::Timed(event) => matches!(
                event.kind,
                TimedEventKind::Note(_) | TimedEventKind::Chord(_) | TimedEventKind::Rest(_)
            ),
            Self::Overlay(event) => matches!(
                event.kind,
                TimelineEventKind::Note { .. } | TimelineEventKind::Rest { .. }
            ),
        }
    }

    fn is_chord_member(&self) -> bool {
        match self {
            Self::Timed(event) => match &event.kind {
                TimedEventKind::Note(note) => note.chord_member,
                _ => false,
            },
            Self::Overlay(event) => match event.kind {
                TimelineEventKind::Note { chord, .. } => chord,
                _ => false,
            },
        }
    }

    fn source_start(&self) -> usize {
        match self {
            Self::Timed(event) => event.source.start,
            Self::Overlay(event) => event.span.start,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct TupletNumbers {
    pairs: Vec<(u32, u32)>,
}

impl TupletNumbers {
    pub(crate) fn number_for(&self, pair_id: u32) -> u32 {
        self.pairs
            .iter()
            .find_map(|(pair, number)| (*pair == pair_id).then_some(*number))
            .unwrap_or(1)
    }
}

struct XmlWriter {
    output: String,
    indent: usize,
}

impl XmlWriter {
    fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
        }
    }

    fn finish(self) -> String {
        self.output
    }

    fn declaration(&mut self) {
        self.output
            .push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    }

    fn start(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.write_indent();
        self.output.push('<');
        self.output.push_str(name);
        self.write_attrs(attrs);
        self.output.push_str(">\n");
        self.indent += 1;
    }

    fn end(&mut self, name: &str) {
        self.indent = self.indent.saturating_sub(1);
        self.write_indent();
        self.output.push_str("</");
        self.output.push_str(name);
        self.output.push_str(">\n");
    }

    fn empty(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.write_indent();
        self.output.push('<');
        self.output.push_str(name);
        self.write_attrs(attrs);
        self.output.push_str("/>\n");
    }

    fn text_element(&mut self, name: &str, text: &str) {
        self.text_element_attrs(name, &[], text);
    }

    fn text_element_attrs(&mut self, name: &str, attrs: &[(&str, &str)], text: &str) {
        self.write_indent();
        self.output.push('<');
        self.output.push_str(name);
        self.write_attrs(attrs);
        self.output.push('>');
        self.output.push_str(&escape_xml(text));
        self.output.push_str("</");
        self.output.push_str(name);
        self.output.push_str(">\n");
    }

    fn write_attrs(&mut self, attrs: &[(&str, &str)]) {
        for (name, value) in attrs {
            self.output.push(' ');
            self.output.push_str(name);
            self.output.push_str("=\"");
            self.output.push_str(&escape_xml(value));
            self.output.push('"');
        }
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("  ");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimeModification {
    actual_notes: u32,
    normal_notes: u32,
}

impl From<&TupletAttachment> for TimeModification {
    fn from(tuplet: &TupletAttachment) -> Self {
        Self {
            actual_notes: tuplet.actual_notes,
            normal_notes: tuplet.normal_notes,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum BarlineLocation {
    Left,
    Right,
}

impl BarlineLocation {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum EndingType {
    Start,
    Stop,
}

trait FractionExt {
    fn subtract(self, other: Self) -> Self;
}

impl FractionExt for Fraction {
    fn subtract(self, other: Self) -> Self {
        let numerator = self
            .numerator
            .saturating_mul(other.denominator)
            .saturating_sub(other.numerator.saturating_mul(self.denominator));
        let denominator = self.denominator.saturating_mul(other.denominator);
        Fraction::new(numerator, denominator)
    }
}

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn unsupported_decoration_warning(decoration: &DecorationAttachment) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.musicxml.decoration.unsupported",
        format!(
            "Decoration `{}` is preserved as MusicXML direction text",
            decoration.name
        ),
        decoration.span,
    )
    .with_spec_reference(musicxml_reference("direction"))
    .with_recovery_note(RecoveryNote::new(
        "The decorated note was exported and timing was unchanged.",
    ))
}

fn variable_chord_duration_export_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.musicxml.chord.variable_duration",
        "Variable-duration chord members were exported as same-onset MusicXML chord tones",
        span,
    )
    .with_spec_reference(musicxml_reference("chord"))
    .with_recovery_note(RecoveryNote::new(
        "The following note onset follows the semantic base chord duration.",
    ))
}

fn unsupported_grace_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.musicxml.grace.unsupported",
        "Grace group has no semantic grace-note events to export",
        span,
    )
    .with_spec_reference(musicxml_reference("grace"))
    .with_recovery_note(RecoveryNote::new(
        "The following time-bearing note was exported unchanged.",
    ))
}

fn unsupported_transpose_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.musicxml.transpose.unsupported",
        "Voice transpose value is not a numeric chromatic transposition",
        span,
    )
    .with_spec_reference(musicxml_reference("transpose"))
    .with_recovery_note(RecoveryNote::new(
        "The transposition text was preserved in the semantic voice properties.",
    ))
}

fn non_integral_duration_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.musicxml.duration.non_integral",
        "Duration does not map exactly to the selected MusicXML divisions",
        span,
    )
    .with_spec_reference(musicxml_reference("duration"))
    .with_recovery_note(RecoveryNote::new(
        "The duration was truncated to a positive MusicXML duration value.",
    ))
}

fn unsupported_note_type_warning(span: Span, duration: Fraction) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.musicxml.duration.unsupported_note_type",
        format!(
            "Duration {}/{} does not map cleanly to a supported MusicXML note type",
            duration.numerator, duration.denominator
        ),
        span,
    )
    .with_spec_reference(musicxml_reference("type"))
    .with_recovery_note(RecoveryNote::new(
        "A valid MusicXML duration was exported with a conservative quarter-note type.",
    ))
}

fn unsupported_duration_diagnostics(_attachments: &EventAttachments) -> Vec<Diagnostic> {
    Vec::new()
}

fn musicxml_reference(element: &str) -> SpecReference {
    SpecReference::new(format!("MusicXML 4.0 `{element}` element"))
        .with_url("https://www.w3.org/2021/06/musicxml40/musicxml-reference/")
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
