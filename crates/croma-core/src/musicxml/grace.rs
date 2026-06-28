use crate::model::{
    EventAttachments, Fraction, GraceEvent, GraceEventKind, GraceGroupAttachment, GraceNoteEvent,
    Part,
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
                    let attachments = grace_event_attachments(group, event, first_note, note);
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
                    let attachments = grace_rest_attachments(group, event, first_note);
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
                            unpitched: false,
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
                            grace_event_attachments(group, event, first_note, note)
                        } else {
                            grace_member_attachments(note)
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
                unpitched: sequence.unpitched,
                grace: true,
                grace_slash: grace_note.slash,
            },
            sequence,
            part,
            tuplet_numbers,
        );
        // Direction-class grace decorations follow the grace note; directions
        // before a grace run are principal-note prefixes.
        self.write_harmony_and_directions(attachments, sequence, part);
    }
}

fn grace_event_attachments(
    group: &GraceGroupAttachment,
    event: &GraceEvent,
    first_note: bool,
    note: &GraceNoteEvent,
) -> EventAttachments {
    let mut attachments = grace_rest_attachments(group, event, first_note);
    attachments.decorations = note.decorations.clone();
    attachments
}

fn grace_rest_attachments(
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

fn grace_member_attachments(note: &GraceNoteEvent) -> EventAttachments {
    EventAttachments {
        decorations: note.decorations.clone(),
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
