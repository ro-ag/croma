use std::cmp::Ordering;

use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
use crate::model::{
    AccidentalMark, BarlineKind, ChordEvent, DecorationAttachment, EventAttachments, Fraction,
    GraceNoteEvent, Measure, MeasureBarline, MeasureId, Part, Pitch, RestEvent, RestVisibility,
    Score, StaffId, TieRole, TimedEvent, TimedEventKind, TimelineEventKind, TupletAttachment,
    TupletRole, VoiceTimedEvent,
};
use crate::parse::ParseReport;

use grace::grace_export_pitch;

mod attributes;
mod barline;
mod direction;
mod grace;
mod harmony;
mod lyric;
mod notation;

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

    /// `W:` post-tune words are text printed after the tune (ABC 2.1), not music
    /// aligned to notes. MusicXML represents such page-level text with
    /// score-header `<credit>` elements rather than in-measure directions.
    fn write_credits(&mut self) {
        for line in &self.score.metadata.post_tune_lyrics {
            if line.text.trim().is_empty() {
                continue;
            }
            self.xml.start("credit", &[("page", "1")]);
            self.xml.text_element("credit-words", &line.text);
            self.xml.end("credit");
        }
    }

    fn write_metadata(&mut self) {
        if let Some(title) = &self.score.metadata.title {
            self.xml.start("work", &[]);
            self.xml.text_element("work-title", &title.text);
            self.xml.end("work");
        }

        if !self.score.metadata.composers.is_empty() {
            self.xml.start("identification", &[]);
            for composer in &self.score.metadata.composers {
                self.xml
                    .text_element_attrs("creator", &[("type", "composer")], &composer.text);
            }
            self.xml.end("identification");
        }
    }

    fn write_part_list(&mut self) {
        self.xml.start("part-list", &[]);
        for (index, part) in self.score.parts.iter().enumerate() {
            let id = part_xml_id(part, index);
            self.xml.start("score-part", &[("id", id.as_str())]);
            self.xml
                .text_element("part-name", part_name(part, self.score).as_str());
            self.xml.end("score-part");
        }
        self.xml.end("part-list");
    }

    fn write_part(&mut self, part: &'score Part, part_index: usize) {
        let id = part_xml_id(part, part_index);
        self.xml.start("part", &[("id", id.as_str())]);
        let mut pending_left_repeat = false;
        for (measure_position, measure_id) in part_measure_ids(part).iter().enumerate() {
            let number = measure_id.number.to_string();
            self.xml.start("measure", &[("number", number.as_str())]);
            if measure_position == 0 {
                self.write_attributes(part);
                self.write_initial_directions(part, part_index == 0);
            }

            if pending_left_repeat {
                self.write_barline(BarlineLocation::Left, BarlineKind::RepeatStart, &[]);
                pending_left_repeat = false;
            }

            let measure_refs = part_measure_refs(part, *measure_id);
            let left_barlines = unique_barlines(&measure_refs, true);
            for barline in &left_barlines {
                self.write_barline(BarlineLocation::Left, barline.kind, &[]);
            }

            let endings = unique_endings(&measure_refs);
            if !endings.is_empty() {
                self.write_ending_barline(BarlineLocation::Left, &endings, EndingType::Start, None);
            }

            let sequences = measure_sequences(part, *measure_id);
            for (sequence_index, sequence) in sequences.iter().enumerate() {
                let cursor = self.write_sequence(sequence, part);
                if sequence_index + 1 < sequences.len() && cursor != Fraction::zero() {
                    self.write_backup(cursor);
                }
            }

            let right_barlines = unique_barlines(&measure_refs, false);
            for barline in &right_barlines {
                let ending_type = (!endings.is_empty()
                    && stops_repeat_ending_barline(barline.kind))
                .then_some(EndingType::Stop);
                if let Some(ending_type) = ending_type {
                    self.write_ending_barline(
                        BarlineLocation::Right,
                        &endings,
                        ending_type,
                        Some(barline.kind),
                    );
                } else {
                    self.write_barline(BarlineLocation::Right, barline.kind, &[]);
                }
                if barline.kind == BarlineKind::RepeatBoth {
                    pending_left_repeat = true;
                }
            }

            // A trailing `|:` (a forward repeat that follows content rather than
            // opening its measure) is stored in this measure but begins the
            // *next* measure's repeated section. Defer it like a `RepeatBoth`'s
            // left half so it is emitted as the next measure's LEFT barline.
            if trailing_left_repeat_pending(&measure_refs) {
                pending_left_repeat = true;
            }

            self.xml.end("measure");
        }
        self.xml.end("part");
    }

    fn write_sequence(&mut self, sequence: &MeasureSequence<'score>, part: &Part) -> Fraction {
        let mut cursor = Fraction::zero();
        let mut last_onset = Fraction::zero();
        let tuplet_numbers = sequence_tuplet_numbers(sequence);
        for event in &sequence.events {
            let onset = event.onset();
            let is_chord_member = event.is_chord_member();
            if is_chord_member && onset == last_onset {
                self.write_event(event, sequence, part, &tuplet_numbers, true);
                continue;
            }
            if cursor.less_than(onset) {
                self.write_forward(onset.subtract(cursor));
                cursor = onset;
            } else if onset.less_than(cursor) {
                self.write_backup(cursor.subtract(onset));
                cursor = onset;
            }
            self.write_event(event, sequence, part, &tuplet_numbers, false);
            if event.advances_time() {
                cursor = cursor.checked_add(event.duration());
                last_onset = onset;
            }
        }
        cursor
    }

    fn write_event(
        &mut self,
        event: &SequenceEvent<'score>,
        sequence: &MeasureSequence<'score>,
        part: &Part,
        tuplet_numbers: &TupletNumbers,
        chord_member: bool,
    ) {
        let attachments = event.attachments();
        self.write_harmony_and_directions(attachments, sequence, part);
        self.write_grace_groups(attachments, sequence, part, tuplet_numbers);
        match event {
            SequenceEvent::Timed(timed) => match &timed.kind {
                TimedEventKind::Note(note) => {
                    self.write_note(
                        NoteWrite {
                            pitch: Some(&note.pitch),
                            rest: None,
                            duration: timed.duration,
                            source: timed.source,
                            written_accidental: note.written_accidental.as_ref(),
                            attachments,
                            chord_member: chord_member || note.chord_member,
                            grace: false,
                            grace_slash: false,
                        },
                        sequence,
                        part,
                        tuplet_numbers,
                    );
                }
                TimedEventKind::Chord(chord) => {
                    self.write_chord(chord, sequence, part, tuplet_numbers);
                }
                TimedEventKind::Rest(rest) => {
                    self.write_note(
                        NoteWrite {
                            pitch: None,
                            rest: Some(rest),
                            duration: timed.duration,
                            source: timed.source,
                            written_accidental: None,
                            attachments,
                            chord_member: false,
                            grace: false,
                            grace_slash: false,
                        },
                        sequence,
                        part,
                        tuplet_numbers,
                    );
                }
                TimedEventKind::Spacer
                | TimedEventKind::Barline(_)
                | TimedEventKind::RepeatEnding(_) => {}
            },
            SequenceEvent::Overlay(timed) => match &timed.kind {
                TimelineEventKind::Note {
                    step,
                    octave,
                    effective_accidental,
                    accidental,
                    accidental_source,
                    chord,
                } => {
                    let pitch = Pitch {
                        step: *step,
                        alter: effective_accidental
                            .map(|accidental| accidental.alter())
                            .unwrap_or(0),
                        octave: *octave,
                        spelling_source: timed.span,
                    };
                    let written_accidental = accidental.map(|kind| AccidentalMark {
                        kind,
                        explicit: true,
                        courtesy: false,
                        source: accidental_source.unwrap_or(timed.span),
                    });
                    self.write_note(
                        NoteWrite {
                            pitch: Some(&pitch),
                            rest: None,
                            duration: timed.duration,
                            source: timed.span,
                            written_accidental: written_accidental.as_ref(),
                            attachments,
                            chord_member: chord_member || *chord,
                            grace: false,
                            grace_slash: false,
                        },
                        sequence,
                        part,
                        tuplet_numbers,
                    );
                }
                TimelineEventKind::Rest { visibility } => {
                    let rest = RestEvent {
                        visibility: *visibility,
                    };
                    self.write_note(
                        NoteWrite {
                            pitch: None,
                            rest: Some(&rest),
                            duration: timed.duration,
                            source: timed.span,
                            written_accidental: None,
                            attachments,
                            chord_member: false,
                            grace: false,
                            grace_slash: false,
                        },
                        sequence,
                        part,
                        tuplet_numbers,
                    );
                }
                TimelineEventKind::Spacer
                | TimelineEventKind::Barline { .. }
                | TimelineEventKind::VariantEnding { .. } => {}
            },
        }
    }

    fn write_chord(
        &mut self,
        chord: &ChordEvent,
        sequence: &MeasureSequence<'score>,
        part: &Part,
        tuplet_numbers: &TupletNumbers,
    ) {
        let variable_durations = chord
            .members
            .iter()
            .any(|member| member.duration != chord.members[0].duration);
        if variable_durations {
            self.diagnostics
                .push(variable_chord_duration_export_warning(chord.source_span));
        }
        for (index, member) in chord.members.iter().enumerate() {
            let attachments = if index == 0 {
                sequence
                    .events
                    .iter()
                    .find_map(|event| match event {
                        SequenceEvent::Timed(timed) if timed.source == chord.source_span => {
                            Some(timed.attachments.clone())
                        }
                        _ => None,
                    })
                    .unwrap_or_else(EventAttachments::default)
            } else {
                member.attachments.clone()
            };
            self.write_note(
                NoteWrite {
                    pitch: Some(&member.pitch),
                    rest: None,
                    duration: member.duration,
                    source: member.source_span,
                    written_accidental: member.written_accidental.as_ref(),
                    attachments: &attachments,
                    chord_member: index > 0,
                    grace: false,
                    grace_slash: false,
                },
                sequence,
                part,
                tuplet_numbers,
            );
        }
    }

    fn write_note(
        &mut self,
        note: NoteWrite<'_>,
        sequence: &MeasureSequence<'score>,
        part: &Part,
        tuplet_numbers: &TupletNumbers,
    ) {
        let print_no = note
            .rest
            .is_some_and(|rest| rest.visibility == RestVisibility::Invisible);
        let attrs = print_no.then_some([("print-object", "no")]);
        let attrs_slice = attrs.as_ref().map_or(&[][..], |attrs| &attrs[..]);
        self.xml.start("note", attrs_slice);
        if note.chord_member {
            self.xml.empty("chord", &[]);
        }
        if note.grace {
            if note.grace_slash {
                self.xml.empty("grace", &[("slash", "yes")]);
            } else {
                self.xml.empty("grace", &[]);
            }
        }
        if let Some(pitch) = note.pitch {
            let pitch = if note.grace {
                grace_export_pitch(
                    pitch,
                    note.written_accidental,
                    self.score.metadata.key.as_ref(),
                )
            } else {
                *pitch
            };
            self.write_pitch(&pitch);
        } else {
            self.xml.empty("rest", &[]);
        }
        let explicit_time_modification =
            note.attachments.tuplets.first().map(TimeModification::from);
        let spelling = note_spelling(note.duration, explicit_time_modification);
        if spelling.unsupported {
            self.diagnostics
                .push(unsupported_note_type_warning(note.source, note.duration));
        }
        if !note.grace {
            let duration = self.duration_to_divisions(note.duration, note.source);
            self.xml.text_element("duration", &duration.to_string());
        }
        self.write_ties(&note.attachments.ties);
        self.xml.text_element("voice", &sequence.voice_number);
        self.xml.text_element("type", spelling.note_type);
        for _ in 0..spelling.dots {
            self.xml.empty("dot", &[]);
        }
        if let Some(accidental) = note.written_accidental
            && accidental.explicit
            && self.score.accidental_policy.preserve_explicit_accidentals
        {
            self.xml
                .text_element("accidental", accidental.kind.musicxml_name());
        }
        let time_modification = explicit_time_modification.or(spelling.time_modification);
        if let Some(time_modification) = time_modification {
            self.write_time_modification(time_modification);
        }
        if part.staves.len() > 1 {
            self.xml
                .text_element("staff", &sequence.staff.value.to_string());
        }
        self.write_notations(note.attachments, time_modification, tuplet_numbers);
        self.write_lyrics(&note.attachments.lyrics);
        self.xml.end("note");
    }

    fn write_pitch(&mut self, pitch: &Pitch) {
        self.xml.start("pitch", &[]);
        self.xml.text_element("step", &pitch.step.to_string());
        if pitch.alter != 0 {
            self.xml.text_element("alter", &pitch.alter.to_string());
        }
        self.xml.text_element("octave", &pitch.octave.to_string());
        self.xml.end("pitch");
    }

    fn write_ties(&mut self, ties: &[crate::model::TieAttachment]) {
        for tie in ties {
            self.xml.empty(
                "tie",
                &[(
                    "type",
                    match tie.role {
                        TieRole::Start => "start",
                        TieRole::Stop => "stop",
                    },
                )],
            );
        }
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
struct NoteWrite<'a> {
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
struct MeasureSequence<'score> {
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

fn sequence_tuplet_numbers(sequence: &MeasureSequence<'_>) -> TupletNumbers {
    let mut numbers = TupletNumbers::default();
    let mut active = Vec::<(u32, u32)>::new();

    for event in &sequence.events {
        for tuplet in event
            .attachments()
            .tuplets
            .iter()
            .filter(|tuplet| tuplet.role == TupletRole::Start)
        {
            if numbers
                .pairs
                .iter()
                .any(|(pair, _)| *pair == tuplet.pair_id)
            {
                continue;
            }
            let number = next_tuplet_number(&active);
            numbers.pairs.push((tuplet.pair_id, number));
            active.push((tuplet.pair_id, number));
        }

        for tuplet in event
            .attachments()
            .tuplets
            .iter()
            .filter(|tuplet| tuplet.role == TupletRole::Stop)
        {
            if !numbers
                .pairs
                .iter()
                .any(|(pair, _)| *pair == tuplet.pair_id)
            {
                numbers.pairs.push((tuplet.pair_id, 1));
            }
            active.retain(|(pair, _)| *pair != tuplet.pair_id);
        }
    }

    numbers
}

fn next_tuplet_number(active: &[(u32, u32)]) -> u32 {
    for number in 1..=16 {
        if !active
            .iter()
            .any(|(_, active_number)| *active_number == number)
        {
            return number;
        }
    }
    16
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

fn part_xml_id(part: &Part, index: usize) -> String {
    let value = part.id.value.trim();
    let fallback = format!("P{}", index + 1);
    let raw = if value.is_empty() {
        fallback.as_str()
    } else {
        value
    };
    let mut id = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if id
        .chars()
        .next()
        .is_none_or(|ch| !ch.is_ascii_alphabetic() && ch != '_')
    {
        id.insert(0, 'P');
    }
    id
}

fn part_name(part: &Part, score: &Score) -> String {
    part.name
        .as_ref()
        .or(score.metadata.title.as_ref())
        .map(|line| line.text.clone())
        .or_else(|| {
            part.voices.iter().find_map(|voice| {
                voice
                    .properties
                    .name
                    .as_ref()
                    .or(voice.properties.nm.as_ref())
                    .map(|line| line.text.clone())
            })
        })
        .unwrap_or_else(|| "Music".to_owned())
}

fn part_measure_ids(part: &Part) -> Vec<MeasureId> {
    let mut ids = part
        .voices
        .iter()
        .flat_map(|voice| voice.measures.iter().map(|measure| measure.id))
        .collect::<Vec<_>>();
    ids.sort_by_key(|id| (id.index, id.number));
    ids.dedup();
    ids
}

fn part_measure_refs(part: &Part, id: MeasureId) -> Vec<&Measure> {
    part.voices
        .iter()
        .flat_map(|voice| &voice.measures)
        .filter(|measure| measure.id == id)
        .collect()
}

fn measure_sequences<'score>(part: &'score Part, id: MeasureId) -> Vec<MeasureSequence<'score>> {
    let mut sequences = Vec::new();
    let base_count = part.voices.len();
    for (voice_index, voice) in part.voices.iter().enumerate() {
        let voice_number = (voice_index + 1).to_string();
        let events = voice
            .events
            .iter()
            .filter(|event| event.measure == id)
            .filter(|event| {
                matches!(
                    event.kind,
                    TimedEventKind::Note(_)
                        | TimedEventKind::Chord(_)
                        | TimedEventKind::Rest(_)
                        | TimedEventKind::Spacer
                )
            })
            .map(SequenceEvent::Timed)
            .collect::<Vec<_>>();
        if !events.is_empty() {
            sequences.push(MeasureSequence {
                voice_number,
                staff: voice.staff,
                events,
            });
        }
        if let Some(measure) = voice.measures.iter().find(|measure| measure.id == id) {
            for (overlay_index, overlay) in measure.overlays.iter().enumerate() {
                let overlay_events = overlay
                    .events
                    .iter()
                    .filter(|event| {
                        matches!(
                            event.kind,
                            TimelineEventKind::Note { .. }
                                | TimelineEventKind::Rest { .. }
                                | TimelineEventKind::Spacer
                        )
                    })
                    .map(SequenceEvent::Overlay)
                    .collect::<Vec<_>>();
                if overlay_events.is_empty() {
                    continue;
                }
                sequences.push(MeasureSequence {
                    voice_number: (base_count + overlay_index + 1).to_string(),
                    staff: voice.staff,
                    events: overlay_events,
                });
            }
        }
    }
    for sequence in &mut sequences {
        sequence.events.sort_by(|left, right| {
            compare_fraction(left.onset(), right.onset())
                .then_with(|| left.source_start().cmp(&right.source_start()))
        });
    }
    sequences
}

