use std::cmp::Ordering;

use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
use crate::model::{
    AccidentalMark, AlignedLyric, AlignedSymbolKind, AnnotationPlacementModel, BarlineKind,
    ChordEvent, DecorationAttachment, EventAttachments, Fraction, GraceEventKind,
    GraceGroupAttachment, GraceNoteEvent, KeySignatureModel, Measure, MeasureBarline, MeasureId,
    Part, Pitch, PreservedDirective, RestEvent, RestVisibility, Score, SlurRole, StaffId,
    TextAttachment, TieRole, TimedEvent, TimedEventKind, TimelineEventKind, TupletAttachment,
    TupletRole, VoiceTimedEvent,
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
            if clef.octave_change != 0 {
                self.xml
                    .text_element("clef-octave-change", &clef.octave_change.to_string());
            }
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
        let display_duration = grace_display_duration(group.note_count);
        for event in &group.events {
            match &event.kind {
                GraceEventKind::Note(note) => {
                    self.write_grace_note(
                        GraceNoteWrite {
                            note,
                            source: event.source_span,
                            chord_member: false,
                            slash: group.slash.is_some(),
                            display_duration,
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
                            duration: display_duration,
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
                                display_duration,
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
                duration: grace_note.display_duration,
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
            if matches!(
                lyric.control,
                crate::model::LyricControl::Skip | crate::model::LyricControl::Hyphen
            ) {
                continue;
            }
            let number = lyric.verse.to_string();
            self.xml.start("lyric", &[("number", number.as_str())]);
            match lyric.control {
                crate::model::LyricControl::Syllable => {
                    self.xml.text_element("syllabic", "single");
                    self.xml.text_element("text", &lyric.text);
                }
                crate::model::LyricControl::Hyphen => {}
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
            self.write_chord_symbol(&symbol.text, sequence);
        }
        for symbol in attachments
            .symbols
            .iter()
            .filter(|symbol| symbol.kind == AlignedSymbolKind::ChordSymbol)
        {
            self.write_chord_symbol(&symbol.text, sequence);
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

    fn write_chord_symbol(&mut self, text: &str, sequence: &MeasureSequence<'score>) {
        if self.write_harmony(text) {
            return;
        }
        let words = text.trim();
        if !words.is_empty() {
            self.write_direction_words(
                words,
                None,
                Some(sequence.voice_number.as_str()),
                Some(sequence.staff.value),
            );
        }
    }

    fn write_harmony(&mut self, text: &str) -> bool {
        let Some(chord) = parse_chord_symbol(text) else {
            return false;
        };
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
        if let Some(bass_step) = chord.bass_step {
            self.xml.start("bass", &[]);
            self.xml.text_element("bass-step", &bass_step.to_string());
            if chord.bass_alter != 0 {
                self.xml
                    .text_element("bass-alter", &chord.bass_alter.to_string());
            }
            self.xml.end("bass");
        }
        self.xml.end("harmony");
        true
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
            let first_measure_leading = is_first_measure_leading_barline(measure, barline);
            matches!(
                (left, first_measure_leading, barline.kind),
                (true, _, BarlineKind::RepeatStart)
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

fn is_first_measure_leading_barline(measure: &Measure, barline: &MeasureBarline) -> bool {
    measure.id.index == 0 && measure.source_span.start == barline.span.start
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
    octave_change: i8,
}

fn clef_model(clef: Option<&str>) -> ClefModel {
    let clef = clef.unwrap_or("treble").to_ascii_lowercase();
    let octave_change = if clef.contains("-15") {
        -2
    } else if clef.contains("+15") {
        2
    } else if clef.contains("-8") {
        -1
    } else if clef.contains("+8") {
        1
    } else {
        0
    };
    let (sign, line) = if clef.contains("bass") {
        ("F", "4")
    } else if clef.contains("alto") {
        ("C", "3")
    } else if clef.contains("tenor") {
        ("C", "4")
    } else if clef.contains("perc") {
        ("percussion", "2")
    } else {
        ("G", "2")
    };
    ClefModel {
        sign,
        line,
        octave_change,
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

fn grace_display_duration(note_count: u32) -> Fraction {
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

fn grace_export_pitch(
    pitch: &Pitch,
    written_accidental: Option<&AccidentalMark>,
    key: Option<&KeySignatureModel>,
) -> Pitch {
    if written_accidental.is_some() {
        return *pitch;
    }
    let Some(key) = key else {
        return *pitch;
    };
    let alter = key_signature_alter(key, pitch.step);
    if alter == pitch.alter {
        return *pitch;
    }
    Pitch { alter, ..*pitch }
}

fn key_signature_alter(key: &KeySignatureModel, step: char) -> i8 {
    let step = step.to_ascii_uppercase();
    if let Some(accidental) = key
        .explicit_accidentals
        .iter()
        .find(|accidental| accidental.step == step)
    {
        return accidental.accidental.alter();
    }

    if key.fifths > 0 {
        sharp_key_steps(key.fifths).contains(&step).then_some(1)
    } else if key.fifths < 0 {
        flat_key_steps(key.fifths).contains(&step).then_some(-1)
    } else {
        None
    }
    .unwrap_or(0)
}

const SHARP_KEY_STEPS: [char; 7] = ['F', 'C', 'G', 'D', 'A', 'E', 'B'];
const FLAT_KEY_STEPS: [char; 7] = ['B', 'E', 'A', 'D', 'G', 'C', 'F'];

fn sharp_key_steps(fifths: i8) -> &'static [char] {
    let count = fifths.clamp(0, 7) as usize;
    &SHARP_KEY_STEPS[..count]
}

fn flat_key_steps(fifths: i8) -> &'static [char] {
    let count = fifths.saturating_abs().clamp(0, 7) as usize;
    &FLAT_KEY_STEPS[..count]
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
    bass_step: Option<char>,
    bass_alter: i8,
    kind: &'static str,
}

fn parse_chord_symbol(text: &str) -> Option<ParsedChordSymbol> {
    let (root_text, bass_text) = text.split_once('/').unwrap_or((text, ""));
    let root = parse_chord_tone(root_text)?;
    let bass = if text.contains('/') {
        Some(parse_chord_tone(bass_text)?)
    } else {
        None
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
    Some(ParsedChordSymbol {
        root_step: root.step,
        root_alter: root.alter,
        bass_step: bass.map(|tone| tone.step),
        bass_alter: bass.map(|tone| tone.alter).unwrap_or(0),
        kind,
    })
}

#[derive(Debug, Clone, Copy)]
struct ChordTone {
    step: char,
    alter: i8,
}

fn parse_chord_tone(text: &str) -> Option<ChordTone> {
    let mut chars = text.trim_start().chars().peekable();
    let step = chars.next()?.to_ascii_uppercase();
    if !matches!(step, 'A'..='G') {
        return None;
    }
    let alter = match chars.peek().copied() {
        Some('#') => 1,
        Some('b' | '-') => -1,
        _ => 0,
    };
    Some(ChordTone { step, alter })
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
    fn slash_chord_symbols_export_bass_step_and_alter() {
        let source = "X:1\nT:Slash Chords\nM:4/4\nL:1/4\nK:C\n\"C/E\"C \"D-/A-\"D|\n";
        let export = export_musicxml(source).expect("slash chords should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<root-step>C</root-step>"));
        assert!(export.musicxml.contains("<bass-step>E</bass-step>"));
        assert!(export.musicxml.contains("<root-step>D</root-step>"));
        assert!(export.musicxml.contains("<root-alter>-1</root-alter>"));
        assert!(export.musicxml.contains("<bass-step>A</bass-step>"));
        assert!(export.musicxml.contains("<bass-alter>-1</bass-alter>"));
    }

    #[test]
    fn malformed_quoted_chord_strings_export_as_words_not_fake_harmony() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"(A7)\"C \"C/\"D|\n";
        let export = export_musicxml(source).expect("malformed chord text should export");

        assert_balanced_xml(&export.musicxml);
        assert_eq!(count(&export.musicxml, "<harmony>"), 0);
        assert_eq!(count(&export.musicxml, "<root-step>"), 0);
        assert!(export.musicxml.contains("<words>(A7)</words>"));
        assert!(export.musicxml.contains("<words>C/</words>"));
    }

    #[test]
    fn leading_whitespace_valid_chord_symbols_still_export_harmony() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"  G7\"C \" C/E\"D|\n";
        let export = export_musicxml(source).expect("valid spaced chords should export");

        assert_balanced_xml(&export.musicxml);
        assert_eq!(count(&export.musicxml, "<harmony>"), 2);
        assert!(export.musicxml.contains("<root-step>G</root-step>"));
        assert!(export.musicxml.contains("<bass-step>E</bass-step>"));
        assert!(!export.musicxml.contains("<words>G7</words>"));
        assert!(!export.musicxml.contains("<words>C/E</words>"));
    }

    #[test]
    fn placement_prefixed_annotations_remain_words() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"^slow\"C \"_soft\"D|\n";
        let export = export_musicxml(source).expect("annotations should export");

        assert_balanced_xml(&export.musicxml);
        assert_eq!(count(&export.musicxml, "<harmony>"), 0);
        assert!(export.musicxml.contains("<direction placement=\"above\">"));
        assert!(export.musicxml.contains("<direction placement=\"below\">"));
        assert!(export.musicxml.contains("<words>slow</words>"));
        assert!(export.musicxml.contains("<words>soft</words>"));
    }

    #[test]
    fn initial_barlines_do_not_emit_musicxml_heavy_light() {
        let source = "X:1\nT:Initial Barline\nM:4/4\nL:1/4\nK:C\nC |[| D |]\n";
        let export = export_musicxml(source).expect("initial barline should export");

        assert_balanced_xml(&export.musicxml);
        assert!(
            !export
                .musicxml
                .contains("<bar-style>heavy-light</bar-style>")
        );
        assert!(
            export
                .musicxml
                .contains("<bar-style>light-heavy</bar-style>")
        );
    }

    #[test]
    fn leading_plain_barline_exports_notes_in_measure_one() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n| C D E F |]\n";
        let export = export_musicxml(source).expect("leading barline should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1"]);
        assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
        assert_eq!(note_durations(&measures[0]), vec![8, 8, 8, 8]);
        assert!(!measures[0].notes.iter().any(|note| note.rest));
    }

    #[test]
    fn leading_repeat_start_exports_left_repeat_without_empty_measure() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n|: C D E F :|\n";
        let export = export_musicxml(source).expect("leading repeat should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1"]);
        assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
        assert_eq!(note_durations(&measures[0]), vec![8, 8, 8, 8]);
        assert!(has_barline(&measures[0], "left", None, Some("forward")));
        assert!(has_barline(&measures[0], "right", None, Some("backward")));
    }

    #[test]
    fn leading_double_and_final_barlines_do_not_create_empty_measure() {
        for prefix in ["||", "|]"] {
            let source = format!("X:1\nM:4/4\nL:1/4\nK:C\n{prefix} C D E F |]\n");
            let export = export_musicxml(&source).expect("leading section barline should export");

            assert_balanced_xml(&export.musicxml);
            let measures = musicxml_measures(&export.musicxml);
            assert_eq!(measure_numbers(&measures), vec!["1"], "{prefix}");
            assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
            assert_eq!(note_durations(&measures[0]), vec![8, 8, 8, 8]);
            assert!(
                !measures[0]
                    .barlines
                    .iter()
                    .any(|barline| barline.location == "left")
            );
            assert!(has_barline(
                &measures[0],
                "right",
                Some("light-heavy"),
                None
            ));
        }
    }

    #[test]
    fn leading_liberal_barline_diagnoses_and_keeps_measure_timing() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n[::] C D E F |]\n";
        let export = export_musicxml(source).expect("liberal leading barline should recover");

        assert_balanced_xml(&export.musicxml);
        assert_diagnostic_span(
            source,
            &export.diagnostics,
            "abc.music.barline.liberal",
            "[::]",
        );
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1"]);
        assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
        assert_eq!(note_durations(&measures[0]), vec![8, 8, 8, 8]);
        assert!(!measures[0].notes.iter().any(|note| note.rest));
    }

    #[test]
    fn leading_double_repeat_start_exports_left_repeat_without_empty_measure() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n||: C D E F :||\n";
        let export = export_musicxml(source).expect("combined leading repeat should export");

        assert_balanced_xml(&export.musicxml);
        assert!(
            !export
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.barline.liberal")
        );
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1"]);
        assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
        assert!(has_barline(&measures[0], "left", None, Some("forward")));
        assert!(has_barline(&measures[0], "right", None, Some("backward")));
    }

    #[test]
    fn repeat_end_double_after_notes_exports_right_repeat_and_new_measure() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\nC D E F :|| G A B c |]\n";
        let export = export_musicxml(source).expect("combined repeat end should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
        assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
        assert_eq!(note_steps(&measures[1]), vec!['G', 'A', 'B', 'C']);
        assert!(has_barline(&measures[0], "right", None, Some("backward")));
    }

    #[test]
    fn repeat_both_between_sections_exports_right_then_left_repeat() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\nC D E F :||: G A B c |]\n";
        let export = export_musicxml(source).expect("repeat-both barline should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
        assert!(has_barline(&measures[0], "right", None, Some("backward")));
        assert!(has_barline(&measures[1], "left", None, Some("forward")));
        assert_eq!(note_steps(&measures[1]), vec!['G', 'A', 'B', 'C']);
    }

    #[test]
    fn triple_repeat_extensions_export_repeat_edges() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n|:: C D E F ::|\n";
        let export = export_musicxml(source).expect("triple repeat barlines should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1"]);
        assert!(has_barline(&measures[0], "left", None, Some("forward")));
        assert!(has_barline(&measures[0], "right", None, Some("backward")));
        assert!(
            measures[0]
                .barlines
                .iter()
                .all(|barline| barline.repeat_times.is_none())
        );
        assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    }

    #[test]
    fn excessive_repeat_dots_are_liberal_policy_not_repeat_count() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n|::: C D E F :::|\n";
        let export = export_musicxml(source).expect("liberal repeat dots should recover");

        assert_balanced_xml(&export.musicxml);
        assert_eq!(
            export
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "abc.music.barline.liberal")
                .count(),
            2
        );
        assert_diagnostic_span(
            source,
            &export.diagnostics,
            "abc.music.barline.liberal",
            "|:::",
        );
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1"]);
        assert!(measures[0].barlines.is_empty());
        assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
    }

    #[test]
    fn words_containing_double_colon_pipe_do_not_emit_repeat_barlines() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\nC D E F |]\nW::| Cross over two couples\n";
        let export = export_musicxml(source).expect("words field should not affect barlines");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1"]);
        assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
        assert!(has_barline(
            &measures[0],
            "right",
            Some("light-heavy"),
            None
        ));
        assert!(
            measures[0]
                .barlines
                .iter()
                .all(|barline| barline.repeat_direction.is_none())
        );
    }

    #[test]
    fn tune_014868_style_leading_double_repeat_has_no_empty_measure() {
        let source = concat!(
            "X:260\n",
            "T:Bag o' Spuds -- Am\n",
            "M:4/4\n",
            "R:Reel\n",
            "K:Am\n",
            "||:\"Am\"A2eA BAeA|ABcd ecdB|\"G\"G2BG DGBG|GABc \"Em\"dBcB|\n",
            "\"Am\"A2eA BAeA|ABcd \"G\"ecdB|\"F\"ABcd efge|\"Em\"dBGB BAA2:|\n",
            "|:\"Am\"a2ea ageg|agbg agef|\"G\"gedc BGBd|g2ga bgeg|\n",
            "\"Am\"a2ea ageg|agbg ageg|\"G\"d3e g3e|\"Em\"dBGB BAA2:|\n",
        );
        let export = export_musicxml(source).expect("leading combined repeat should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(
            measure_numbers(&measures),
            (1..=16)
                .map(|number| number.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            note_steps(&measures[0]),
            vec!['A', 'E', 'A', 'B', 'A', 'E', 'A']
        );
        assert_eq!(note_durations(&measures[0]), vec![8, 4, 4, 4, 4, 4, 4]);
        assert!(has_barline(&measures[0], "left", None, Some("forward")));
        assert!(has_barline(&measures[7], "right", None, Some("backward")));
        assert!(has_barline(&measures[8], "left", None, Some("forward")));
        assert!(has_barline(&measures[15], "right", None, Some("backward")));
        assert!(!measures[0].notes.iter().any(|note| note.rest));
    }

    #[test]
    fn bracketed_repeat_start_and_final_repeat_end_export_repeat_edges() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n[|: C D E F :|]\n";
        let export = export_musicxml(source).expect("bracketed repeat barlines should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1"]);
        assert!(has_barline(&measures[0], "left", None, Some("forward")));
        assert!(has_barline(&measures[0], "right", None, Some("backward")));
    }

    #[test]
    fn adjacent_repeat_end_and_second_ending_starts_next_measure() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n|: C D E F |1 G A B c :|2 D E F G |]\n";
        let export = export_musicxml(source).expect("adjacent repeat ending should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1", "2", "3"]);
        assert!(has_ending(&measures[1], "left", "1", "start"));
        assert!(has_ending(&measures[1], "right", "1", "stop"));
        assert!(has_ending(&measures[2], "left", "2", "start"));
        assert!(has_ending(&measures[2], "right", "2", "stop"));
        assert!(has_barline(&measures[1], "right", None, Some("backward")));
    }

    #[test]
    fn internal_rest_measure_is_preserved_after_leading_barline_policy() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\nC D E F | z4 | G A B c |]\n";
        let export = export_musicxml(source).expect("internal rest measure should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1", "2", "3"]);
        assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
        assert_eq!(measures[1].notes.len(), 1);
        assert!(measures[1].notes[0].rest);
        assert_eq!(measures[1].notes[0].duration, Some(32));
        assert_eq!(note_steps(&measures[2]), vec!['G', 'A', 'B', 'C']);
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
    fn lyric_hyphen_controls_do_not_export_as_sung_text() {
        let source = "X:1\nT:Hyphen Lyrics\nM:4/4\nL:1/4\nK:C\nC D E F|\nw: A-des-te fi-del\n";
        let export = export_musicxml(source).expect("hyphen lyric score should export");

        assert_balanced_xml(&export.musicxml);
        assert_eq!(count(&export.musicxml, "<lyric number=\"1\">"), 4);
        assert!(export.musicxml.contains("<text>A</text>"));
        assert!(export.musicxml.contains("<text>des</text>"));
        assert!(export.musicxml.contains("<text>te</text>"));
        assert!(export.musicxml.contains("<text>fi</text>"));
        assert!(!export.musicxml.contains("<text>del</text>"));
        assert!(!export.musicxml.contains("<text>-</text>"));
        assert_eq!(
            export
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "abc.lyric.syllable_count")
                .count(),
            1
        );
    }

    #[test]
    fn escaped_literal_hyphen_in_lyrics_still_exports_as_text() {
        let source = "X:1\nT:Literal Hyphen Lyrics\nM:2/4\nL:1/4\nK:C\nC D|\nw: \\-dash end\n";
        let export = export_musicxml(source).expect("literal hyphen lyric score should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<text>-dash</text>"));
        assert!(!export.musicxml.contains("<text>-</text>"));
        assert!(export.diagnostics.is_empty());
    }

    #[test]
    fn lyric_underscore_exports_melisma_extender_without_sung_text() {
        let source = "X:1\nT:Melisma Lyrics\nM:3/4\nL:1/4\nK:C\nC D E|\nw: time_ day\n";
        let export = export_musicxml(source).expect("melisma lyric score should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<text>time</text>"));
        assert!(export.musicxml.contains("<extend/>"));
        assert!(export.musicxml.contains("<text>day</text>"));
        assert!(!export.musicxml.contains("<text>_</text>"));
        assert!(export.diagnostics.is_empty());
    }

    #[test]
    fn clef_octave_suffix_shifts_notes_and_marks_the_clef() {
        // `clef=treble-8` writes the notes one octave lower and adds a matching
        // clef-octave-change, like abc2xml. `C` (octave 4) becomes octave 3.
        let source = "X:1\nL:1/4\nK:C\nV:1 clef=treble-8\nC2 C2|\n";
        let export = export_musicxml(source).expect("treble-8 score should export");
        assert_balanced_xml(&export.musicxml);
        assert!(
            export
                .musicxml
                .contains("<clef-octave-change>-1</clef-octave-change>")
        );
        assert!(export.musicxml.contains("<octave>3</octave>"));
        assert!(!export.musicxml.contains("<octave>4</octave>"));
    }

    #[test]
    fn bare_voice_switch_keeps_each_header_clef() {
        // Header voice definitions carry clefs; a later bare `V:n` switch in the
        // body must not wipe them. Each voice keeps its own clef and octave.
        let source = concat!(
            "X:1\nL:1/4\n",
            "V:1 clef=treble\nV:2 clef=bass\nK:C\n",
            "V:1\nc c|\n",
            "V:2\nC, C,|\n",
        );
        let export = export_musicxml(source).expect("multi-voice score should export");
        assert_balanced_xml(&export.musicxml);
        let p2 = export
            .musicxml
            .split("<part id=\"P2\">")
            .nth(1)
            .expect("part P2");
        assert!(
            p2.contains("<sign>F</sign>"),
            "V:2 should keep its bass clef"
        );
    }

    #[test]
    fn each_voice_becomes_its_own_part() {
        // A multi-voice tune exports as one score with one <part> per voice, in
        // voice order, all in a single document (matching abc2xml/music21).
        let source = "X:1\nL:1/4\nK:C\nV:1\nC D|E F|\nV:2\nG A|B c|\nV:3\nc B|A G|\n";
        let export = export_musicxml(source).expect("multi-voice score should export");
        assert_balanced_xml(&export.musicxml);
        assert_eq!(count(&export.musicxml, "<score-partwise"), 1);
        assert_eq!(count(&export.musicxml, "<part id="), 3);
        for id in ["P1", "P2", "P3"] {
            assert!(
                export.musicxml.contains(&format!("<part id=\"{id}\"")),
                "missing part {id}"
            );
        }
    }

    #[test]
    fn single_voice_tune_stays_one_part() {
        let source = "X:1\nL:1/4\nK:C\nC D E F|\n";
        let export = export_musicxml(source).expect("single-voice score should export");
        assert_eq!(count(&export.musicxml, "<part id="), 1);
    }

    #[test]
    fn inline_key_change_applies_to_following_accidentals() {
        // `[K:D]` mid-tune must make the following notes use the D-major key
        // signature: the C in the second measure becomes C-sharp.
        let source = "X:1\nL:1/8\nK:C\nCEG c|[K:D]CEG c|\n";
        let export = export_musicxml(source).expect("inline key change should export");
        assert_balanced_xml(&export.musicxml);
        let second = export
            .musicxml
            .split("<measure number=\"2\">")
            .nth(1)
            .expect("second measure");
        assert!(
            second.contains("<step>C</step>\n          <alter>1</alter>"),
            "second measure C should be sharp under inline K:D"
        );
    }

    #[test]
    fn inline_clef_only_key_field_does_not_reset_the_signature() {
        // `[K:clef=bass]` only changes the clef; it must not be misread as a key
        // change that wipes the D-major signature (the following F stays F#).
        let source = "X:1\nL:1/8\nK:D\nFAd f|[K:clef=bass]FAd f|\n";
        let export = export_musicxml(source).expect("inline clef change should export");
        assert_balanced_xml(&export.musicxml);
        let second = export
            .musicxml
            .split("<measure number=\"2\">")
            .nth(1)
            .expect("second measure");
        assert!(
            second.contains("<step>F</step>\n          <alter>1</alter>"),
            "F should stay sharp; clef-only inline key must not reset the signature"
        );
    }

    #[test]
    fn escaped_literal_underscore_in_lyrics_still_exports_as_text() {
        let source = "X:1\nT:Literal Underscore Lyrics\nM:2/4\nL:1/4\nK:C\nC D|\nw: \\_hold end\n";
        let export = export_musicxml(source).expect("literal underscore lyric score should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<text>_hold</text>"));
        assert!(!export.musicxml.contains("<text>_</text>"));
        assert!(export.diagnostics.is_empty());
    }

    #[test]
    fn lyric_nbsp_inside_tune_000509_style_word_is_not_a_separator() {
        let source = "X:1\nT:NBSP Melisma Lyrics\nM:6/4\nL:1/4\nK:C\nC D E F G A|\nw: A-ten-toÃ\u{00a0}a-do_ra\n";
        let export = export_musicxml(source).expect("NBSP lyric score should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<text>toÃ\u{00a0}a</text>"));
        assert!(export.musicxml.contains("<text>do</text>"));
        assert!(export.musicxml.contains("<extend/>"));
        assert!(!export.musicxml.contains("<text>toÃ</text>"));
        assert!(export.diagnostics.is_empty());
    }

    #[test]
    fn ties_across_barlines_export_start_and_stop_without_diagnostic() {
        let source = "X:1\nM:2/4\nL:1/4\nK:C\nC- | C D |\n";
        let export = export_musicxml(source).expect("cross-bar tie should export");

        assert_balanced_xml(&export.musicxml);
        assert!(
            !export
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.unmatched_tie")
        );
        assert_eq!(count(&export.musicxml, "<tie type=\"start\"/>"), 1);
        assert_eq!(count(&export.musicxml, "<tie type=\"stop\"/>"), 1);
        assert_eq!(count(&export.musicxml, "<tied type=\"start\""), 1);
        assert_eq!(count(&export.musicxml, "<tied type=\"stop\""), 1);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
        assert_eq!(measures[0].notes.len(), 1);
        assert_eq!(measures[1].notes.len(), 2);
    }

    #[test]
    fn grace_notes_export_reference_compatible_display_types_without_duration() {
        let source = "X:1\nT:Grace Display\nM:4/4\nL:1/4\nK:C\n{g}C {de}D|\n";
        let export = export_musicxml(source).expect("grace note should export");

        assert_balanced_xml(&export.musicxml);
        assert_eq!(count(&export.musicxml, "<grace/>"), 3);
        assert_eq!(count(&export.musicxml, "<type>eighth</type>"), 1);
        assert_eq!(count(&export.musicxml, "<type>16th</type>"), 2);
        assert!(!export.musicxml.contains("<duration>0</duration>"));
    }

    #[test]
    fn grace_notes_apply_implicit_key_signature_alter() {
        let source = "X:1\nT:Grace Key\nM:4/4\nL:1/4\nK:D\n{f}A {=f}A|\n";
        let export = export_musicxml(source).expect("grace key accidental should export");

        assert_balanced_xml(&export.musicxml);
        assert_eq!(count(&export.musicxml, "<grace/>"), 2);
        assert_eq!(count(&export.musicxml, "<alter>1</alter>"), 1);
        assert!(export.musicxml.contains("<accidental>natural</accidental>"));
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
        // V:1 (with its `&` overlay) and V:2 each become their own part. The
        // overlay still adds a second voice within V:1's part, so a backup and
        // `<voice>2</voice>` appear; V:2 is part P2, not a third voice.
        assert_eq!(count(&export.musicxml, "<part id="), 2);
        assert!(export.musicxml.contains("<part id=\"P2\""));
        assert!(export.musicxml.contains("<backup>"));
        assert!(export.musicxml.contains("<voice>2</voice>"));
        assert!(!export.musicxml.contains("<voice>3</voice>"));
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

    #[derive(Debug)]
    struct XmlMeasure {
        number: String,
        notes: Vec<XmlNote>,
        barlines: Vec<XmlBarline>,
    }

    #[derive(Debug)]
    struct XmlNote {
        rest: bool,
        step: Option<char>,
        duration: Option<u32>,
    }

    #[derive(Debug)]
    struct XmlBarline {
        location: String,
        bar_style: Option<String>,
        repeat_direction: Option<String>,
        repeat_times: Option<String>,
        endings: Vec<XmlEnding>,
    }

    #[derive(Debug)]
    struct XmlEnding {
        number: String,
        kind: String,
    }

    fn musicxml_measures(xml: &str) -> Vec<XmlMeasure> {
        let mut measures = Vec::new();
        let mut index = 0;
        while let Some(offset) = xml[index..].find("<measure ") {
            let start = index + offset;
            let open_end = xml[start..]
                .find('>')
                .map(|end| start + end)
                .expect("measure start tag should terminate");
            let end_tag = "</measure>";
            let end = xml[open_end..]
                .find(end_tag)
                .map(|end| open_end + end)
                .expect("measure should have closing tag");
            let open_tag = &xml[start..=open_end];
            let body = &xml[open_end + 1..end];
            measures.push(XmlMeasure {
                number: attr_value(open_tag, "number").expect("measure should have number"),
                notes: musicxml_notes(body),
                barlines: musicxml_barlines(body),
            });
            index = end + end_tag.len();
        }
        measures
    }

    fn musicxml_notes(xml: &str) -> Vec<XmlNote> {
        let mut notes = Vec::new();
        let mut index = 0;
        while let Some(offset) = xml[index..].find("<note") {
            let start = index + offset;
            let open_end = xml[start..]
                .find('>')
                .map(|end| start + end)
                .expect("note start tag should terminate");
            let end_tag = "</note>";
            let end = xml[open_end..]
                .find(end_tag)
                .map(|end| open_end + end)
                .expect("note should have closing tag");
            let body = &xml[open_end + 1..end];
            notes.push(XmlNote {
                rest: body.contains("<rest"),
                step: element_text(body, "step").and_then(|text| text.chars().next()),
                duration: element_text(body, "duration").and_then(|text| text.parse().ok()),
            });
            index = end + end_tag.len();
        }
        notes
    }

    fn musicxml_barlines(xml: &str) -> Vec<XmlBarline> {
        let mut barlines = Vec::new();
        let mut index = 0;
        while let Some(offset) = xml[index..].find("<barline ") {
            let start = index + offset;
            let open_end = xml[start..]
                .find('>')
                .map(|end| start + end)
                .expect("barline start tag should terminate");
            let end_tag = "</barline>";
            let end = xml[open_end..]
                .find(end_tag)
                .map(|end| open_end + end)
                .expect("barline should have closing tag");
            let open_tag = &xml[start..=open_end];
            let body = &xml[open_end + 1..end];
            let repeat_direction = body.find("<repeat ").and_then(|offset| {
                let repeat_start = offset;
                let repeat_end = body[repeat_start..]
                    .find('>')
                    .map(|end| repeat_start + end)?;
                attr_value(&body[repeat_start..=repeat_end], "direction")
            });
            let repeat_times = body.find("<repeat ").and_then(|offset| {
                let repeat_start = offset;
                let repeat_end = body[repeat_start..]
                    .find('>')
                    .map(|end| repeat_start + end)?;
                attr_value(&body[repeat_start..=repeat_end], "times")
            });
            barlines.push(XmlBarline {
                location: attr_value(open_tag, "location").expect("barline should have location"),
                bar_style: element_text(body, "bar-style"),
                repeat_direction,
                repeat_times,
                endings: musicxml_endings(body),
            });
            index = end + end_tag.len();
        }
        barlines
    }

    fn musicxml_endings(xml: &str) -> Vec<XmlEnding> {
        let mut endings = Vec::new();
        let mut index = 0;
        while let Some(offset) = xml[index..].find("<ending ") {
            let start = index + offset;
            let end = xml[start..]
                .find('>')
                .map(|end| start + end)
                .expect("ending tag should terminate");
            let tag = &xml[start..=end];
            endings.push(XmlEnding {
                number: attr_value(tag, "number").expect("ending should have number"),
                kind: attr_value(tag, "type").expect("ending should have type"),
            });
            index = end + 1;
        }
        endings
    }

    fn measure_numbers(measures: &[XmlMeasure]) -> Vec<&str> {
        measures
            .iter()
            .map(|measure| measure.number.as_str())
            .collect()
    }

    fn note_steps(measure: &XmlMeasure) -> Vec<char> {
        measure.notes.iter().filter_map(|note| note.step).collect()
    }

    fn note_durations(measure: &XmlMeasure) -> Vec<u32> {
        measure
            .notes
            .iter()
            .filter_map(|note| note.duration)
            .collect()
    }

    fn has_barline(
        measure: &XmlMeasure,
        location: &str,
        bar_style: Option<&str>,
        repeat_direction: Option<&str>,
    ) -> bool {
        measure.barlines.iter().any(|barline| {
            barline.location == location
                && barline.bar_style.as_deref() == bar_style
                && barline.repeat_direction.as_deref() == repeat_direction
        })
    }

    fn has_ending(measure: &XmlMeasure, location: &str, number: &str, kind: &str) -> bool {
        measure.barlines.iter().any(|barline| {
            barline.location == location
                && barline
                    .endings
                    .iter()
                    .any(|ending| ending.number == number && ending.kind == kind)
        })
    }

    fn attr_value(tag: &str, attr: &str) -> Option<String> {
        let pattern = format!("{attr}=\"");
        let start = tag.find(&pattern)? + pattern.len();
        let end = tag[start..].find('"')?;
        Some(tag[start..start + end].to_owned())
    }

    fn element_text(block: &str, element: &str) -> Option<String> {
        let open = format!("<{element}>");
        let close = format!("</{element}>");
        let start = block.find(&open)? + open.len();
        let end = block[start..].find(&close)? + start;
        Some(block[start..end].to_owned())
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
