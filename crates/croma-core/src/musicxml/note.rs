use std::collections::HashMap;

use crate::model::{
    AccidentalMark, ChordEvent, EventAttachments, Fraction, MeterModel, Part, Pitch, RestEvent,
    RestVisibility, TieRole, TimedEventKind, TimelineEventKind, TupletRole,
};

use super::engrave::beam::{self, BeamInput, BeamSegment, Meter};
use super::engrave::stem;
use super::{
    FractionExt, MeasureSequence, MusicXmlWriter, NoteWrite, SequenceEvent, TimeModification,
    TupletNumbers, unsupported_note_type_warning, unsupported_tuplet_time_modification_warning,
    variable_chord_duration_export_warning,
};

/// The engraving hints computed for one sequence, consumed by the note writer.
#[derive(Default)]
pub(crate) struct SequencePlan {
    /// `<beam>` segments per beamable event, keyed by index in `sequence.events`.
    beams: HashMap<usize, Vec<BeamSegment>>,
    /// Tuplet-bracket decision per tuplet pair (`true` = show bracket).
    brackets: HashMap<u32, bool>,
    /// Stem direction (`true` = up) per stemmed event index.
    stems: HashMap<usize, bool>,
    /// Multi-voice rest `<display-step>`/`<display-octave>` per rest event index.
    rest_display: HashMap<usize, (char, i8)>,
}

/// The per-event engraving hints threaded into the note writer.
#[derive(Clone, Copy)]
pub(crate) struct EventEngrave<'a> {
    beams: &'a [BeamSegment],
    stem: Option<bool>,
    rest_display: Option<(char, i8)>,
}

impl EventEngrave<'_> {
    pub(crate) const EMPTY: EventEngrave<'static> = EventEngrave {
        beams: &[],
        stem: None,
        rest_display: None,
    };
}

/// MusicXML ticks (MuseScore convention: quarter = 480, whole = 1920) for a
/// whole-note-unit fraction. Used by the beam engine's grouping table.
fn frac_ticks(f: Fraction) -> i64 {
    i64::from(f.numerator) * 1920 / i64::from(f.denominator.max(1))
}

/// Written beam/flag count for a MusicXML note type (eighth = 1, 16th = 2, ...);
/// `0` for quarter-or-longer, which never beams.
fn beam_flag_count(note_type: &str) -> u8 {
    match note_type {
        "eighth" => 1,
        "16th" => 2,
        "32nd" => 3,
        "64th" => 4,
        "128th" => 5,
        "256th" => 6,
        "512th" => 7,
        "1024th" => 8,
        _ => 0,
    }
}

/// The staff-middle-line distances (one per pitch, `>0` = below the line) of a
/// time-advancing event under a clef, for stem direction. Rests yield an empty list.
fn event_distances(event: &SequenceEvent<'_>, clef: Option<&str>) -> Vec<i32> {
    match event {
        SequenceEvent::Timed(timed) => match &timed.kind {
            TimedEventKind::Note(note) => {
                vec![stem::middle_distance(
                    clef,
                    note.pitch.step,
                    note.pitch.octave,
                )]
            }
            TimedEventKind::Chord(chord) => chord
                .members
                .iter()
                .map(|member| stem::middle_distance(clef, member.pitch.step, member.pitch.octave))
                .collect(),
            _ => Vec::new(),
        },
        SequenceEvent::Overlay(overlay) => match overlay.kind {
            TimelineEventKind::Note { step, octave, .. } => {
                vec![stem::middle_distance(clef, step, octave)]
            }
            _ => Vec::new(),
        },
    }
}

/// The beam-grouping meter grid for the currently active meter, or `None` for a free
/// or unparseable meter (beams are then not computed).
fn meter_grid(meter: Option<&MeterModel>) -> Option<Meter> {
    let meter = meter?;
    if meter.free_meter {
        return None;
    }
    let display = meter.display.trim();
    let (numerator, denominator) = match display {
        "C" => (4, 4),
        "C|" => (2, 2),
        _ => {
            let (num, den) = display.split_once('/')?;
            (num.trim().parse().ok()?, den.trim().parse().ok()?)
        }
    };
    Some(Meter {
        numerator,
        denominator,
    })
}