fn compare_fraction(left: Fraction, right: Fraction) -> Ordering {
    (u64::from(left.numerator) * u64::from(right.denominator))
        .cmp(&(u64::from(right.numerator) * u64::from(left.denominator)))
}

fn unique_barlines(measures: &[&Measure], left: bool) -> Vec<MeasureBarline> {
    let mut barlines = measures
        .iter()
        .flat_map(|measure| measure.barlines.iter().map(|barline| (*measure, barline)))
        .filter(|(measure, barline)| {
            // A `RepeatStart` is a legitimate LEFT barline only when it leads
            // its measure (no note content precedes it). A trailing `|:` that
            // follows content marks the start of the *next* section's body and
            // is deferred (see `trailing_left_repeat_pending`), so it is never
            // emitted as this measure's left barline.
            let leading = is_leading_barline(measure, barline);
            matches!(
                (left, leading, barline.kind),
                (true, true, BarlineKind::RepeatStart)
                    | (
                        false,
                        false,
                        BarlineKind::Double
                            | BarlineKind::Final
                            | BarlineKind::RepeatEnd
                            | BarlineKind::RepeatBoth
                            | BarlineKind::Dotted
                            | BarlineKind::Invisible
                    )
            )
        })
        .map(|(_, barline)| *barline)
        .collect::<Vec<_>>();
    barlines.sort_by_key(|barline| (barline.span.start, barline.span.end, barline.kind as u8));
    barlines.dedup_by_key(|barline| (barline.kind, barline.span));
    barlines
}

