use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
use crate::model::{
    AccidentalMark, ClefChangeModel, DecorationAttachment, EventAttachments, Fraction,
    GraceNoteEvent, Pitch, RestEvent, RestVisibility, Score, SlurRole, StaffId, TimedEvent,
    TimedEventKind, TimelineEventKind, TupletAttachment, VoiceTimedEvent, XVOICE_SLUR_PAIR_ID_BASE,
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
#[cfg(feature = "musicxml-reader")]
pub mod read;
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
    /// The key in effect at the current write position — the header key until
    /// a mid-tune `KeyChange` event passes through `write_event`. Used for
    /// implicit grace-note alters. Reset to the header key at each part start.
    /// (Voices within a part share this; per-voice inline divergence is rare
    /// and only affects implicit grace spelling.)
    active_key: Option<crate::model::KeySignatureModel>,
    slur_numbers: SlurNumbers,
    lyric_hyphen_open: Vec<OpenLyricHyphen>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenLyricHyphen {
    voice_key: String,
    verse: u32,
}

impl<'score> MusicXmlWriter<'score> {
    fn new(score: &'score Score) -> Self {
        Self {
            score,
            xml: XmlWriter::new(),
            active_key: score.metadata.key.clone(),
            slur_numbers: SlurNumbers::default(),
            lyric_hyphen_open: Vec::new(),
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
    measure_rest: bool,
    unpitched: bool,
    grace: bool,
    grace_slash: bool,
    /// Time-modification a chord MEMBER inherits from the chord's tuplet. A member
    /// carries no `tuplets` of its own (the bracket belongs to the head), so without
    /// this it would emit no `<time-modification>` and lose the ratio on re-export.
    /// `None` for the head and every non-member note — they derive it from their own
    /// `attachments.tuplets`.
    chord_tuplet_time_modification: Option<TimeModification>,
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
    slur_voice_key: String,
    staff: StaffId,
    expected_duration: Option<Fraction>,
    actual_duration: Fraction,
    unpitched: bool,
    musicxml_sequence_backup: Option<Fraction>,
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

    fn clef_cursor_script(&self) -> Option<(&ClefChangeModel, Fraction, Fraction)> {
        match self {
            Self::Timed(event) => match &event.kind {
                TimedEventKind::ClefChange(clef) => Some((
                    clef,
                    clef.musicxml_cursor_pre_backup?,
                    clef.musicxml_cursor_back?,
                )),
                _ => None,
            },
            Self::Overlay(_) => None,
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

    fn emits_musicxml(&self) -> bool {
        match self {
            Self::Timed(event) => {
                matches!(
                    event.kind,
                    TimedEventKind::Note(_)
                        | TimedEventKind::Chord(_)
                        | TimedEventKind::Rest(_)
                        | TimedEventKind::KeyChange(_)
                        | TimedEventKind::MeterChange(_)
                        | TimedEventKind::ClefChange(_)
                        | TimedEventKind::TempoChange(_)
                        | TimedEventKind::SectionLabel(_)
                ) || !event.attachments.is_empty()
            }
            Self::Overlay(event) => {
                matches!(
                    event.kind,
                    TimelineEventKind::Note { .. } | TimelineEventKind::Rest { .. }
                ) || !event.attachments.is_empty()
            }
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

impl MeasureSequence<'_> {
    fn is_full_measure_rest(&self, onset: Fraction, duration: Fraction, rest: &RestEvent) -> bool {
        rest.visibility == RestVisibility::Visible
            && onset == Fraction::zero()
            && self
                .expected_duration
                .is_some_and(|expected| expected == duration && self.actual_duration == expected)
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

#[derive(Debug, Default)]
pub(crate) struct SlurNumbers {
    active: Vec<ActiveSlurNumber>,
}

impl SlurNumbers {
    pub(crate) fn number_for(&mut self, slur_voice_key: &str, pair_id: u32, role: SlurRole) -> u32 {
        // A cross-voice slur (reconstructed from `[I:croma-xvoice-slur]`) has its
        // two ends under different voice keys, so the per-voice start/stop
        // bookkeeping cannot pair them. Number them by `pair_id` alone — which is
        // unique in the reserved range — so both ends share one `<slur number>`.
        if pair_id >= XVOICE_SLUR_PAIR_ID_BASE {
            return self.cross_voice_number(pair_id);
        }
        match role {
            SlurRole::Start => self.start(slur_voice_key, pair_id),
            SlurRole::Stop => self.stop(slur_voice_key, pair_id),
        }
    }

    /// Number both ends of a cross-voice slur identically, independent of which
    /// end the writer emits first (voice order can place the stop before the
    /// start). The first end seen reserves the lowest free number; the second
    /// end reuses and releases it.
    fn cross_voice_number(&mut self, pair_id: u32) -> u32 {
        if let Some(index) = self
            .active
            .iter()
            .position(|active| active.pair_id == pair_id)
        {
            return self.active.remove(index).number;
        }
        let number = self.lowest_available_number();
        self.active.push(ActiveSlurNumber {
            slur_voice_key: String::new(),
            pair_id,
            number,
        });
        number
    }

    fn start(&mut self, slur_voice_key: &str, pair_id: u32) -> u32 {
        if let Some(active) = self
            .active
            .iter()
            .find(|active| active.slur_voice_key == slur_voice_key && active.pair_id == pair_id)
        {
            return active.number;
        }

        let preferred = pair_id.max(1);
        let number = if self.number_is_active(preferred) {
            self.lowest_available_number()
        } else {
            preferred
        };
        self.active.push(ActiveSlurNumber {
            slur_voice_key: slur_voice_key.to_owned(),
            pair_id,
            number,
        });
        number
    }

    fn stop(&mut self, slur_voice_key: &str, pair_id: u32) -> u32 {
        let Some(index) = self.active.iter().position(|active| {
            active.slur_voice_key == slur_voice_key && active.pair_id == pair_id
        }) else {
            return 1;
        };
        self.active.remove(index).number
    }

    fn lowest_available_number(&self) -> u32 {
        for number in 1..=u32::MAX {
            if !self.number_is_active(number) {
                return number;
            }
        }
        u32::MAX
    }

    fn number_is_active(&self, number: u32) -> bool {
        self.active.iter().any(|active| active.number == number)
    }
}

#[derive(Debug)]
struct ActiveSlurNumber {
    slur_voice_key: String,
    pair_id: u32,
    number: u32,
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

impl TimeModification {
    pub(crate) fn composite(tuplets: &[TupletAttachment]) -> Result<Option<Self>, ()> {
        let mut seen_pairs = Vec::new();
        let mut actual_notes = 1u32;
        let mut normal_notes = 1u32;
        for tuplet in tuplets {
            if seen_pairs.contains(&tuplet.pair_id) {
                continue;
            }
            seen_pairs.push(tuplet.pair_id);
            let Some(product) = checked_ratio_product(
                actual_notes,
                normal_notes,
                tuplet.actual_notes,
                tuplet.normal_notes,
            ) else {
                return Err(());
            };
            actual_notes = product.0;
            normal_notes = product.1;
        }
        // A 1:1 composite is an identity ratio (e.g. a foreign display-only
        // `<tuplet>` bracket carrying no `<time-modification>`): it neither
        // compresses the written duration nor warrants a `<time-modification>`
        // element, so report no modification while the bracket still emits.
        Ok(
            (!seen_pairs.is_empty() && (actual_notes, normal_notes) != (1, 1)).then_some(Self {
                actual_notes,
                normal_notes,
            }),
        )
    }
}

fn checked_ratio_product(
    actual: u32,
    normal: u32,
    factor_actual: u32,
    factor_normal: u32,
) -> Option<(u32, u32)> {
    let actual = u64::from(actual) * u64::from(factor_actual);
    let normal = u64::from(normal) * u64::from(factor_normal);
    ratio_to_u32(actual, normal)
}

fn ratio_to_u32(actual: u64, normal: u64) -> Option<(u32, u32)> {
    if actual <= u64::from(u32::MAX) && normal <= u64::from(u32::MAX) {
        return Some((actual as u32, normal as u32));
    }
    let gcd = gcd_u64(actual, normal);
    let actual = actual / gcd;
    let normal = normal / gcd;
    (actual <= u64::from(u32::MAX) && normal <= u64::from(u32::MAX))
        .then_some((actual as u32, normal as u32))
}

fn gcd_u64(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EndingType {
    Start,
    Stop,
    Discontinue,
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
            "Decoration `{}` has no MusicXML mapping and was ignored",
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

fn unsupported_tuplet_time_modification_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.musicxml.tuplet.time_modification_overflow",
        "Nested tuplet ratio product is too large for a supported MusicXML time-modification",
        span,
    )
    .with_spec_reference(musicxml_reference("time-modification"))
    .with_recovery_note(RecoveryNote::new(
        "Tuplet notation was exported, but the oversized composite time-modification was omitted.",
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