impl<'score> MusicXmlWriter<'score> {
    pub(crate) fn write_sequence(
        &mut self,
        sequence: &MeasureSequence<'score>,
        part: &Part,
    ) -> Fraction {
        let mut cursor = Fraction::zero();
        let mut last_onset = Fraction::zero();
        let tuplet_numbers = sequence_tuplet_numbers(sequence);
        let plan = self.build_engrave_plan(sequence);
        self.tuplet_brackets = plan.brackets;
        for (index, event) in sequence.events.iter().enumerate() {
            let onset = event.onset();
            let is_chord_member = event.is_chord_member();
            if is_chord_member && onset == last_onset {
                self.write_event(
                    event,
                    sequence,
                    part,
                    &tuplet_numbers,
                    true,
                    EventEngrave::EMPTY,
                );
                continue;
            }
            if let Some((clef, pre_backup, cursor_forward)) = event.clef_cursor_script() {
                let attachments = event.attachments();
                self.write_harmony_and_directions(attachments, sequence, part);
                self.write_grace_groups(attachments, sequence, part, &tuplet_numbers);
                self.write_backup(pre_backup);
                cursor = cursor.subtract(pre_backup);
                self.write_mid_tune_clef(clef, sequence.staff, part);
                self.write_forward(cursor_forward);
                cursor = cursor.checked_add(cursor_forward);
                continue;
            }
            if cursor.less_than(onset) {
                self.write_forward(onset.subtract(cursor));
                cursor = onset;
            } else if onset.less_than(cursor) {
                self.write_backup(cursor.subtract(onset));
                cursor = onset;
            }
            let engrave = EventEngrave {
                beams: plan.beams.get(&index).map_or(&[][..], Vec::as_slice),
                stem: plan.stems.get(&index).copied(),
                rest_display: plan.rest_display.get(&index).copied(),
            };
            self.write_event(event, sequence, part, &tuplet_numbers, false, engrave);
            if event.advances_time() {
                cursor = cursor.checked_add(event.duration());
                last_onset = onset;
            }
        }
        cursor
    }

