use std::cmp::Ordering;

use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
use crate::model::{
    AccidentalMark, AlignedLyric, AlignedSymbolKind, AnnotationPlacementModel, BarlineKind,
    ChordEvent, DecorationAttachment, EventAttachments, Fraction, GraceEventKind,
    GraceGroupAttachment, GraceNoteEvent, KeySignatureModel, Measure, MeasureBarline, MeasureId,
    Part, Pitch, PreservedDirective, RestEvent, RestVisibility, Score, SlurRole, StaffId,
    TempoBeat, TempoModel, TextAttachment, TieRole, TimedEvent, TimedEventKind, TimelineEventKind,
    TupletAttachment, TupletRole, VoiceTimedEvent,
};
use crate::parse::ParseReport;

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

    fn write_initial_directions(&mut self, part: &Part, is_first_part: bool) {
        // Score-level directions (tempo and preserved `%%` directives) belong to
        // the score once, not to every part. With one part per voice, emitting
        // them in each part duplicated them N times. `W:` post-tune verses are
        // emitted separately as score-header credits (see `write_credits`).
        if !is_first_part {
            return;
        }
        if let Some(tempo_model) = &self.score.metadata.tempo_model {
            self.write_tempo_direction(tempo_model);
        } else if let Some(tempo) = &self.score.metadata.tempo {
            self.write_direction_words(&tempo.text, None, Some("1"), Some(1));
        }
        for directive in &self.score.metadata.preserved_directives {
            self.write_preserved_directive(directive, part);
        }
    }

    /// Emit a `Q:` tempo as a MusicXML `<metronome>` direction (matching the
    /// abc2xml reference), falling back to plain `<words>` when the field has no
    /// numeric tempo. A `<sound tempo=...>` is always emitted: quarter-notes per
    /// minute for a numeric tempo, or a default of 120 for text-only tempos.
    fn write_tempo_direction(&mut self, tempo: &TempoModel) {
        let beat_unit = tempo.beat.and_then(beat_unit_model);
        // A numeric tempo we cannot map to a beat unit falls back to words using
        // the raw field text, preserving prior behavior for exotic forms.
        if tempo.beat.is_some() && beat_unit.is_none() {
            if let Some(raw) = &self.score.metadata.tempo {
                self.write_direction_words(&raw.text, None, Some("1"), Some(1));
            }
            return;
        }

        self.xml.start("direction", &[("placement", "above")]);
        if let Some(text) = &tempo.text {
            self.xml.start("direction-type", &[]);
            self.xml.text_element("words", text);
            self.xml.end("direction-type");
        }
        if let Some(unit) = &beat_unit {
            self.xml.start("direction-type", &[]);
            self.xml.start("metronome", &[]);
            self.xml.text_element("beat-unit", unit.name);
            if unit.dotted {
                self.xml.empty("beat-unit-dot", &[]);
            }
            self.xml.text_element(
                "per-minute",
                &tempo.beat.expect("beat present").bpm.to_string(),
            );
            self.xml.end("metronome");
            self.xml.end("direction-type");
        }
        let sound_tempo = match tempo.beat {
            Some(beat) => sound_tempo_qpm(beat),
            None => 120.0,
        };
        self.xml
            .empty("sound", &[("tempo", &format!("{sound_tempo:.2}"))]);
        self.xml.end("direction");
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
                            display_duration: grace_display_duration(
                                group.note_count,
                                note.length_multiplier,
                            ),
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
                            duration: grace_base_unit(group.note_count),
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
                                display_duration: grace_display_duration(
                                    group.note_count,
                                    note.length_multiplier,
                                ),
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
        let has_notation_decorations = attachments
            .decorations
            .iter()
            .any(|decoration| decoration_notation(decoration).is_some());
        if !(has_tied || has_slurs || has_tuplets || has_notation_decorations) {
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
        if has_notation_decorations {
            let kinds = |want: fn(NotationKind) -> Option<&'static str>| {
                attachments
                    .decorations
                    .iter()
                    .filter_map(|decoration| decoration_notation(decoration).and_then(want))
                    .collect::<Vec<_>>()
            };
            // MusicXML groups these per category, in schema order: ornaments,
            // technical, articulations, then fermata.
            let ornaments = kinds(|kind| match kind {
                NotationKind::Ornament(name) => Some(name),
                _ => None,
            });
            if !ornaments.is_empty() {
                self.xml.start("ornaments", &[]);
                for name in ornaments {
                    self.xml.empty(name, &[]);
                }
                self.xml.end("ornaments");
            }
            let technical = kinds(|kind| match kind {
                NotationKind::Technical(name) => Some(name),
                _ => None,
            });
            if !technical.is_empty() {
                self.xml.start("technical", &[]);
                for name in technical {
                    self.xml.empty(name, &[]);
                }
                self.xml.end("technical");
            }
            let articulations = kinds(|kind| match kind {
                NotationKind::Articulation(name) => Some(name),
                _ => None,
            });
            if !articulations.is_empty() {
                self.xml.start("articulations", &[]);
                for name in articulations {
                    self.xml.empty(name, &[]);
                }
                self.xml.end("articulations");
            }
            for kind in attachments
                .decorations
                .iter()
                .filter_map(decoration_notation)
            {
                if let NotationKind::Fermata(fermata_type) = kind {
                    self.xml.empty("fermata", &[("type", fermata_type)]);
                }
            }
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
            } else if is_suppressed_decoration(decoration.name.as_str()) {
                // No clean MusicXML equivalent (e.g. the Irish roll `~`).
                // abc2xml emits nothing; suppress without a words direction or
                // an unsupported-decoration diagnostic.
            } else if decoration_notation(decoration).is_none() {
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
        // Trailing chord degrees are emitted as added degrees, mirroring
        // abc2xml (which only ever produces `degree-type = add`).
        for degree in &chord.degrees {
            self.xml.start("degree", &[]);
            self.xml
                .text_element("degree-value", &degree.value.to_string());
            self.xml
                .text_element("degree-alter", &degree.alter.to_string());
            self.xml.text_element("degree-type", "add");
            self.xml.end("degree");
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

/// A MusicXML beat unit: a note-type name plus whether it carries a single dot.
struct BeatUnit {
    name: &'static str,
    dotted: bool,
}

/// Map a tempo beat fraction to a MusicXML `<beat-unit>` (with optional dot).
///
/// Plain powers of two map directly (1/4 -> quarter). A numerator of 3 over a
/// power of two is a dotted unit one power larger (3/8 -> dotted quarter). Forms
/// that do not map cleanly return `None` so the writer falls back to words.
fn beat_unit_model(beat: TempoBeat) -> Option<BeatUnit> {
    let name_for = |denominator: u32| -> Option<&'static str> {
        match denominator {
            1 => Some("whole"),
            2 => Some("half"),
            4 => Some("quarter"),
            8 => Some("eighth"),
            16 => Some("16th"),
            32 => Some("32nd"),
            64 => Some("64th"),
            _ => None,
        }
    };
    match beat.beat_numerator {
        1 => name_for(beat.beat_denominator).map(|name| BeatUnit {
            name,
            dotted: false,
        }),
        // 3/(2^k) is a dotted note one power larger, e.g. 3/8 -> dotted quarter.
        3 if beat.beat_denominator.is_multiple_of(2) => {
            name_for(beat.beat_denominator / 2).map(|name| BeatUnit { name, dotted: true })
        }
        _ => None,
    }
}

/// Quarter-notes-per-minute for a `<sound tempo>` value: `beat * 4 * bpm`.
fn sound_tempo_qpm(beat: TempoBeat) -> f64 {
    (f64::from(beat.beat_numerator) / f64::from(beat.beat_denominator)) * 4.0 * f64::from(beat.bpm)
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

/// Count-based grace base unit, matching abc2xml: 1/8 for a single grace note in
/// the group, 1/16 otherwise. The grace note's written length modifier is
/// applied on top of this (see [`grace_display_duration`]).
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

/// Display duration of a single grace note: the count-based base unit scaled by
/// the grace note's written length modifier (`/` -> 1/2, `2` -> 2, ...). The
/// resulting fraction drives the `<type>`/`<dots>` spelling; grace notes still
/// carry no `<duration>` element.
fn grace_display_duration(note_count: u32, length_multiplier: Fraction) -> Fraction {
    grace_base_unit(note_count).checked_mul(length_multiplier)
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
    degrees: Vec<ChordDegree>,
}

#[derive(Debug, Clone, Copy)]
struct ChordDegree {
    value: u8,
    alter: i8,
}

/// Chord-quality token table, mirroring abc2xml's `compChordTab`
/// (`docs/.../abc2xml.py`). Ordered longest-token-first so a greedy prefix
/// match consumes the maximal quality (e.g. `maj7` before `m`/`ma`). The first
/// matched token determines `<kind>`; this is the closed MusicXML kind enum
/// that ABC commonly uses. Anything not listed falls back to `major`, matching
/// abc2xml's `chordTab.get(token, 'major')`.
const CHORD_QUALITY_TABLE: &[(&str, &str)] = &[
    // Seventh chords (longest first).
    ("maj7", "major-seventh"),
    ("Maj7", "major-seventh"),
    ("min7", "minor-seventh"),
    ("dim7", "diminished-seventh"),
    ("aug7", "augmented-seventh"),
    ("mi7b5", "half-diminished"),
    ("m7b5", "half-diminished"),
    ("ma7", "major-seventh"),
    ("M7", "major-seventh"),
    ("mi7", "minor-seventh"),
    ("m7", "minor-seventh"),
    ("o7", "diminished-seventh"),
    ("-7", "minor-seventh"),
    ("+7", "augmented-seventh"),
    ("7", "dominant"),
    // Sixth chords.
    ("min6", "minor-sixth"),
    ("ma6", "major-sixth"),
    ("M6", "major-sixth"),
    ("mi6", "minor-sixth"),
    ("m6", "minor-sixth"),
    ("6", "major-sixth"),
    // Ninth chords.
    ("maj9", "major-ninth"),
    ("Maj9", "major-ninth"),
    ("min9", "minor-ninth"),
    ("ma9", "major-ninth"),
    ("M9", "major-ninth"),
    ("mi9", "minor-ninth"),
    ("m9", "minor-ninth"),
    ("9", "dominant-ninth"),
    // Eleventh chords.
    ("maj11", "major-11th"),
    ("Maj11", "major-11th"),
    ("min11", "minor-11th"),
    ("ma11", "major-11th"),
    ("M11", "major-11th"),
    ("mi11", "minor-11th"),
    ("m11", "minor-11th"),
    ("11", "dominant-11th"),
    // Thirteenth chords.
    ("maj13", "major-13th"),
    ("Maj13", "major-13th"),
    ("min13", "minor-13th"),
    ("ma13", "major-13th"),
    ("M13", "major-13th"),
    ("mi13", "minor-13th"),
    ("m13", "minor-13th"),
    ("13", "dominant-13th"),
    // Triads (must come after the extended qualities above).
    ("maj", "major"),
    ("Maj", "major"),
    ("aug", "augmented"),
    ("dim", "diminished"),
    ("min", "minor"),
    ("ma", "major"),
    ("mi", "minor"),
    ("M", "major"),
    ("m", "minor"),
    ("o", "diminished"),
    ("+", "augmented"),
    ("-", "minor"),
];

/// Suspended-quality tokens. abc2xml parses an optional suspended token *after*
/// the main quality but keeps only the first kind token for `<kind>`; when a
/// suspended token stands alone it determines the kind. Ordered longest-first.
const SUSPENDED_TABLE: &[(&str, &str)] = &[
    ("sus4", "suspended-fourth"),
    ("sus2", "suspended-second"),
    ("sus", "suspended-fourth"),
];

/// Structural chord-symbol parser following the ABC 2.1 §4.18 grammar as
/// implemented by abc2xml (the comparison baseline):
///
/// ```text
/// chordsym = root accidental? quality? suspended? degree* ("/" bass)?
/// ```
///
/// Returns `None` (so the symbol is emitted as plain `<words>`) when any part
/// fails to parse or unconsumed text remains, exactly mirroring abc2xml's
/// pyparsing behaviour (e.g. `Cadd9`, `Cbb`, `NC` are not harmony). A trailing
/// parenthesised group is suppressed.
fn parse_chord_symbol(text: &str) -> Option<ParsedChordSymbol> {
    let trimmed = text.trim();
    // A trailing parenthesised group is dropped by abc2xml (`C(no3)` -> major).
    let core = match trimmed.find('(') {
        Some(open) if trimmed.ends_with(')') => trimmed[..open].trim_end(),
        _ => trimmed,
    };

    let (root, rest) = parse_chord_tone(core)?;
    // The bass `/X` is split off the tail first; the rest before it is the
    // quality + degrees.
    let (quality_part, bass) = match rest.split_once('/') {
        Some((head, bass_text)) => {
            let (bass_tone, bass_rest) = parse_chord_tone(bass_text)?;
            if !bass_rest.is_empty() {
                return None;
            }
            (head, Some(bass_tone))
        }
        None => (rest, None),
    };

    let quality = quality_part.trim();
    // Optional quality token (greedy longest prefix), then optional suspended
    // token. The kind comes from the first matched token; abc2xml drops the
    // suspended token from <kind> when a quality precedes it.
    let mut remaining = quality;
    let mut kind = "major";
    let mut matched_any = false;
    if let Some((token, mapped)) = match_prefix(remaining, CHORD_QUALITY_TABLE) {
        kind = mapped;
        matched_any = true;
        remaining = &remaining[token.len()..];
    }
    if let Some((token, mapped)) = match_prefix(remaining, SUSPENDED_TABLE) {
        if !matched_any {
            kind = mapped;
        }
        remaining = &remaining[token.len()..];
    }

    // Zero or more trailing chord degrees: `[#=b]?(2|4|5|6|7|9|11|13)`.
    let mut degrees = Vec::new();
    loop {
        let trimmed_remaining = remaining.trim_start();
        match parse_chord_degree(trimmed_remaining) {
            Some((degree, after)) => {
                degrees.push(degree);
                remaining = after;
            }
            None => {
                remaining = trimmed_remaining;
                break;
            }
        }
    }

    if !remaining.is_empty() {
        // Unconsumed text means this is not a recognised chord symbol.
        return None;
    }

    Some(ParsedChordSymbol {
        root_step: root.step,
        root_alter: root.alter,
        bass_step: bass.map(|tone| tone.step),
        bass_alter: bass.map(|tone| tone.alter).unwrap_or(0),
        kind,
        degrees,
    })
}

/// Returns the matching `(token, mapped_value)` for the longest table entry
/// that is a prefix of `text`, or `None`.
fn match_prefix(
    text: &str,
    table: &[(&'static str, &'static str)],
) -> Option<(&'static str, &'static str)> {
    table
        .iter()
        .find(|(token, _)| text.starts_with(token))
        .map(|(token, mapped)| (*token, *mapped))
}

/// Parses one chord degree `[#=b]?(2|4|5|6|7|9|11|13)` from the start of `text`,
/// returning the degree and the unconsumed tail.
fn parse_chord_degree(text: &str) -> Option<(ChordDegree, &str)> {
    let (alter, after_accidental) = match text.as_bytes().first() {
        Some(b'#') => (1, &text[1..]),
        Some(b'b') => (-1, &text[1..]),
        Some(b'=') => (0, &text[1..]),
        _ => (0, text),
    };
    // Match the two-digit degrees before the single-digit ones.
    for value in [13u8, 11, 9, 7, 6, 5, 4, 2] {
        let token = value.to_string();
        if after_accidental.starts_with(&token) {
            return Some((
                ChordDegree { value, alter },
                &after_accidental[token.len()..],
            ));
        }
    }
    None
}

#[derive(Debug, Clone, Copy)]
struct ChordTone {
    step: char,
    alter: i8,
}

/// Parses a chord root/bass tone `[A-G][#b]?` from the start of `text`,
/// returning the tone and the unconsumed tail. Only a single accidental is
/// accepted, matching abc2xml (which rejects `Cbb`/`C##`).
fn parse_chord_tone(text: &str) -> Option<(ChordTone, &str)> {
    let mut chars = text.char_indices();
    let (_, first) = chars.next()?;
    let step = first.to_ascii_uppercase();
    if !matches!(step, 'A'..='G') {
        return None;
    }
    let mut consumed = first.len_utf8();
    // Only a single `#`/`b`/`=` accidental, like abc2xml's `chord_accidental`.
    // A `-` is the minor-quality token, never a root/bass flat.
    let alter = match text[consumed..].chars().next() {
        Some('#') => {
            consumed += 1;
            1
        }
        Some('b') => {
            consumed += 1;
            -1
        }
        Some('=') => {
            consumed += 1;
            0
        }
        _ => 0,
    };
    Some((ChordTone { step, alter }, &text[consumed..]))
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

/// MusicXML `<notations>` category and element for an ABC `!decoration!`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotationKind {
    /// Inside `<ornaments>` (e.g. trill, mordent, turn).
    Ornament(&'static str),
    /// Inside `<articulations>` (e.g. staccato, accent, tenuto).
    Articulation(&'static str),
    /// Inside `<technical>` (e.g. up-bow, down-bow, open string).
    Technical(&'static str),
    /// A `<fermata>` element with the given type attribute.
    Fermata(&'static str),
}

/// Map an ABC decoration to its MusicXML notation, per the ABC 2.1 decoration
/// list and the MusicXML notation categories. Decorations handled elsewhere as
/// dynamics or directions return `None`.
fn decoration_notation(decoration: &DecorationAttachment) -> Option<NotationKind> {
    Some(match decoration.name.as_str() {
        "." | "staccato" => NotationKind::Articulation("staccato"),
        ">" | "accent" | "emphasis" => NotationKind::Articulation("accent"),
        "tenuto" => NotationKind::Articulation("tenuto"),
        "wedge" => NotationKind::Articulation("staccatissimo"),
        "marcato" => NotationKind::Articulation("strong-accent"),
        "breath" => NotationKind::Articulation("breath-mark"),
        "fermata" => NotationKind::Fermata("upright"),
        "invertedfermata" => NotationKind::Fermata("inverted"),
        "trill" => NotationKind::Ornament("trill-mark"),
        "mordent" | "lowermordent" => NotationKind::Ornament("mordent"),
        "uppermordent" | "pralltriller" => NotationKind::Ornament("inverted-mordent"),
        "turn" => NotationKind::Ornament("turn"),
        "invertedturn" => NotationKind::Ornament("inverted-turn"),
        "upbow" => NotationKind::Technical("up-bow"),
        "downbow" => NotationKind::Technical("down-bow"),
        "open" => NotationKind::Technical("open-string"),
        "thumb" => NotationKind::Technical("thumb-position"),
        "snap" => NotationKind::Technical("snap-pizzicato"),
        _ => return None,
    })
}

/// Decorations that have no clean MusicXML equivalent and are intentionally not
/// emitted (matching abc2xml, which emits nothing). They must not fall through
/// to a `<words>` direction.
fn is_suppressed_decoration(name: &str) -> bool {
    // `~` (Irish roll / general gracing) normalizes to the canonical `roll`.
    matches!(name, "roll")
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
        // `G7&<>'` has junk after the quality token, so (like abc2xml) it is
        // not a recognised chord symbol and is emitted as escaped words.
        assert!(export.musicxml.contains("G7&amp;&lt;&gt;&apos;"));
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
        // `Db/Ab` uses `b` as the root/bass flat (abc2xml's chord accidental);
        // `-` is reserved for the minor quality and is not a flat.
        let source = "X:1\nT:Slash Chords\nM:4/4\nL:1/4\nK:C\n\"C/E\"C \"Db/Ab\"D|\n";
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
    fn chord_qualities_map_to_musicxml_kinds_matching_abc2xml() {
        // Each chord symbol must classify to the same <kind> abc2xml emits, so
        // music21 re-renders identical figures from <kind> (it ignores text=).
        let cases = [
            ("F#dim", "diminished"),
            ("Cdim7", "diminished-seventh"),
            ("Caug", "augmented"),
            ("C+", "augmented"),
            ("Co", "diminished"),
            ("C-", "minor"),
            ("Dsus4", "suspended-fourth"),
            ("Dsus2", "suspended-second"),
            ("Csus", "suspended-fourth"),
            ("Cmaj7", "major-seventh"),
            ("CM7", "major-seventh"),
            ("Cm6", "minor-sixth"),
            ("C6", "major-sixth"),
            ("Cm7", "minor-seventh"),
            ("C7", "dominant"),
            ("Cm", "minor"),
            ("C", "major"),
            // Ninth / eleventh / thirteenth kinds.
            ("C9", "dominant-ninth"),
            ("Cmaj9", "major-ninth"),
            ("Cm9", "minor-ninth"),
            ("C11", "dominant-11th"),
            ("Cm11", "minor-11th"),
            ("C13", "dominant-13th"),
            ("Cmaj13", "major-13th"),
            ("Cm13", "minor-13th"),
            // Half-diminished.
            ("Cm7b5", "half-diminished"),
            ("Cmin7b5", "minor-seventh"),
            // Suspended after a seventh keeps only the first kind token.
            ("C7sus4", "dominant"),
            ("Cmaj7sus4", "major-seventh"),
        ];
        for (symbol, expected_kind) in cases {
            let source = format!("X:1\nM:4/4\nL:1/4\nK:C\n\"{symbol}\"C4|\n");
            let export =
                export_musicxml(&source).unwrap_or_else(|_| panic!("chord {symbol} should export"));
            assert_balanced_xml(&export.musicxml);
            let expected = format!("<kind text=\"{symbol}\">{expected_kind}</kind>");
            assert!(
                export.musicxml.contains(&expected),
                "chord {symbol} should map to {expected_kind}; got:\n{}",
                export.musicxml
            );
        }
    }

    #[test]
    fn power_chord_exports_major_kind_with_add_fifth_degree() {
        // abc2xml has no `power` quality: `A5` parses as a major triad with a
        // trailing `5` chord degree, emitted as an added fifth.
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"A5\"C4|\n";
        let export = export_musicxml(source).expect("power chord should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<root-step>A</root-step>"));
        assert!(export.musicxml.contains(">major</kind>"));
        assert!(export.musicxml.contains("<degree-value>5</degree-value>"));
        assert!(export.musicxml.contains("<degree-alter>0</degree-alter>"));
        assert!(export.musicxml.contains("<degree-type>add</degree-type>"));
    }

    #[test]
    fn altered_trailing_degrees_export_as_add_with_alter() {
        // Trailing chord degrees with a `#`/`b` accidental become added degrees
        // with the corresponding alter, matching abc2xml exactly.
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"C7b9#5\"C4|\n";
        let export = export_musicxml(source).expect("altered chord should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains(">dominant</kind>"));
        assert!(export.musicxml.contains("<degree-value>9</degree-value>"));
        assert!(export.musicxml.contains("<degree-alter>-1</degree-alter>"));
        assert!(export.musicxml.contains("<degree-value>5</degree-value>"));
        assert!(export.musicxml.contains("<degree-alter>1</degree-alter>"));
        assert_eq!(count(&export.musicxml, "<degree>"), 2);
    }

    #[test]
    fn add_and_omit_word_chords_export_as_words_not_harmony() {
        // abc2xml's chord grammar has no `add`/`no` tokens, so these symbols do
        // not parse as harmony at all and fall through to plain text.
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"Cadd9\"C \"C9no3\"D \"Cadd11\"E|\n";
        let export = export_musicxml(source).expect("word chords should export");

        assert_balanced_xml(&export.musicxml);
        assert_eq!(count(&export.musicxml, "<harmony>"), 0);
        assert!(export.musicxml.contains("<words>Cadd9</words>"));
        assert!(export.musicxml.contains("<words>C9no3</words>"));
        assert!(export.musicxml.contains("<words>Cadd11</words>"));
    }

    #[test]
    fn double_accidental_and_garbage_roots_are_not_harmony() {
        // abc2xml accepts only a single root accidental and rejects unparsable
        // tails, so these fall through to words rather than fake harmony.
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"Cbb\"C \"C##\"D \"Cx\"E \"NC\"F|\n";
        let export = export_musicxml(source).expect("garbage chords should export");

        assert_balanced_xml(&export.musicxml);
        assert_eq!(count(&export.musicxml, "<harmony>"), 0);
        assert!(export.musicxml.contains("<words>Cbb</words>"));
        assert!(export.musicxml.contains("<words>C##</words>"));
        assert!(export.musicxml.contains("<words>Cx</words>"));
        assert!(export.musicxml.contains("<words>NC</words>"));
    }

    #[test]
    fn parenthesized_chord_suffix_is_suppressed() {
        // A trailing parenthesized group is dropped: `C(no3)` is a plain major.
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"C(no3)\"C4|\n";
        let export = export_musicxml(source).expect("parenthesized chord should export");

        assert_balanced_xml(&export.musicxml);
        assert_eq!(count(&export.musicxml, "<harmony>"), 1);
        assert!(export.musicxml.contains(">major</kind>"));
        assert_eq!(count(&export.musicxml, "<degree>"), 0);
    }

    #[test]
    fn slash_chord_with_quality_keeps_kind_and_bass() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n\"Cm7/Bb\"C4|\n";
        let export = export_musicxml(source).expect("slash chord should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains(">minor-seventh</kind>"));
        assert!(export.musicxml.contains("<bass-step>B</bass-step>"));
        assert!(export.musicxml.contains("<bass-alter>-1</bass-alter>"));
    }

    #[test]
    fn tempo_beat_equals_bpm_emits_metronome() {
        let source = "X:1\nM:4/4\nL:1/4\nQ:1/4=104\nK:C\nC4|\n";
        let export = export_musicxml(source).expect("tempo score should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<metronome>"));
        assert!(export.musicxml.contains("<beat-unit>quarter</beat-unit>"));
        assert!(export.musicxml.contains("<per-minute>104</per-minute>"));
        assert!(export.musicxml.contains("<sound tempo=\"104.00\""));
        assert!(!export.musicxml.contains("<words>1/4=104</words>"));
    }

    #[test]
    fn tempo_dotted_beat_unit_emits_metronome_dot() {
        let source = "X:1\nM:4/4\nL:1/4\nQ:3/8=100\nK:C\nC4|\n";
        let export = export_musicxml(source).expect("tempo score should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<beat-unit>quarter</beat-unit>"));
        assert!(export.musicxml.contains("<beat-unit-dot"));
        assert!(export.musicxml.contains("<per-minute>100</per-minute>"));
        assert!(export.musicxml.contains("<sound tempo=\"150.00\""));
        assert!(!export.musicxml.contains("<words>3/8=100</words>"));
    }

    #[test]
    fn tempo_bare_number_uses_unit_note_length() {
        let source = "X:1\nM:4/4\nL:1/8\nQ:120\nK:C\nC4|\n";
        let export = export_musicxml(source).expect("tempo score should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<beat-unit>eighth</beat-unit>"));
        assert!(export.musicxml.contains("<per-minute>120</per-minute>"));
        assert!(export.musicxml.contains("<sound tempo=\"60.00\""));
        assert!(!export.musicxml.contains("<words>120</words>"));
    }

    #[test]
    fn tempo_text_only_stays_words_with_default_sound() {
        let source = "X:1\nM:4/4\nL:1/4\nQ:\"Slow\"\nK:C\nC4|\n";
        let export = export_musicxml(source).expect("tempo score should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<words>Slow</words>"));
        assert!(export.musicxml.contains("<sound tempo=\"120.00\""));
        assert_eq!(count(&export.musicxml, "<metronome>"), 0);
    }

    #[test]
    fn tempo_text_plus_beat_emits_words_and_metronome() {
        let source = "X:1\nM:4/4\nL:1/4\nQ:\"allegretto\" 1/4=110\nK:C\nC4|\n";
        let export = export_musicxml(source).expect("tempo score should export");

        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<words>allegretto</words>"));
        assert!(export.musicxml.contains("<metronome>"));
        assert!(export.musicxml.contains("<beat-unit>quarter</beat-unit>"));
        assert!(export.musicxml.contains("<per-minute>110</per-minute>"));
        assert!(export.musicxml.contains("<sound tempo=\"110.00\""));
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
    fn pickup_repeat_start_places_forward_repeat_on_repeated_section() {
        // ABC 2.1 §6: `|:` after a pickup marks the START of the repeated
        // section, so the forward repeat belongs to the LEFT of measure 2
        // (`CDEF`), not the pickup measure 1 (`E`).
        let source = "X:1\nM:4/4\nL:1/4\nK:C\nE|:CDEF|GABc:|]\n";
        let export = export_musicxml(source).expect("pickup repeat should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1", "2", "3"]);
        assert_eq!(note_steps(&measures[0]), vec!['E']);
        assert_eq!(note_steps(&measures[1]), vec!['C', 'D', 'E', 'F']);
        assert!(
            !has_barline(&measures[0], "left", None, Some("forward")),
            "pickup measure must not carry the forward repeat"
        );
        assert!(has_barline(&measures[1], "left", None, Some("forward")));
        assert!(has_barline(&measures[2], "right", None, Some("backward")));
    }

    #[test]
    fn mid_tune_repeat_start_places_forward_repeat_on_repeated_section() {
        // `|:` after content mid-tune marks the start of the repeated section:
        // forward repeat belongs to the LEFT of measure 3 (`cBAG`).
        let source = "X:1\nM:4/4\nL:1/4\nK:C\nCDEF|GABc|:cBAG|FEDC:|]\n";
        let export = export_musicxml(source).expect("mid-tune repeat should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1", "2", "3", "4"]);
        assert_eq!(note_steps(&measures[1]), vec!['G', 'A', 'B', 'C']);
        assert_eq!(note_steps(&measures[2]), vec!['C', 'B', 'A', 'G']);
        assert!(
            !has_barline(&measures[1], "left", None, Some("forward")),
            "measure preceding the repeat must not carry the forward repeat"
        );
        assert!(has_barline(&measures[2], "left", None, Some("forward")));
        assert!(has_barline(&measures[3], "right", None, Some("backward")));
    }

    #[test]
    fn leading_repeat_start_after_header_stays_on_its_own_measure() {
        // Non-regression: a `|:` with no preceding content in its measure is a
        // legitimate LEFT barline of measure 1 and must stay there.
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n|:CDEF|GABc:|]\n";
        let export = export_musicxml(source).expect("leading repeat should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1", "2"]);
        assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
        assert!(has_barline(&measures[0], "left", None, Some("forward")));
        assert!(has_barline(&measures[1], "right", None, Some("backward")));
    }

    #[test]
    fn double_then_repeat_start_after_content_defers_forward_repeat() {
        // `||:` after content (`Double` + `RepeatStart`) must not drop the
        // forward repeat and must place it on the measure beginning the body.
        let source = "X:1\nM:4/4\nL:1/4\nK:C\nCDEF||:GABc|cBAG:|]\n";
        let export = export_musicxml(source).expect("double-then-repeat should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(measure_numbers(&measures), vec!["1", "2", "3"]);
        assert_eq!(note_steps(&measures[0]), vec!['C', 'D', 'E', 'F']);
        assert_eq!(note_steps(&measures[1]), vec!['G', 'A', 'B', 'C']);
        assert!(
            !has_barline(&measures[0], "left", None, Some("forward")),
            "first measure must not carry the forward repeat"
        );
        assert!(has_barline(&measures[1], "left", None, Some("forward")));
        assert!(has_barline(&measures[2], "right", None, Some("backward")));
    }

    #[test]
    fn section_final_barline_followed_by_regular_is_preserved() {
        // Bug C: `|]` (light-heavy section barline) immediately followed by `|`
        // must be kept as the RIGHT barline of its measure (`GABc`).
        let source = "X:1\nM:4/4\nL:1/4\nK:C\nCDEF|GABc|]|cBAG|FEDC|]\n";
        let export = export_musicxml(source).expect("section final barline should export");

        assert_balanced_xml(&export.musicxml);
        let measures = musicxml_measures(&export.musicxml);
        assert_eq!(note_steps(&measures[1]), vec!['G', 'A', 'B', 'C']);
        assert!(
            has_barline(&measures[1], "right", Some("light-heavy"), None),
            "the section final barline after measure 2 must be preserved"
        );
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
    fn unclosed_decoration_does_not_swallow_following_notes() {
        // `!f2e2f2` is a stray `!` before notes (a deprecated line-break or
        // typo), not a decoration named "f2e2f2". The notes must survive.
        let source = "X:1\nL:1/8\nK:C\nA !f2e2f2 | g2|\n";
        let export = export_musicxml(source).expect("stray-bang score should export");
        assert_balanced_xml(&export.musicxml);
        assert_eq!(count(&export.musicxml, "<step>F</step>"), 2);
        assert_eq!(count(&export.musicxml, "<step>E</step>"), 1);
        assert_eq!(count(&export.musicxml, "<step>A</step>"), 1);
        assert_eq!(count(&export.musicxml, "<step>G</step>"), 1);
    }

    #[test]
    fn chords_adjacent_to_barlines_are_not_swallowed() {
        // `|[G2C,2]` and `][` must keep the chords intact: the `[` opens a chord,
        // it is not part of a liberal `|[` / `][` barline. Two 4/4 measures of
        // four quarter chords each, not many tiny measures.
        let source =
            "X:1\nL:1/8\nM:4/4\nK:C\n[G2C,2][c2C,2][A2F,2][e2C,2]|[F2D,2][D2D,2][A2D,2][d2D,2]|\n";
        let export = export_musicxml(source).expect("bass-chord score should export");
        assert_balanced_xml(&export.musicxml);
        assert_eq!(count(&export.musicxml, "<measure "), 2);
        assert!(
            !export
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.barline.liberal"),
            "no chord bracket should be read as a liberal barline"
        );
    }

    #[test]
    fn staccato_chord_keeps_its_length_suffix() {
        // `.[CE]2` is a staccato chord of length 2 (a quarter at L:1/8), not a
        // dotted barline. The leading `.` must not swallow the chord's length.
        let source = "X:1\nL:1/8\nK:C\n.[CE]2 [CE]|\n";
        let export = export_musicxml(source).expect("staccato chord should export");
        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<staccato/>"));
        // Two chords (each two notes) in one measure: the first is a quarter.
        assert_eq!(count(&export.musicxml, "<chord/>"), 2);
        assert_eq!(count(&export.musicxml, "<type>quarter</type>"), 2);
        assert_eq!(count(&export.musicxml, "<type>eighth</type>"), 2);
    }

    #[test]
    fn decorations_map_to_notation_elements_not_words() {
        // ABC decorations map to MusicXML notation categories, not <words>:
        // fermata -> <fermata>, staccato/accent -> <articulations>,
        // up-bow -> <technical>, trill -> <ornaments>.
        let source = "X:1\nL:1/4\nK:C\n!fermata!.C !accent!D|!upbow!E !trill!F|\n";
        let export = export_musicxml(source).expect("decorated score should export");
        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<fermata type=\"upright\"/>"));
        assert!(export.musicxml.contains("<articulations>"));
        assert!(export.musicxml.contains("<staccato/>"));
        assert!(export.musicxml.contains("<accent/>"));
        assert!(export.musicxml.contains("<technical>"));
        assert!(export.musicxml.contains("<up-bow/>"));
        assert!(export.musicxml.contains("<ornaments>"));
        assert!(export.musicxml.contains("<trill-mark/>"));
        assert!(!export.musicxml.contains("<words>fermata</words>"));
        assert!(!export.musicxml.contains("<words>accent</words>"));
    }

    #[test]
    fn shorthand_decorations_map_to_notation_elements_not_words() {
        // ABC 2.1 §4.14 single-char shorthand decorations are the canonical
        // equivalents of the long-form `!...!` names and must map to the same
        // MusicXML notation/symbol output, never to <words> directions.
        let source = "X:1\nL:1/4\nK:C\nHC TD|uE vF|MG Pa|\n";
        let export = export_musicxml(source).expect("shorthand decorations should export");
        assert_balanced_xml(&export.musicxml);
        // H -> fermata
        assert!(export.musicxml.contains("<fermata type=\"upright\"/>"));
        // T -> trill
        assert!(export.musicxml.contains("<trill-mark/>"));
        // u -> up-bow, v -> down-bow
        assert!(export.musicxml.contains("<up-bow/>"));
        assert!(export.musicxml.contains("<down-bow/>"));
        // M -> lowermordent (mordent), P -> uppermordent (inverted-mordent)
        assert!(export.musicxml.contains("<mordent/>"));
        assert!(export.musicxml.contains("<inverted-mordent/>"));
        // No raw shorthand chars leak out as <words>.
        for raw in ["H", "T", "u", "v", "M", "P"] {
            assert!(
                !export.musicxml.contains(&format!("<words>{raw}</words>")),
                "shorthand `{raw}` should not be emitted as <words>"
            );
        }
    }

    #[test]
    fn shorthand_accent_maps_to_articulation_not_words() {
        let source = "X:1\nL:1/4\nK:C\nLC D|\n";
        let export = export_musicxml(source).expect("shorthand accent should export");
        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<articulations>"));
        assert!(export.musicxml.contains("<accent/>"));
        assert!(!export.musicxml.contains("<words>L</words>"));
    }

    #[test]
    fn shorthand_segno_and_coda_map_to_direction_symbols_not_words() {
        let source = "X:1\nL:1/4\nK:C\nSC OD|\n";
        let export = export_musicxml(source).expect("shorthand segno/coda should export");
        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<segno/>"));
        assert!(export.musicxml.contains("<coda/>"));
        assert!(!export.musicxml.contains("<words>S</words>"));
        assert!(!export.musicxml.contains("<words>O</words>"));
    }

    #[test]
    fn shorthand_roll_emits_neither_words_nor_diagnostic() {
        // `~` (Irish roll / general gracing) has no clean MusicXML equivalent;
        // abc2xml emits nothing. The hard requirement is that it must NOT become
        // a <words> direction, which would show up as an extra music21 direction.
        let source = "X:1\nL:1/4\nK:C\n~C D|\n";
        let export = export_musicxml(source).expect("shorthand roll should export");
        assert_balanced_xml(&export.musicxml);
        assert!(!export.musicxml.contains("<words>~</words>"));
        assert!(!export.musicxml.contains("<words>roll</words>"));
        // Suppressed cleanly: no unsupported-decoration diagnostic for `~`.
        assert!(
            !export
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.musicxml.decoration.unsupported"),
            "roll should be suppressed without an unsupported-decoration diagnostic"
        );
        // The notes and their timing survive.
        assert_eq!(count(&export.musicxml, "<note>"), 2);
    }

    #[test]
    fn user_defined_symbol_expands_to_its_notation_not_words() {
        // `U:T=!trill!` redefines T; a `U:`-defined letter must expand to its
        // definition and map through the same notation path.
        let source = "X:1\nU:W=!trill!\nL:1/4\nK:C\nWC D|\n";
        let export = export_musicxml(source).expect("user symbol should export");
        assert_balanced_xml(&export.musicxml);
        assert!(export.musicxml.contains("<trill-mark/>"));
        assert!(!export.musicxml.contains("<words>W</words>"));
    }

    #[test]
    fn post_tune_words_export_as_credits_not_in_measure_directions() {
        // `W:` words are printed after the tune (ABC 2.1), so they belong in
        // score-header <credit> elements, not as in-measure <words> directions.
        let source =
            "X:1\nT:Song\nL:1/4\nK:C\nC D E F|\nW: Verse one here\nW: Verse two here\nW:\n";
        let export = export_musicxml(source).expect("post-tune words should export");
        assert_balanced_xml(&export.musicxml);
        assert!(
            export
                .musicxml
                .contains("<credit-words>Verse one here</credit-words>")
        );
        assert!(
            export
                .musicxml
                .contains("<credit-words>Verse two here</credit-words>")
        );
        // The empty `W:` line is skipped, and no verse leaks into a direction.
        assert!(!export.musicxml.contains("<words>Verse one here</words>"));
        assert_eq!(count(&export.musicxml, "<credit-words>"), 2);
    }

    #[test]
    fn staves_parenthesis_group_merges_voices_into_one_part() {
        // `%%staves 1 (2 3) 4`: voices 2 and 3 share one part; 1 and 4 are their
        // own parts, giving three parts.
        let source = concat!(
            "X:1\nL:1/4\n%%staves 1 (2 3) 4\n",
            "V:1\nV:2\nV:3\nV:4\nK:C\n",
            "V:1\nC D|\nV:2\nE F|\nV:3\nG A|\nV:4\nc d|\n",
        );
        let export = export_musicxml(source).expect("grouped score should export");
        assert_balanced_xml(&export.musicxml);
        assert_eq!(count(&export.musicxml, "<part id="), 3);
    }

    #[test]
    fn staves_bracket_group_keeps_one_part_per_voice() {
        let source = concat!(
            "X:1\nL:1/4\n%%staves [1 2 3]\n",
            "V:1\nV:2\nV:3\nK:C\n",
            "V:1\nC D|\nV:2\nE F|\nV:3\nG A|\n",
        );
        let export = export_musicxml(source).expect("bracketed score should export");
        assert_eq!(count(&export.musicxml, "<part id="), 3);
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
    fn grace_note_length_modifiers_scale_display_type() {
        // The graphic `<type>` of a grace note must reflect both the count-based
        // base unit and the grace note's written length modifier, matching
        // abc2xml: base 1/8 for a single grace, 1/16 for a group, then multiplied
        // by the note's written length.
        //   {B}      single, no modifier -> 1/8        -> eighth
        //   {B/}     single, half        -> 1/8 * 1/2  -> 16th
        //   {AG}     two graces          -> 1/16 each  -> 16th
        //   {A/G/}   two graces, half    -> 1/16 * 1/2 -> 32nd
        let cases = [
            ("{B}C", "<type>eighth</type>", 1),
            ("{B/}C", "<type>16th</type>", 1),
            ("{AG}C", "<type>16th</type>", 2),
            ("{A/G/}C", "<type>32nd</type>", 2),
        ];
        for (body, expected_type, expected_count) in cases {
            let source = format!("X:1\nT:Grace Length\nM:4/4\nL:1/4\nK:C\n{body}|\n");
            let export = export_musicxml(&source).expect("grace note should export");
            assert_balanced_xml(&export.musicxml);
            assert_eq!(
                count(&export.musicxml, expected_type),
                expected_count,
                "grace body {body} should yield {expected_count}x {expected_type}",
            );
            // Grace notes carry no <duration> element regardless of modifier.
            assert!(
                !export.musicxml.contains("<duration>0</duration>"),
                "grace body {body} must not emit a zero <duration>",
            );
        }
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
        let tune = crate::parse::parse_tune_report_from_document(&document.value)
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
            "[1-",
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
        let tune = crate::parse::parse_tune_report_from_document(&document.value)
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
        let tune = crate::parse::parse_tune_report_from_document(&document.value)
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

    /// Collect the `(step, alter)` of every pitched note in source order.
    fn note_steps_and_alters(xml: &str) -> Vec<(char, i8)> {
        let mut out = Vec::new();
        let mut index = 0;
        while let Some(offset) = xml[index..].find("<step>") {
            let start = index + offset + "<step>".len();
            let end = start + xml[start..].find("</step>").expect("step end");
            let step = xml[start..end].chars().next().expect("step char");
            // The optional <alter> for this pitch lives between the step and the
            // closing </pitch>.
            let pitch_end = end + xml[end..].find("</pitch>").expect("pitch end");
            let alter = xml[end..pitch_end]
                .find("<alter>")
                .map(|alter_offset| {
                    let alter_start = end + alter_offset + "<alter>".len();
                    let alter_end =
                        alter_start + xml[alter_start..].find("</alter>").expect("alter end");
                    xml[alter_start..alter_end]
                        .parse::<i8>()
                        .expect("alter int")
                })
                .unwrap_or(0);
            out.push((step, alter));
            index = pitch_end;
        }
        out
    }

    /// Split the partwise document into its `<part ...>...</part>` bodies.
    fn part_bodies(xml: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut index = 0;
        while let Some(offset) = xml[index..].find("<part ") {
            let start = index + offset;
            let body_start = start + xml[start..].find('>').expect("part open end") + 1;
            let body_end = body_start + xml[body_start..].find("</part>").expect("part end");
            out.push(xml[body_start..body_end].to_owned());
            index = body_end;
        }
        out
    }

    #[test]
    fn inline_key_change_scopes_to_current_voice_only() {
        // V1 switches to C in its third measure; V3 must keep key G, so its F
        // notes stay F# throughout. abc2xml keeps the other voice in key G.
        let source = concat!(
            "X:1\n",
            "M:2/4\n",
            "L:1/8\n",
            "K:G\n",
            "V:1\n",
            "A A A A | A A A A | [K:C] A A A A |\n",
            "V:3\n",
            "F F F F | F F F F | G G G G |\n",
        );
        let export = export_musicxml(source).expect("multi-voice score should export");
        assert_balanced_xml(&export.musicxml);

        let parts = part_bodies(&export.musicxml);
        assert_eq!(parts.len(), 2, "expected two voices/parts");

        // V3 (second part): the eight F notes across the first two measures must
        // all sound F# (alter +1) from key G; the inline [K:C] in V1 must not
        // wipe V3's key signature.
        let v3 = note_steps_and_alters(&parts[1]);
        let f_notes: Vec<(char, i8)> = v3.into_iter().filter(|(step, _)| *step == 'F').collect();
        assert_eq!(f_notes.len(), 8, "V3 should have eight F notes");
        for (step, alter) in f_notes {
            assert_eq!((step, alter), ('F', 1), "V3 F must stay F# under key G");
        }
    }

    #[test]
    fn tie_across_barline_keeps_natural_against_flat_key() {
        // `=B-` ties a natural B across the barline; the stop note must remain
        // natural (alter 0) and not pick up key F's B-flat.
        let source = "X:1\nM:4/4\nL:1/4\nK:F\nA G =B- | B A2 z |\n";
        let export = export_musicxml(source).expect("tied score should export");
        assert_balanced_xml(&export.musicxml);

        let notes = note_steps_and_alters(&export.musicxml);
        let b_notes: Vec<(char, i8)> = notes.into_iter().filter(|(step, _)| *step == 'B').collect();
        assert_eq!(
            b_notes.len(),
            2,
            "expected two B notes (tie start and stop)"
        );
        for (step, alter) in b_notes {
            assert_eq!(
                (step, alter),
                ('B', 0),
                "tied B must stay natural across bar"
            );
        }
    }

    #[test]
    fn tie_across_barline_keeps_flat_against_neutral_key() {
        // `_B-` ties a flat B across the barline in key C; the stop note must
        // remain flat (alter -1) rather than reverting to natural.
        let source = "X:1\nM:4/4\nL:1/4\nK:C\nA G _B- | B A2 z |\n";
        let export = export_musicxml(source).expect("tied score should export");
        assert_balanced_xml(&export.musicxml);

        let notes = note_steps_and_alters(&export.musicxml);
        let b_notes: Vec<(char, i8)> = notes.into_iter().filter(|(step, _)| *step == 'B').collect();
        assert_eq!(
            b_notes.len(),
            2,
            "expected two B notes (tie start and stop)"
        );
        for (step, alter) in b_notes {
            assert_eq!((step, alter), ('B', -1), "tied B must stay flat across bar");
        }
    }
}