/// A barline "leads" its measure when nothing in the measure precedes it, i.e.
/// it opens the measure rather than closing it. ABC stores a trailing `|:`
/// (one that follows note content, like a pickup `E|:` or a mid-tune `...c|:`)
/// in the *preceding* measure's barline vector; such a barline is not leading
/// and its forward-repeat belongs to the measure that begins the repeated body.
fn is_leading_barline(measure: &Measure, barline: &MeasureBarline) -> bool {
    measure.source_span.start == barline.span.start
}

/// True when this measure carries a trailing `RepeatStart` (a `|:` that follows
/// content) whose forward repeat must be deferred to the LEFT barline of the
/// next measure — the one that actually begins the repeated section.
fn trailing_left_repeat_pending(measures: &[&Measure]) -> bool {
    measures.iter().any(|measure| {
        measure.barlines.iter().any(|barline| {
            barline.kind == BarlineKind::RepeatStart && !is_leading_barline(measure, barline)
        })
    })
}

fn stops_repeat_ending_barline(kind: BarlineKind) -> bool {
    matches!(
        kind,
        BarlineKind::Double | BarlineKind::Final | BarlineKind::RepeatEnd | BarlineKind::RepeatBoth
    )
}

fn unique_endings(measures: &[&Measure]) -> Vec<String> {
    let mut endings = measures
        .iter()
        .flat_map(|measure| &measure.repeat_endings)
        .flat_map(|ending| &ending.endings)
        .map(|ending| match ending {
            crate::model::RepeatEndingPartModel::Single(number) => number.to_string(),
            crate::model::RepeatEndingPartModel::Range { start, end } => {
                format!("{start}-{end}")
            }
        })
        .collect::<Vec<_>>();
    endings.sort();
    endings.dedup();
    endings
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
struct NoteSpelling {
    note_type: &'static str,
    dots: u8,
    time_modification: Option<TimeModification>,
    unsupported: bool,
}

fn note_spelling(
    duration: Fraction,
    explicit_time_modification: Option<TimeModification>,
) -> NoteSpelling {
    if duration == Fraction::zero() {
        return NoteSpelling {
            note_type: "eighth",
            dots: 0,
            time_modification: None,
            unsupported: false,
        };
    }

    for candidate in note_type_candidates() {
        for dots in 0..=3 {
            if dotted_fraction(candidate.fraction, dots) == duration {
                return NoteSpelling {
                    note_type: candidate.name,
                    dots,
                    time_modification: None,
                    unsupported: false,
                };
            }
        }
    }

    if let Some(time_modification) = explicit_time_modification {
        let normal_duration = duration.checked_mul(Fraction::new(
            time_modification.actual_notes,
            time_modification.normal_notes,
        ));
        for candidate in note_type_candidates() {
            for dots in 0..=3 {
                if dotted_fraction(candidate.fraction, dots) == normal_duration {
                    return NoteSpelling {
                        note_type: candidate.name,
                        dots,
                        time_modification: None,
                        unsupported: false,
                    };
                }
            }
        }
    }

    for candidate in note_type_candidates() {
        for actual_notes in 2u32..=9 {
            for normal_notes in 1u32..=9 {
                if normal_notes.saturating_mul(2) < actual_notes
                    || normal_notes > actual_notes.saturating_mul(2)
                {
                    continue;
                }
                if candidate
                    .fraction
                    .checked_mul(Fraction::new(normal_notes, actual_notes))
                    == duration
                {
                    return NoteSpelling {
                        note_type: candidate.name,
                        dots: 0,
                        time_modification: Some(TimeModification {
                            actual_notes,
                            normal_notes,
                        }),
                        unsupported: false,
                    };
                }
            }
        }
    }

    NoteSpelling {
        note_type: "quarter",
        dots: 0,
        time_modification: None,
        unsupported: true,
    }
}

#[derive(Debug, Clone, Copy)]
struct NoteTypeCandidate {
    name: &'static str,
    fraction: Fraction,
}

fn note_type_candidates() -> &'static [NoteTypeCandidate] {
    &[
        NoteTypeCandidate {
            name: "breve",
            fraction: Fraction {
                numerator: 2,
                denominator: 1,
            },
        },
        NoteTypeCandidate {
            name: "whole",
            fraction: Fraction {
                numerator: 1,
                denominator: 1,
            },
        },
        NoteTypeCandidate {
            name: "half",
            fraction: Fraction {
                numerator: 1,
                denominator: 2,
            },
        },
        NoteTypeCandidate {
            name: "quarter",
            fraction: Fraction {
                numerator: 1,
                denominator: 4,
            },
        },
        NoteTypeCandidate {
            name: "eighth",
            fraction: Fraction {
                numerator: 1,
                denominator: 8,
            },
        },
        NoteTypeCandidate {
            name: "16th",
            fraction: Fraction {
                numerator: 1,
                denominator: 16,
            },
        },
        NoteTypeCandidate {
            name: "32nd",
            fraction: Fraction {
                numerator: 1,
                denominator: 32,
            },
        },
        NoteTypeCandidate {
            name: "64th",
            fraction: Fraction {
                numerator: 1,
                denominator: 64,
            },
        },
        NoteTypeCandidate {
            name: "128th",
            fraction: Fraction {
                numerator: 1,
                denominator: 128,
            },
        },
    ]
}

fn dotted_fraction(base: Fraction, dots: u8) -> Fraction {
    let mut duration = base;
    let mut dot = base;
    for _ in 0..dots {
        dot = Fraction::new(dot.numerator, dot.denominator.saturating_mul(2));
        duration = duration.checked_add(dot);
    }
    duration
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