    /// Compute the engraving plan (beams, tuplet brackets, stems, multi-voice rest
    /// placement) for one sequence. Each hint is populated only when its option (or a
    /// dependent option) is enabled; a beam grouping is always computed when needed
    /// because tuplet brackets and beamed stem direction depend on it.
    fn build_engrave_plan(&self, sequence: &MeasureSequence<'score>) -> SequencePlan {
        let mut plan = SequencePlan::default();
        let options = self.options;
        let want_tuplet_default =
            options.tuplet_display == crate::options::TupletDisplay::EngravingDefault;
        // Slur/tie placement and stem emission all need the resolved stem direction.
        let want_stems = options.stems || options.slur_placement || options.tie_orientation;
        if !options.beams && !want_tuplet_default && !want_stems && !options.rest_placement {
            return plan;
        }

        let clef = sequence.clef_text.as_deref();
        let mut unit_indices = Vec::new();
        let mut inputs = Vec::new();
        let mut distances = Vec::new();
        let mut has_stem = Vec::new();
        for (index, event) in sequence.events.iter().enumerate() {
            if !event.advances_time() || event.is_chord_member() {
                continue;
            }
            let attachments = event.attachments();
            let time_modification = TimeModification::composite(&attachments.tuplets)
                .ok()
                .flatten();
            let spelling = note_spelling(event.duration(), time_modification);
            let is_rest = event.is_rest();
            inputs.push(BeamInput {
                rtick: frac_ticks(event.onset()),
                dur: frac_ticks(event.duration()),
                beams: beam_flag_count(spelling.note_type),
                is_rest,
                tuplet_id: attachments
                    .tuplets
                    .iter()
                    .map(|tuplet| tuplet.pair_id)
                    .max(),
            });
            distances.push(event_distances(event, clef));
            has_stem.push(!is_rest && stem::note_type_has_stem(spelling.note_type));
            unit_indices.push(index);
        }

        // The beam grouping (empty groups under a free/unknown meter → per-note stems).
        let beam_plan =
            meter_grid(self.active_meter.as_ref()).map(|meter| beam::plan(&inputs, meter));
        let groups: &[Vec<usize>] = beam_plan.as_ref().map_or(&[], |plan| &plan.groups);

        if options.beams
            && let Some(beam_plan) = &beam_plan
        {
            for (unit, segments) in beam_plan.segments.iter().enumerate() {
                if !segments.is_empty() {
                    plan.beams.insert(unit_indices[unit], segments.clone());
                }
            }
        }

        if want_tuplet_default {
            let mut pairs: Vec<u32> = inputs.iter().filter_map(|input| input.tuplet_id).collect();
            pairs.sort_unstable();
            pairs.dedup();
            for pair in pairs {
                plan.brackets
                    .insert(pair, beam::tuplet_shows_bracket(&inputs, groups, pair));
            }
        }

        if want_stems {
            // Map each unit to the beam group it belongs to (positions into `inputs`).
            let mut group_of: Vec<Option<&Vec<usize>>> = vec![None; inputs.len()];
            for group in groups {
                for &pos in group {
                    group_of[pos] = Some(group);
                }
            }
            for pos in 0..inputs.len() {
                if !has_stem[pos] || distances[pos].is_empty() {
                    continue;
                }
                let beam_distances = group_of[pos].map(|group| {
                    group
                        .iter()
                        .flat_map(|&member| distances[member].iter().copied())
                        .collect::<Vec<_>>()
                });
                let up = stem::stem_up(
                    &distances[pos],
                    beam_distances.as_deref(),
                    sequence.staff_multivoice,
                    sequence.staff_voice_slot,
                );
                plan.stems.insert(unit_indices[pos], up);
            }
        }

        if options.rest_placement && sequence.staff_multivoice {
            for pos in 0..inputs.len() {
                if inputs[pos].is_rest {
                    plan.rest_display.insert(
                        unit_indices[pos],
                        stem::rest_display(clef, sequence.staff_voice_slot),
                    );
                }
            }
        }

        plan
    }

