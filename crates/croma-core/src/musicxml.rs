use std::cmp::Ordering;

use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
use crate::model::{
    AccidentalMark, AlignedLyric, AlignedSymbolKind, AnnotationPlacementModel, BarlineKind,
    ChordEvent, DecorationAttachment, EventAttachments, Fraction, GraceEventKind,
    GraceGroupAttachment, GraceNoteEvent, Measure, MeasureBarline, MeasureId, Part, Pitch,
    PreservedDirective, RestEvent, RestVisibility, Score, SlurRole, StaffId, TextAttachment,
    TieRole, TimedEvent, TimedEventKind, TimelineEventKind, TupletAttachment, TupletRole,
    VoiceTimedEvent,
};
use crate::parser::ParseReport;

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
        self.write_part_list();
        for (part_index, part) in self.score.parts.iter().enumerate() {
            self.write_part(part, part_index);
        }
        self.xml.end("score-partwise");
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
                self.write_initial_directions(part);
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
                    && matches!(
                        barline.kind,
                        BarlineKind::RepeatEnd | BarlineKind::RepeatBoth
                    ))
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

            self.xml.end("measure");
        }
        self.xml.end("part");
    }

    fn write_attributes(&mut self, part: &Part) {
        self.xml.start("attributes", &[]);
        self.xml
            .text_element("divisions", &self.score.divisions.max(1).to_string());
        if let Some(key) = &self.score.metadata.key {
            self.xml.start("key", &[]);
            self.xml.text_element("fifths", &key.fifths.to_string());
            for accidental in &key.explicit_accidentals {
                self.xml
                    .text_element("key-step", &accidental.step.to_string());
                self.xml
                    .text_element("key-alter", &accidental.accidental.alter().to_string());
                self.xml
                    .text_element("key-accidental", accidental.accidental.musicxml_name());
            }
            self.xml.end("key");
        }
        if let Some(meter) = &self.score.metadata.meter
            && !meter.free_meter
            && let Some((beats, beat_type, symbol)) = meter_parts(&meter.display)
        {
            let attrs = symbol.map(|symbol| [("symbol", symbol)]);
            let attrs_slice = attrs.as_ref().map_or(&[][..], |attrs| &attrs[..]);
            self.xml.start("time", attrs_slice);
            self.xml.text_element("beats", beats);
            self.xml.text_element("beat-type", beat_type);
            self.xml.end("time");
        }
        if part.staves.len() > 1 {
            self.xml
                .text_element("staves", &part.staves.len().to_string());
        }
        self.write_clefs(part);
        self.write_transpose_if_available(part);
        self.xml.end("attributes");
    }

    fn write_clefs(&mut self, part: &Part) {
        let staves = if part.staves.is_empty() {
            vec![StaffId {
                value: 1,
                span: part.source_span,
            }]
        } else {
            part.staves.iter().map(|staff| staff.id).collect()
        };
        for staff in staves {
            let clef_text = part
                .voices
                .iter()
                .find(|voice| voice.staff.value == staff.value)
                .and_then(|voice| voice.properties.clef.as_ref())
                .map(|clef| clef.text.as_str());
            let clef = clef_model(clef_text);
            let number = staff.value.to_string();
            let attrs = (part.staves.len() > 1).then_some([("number", number.as_str())]);
            let attrs_slice = attrs.as_ref().map_or(&[][..], |attrs| &attrs[..]);
            self.xml.start("clef", attrs_slice);
            self.xml.text_element("sign", clef.sign);
            self.xml.text_element("line", clef.line);
            self.xml.end("clef");
        }
    }

    fn write_transpose_if_available(&mut self, part: &Part) {
        for voice in &part.voices {
            let Some(transpose) = voice.properties.transpose.as_ref() else {
                continue;
            };
            let Ok(chromatic) = transpose.text.trim().parse::<i32>() else {
                self.diagnostics
                    .push(unsupported_transpose_warning(transpose.span));
                continue;
            };
            self.xml.start("transpose", &[]);
            self.xml.text_element("chromatic", &chromatic.to_string());
            self.xml.end("transpose");
            return;
        }
    }

    fn write_initial_directions(&mut self, part: &Part) {
        if let Some(tempo) = &self.score.metadata.tempo {
            self.write_direction_words(&tempo.text, None, Some("1"), Some(1));
        }
        for directive in &self.score.metadata.preserved_directives {
            self.write_preserved_directive(directive, part);
        }
        for words in self
            .score
            .metadata
            .post_tune_lyrics
            .iter()
            .map(|line| &line.text)
        {
            self.write_direction_words(words, None, Some("1"), Some(1));
        }
    }

    fn write_preserved_directive(&mut self, directive: &PreservedDirective, _part: &Part) {
        let text = if directive.value.text.is_empty() {
            format!("%%{}", directive.name.text)
        } else {
            format!("%%{} {}", directive.name.text, directive.value.text)
        };
        self.write_direction_words(&text, None, Some("1"), Some(1));
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

    fn write_grace_groups(
        &mut self,
        attachments: &EventAttachments,
        sequence: &MeasureSequence<'score>,
        part: &Part,
        tuplet_numbers: &TupletNumbers,
    ) {
        for group in &attachments.grace_groups {
            if group.events.is_empty() && group.note_count > 0 {
                self.diagnostics.push(unsupported_grace_warning(group.span));
                continue;
            }
            self.write_grace_group(group, sequence, part, tuplet_numbers);
        }
    }

    fn write_grace_group(
        &mut self,
        group: &GraceGroupAttachment,
        sequence: &MeasureSequence<'score>,
        part: &Part,
        tuplet_numbers: &TupletNumbers,
    ) {
        let mut first_chord_member = true;
        for event in &group.events {
            match &event.kind {
                GraceEventKind::Note(note) => {
                    self.write_grace_note(
                        GraceNoteWrite {
                            note,
                            source: event.source_span,
                            chord_member: false,
                            slash: group.slash.is_some(),
                        },
                        sequence,
                        part,
                        tuplet_numbers,
                    );
                    first_chord_member = false;
                }
                GraceEventKind::Rest(rest) => {
                    self.write_note(
                        NoteWrite {
                            pitch: None,
                            rest: Some(rest),
                            duration: Fraction::zero(),
                            source: event.source_span,
                            written_accidental: None,
                            attachments: &EventAttachments::default(),
                            chord_member: false,
                            grace: true,
                            grace_slash: group.slash.is_some(),
                        },
                        sequence,
                        part,
                        tuplet_numbers,
                    );
                    first_chord_member = false;
                }
                GraceEventKind::Chord(notes) => {
                    for note in notes {
                        self.write_grace_note(
                            GraceNoteWrite {
                                note,
                                source: event.source_span,
                                chord_member: !first_chord_member,
                                slash: group.slash.is_some(),
                            },
                            sequence,
                            part,
                            tuplet_numbers,
                        );
                        first_chord_member = false;
                    }
                }
            }
        }
    }

    fn write_grace_note(
        &mut self,
        grace_note: GraceNoteWrite<'_>,
        sequence: &MeasureSequence<'score>,
        part: &Part,
        tuplet_numbers: &TupletNumbers,
    ) {
        self.write_note(
            NoteWrite {
                pitch: Some(&grace_note.note.pitch),
                rest: None,
                duration: Fraction::zero(),
                source: grace_note.source,
                written_accidental: grace_note.note.written_accidental.as_ref(),
                attachments: &EventAttachments::default(),
                chord_member: grace_note.chord_member,
                grace: true,
                grace_slash: grace_note.slash,
            },
            sequence,
            part,
            tuplet_numbers,
        );
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
            self.write_pitch(pitch);
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

    fn write_notations(
        &mut self,
        attachments: &EventAttachments,
        time_modification: Option<TimeModification>,
        tuplet_numbers: &TupletNumbers,
    ) {
        let has_tied = !attachments.ties.is_empty();
        let has_slurs = !attachments.slurs.is_empty();
        let has_tuplets = attachments
            .tuplets
            .iter()
            .any(|tuplet| matches!(tuplet.role, TupletRole::Start | TupletRole::Stop));
        let has_ornaments = attachments
            .decorations
            .iter()
            .any(|decoration| notation_decoration(decoration).is_some());
        if !(has_tied || has_slurs || has_tuplets || has_ornaments) {
            return;
        }
        self.xml.start("notations", &[]);
        for tie in &attachments.ties {
            let number = tie.pair_id.to_string();
            let mut attrs = vec![
                (
                    "type",
                    match tie.role {
                        TieRole::Start => "start",
                        TieRole::Stop => "stop",
                    },
                ),
                ("number", number.as_str()),
            ];
            if tie.dotted {
                attrs.push(("line-type", "dotted"));
            }
            self.xml.empty("tied", &attrs);
        }
        for slur in &attachments.slurs {
            let number = slur.pair_id.to_string();
            let mut attrs = vec![
                (
                    "type",
                    match slur.role {
                        SlurRole::Start => "start",
                        SlurRole::Stop => "stop",
                    },
                ),
                ("number", number.as_str()),
            ];
            if slur.dotted {
                attrs.push(("line-type", "dotted"));
            }
            self.xml.empty("slur", &attrs);
        }
        for tuplet in &attachments.tuplets {
            let Some(tuplet_type) = (match tuplet.role {
                TupletRole::Start => Some("start"),
                TupletRole::Stop => Some("stop"),
                TupletRole::Continue => None,
            }) else {
                continue;
            };
            let number = tuplet_numbers.number_for(tuplet.pair_id).to_string();
            self.xml.empty(
                "tuplet",
                &[("type", tuplet_type), ("number", number.as_str())],
            );
        }
        if has_ornaments {
            self.xml.start("ornaments", &[]);
            for decoration in &attachments.decorations {
                if let Some(name) = notation_decoration(decoration) {
                    self.xml.empty(name, &[]);
                }
            }
            self.xml.end("ornaments");
        }
        if time_modification.is_none() {
            self.diagnostics
                .extend(unsupported_duration_diagnostics(attachments));
        }
        self.xml.end("notations");
    }

    fn write_time_modification(&mut self, time_modification: TimeModification) {
        self.xml.start("time-modification", &[]);
        self.xml
            .text_element("actual-notes", &time_modification.actual_notes.to_string());
        self.xml
            .text_element("normal-notes", &time_modification.normal_notes.to_string());
        self.xml.end("time-modification");
    }

    fn write_lyrics(&mut self, lyrics: &[AlignedLyric]) {
        for lyric in lyrics {
            if lyric.control == crate::model::LyricControl::Skip {
                continue;
            }
            let number = lyric.verse.to_string();
            self.xml.start("lyric", &[("number", number.as_str())]);
            match lyric.control {
                crate::model::LyricControl::Syllable => {
                    self.xml.text_element("syllabic", "single");
                    self.xml.text_element("text", &lyric.text);
                }
                crate::model::LyricControl::Hyphen => {
                    self.xml.text_element("syllabic", "middle");
                    self.xml.text_element("text", &lyric.text);
                }
                crate::model::LyricControl::Extender => {
                    self.xml.empty("extend", &[]);
                }
                crate::model::LyricControl::Skip => {}
            }
            self.xml.end("lyric");
        }
    }

    fn write_harmony_and_directions(
        &mut self,
        attachments: &EventAttachments,
        sequence: &MeasureSequence<'score>,
        part: &Part,
    ) {
        for symbol in &attachments.chord_symbols {
            self.write_harmony(&symbol.text);
        }
        for symbol in attachments
            .symbols
            .iter()
            .filter(|symbol| symbol.kind == AlignedSymbolKind::ChordSymbol)
        {
            self.write_harmony(&symbol.text);
        }
        for annotation in &attachments.annotations {
            let text = annotation_text(annotation);
            self.write_direction_words(
                text,
                annotation.placement,
                Some(sequence.voice_number.as_str()),
                Some(sequence.staff.value),
            );
        }
        for symbol in attachments.symbols.iter().filter(|symbol| {
            matches!(
                symbol.kind,
                AlignedSymbolKind::Annotation
                    | AlignedSymbolKind::Raw
                    | AlignedSymbolKind::Decoration
            )
        }) {
            self.write_direction_words(
                &symbol.text,
                None,
                Some(sequence.voice_number.as_str()),
                Some(sequence.staff.value),
            );
        }
        for decoration in &attachments.decorations {
            if let Some(dynamic) = dynamic_decoration(decoration.name.as_str()) {
                self.write_dynamic(dynamic, sequence, part);
            } else if let Some(direction) = symbol_direction(decoration.name.as_str()) {
                self.write_direction_type(direction, sequence, part);
            } else if notation_decoration(decoration).is_none() {
                self.diagnostics
                    .push(unsupported_decoration_warning(decoration));
                self.write_direction_words(
                    &decoration.name,
                    None,
                    Some(sequence.voice_number.as_str()),
                    Some(sequence.staff.value),
                );
            }
        }
    }

    fn write_harmony(&mut self, text: &str) {
        let chord = parse_chord_symbol(text);
        self.xml.start("harmony", &[]);
        self.xml.start("root", &[]);
        self.xml
            .text_element("root-step", &chord.root_step.to_string());
        if chord.root_alter != 0 {
            self.xml
                .text_element("root-alter", &chord.root_alter.to_string());
        }
        self.xml.end("root");
        self.xml
            .text_element_attrs("kind", &[("text", text)], chord.kind);
        self.xml.end("harmony");
    }

    fn write_dynamic(
        &mut self,
        dynamic: &'static str,
        sequence: &MeasureSequence<'score>,
        part: &Part,
    ) {
        self.xml.start("direction", &[("placement", "below")]);
        self.xml.start("direction-type", &[]);
        self.xml.start("dynamics", &[]);
        self.xml.empty(dynamic, &[]);
        self.xml.end("dynamics");
        self.xml.end("direction-type");
        self.xml.text_element("voice", &sequence.voice_number);
        if part.staves.len() > 1 {
            self.xml
                .text_element("staff", &sequence.staff.value.to_string());
        }
        self.xml.end("direction");
    }

    fn write_direction_type(
        &mut self,
        direction: DirectionSymbol,
        sequence: &MeasureSequence<'score>,
        part: &Part,
    ) {
        self.xml.start("direction", &[("placement", "above")]);
        self.xml.start("direction-type", &[]);
        match direction {
            DirectionSymbol::Coda => self.xml.empty("coda", &[]),
            DirectionSymbol::Segno => self.xml.empty("segno", &[]),
        }
        self.xml.end("direction-type");
        self.xml.text_element("voice", &sequence.voice_number);
        if part.staves.len() > 1 {
            self.xml
                .text_element("staff", &sequence.staff.value.to_string());
        }
        self.xml.end("direction");
    }

    fn write_direction_words(
        &mut self,
        text: &str,
        placement: Option<AnnotationPlacementModel>,
        voice: Option<&str>,
        staff: Option<u32>,
    ) {
        let placement_attr = placement.map(placement_name);
        let attrs = placement_attr.map(|placement| [("placement", placement)]);
        let attrs_slice = attrs.as_ref().map_or(&[][..], |attrs| &attrs[..]);
        self.xml.start("direction", attrs_slice);
        self.xml.start("direction-type", &[]);
        self.xml.text_element("words", text);
        self.xml.end("direction-type");
        if let Some(voice) = voice {
            self.xml.text_element("voice", voice);
        }
        if let Some(staff) = staff
            && staff > 1
        {
            self.xml.text_element("staff", &staff.to_string());
        }
        self.xml.end("direction");
    }

    fn write_barline(
        &mut self,
        location: BarlineLocation,
        kind: BarlineKind,
        ending_children: &[EndingChild<'_>],
    ) {
        match kind {
            BarlineKind::Regular | BarlineKind::Liberal if ending_children.is_empty() => {
                return;
            }
            _ => {}
        }
        let location = location.as_str();
        self.xml.start("barline", &[("location", location)]);
        match kind {
            BarlineKind::Double => self.xml.text_element("bar-style", "light-light"),
            BarlineKind::Final => self.xml.text_element("bar-style", "light-heavy"),
            BarlineKind::Initial => self.xml.text_element("bar-style", "heavy-light"),
            BarlineKind::RepeatStart => {
                self.xml.empty("repeat", &[("direction", "forward")]);
            }
            BarlineKind::RepeatEnd => {
                self.xml.empty("repeat", &[("direction", "backward")]);
            }
            BarlineKind::RepeatBoth => {
                self.xml.empty("repeat", &[("direction", "backward")]);
            }
            BarlineKind::Dotted => self.xml.text_element("bar-style", "dotted"),
            BarlineKind::Invisible => self.xml.text_element("bar-style", "none"),
            BarlineKind::Regular | BarlineKind::Liberal => {}
        }
        for child in ending_children {
            self.xml.empty(
                "ending",
                &[
                    ("number", child.number),
                    (
                        "type",
                        match child.kind {
                            EndingType::Start => "start",
                            EndingType::Stop => "stop",
                        },
                    ),
                ],
            );
        }
        self.xml.end("barline");
    }

    fn write_ending_barline(
        &mut self,
        location: BarlineLocation,
        endings: &[String],
        ending_type: EndingType,
        repeat_kind: Option<BarlineKind>,
    ) {
        let children = endings
            .iter()
            .map(|number| EndingChild {
                number: number.as_str(),
                kind: ending_type,
            })
            .collect::<Vec<_>>();
        self.write_barline(
            location,
            repeat_kind.unwrap_or(BarlineKind::Regular),
            &children,
        );
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
struct TupletNumbers {
    pairs: Vec<(u32, u32)>,
}

impl TupletNumbers {
    fn number_for(&self, pair_id: u32) -> u32 {
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
            if numbers.pairs.iter().any(|(pair, _)| *pair == tuplet.pair_id) {
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
            if !numbers.pairs.iter().any(|(pair, _)| *pair == tuplet.pair_id) {
                numbers.pairs.push((tuplet.pair_id, 1));
            }
            active.retain(|(pair, _)| *pair != tuplet.pair_id);
        }
    }

    numbers
}

fn next_tuplet_number(active: &[(u32, u32)]) -> u32 {
    for number in 1..=16 {
        if !active.iter().any(|(_, active_number)| *active_number == number) {
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
        .flat_map(|measure| &measure.barlines)
        .copied()
        .filter(|barline| {
            matches!(
                (left, barline.kind),
                (true, BarlineKind::Initial | BarlineKind::RepeatStart)
                    | (
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
        .collect::<Vec<_>>();
    barlines.sort_by_key(|barline| (barline.span.start, barline.span.end, barline.kind as u8));
    barlines.dedup_by_key(|barline| (barline.kind, barline.span));
    barlines
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

fn meter_parts(display: &str) -> Option<(&str, &str, Option<&'static str>)> {
    match display.trim() {
        "C" => Some(("4", "4", Some("common"))),
        "C|" => Some(("2", "2", Some("cut"))),
        "none" | "M:none" => None,
        value => {
            let (beats, beat_type) = value.split_once('/')?;
            Some((beats.trim(), beat_type.trim(), None))
        }
    }
}

struct ClefModel {
    sign: &'static str,
    line: &'static str,
}

fn clef_model(clef: Option<&str>) -> ClefModel {
    let clef = clef.unwrap_or("treble").to_ascii_lowercase();
    if clef.contains("bass") {
        ClefModel {
            sign: "F",
            line: "4",
        }
    } else if clef.contains("alto") {
        ClefModel {
            sign: "C",
            line: "3",
        }
    } else if clef.contains("tenor") {
        ClefModel {
            sign: "C",
            line: "4",
        }
    } else if clef.contains("perc") {
        ClefModel {
            sign: "percussion",
            line: "2",
        }
    } else {
        ClefModel {
            sign: "G",
            line: "2",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimeModification {
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

#[derive(Debug, Clone)]
struct ParsedChordSymbol {
    root_step: char,
    root_alter: i8,
    kind: &'static str,
}

fn parse_chord_symbol(text: &str) -> ParsedChordSymbol {
    let mut chars = text.chars();
    let root_step = chars
        .find(|ch| matches!(ch.to_ascii_uppercase(), 'A'..='G'))
        .map(|ch| ch.to_ascii_uppercase())
        .unwrap_or('C');
    let root_alter = if text.contains('#') {
        1
    } else if text.contains('b') {
        -1
    } else {
        0
    };
    let lower = text.to_ascii_lowercase();
    let kind = if lower.contains("maj7") {
        "major-seventh"
    } else if lower.contains("m7") || lower.contains("min7") {
        "minor-seventh"
    } else if lower.contains('7') {
        "dominant"
    } else if lower.contains('m') || lower.contains("min") {
        "minor"
    } else {
        "other"
    };
    ParsedChordSymbol {
        root_step,
        root_alter,
        kind,
    }
}

fn annotation_text(annotation: &TextAttachment) -> &str {
    match annotation.placement {
        Some(_) => annotation
            .text
            .strip_prefix(['^', '_', '<', '>', '@'])
            .unwrap_or(&annotation.text),
        None => &annotation.text,
    }
}

fn placement_name(placement: AnnotationPlacementModel) -> &'static str {
    match placement {
        AnnotationPlacementModel::Above => "above",
        AnnotationPlacementModel::Below => "below",
        AnnotationPlacementModel::Left
        | AnnotationPlacementModel::Right
        | AnnotationPlacementModel::Free => "above",
    }
}

fn dynamic_decoration(name: &str) -> Option<&'static str> {
    match name {
        "p" => Some("p"),
        "pp" => Some("pp"),
        "ppp" => Some("ppp"),
        "f" => Some("f"),
        "ff" => Some("ff"),
        "fff" => Some("fff"),
        "mp" => Some("mp"),
        "mf" => Some("mf"),
        "sfz" => Some("sfz"),
        _ => None,
    }
}

fn notation_decoration(decoration: &DecorationAttachment) -> Option<&'static str> {
    match decoration.name.as_str() {
        "." | "staccato" => Some("staccato"),
        "trill" => Some("trill-mark"),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
enum DirectionSymbol {
    Coda,
    Segno,
}

fn symbol_direction(name: &str) -> Option<DirectionSymbol> {
    match name.to_ascii_lowercase().as_str() {
        "coda" => Some(DirectionSymbol::Coda),
        "segno" => Some(DirectionSymbol::Segno),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
enum BarlineLocation {
    Left,
    Right,
}

impl BarlineLocation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum EndingType {
    Start,
    Stop,
}

struct EndingChild<'a> {
    number: &'a str,
    kind: EndingType,
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
mod tests {
    use super::*;
    use crate::{ParseOptions, export_musicxml, parse_document};

    #[test]
    fn simple_score_emits_partwise_and_core_attributes() {
        let export = export_musicxml("X:1\nT:Scale\nM:6/8\nL:1/8\nK:G\nC2 z x|\n")
            .expect("score should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<score-partwise version=\"4.0\">"));
        assert!(export.musicxml.contains("<score-part id=\"P1\">"));
        assert!(export.musicxml.contains("<part id=\"P1\">"));
        assert!(export.musicxml.contains("<part-name>Scale</part-name>"));
        assert!(export.musicxml.contains("<divisions>8</divisions>"));
        assert!(export.musicxml.contains("<fifths>1</fifths>"));
        assert!(export.musicxml.contains("<beats>6</beats>"));
        assert!(export.musicxml.contains("<beat-type>8</beat-type>"));
        assert!(export.musicxml.contains("<sign>G</sign>"));
        assert!(export.musicxml.contains("<duration>8</duration>"));
        assert!(export.musicxml.contains("<type>quarter</type>"));
        assert!(export.musicxml.contains("<type>eighth</type>"));
        assert!(export.musicxml.contains("<note print-object=\"no\">"));
    }

    #[test]
    fn text_output_is_escaped_for_metadata_lyrics_harmony_and_directions() {
        let source = concat!(
            "X:1\n",
            "T:A&B <T> \"Q\" 'R'\n",
            "C:Comp & < > \" '\n",
            "Q:Fast & < > \" '\n",
            "L:1/8\n",
            "K:C\n",
            "\"G7&<>'\"\"^Ann & < > '\"C D|\n",
            "w: lyr&<>' two\n",
            "%%foo dir & < > \" '\n",
        );
        let export = export_musicxml(source).expect("escaped text score should export");

        assert_balanced_xml(&export.musicxml);
        assert!(
            export
                .musicxml
                .contains("A&amp;B &lt;T&gt; &quot;Q&quot; &apos;R&apos;")
        );
        assert!(
            export
                .musicxml
                .contains("Comp &amp; &lt; &gt; &quot; &apos;")
        );
        assert!(
            export
                .musicxml
                .contains("Fast &amp; &lt; &gt; &quot; &apos;")
        );
        assert!(export.musicxml.contains("text=\"G7&amp;&lt;&gt;&apos;\""));
        assert!(export.musicxml.contains("Ann &amp; &lt; &gt; &apos;"));
        assert!(export.musicxml.contains("lyr&amp;&lt;&gt;&apos;"));
        assert!(
            export
                .musicxml
                .contains("%%foo dir &amp; &lt; &gt; &quot; &apos;")
        );
    }

    #[test]
    fn chords_grace_tuplets_ties_slurs_and_lyrics_export() {
        let source = "X:1\nT:Features\nM:4/4\nL:1/8\nK:C\n{g}[CEG] (3D-D F (G A)|\nw: chord trip let slur end\n";
        let export = export_musicxml(source).expect("feature score should export");

        assert_balanced_xml(&export.musicxml);
        assert_eq!(count(&export.musicxml, "<chord/>"), 2);
        assert!(export.musicxml.contains("<grace/>"));
        assert!(export.musicxml.contains("<time-modification>"));
        assert!(export.musicxml.contains("<actual-notes>3</actual-notes>"));
        assert!(export.musicxml.contains("<normal-notes>2</normal-notes>"));
        assert!(export.musicxml.contains("<tuplet type=\"start\""));
        assert!(export.musicxml.contains("<tuplet type=\"stop\""));
        assert!(export.musicxml.contains("<tie type=\"start\"/>"));
        assert!(export.musicxml.contains("<tied type=\"start\""));
        assert!(export.musicxml.contains("<slur type=\"start\""));
        assert!(export.musicxml.contains("<slur type=\"stop\""));
        assert!(export.musicxml.contains("<text>chord</text>"));
    }

    #[test]
    fn sequential_tuplets_reuse_musicxml_number_levels() {
        let source = concat!(
            "X:1\n",
            "T:Many Tuplets\n",
            "M:4/4\n",
            "L:1/16\n",
            "K:C\n",
            "(3CDE (3DEF (3EFG (3FGA (3GAB (3ABc (3Bcd|\n",
        );
        let export = export_musicxml(source).expect("many sequential tuplets should export");

        assert_balanced_xml(&export.musicxml);
        assert_eq!(
            count(&export.musicxml, "<tuplet type=\"start\" number=\"1\"/>"),
            7
        );
        assert_eq!(
            count(&export.musicxml, "<tuplet type=\"stop\" number=\"1\"/>"),
            7
        );
        assert!(!export.musicxml.contains("number=\"7\""));
    }

    #[test]
    fn reduced_duration_note_types_do_not_emit_spurious_tuplets() {
        let source = "X:1\nT:Long notes\nM:4/4\nL:1/4\nK:C\nC2 D4|\n";
        let export = export_musicxml(source).expect("long note types should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<type>half</type>"));
        assert!(export.musicxml.contains("<type>whole</type>"));
        assert!(!export.musicxml.contains("<time-modification>"));
    }

    #[test]
    fn repeats_endings_multiple_voices_and_overlays_use_timeline_elements() {
        let source = concat!(
            "X:1\n",
            "M:2/4\n",
            "L:1/8\n",
            "K:C\n",
            "V:1\n",
            "|: C D & E F :| [1 G A | [2 B c |]\n",
            "V:2\n",
            "C2 D2|E2 F2|\n",
        );
        let export = export_musicxml(source).expect("timeline score should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<repeat direction=\"forward\"/>"));
        assert!(export.musicxml.contains("<repeat direction=\"backward\"/>"));
        assert!(
            export
                .musicxml
                .contains("<ending number=\"1\" type=\"start\"/>")
        );
        assert!(
            export
                .musicxml
                .contains("<ending number=\"2\" type=\"start\"/>")
        );
        assert!(export.musicxml.contains("<backup>"));
        assert!(export.musicxml.contains("<voice>2</voice>"));
        assert!(export.musicxml.contains("<voice>3</voice>"));
    }

    #[test]
    fn semantic_onset_gaps_emit_forward() {
        let source = "X:1\nL:1/8\nK:C\nC D|\n";
        let document = parse_document(source, ParseOptions::default());
        let tune = crate::parser::parse_tune_report_from_document(&document.value)
            .value
            .expect("expected tune");
        let mut score = tune.score;
        score.parts[0].voices[0].events[1].onset = Fraction::new(2, 8);
        let report = write_score_partwise(&score);

        assert_balanced_xml(&report.value);
        assert!(report.value.contains("<forward>"));
        assert!(report.value.contains("<duration>4</duration>"));
    }

    #[test]
    fn unsupported_decoration_diagnoses_without_dropping_note_or_timing() {
        let source = "X:1\nL:1/8\nK:C\n!unknown!C D|\n";
        let export = export_musicxml(source).expect("unsupported decoration should recover");

        assert_balanced_xml(&export.musicxml);
        assert_diagnostic_span(
            source,
            &export.diagnostics,
            "abc.musicxml.decoration.unsupported",
            "!unknown!",
        );
        assert_eq!(count(&export.musicxml, "<note>"), 2);
        assert_eq!(count(&export.musicxml, "<duration>4</duration>"), 2);
    }

    #[test]
    fn variable_duration_chord_diagnoses_and_keeps_following_timing_valid() {
        let source = "X:1\nL:1/8\nK:C\n[E2G6] C|\n";
        let export = export_musicxml(source).expect("variable chord should recover");

        assert_balanced_xml(&export.musicxml);
        assert_diagnostic_span(
            source,
            &export.diagnostics,
            "abc.music.chord.variable_duration",
            "[E2G6]",
        );
        assert_diagnostic_span(
            source,
            &export.diagnostics,
            "abc.musicxml.chord.variable_duration",
            "E2G6",
        );
        assert_eq!(count(&export.musicxml, "<chord/>"), 1);
        assert_eq!(count(&export.musicxml, "<note>"), 3);
        assert!(!export.musicxml.contains("<backup>"));
    }

    #[test]
    fn incomplete_overlay_diagnoses_and_later_measures_stay_stable() {
        let source = "X:1\nL:1/8\nK:C\nC D & E|F G|\n";
        let export = export_musicxml(source).expect("incomplete overlay should recover");

        assert_balanced_xml(&export.musicxml);
        assert_diagnostic_span(
            source,
            &export.diagnostics,
            "abc.voice.overlay_incomplete_measure",
            "&",
        );
        assert!(export.musicxml.contains("<measure number=\"2\">"));
        assert!(export.musicxml.contains("<step>F</step>"));
        assert!(export.musicxml.contains("<step>G</step>"));
    }

    #[test]
    fn bad_tuplet_count_diagnoses_without_bogus_tuplet_notation_pairs() {
        let source = "X:1\nL:1/8\nK:C\n(3C|D E|\n";
        let export = export_musicxml(source).expect("short tuplet should recover");

        assert_balanced_xml(&export.musicxml);
        assert_diagnostic_span(
            source,
            &export.diagnostics,
            "abc.music.tuplet.too_few_notes",
            "(3",
        );
        assert!(export.musicxml.contains("<time-modification>"));
        assert!(!export.musicxml.contains("<tuplet type=\"start\""));
        assert!(!export.musicxml.contains("<tuplet type=\"stop\""));
        assert!(export.musicxml.contains("<measure number=\"2\">"));
    }

    #[test]
    fn unmatched_tie_and_slur_do_not_create_musicxml_pairs() {
        let source = "X:1\nL:1/8\nK:C\nC- D )E|\n";
        let export = export_musicxml(source).expect("unmatched tie and slur should recover");

        assert_balanced_xml(&export.musicxml);
        assert_diagnostic_span(source, &export.diagnostics, "abc.music.unmatched_tie", "-");
        assert_diagnostic_span(source, &export.diagnostics, "abc.music.unmatched_slur", ")");
        assert!(!export.musicxml.contains("<tie "));
        assert!(!export.musicxml.contains("<tied "));
        assert!(!export.musicxml.contains("<slur "));
        assert_eq!(count(&export.musicxml, "<note>"), 3);
    }

    #[test]
    fn malformed_repeat_ending_keeps_measure_structure_valid() {
        let source = "X:1\nL:1/8\nK:C\nC|[1- D|E|\n";
        let export = export_musicxml(source).expect("malformed repeat ending should recover");

        assert_balanced_xml(&export.musicxml);
        assert_diagnostic_span(
            source,
            &export.diagnostics,
            "abc.music.invalid_repeat_ending",
            "1-",
        );
        assert!(!export.musicxml.contains("<ending number=\"1-\""));
        assert!(export.musicxml.contains("<measure number=\"1\">"));
        assert!(export.musicxml.contains("<measure number=\"2\">"));
        assert!(export.musicxml.contains("<measure number=\"3\">"));
        assert_eq!(count(&export.musicxml, "<note>"), 3);
    }

    #[test]
    fn non_integral_duration_reports_precise_writer_diagnostic() {
        let source = "X:1\nL:1/8\nK:C\nC D|\n";
        let document = parse_document(source, ParseOptions::default());
        let tune = crate::parser::parse_tune_report_from_document(&document.value)
            .value
            .expect("expected tune");
        let mut score = tune.score;
        score.divisions = 1;
        let report = write_score_partwise(&score);

        assert_balanced_xml(&report.value);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.musicxml.duration.non_integral")
        );
        assert!(report.value.contains("<duration>1</duration>"));
    }

    #[test]
    fn unsupported_note_type_duration_reports_precise_writer_diagnostic() {
        let source = "X:1\nL:1/8\nK:C\nC|\n";
        let document = parse_document(source, ParseOptions::default());
        let tune = crate::parser::parse_tune_report_from_document(&document.value)
            .value
            .expect("expected tune");
        let mut score = tune.score;
        score.divisions = 13;
        score.parts[0].voices[0].events[0].duration = Fraction::new(7, 13);
        let report = write_score_partwise(&score);

        assert_balanced_xml(&report.value);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.musicxml.duration.unsupported_note_type")
        );
        assert!(report.value.contains("<duration>28</duration>"));
    }

    #[test]
    fn unknown_directive_is_direction_metadata_not_music() {
        let source = "X:1\nK:C\n%%foo bar & < >\nC|\n";
        let export = export_musicxml(source).expect("unknown directive should recover");

        assert_balanced_xml(&export.musicxml);
        assert_diagnostic_span(
            source,
            &export.diagnostics,
            "abc.directive.unsupported",
            "foo",
        );
        assert!(export.musicxml.contains("%%foo bar &amp; &lt; &gt;"));
        assert_eq!(count(&export.musicxml, "<note>"), 1);
    }

    fn count(haystack: &str, needle: &str) -> usize {
        haystack.matches(needle).count()
    }

    fn assert_diagnostic_span(
        source: &str,
        diagnostics: &[Diagnostic],
        code: &'static str,
        snippet: &str,
    ) {
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == code)
            .unwrap_or_else(|| panic!("expected diagnostic {code}"));
        assert_eq!(&source[diagnostic.span.start..diagnostic.span.end], snippet);
    }

    fn assert_balanced_xml(xml: &str) {
        let mut stack: Vec<String> = Vec::new();
        let mut index = 0;
        while let Some(offset) = xml[index..].find('<') {
            let start = index + offset;
            let end = xml[start..]
                .find('>')
                .map(|end| start + end)
                .unwrap_or_else(|| panic!("unterminated XML tag at byte {start}"));
            let tag = &xml[start + 1..end];
            if tag.starts_with('?') || tag.starts_with('!') {
                index = end + 1;
                continue;
            }
            if let Some(name) = tag.strip_prefix('/') {
                let expected = stack.pop().expect("unexpected closing XML tag");
                assert_eq!(name.trim(), expected);
            } else if !tag.trim_end().ends_with('/') {
                let name = tag
                    .split_whitespace()
                    .next()
                    .expect("XML tag should have a name")
                    .trim_end_matches('/');
                stack.push(name.to_owned());
            }
            index = end + 1;
        }
        assert!(stack.is_empty(), "unclosed XML tags: {stack:?}");
    }
}
