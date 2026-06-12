use crate::model::{
    AccidentalMark, EventAttachments, Fraction, GraceEvent, GraceEventKind, GraceGroupAttachment,
    KeySignatureModel, Part, Pitch,
};

use super::{
    GraceNoteWrite, MeasureSequence, MusicXmlWriter, NoteWrite, TupletNumbers,
    unsupported_grace_warning,
};

impl<'score> MusicXmlWriter<'score> {
    pub(crate) fn write_grace_groups(
        &mut self,
        attachments: &EventAttachments,
        sequence: &MeasureSequence<'score>,
        part: &Part,
        tuplet_numbers: &TupletNumbers,
    ) {
        self.write_grace_group_list(&attachments.grace_groups, sequence, part, tuplet_numbers);
    }

    pub(crate) fn write_after_grace_groups(
        &mut self,
        attachments: &EventAttachments,
        sequence: &MeasureSequence<'score>,
        part: &Part,
        tuplet_numbers: &TupletNumbers,
    ) {
        self.write_grace_group_list(
            &attachments.after_grace_groups,
            sequence,
            part,
            tuplet_numbers,
        );
    }

    fn write_grace_group_list(
        &mut self,
        groups: &[GraceGroupAttachment],
        sequence: &MeasureSequence<'score>,
        part: &Part,
        tuplet_numbers: &TupletNumbers,
    ) {
        for group in groups {
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
        // Slurs that opened before the grace `{` (`({grace}note)`) still bind
        // to the FIRST grace note. Slurs written inside the braces bind to the
        // individual grace events that lowering paired from source order.
        let mut first_note = true;
        let mut first_chord_member = true;
        for event in &group.events {
            match &event.kind {
                GraceEventKind::Note(note) => {
                    let attachments = grace_note_attachments(group, event, first_note);
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
                        &attachments,
                        sequence,
                        part,
                        tuplet_numbers,
                    );
                    first_note = false;
                    first_chord_member = false;
                }
                GraceEventKind::Rest(rest) => {
                    let attachments = grace_note_attachments(group, event, first_note);
                    self.write_note(
                        NoteWrite {
                            pitch: None,
                            rest: Some(rest),
                            duration: grace_base_unit(group.note_count),
                            source: event.source_span,
                            written_accidental: None,
                            attachments: &attachments,
                            chord_member: false,
                            measure_rest: false,
                            grace: true,
                            grace_slash: group.slash.is_some(),
                        },
                        sequence,
                        part,
                        tuplet_numbers,
                    );
                    first_note = false;
                    first_chord_member = false;
                }
                GraceEventKind::Chord(notes) => {
                    let mut event_first_note = true;
                    for note in notes {
                        let attachments = if event_first_note {
                            grace_note_attachments(group, event, first_note)
                        } else {
                            EventAttachments::default()
                        };
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
                            &attachments,
                            sequence,
                            part,
                            tuplet_numbers,
                        );
                        first_note = false;
                        first_chord_member = false;
                        event_first_note = false;
                    }
                }
            }
        }
    }

    fn write_grace_note(
        &mut self,
        grace_note: GraceNoteWrite<'_>,
        attachments: &EventAttachments,
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
                attachments,
                chord_member: grace_note.chord_member,
                measure_rest: false,
                grace: true,
                grace_slash: grace_note.slash,
            },
            sequence,
            part,
            tuplet_numbers,
        );
    }
}

fn grace_note_attachments(
    group: &GraceGroupAttachment,
    event: &GraceEvent,
    first_note: bool,
) -> EventAttachments {
    let mut slurs = Vec::new();
    if first_note {
        slurs.extend(group.slurs.iter().copied());
    }
    slurs.extend(event.slurs.iter().copied());
    EventAttachments {
        slurs,
        ..EventAttachments::default()
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

pub(crate) fn grace_export_pitch(
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