    fn write_event(
        &mut self,
        event: &SequenceEvent<'score>,
        sequence: &MeasureSequence<'score>,
        part: &Part,
        tuplet_numbers: &TupletNumbers,
        chord_member: bool,
        engrave: EventEngrave<'_>,
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
                            measure_rest: false,
                            unpitched: sequence.unpitched,
                            grace: false,
                            grace_slash: false,
                            chord_tuplet_time_modification: None,
                        },
                        sequence,
                        part,
                        tuplet_numbers,
                        engrave,
                    );
                }
                TimedEventKind::Chord(chord) => {
                    self.write_chord(chord, attachments, sequence, part, tuplet_numbers, engrave);
                }
                TimedEventKind::Rest(rest) => {
                    if timed.attachments.musicxml_forward {
                        self.write_forward(timed.duration);
                        return;
                    }
                    self.write_note(
                        NoteWrite {
                            pitch: None,
                            rest: Some(rest),
                            duration: timed.duration,
                            source: timed.source,
                            written_accidental: None,
                            attachments,
                            chord_member: false,
                            measure_rest: sequence.is_full_measure_rest(
                                timed.onset,
                                timed.duration,
                                rest,
                            ),
                            unpitched: false,
                            grace: false,
                            grace_slash: false,
                            chord_tuplet_time_modification: None,
                        },
                        sequence,
                        part,
                        tuplet_numbers,
                        engrave,
                    );
                }
                TimedEventKind::Spacer
                | TimedEventKind::Barline(_)
                | TimedEventKind::RepeatEnding(_)
                | TimedEventKind::RepeatEndingClose(_) => {}
                // Emission lands in the mid-tune attributes pass (write_event
                // is reached once measure_sequences admits these).
                TimedEventKind::KeyChange(key) => {
                    self.active_key = Some(key.clone());
                    self.write_mid_tune_key(key);
                }
                TimedEventKind::MeterChange(meter) => {
                    self.active_meter = Some(meter.clone());
                    self.write_mid_tune_meter(meter);
                }
                TimedEventKind::ClefChange(clef) => {
                    if let Some(cursor_back) = clef.musicxml_cursor_back {
                        self.write_backup(cursor_back);
                        self.write_mid_tune_clef(clef, sequence.staff, part);
                        self.write_forward(cursor_back);
                    } else {
                        self.write_mid_tune_clef(clef, sequence.staff, part);
                    }
                }
                TimedEventKind::TempoChange(tempo) => self.write_tempo_direction(tempo),
                TimedEventKind::SectionLabel(label) => self.write_rehearsal_direction(label),
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
                            measure_rest: false,
                            unpitched: sequence.unpitched,
                            grace: false,
                            grace_slash: false,
                            chord_tuplet_time_modification: None,
                        },
                        sequence,
                        part,
                        tuplet_numbers,
                        engrave,
                    );
                }
                TimelineEventKind::Rest { visibility, .. } => {
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
                            measure_rest: sequence.is_full_measure_rest(
                                timed.onset,
                                timed.duration,
                                &rest,
                            ),
                            unpitched: false,
                            grace: false,
                            grace_slash: false,
                            chord_tuplet_time_modification: None,
                        },
                        sequence,
                        part,
                        tuplet_numbers,
                        engrave,
                    );
                }
                TimelineEventKind::KeyChange(_)
                | TimelineEventKind::MeterChange(_)
                | TimelineEventKind::ClefChange(_)
                | TimelineEventKind::TempoChange(_)
                | TimelineEventKind::SectionLabel(_) => {}
                TimelineEventKind::Spacer
                | TimelineEventKind::Barline { .. }
                | TimelineEventKind::VariantEnding { .. }
                | TimelineEventKind::VariantEndingClose { .. } => {}
            },
        }
        self.write_after_grace_groups(attachments, sequence, part, tuplet_numbers);
    }

    fn write_chord(
        &mut self,
        chord: &ChordEvent,
        event_attachments: &EventAttachments,
        sequence: &MeasureSequence<'score>,
        part: &Part,
        tuplet_numbers: &TupletNumbers,
        engrave: EventEngrave<'_>,
    ) {
        let variable_durations = chord
            .members
            .iter()
            .any(|member| member.duration != chord.members[0].duration);
        if variable_durations {
            self.diagnostics
                .push(variable_chord_duration_export_warning(chord.source_span));
        }
        // The whole chord shares one tuplet, recorded on the head's (event)
        // attachments. Members carry no `tuplets`, so without inheriting this they
        // would emit no `<time-modification>` and drop the ratio. The head derives
        // it from `event_attachments` itself; members get it as the override.
        let chord_tuplet_time_modification =
            TimeModification::composite(&event_attachments.tuplets)
                .ok()
                .flatten()
                // A measured tremolo on a chord carries its `<time-modification>` with no
                // `<tuplet>`; members inherit it from the chord's tremolo decoration too.
                .or_else(|| tremolo_time_modification_for(event_attachments));
        for (index, member) in chord.members.iter().enumerate() {
            let attachments = if index == 0 {
                event_attachments.clone()
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
                    measure_rest: false,
                    unpitched: sequence.unpitched,
                    grace: false,
                    grace_slash: false,
                    chord_tuplet_time_modification: (index > 0)
                        .then_some(chord_tuplet_time_modification)
                        .flatten(),
                },
                sequence,
                part,
                tuplet_numbers,
                // The `<beam>` belongs to the chord as a unit and is written on the
                // head note only; members carry `<chord/>` and no beam. Every chord
                // note shares the chord's stem direction.
                if index == 0 {
                    engrave
                } else {
                    EventEngrave {
                        beams: &[],
                        rest_display: None,
                        ..engrave
                    }
                },
            );
        }
    }

    pub(crate) fn write_note(
        &mut self,
        note: NoteWrite<'_>,
        sequence: &MeasureSequence<'score>,
        part: &Part,
        tuplet_numbers: &TupletNumbers,
        engrave: EventEngrave<'_>,
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
            if note.unpitched {
                self.write_unpitched(pitch);
            } else {
                self.write_pitch(pitch);
            }
        } else {
            self.write_rest_element(note.measure_rest, engrave.rest_display);
        }
        // A chord member inherits the chord's tuplet ratio (it has no `tuplets` of
        // its own — the bracket lives on the head); every other note derives the
        // ratio from its own `tuplets`. Either way the ratio drives both the
        // written-duration spelling and the `<time-modification>` element below,
        // while the `<tuplet>` notation bracket still comes only from
        // `attachments.tuplets` (so members emit the ratio but not the bracket).
        let explicit_time_modification = match note.chord_tuplet_time_modification {
            Some(time_modification) => Some(time_modification),
            None => match TimeModification::composite(&note.attachments.tuplets) {
                // A measured-tremolo note carries a `<time-modification>` with no
                // `<tuplet>`; recover it from the tremolo decoration's carrier so
                // the written value re-doubles and the element re-emits.
                Ok(None) => tremolo_time_modification_for(note.attachments),
                Ok(time_modification) => time_modification,
                Err(()) => {
                    self.diagnostics
                        .push(unsupported_tuplet_time_modification_warning(note.source));
                    None
                }
            },
        };
        let spelling = note_spelling(note.duration, explicit_time_modification);
        // A rest whose duration has no plain note-type would otherwise make
        // `note_spelling` FABRICATE a tuplet `<time-modification>` (e.g. a 5/2-quarter
        // rest gaining a phantom 8:5) — but a foreign rest carries no real tuplet and
        // MusicXML allows a bare rest with only `<duration>`. So omit the synthesized
        // `<type>`/`<time-modification>` for ANY such rest (not just full-measure
        // rests). A rest with a REAL tuplet has `explicit_time_modification = Some`,
        // so it is spared and keeps its genuine ratio. Notes are untouched.
        let omit_inexpressible_rest_spelling = note.rest.is_some()
            && explicit_time_modification.is_none()
            && (spelling.unsupported || spelling.time_modification.is_some());
        if spelling.unsupported && !omit_inexpressible_rest_spelling {
            self.diagnostics
                .push(unsupported_note_type_warning(note.source, note.duration));
        }
        if !note.grace {
            let duration = self.duration_to_divisions(note.duration, note.source);
            self.xml.text_element("duration", &duration.to_string());
        }
        self.write_ties(&note.attachments.ties);
        if note.pitch.is_some()
            && !note.grace
            && let Some(instrument) = &note.attachments.instrument
        {
            self.xml
                .empty("instrument", &[("id", instrument.id.as_str())]);
        }
        self.xml.text_element("voice", &sequence.voice_number);
        if !omit_inexpressible_rest_spelling {
            self.xml.text_element("type", spelling.note_type);
            for _ in 0..spelling.dots {
                self.xml.empty("dot", &[]);
            }
        }
        if let Some(accidental) = note.written_accidental
            && accidental.explicit
            && self.score.accidental_policy.preserve_explicit_accidentals
        {
            self.xml
                .text_element("accidental", accidental.kind.musicxml_name());
        }
        let time_modification = if omit_inexpressible_rest_spelling {
            None
        } else {
            explicit_time_modification.or(spelling.time_modification)
        };
        if let Some(time_modification) = time_modification {
            self.write_time_modification(time_modification);
        }
        if self.options.stems
            && let Some(up) = engrave.stem
        {
            self.xml
                .text_element("stem", if up { "up" } else { "down" });
        }
        if part.staves.len() > 1 {
            self.xml
                .text_element("staff", &sequence.staff.value.to_string());
        }
        for segment in engrave.beams {
            self.xml.text_element_attrs(
                "beam",
                &[("number", &segment.level.to_string())],
                segment.text.as_str(),
            );
        }
        let ordered_attachments;
        let notation_attachments = if note.attachments.tuplets.len() > 1 {
            ordered_attachments = ordered_tuplet_notation_attachments(note.attachments);
            &ordered_attachments
        } else {
            note.attachments
        };
        self.write_notations(
            notation_attachments,
            time_modification,
            tuplet_numbers,
            sequence,
            engrave.stem,
        );
        self.write_lyrics(&note.attachments.lyrics, &sequence.slur_voice_key);
        self.xml.end("note");
    }

    /// Emit a `<rest>` element, optionally with the multi-voice `<display-step>` /
    /// `<display-octave>` position (present only under the rest-placement option).
    fn write_rest_element(&mut self, measure_rest: bool, display: Option<(char, i8)>) {
        let measure_attr: &[(&str, &str)] = if measure_rest {
            &[("measure", "yes")]
        } else {
            &[]
        };
        match display {
            Some((step, octave)) => {
                self.xml.start("rest", measure_attr);
                self.xml.text_element("display-step", &step.to_string());
                self.xml.text_element("display-octave", &octave.to_string());
                self.xml.end("rest");
            }
            None => self.xml.empty("rest", measure_attr),
        }
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

    fn write_unpitched(&mut self, pitch: &Pitch) {
        self.xml.start("unpitched", &[]);
        self.xml
            .text_element("display-step", &pitch.step.to_string());
        self.xml
            .text_element("display-octave", &pitch.octave.to_string());
        self.xml.end("unpitched");
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
}

/// The measured-tremolo `<time-modification>` carried by one of a note's tremolo
/// decorations (`musicxml-tremolo-{type}-{marks}-tm-{actual}-{normal}`), if any.
fn tremolo_time_modification_for(attachments: &EventAttachments) -> Option<TimeModification> {
    attachments
        .decorations
        .iter()
        .find_map(|decoration| super::notation::tremolo_time_modification(&decoration.name))
}

fn ordered_tuplet_notation_attachments(attachments: &EventAttachments) -> EventAttachments {
    let mut ordered = attachments.clone();
    ordered.tuplets.sort_by(|a, b| {
        let role_rank = |role| match role {
            TupletRole::Start => 0u8,
            TupletRole::Continue => 1,
            TupletRole::Stop => 2,
        };
        role_rank(a.role)
            .cmp(&role_rank(b.role))
            .then_with(|| match a.role {
                TupletRole::Stop => b.pair_id.cmp(&a.pair_id),
                TupletRole::Start | TupletRole::Continue => a.pair_id.cmp(&b.pair_id),
            })
    });
    ordered
}

fn sequence_tuplet_numbers(sequence: &MeasureSequence<'_>) -> TupletNumbers {
    let mut numbers = TupletNumbers::default();
    let mut active = Vec::<(u32, u32)>::new();

    for event in &sequence.events {
        let mut starts = event
            .attachments()
            .tuplets
            .iter()
            .filter(|tuplet| tuplet.role == TupletRole::Start)
            .collect::<Vec<_>>();
        starts.sort_by_key(|tuplet| tuplet.pair_id);
        for tuplet in starts {
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

        let mut stops = event
            .attachments()
            .tuplets
            .iter()
            .filter(|tuplet| tuplet.role == TupletRole::Stop)
            .collect::<Vec<_>>();
        stops.sort_by_key(|tuplet| std::cmp::Reverse(tuplet.pair_id));
        for tuplet in stops {
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

    // An explicit tuplet time-modification spells the WRITTEN (de-tupletted)
    // duration: sounding x actual/normal (ABC 2.1 §4.13). Spelling the
    // sounding duration plainly first produced internally-inconsistent
    // type+dots/<time-modification> pairs (a 6/8 quadruplet member became a
    // dotted 16th under 4:3 instead of an eighth).
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
            name: "maxima",
            fraction: Fraction {
                numerator: 8,
                denominator: 1,
            },
        },
        NoteTypeCandidate {
            name: "long",
            fraction: Fraction {
                numerator: 4,
                denominator: 1,
            },
        },
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
