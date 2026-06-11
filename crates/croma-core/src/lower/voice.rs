use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span};
use crate::lower::accidental::{KeyAccidentalPolicy, MeasureAccidental, key_accidental_policy};
use crate::lower::{abc_broken_rhythm_reference, abc_chord_reference, abc_slur_reference};
use crate::model::{
    Accidental, AccidentalMark, AnnotationPlacementModel, DecorationAttachment,
    DecorationSourceKind, Event, EventAttachments, Fraction, GraceEvent, GraceEventKind,
    GraceGroupAttachment, GraceNoteEvent, LoweredEventAtom, LoweredEventAtomKind, Pitch, RestEvent,
    SlurAttachment, SlurRole, TextAttachment, VoiceId, VoicePropertiesModel,
};
use crate::parse::field::KeySignature;
use crate::syntax::{
    AnnotationPlacement, AttachmentBundle, BrokenRhythmDirection, BrokenRhythmSyntax, ChordSyntax,
    DecorationKind, GraceElementSyntax, GraceGroupSyntax, LengthSyntax, NoteSyntax, OctaveMark,
    OverlaySyntax, QuotedTextKind, RestSyntax, SlurDirection, SlurSyntax, TieSyntax,
    VariantEndingSyntax,
};

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum LoweredEvent {
    Timed(LoweredTimedEvent),
    Untimed(Event),
    Overlay(OverlaySyntax),
    VariantEnding(VariantEndingSyntax),
    KeyChange(crate::model::KeySignatureModel),
    MeterChange(crate::model::MeterModel),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoweredTimedEvent {
    pub(crate) event: LoweredEventAtom,
    pub(crate) line_index: usize,
    pub(crate) source_order: u32,
    pub(crate) alignable: bool,
    pub(crate) attachments: EventAttachments,
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveTuplet {
    pub(crate) pair_id: u32,
    pub(crate) span: Span,
    pub(crate) remaining: u32,
    pub(crate) actual_notes: u32,
    pub(crate) normal_notes: u32,
    pub(crate) multiplier: Fraction,
    pub(crate) groups: Vec<Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompletedTuplet {
    pub(crate) pair_id: u32,
    pub(crate) span: Span,
    pub(crate) actual_notes: u32,
    pub(crate) normal_notes: u32,
    pub(crate) groups: Vec<Vec<usize>>,
}

#[derive(Debug)]
pub(crate) struct LoweringState {
    pub(crate) id: VoiceId,
    pub(crate) properties: VoicePropertiesModel,
    pub(crate) source_span: Span,
    pub(crate) unit: Fraction,
    pub(crate) lowered: Vec<LoweredEvent>,
    pub(crate) time_groups: Vec<Vec<usize>>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) active_tuplets: Vec<ActiveTuplet>,
    pub(crate) pending_broken: Option<PendingBrokenRhythm>,
    /// Whether a timed note group has been emitted in the *current* measure and
    /// can therefore serve as the left operand of a broken-rhythm sign. Reset to
    /// `false` at every barline so a `>`/`<` arriving right after a bar does not
    /// bind backward across it (ABC 2.1 §4.4).
    pub(crate) broken_left_available: bool,
    pub(crate) key_accidentals: Vec<KeyAccidentalPolicy>,
    pub(crate) accidental_state: Vec<MeasureAccidental>,
    pub(crate) pending_ties: Vec<PendingTie>,
    pub(crate) next_tie_id: u32,
    pub(crate) pending_slur_starts: Vec<OpenSlur>,
    pub(crate) open_slurs: Vec<OpenSlur>,
    pub(crate) next_slur_id: u32,
    pub(crate) next_tuplet_id: u32,
    /// Grace groups flushed out of the parser's pending attachments by an
    /// intervening barline (`{g}|`), inline field (`{g}[M:3/4]c`), tie, overlay,
    /// or other flush trigger before their note was parsed (ABC 2.1 §4.20). The
    /// parser emits these as standalone `MusicItem::GraceGroup` items; we buffer
    /// them here and merge them into the next timed event's grace groups so the
    /// grace still attaches to the note it precedes. Dropped at hard boundaries
    /// (barline, voice switch, end of tune) when no timed note follows — a grace
    /// with no following note is void.
    pub(crate) pending_grace_groups: Vec<GraceGroupSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingBrokenRhythm {
    pub(crate) span: Span,
    pub(crate) left_group: Vec<usize>,
    pub(crate) left_multiplier: Fraction,
    pub(crate) right_multiplier: Fraction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingTie {
    pub(crate) event_index: usize,
    /// Pitch signature `(step, octave)` of the start note, captured when the
    /// tie was registered. Used to match the correct member in the next group.
    pub(crate) signature: (char, i8),
    pub(crate) marker: TieSyntax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpenSlur {
    pub(crate) pair_id: u32,
    pub(crate) marker: SlurSyntax,
}

impl LoweringState {
    pub(crate) fn new(
        id: VoiceId,
        properties: VoicePropertiesModel,
        unit: Fraction,
        key: Option<&KeySignature>,
    ) -> Self {
        let source_span = id.span;
        Self {
            id,
            properties,
            source_span,
            unit,
            lowered: Vec::new(),
            time_groups: Vec::new(),
            diagnostics: Vec::new(),
            active_tuplets: Vec::new(),
            pending_broken: None,
            broken_left_available: false,
            key_accidentals: key_accidental_policy(key),
            accidental_state: Vec::new(),
            pending_ties: Vec::new(),
            next_tie_id: 1,
            pending_slur_starts: Vec::new(),
            open_slurs: Vec::new(),
            next_slur_id: 1,
            next_tuplet_id: 1,
            pending_grace_groups: Vec::new(),
        }
    }

    /// Build the lowered attachment bundle for a timed event, prepending any
    /// grace groups that the parser flushed ahead of their note (see
    /// `pending_grace_groups`). Prepending preserves source order: a flushed
    /// grace was written before the note's own attachments, and multiple flushed
    /// graces keep their relative order. The buffer is drained once consumed.
    fn take_timed_attachments(&mut self, bundle: &AttachmentBundle) -> EventAttachments {
        let mut attachments = attachment_bundle_model(bundle);
        if !self.pending_grace_groups.is_empty() {
            let mut graces: Vec<_> = self
                .pending_grace_groups
                .drain(..)
                .map(|grace| grace_group_attachment_model(&grace))
                .collect();
            graces.append(&mut attachments.grace_groups);
            attachments.grace_groups = graces;
        }
        attachments
    }

    pub(crate) fn push_note_group(
        &mut self,
        note: &NoteSyntax,
        line_index: usize,
        source_order: u32,
    ) {
        let octave = lowered_octave(note).saturating_add(voice_octave_shift(&self.properties));
        let written_accidental = note.accidental.map(|accidental| accidental.sign);
        let (effective_accidental, accidental_source) = self.effective_accidental(
            note.pitch.step,
            octave,
            written_accidental,
            note.accidental.map(|accidental| accidental.span),
        );
        let attachments = self.take_timed_attachments(&note.attachments);
        self.push_time_group(
            vec![(
                LoweredEventAtom {
                    kind: LoweredEventAtomKind::Note {
                        step: note.pitch.step.to_ascii_uppercase(),
                        octave,
                        accidental: written_accidental,
                        effective_accidental,
                        accidental_source,
                        chord: false,
                        span: note.span,
                    },
                    duration: self
                        .unit
                        .checked_mul(length_multiplier(note.length.as_ref())),
                },
                true,
                attachments,
            )],
            line_index,
            source_order,
        );
    }

    pub(crate) fn push_rest_group(
        &mut self,
        rest: &RestSyntax,
        line_index: usize,
        source_order: u32,
    ) {
        let attachments = self.take_timed_attachments(&rest.attachments);
        self.push_time_group(
            vec![(
                LoweredEventAtom {
                    kind: LoweredEventAtomKind::Rest {
                        visibility: rest.visibility,
                        span: rest.span,
                    },
                    duration: self
                        .unit
                        .checked_mul(length_multiplier(rest.length.as_ref())),
                },
                false,
                attachments,
            )],
            line_index,
            source_order,
        );
    }

    pub(crate) fn push_chord_group(
        &mut self,
        chord: &ChordSyntax,
        line_index: usize,
        source_order: u32,
    ) {
        if chord.members.is_empty() {
            return;
        }

        let outer_multiplier = length_multiplier(chord.length.as_ref());
        let first_duration = chord.members.first().map(|member| {
            length_multiplier(member.note.length.as_ref()).checked_mul(outer_multiplier)
        });
        if let Some(first_duration) = first_duration
            && chord.members.iter().any(|member| {
                length_multiplier(member.note.length.as_ref()).checked_mul(outer_multiplier)
                    != first_duration
            })
        {
            self.diagnostics
                .push(variable_chord_duration_warning(chord.span));
        }

        // Grace groups flushed ahead of this chord attach to the chord as a whole
        // (its first member), mirroring chord-level attachments. Drain the buffer
        // before the per-member closure, which borrows `self` immutably.
        let mut pending_graces: Vec<GraceGroupAttachment> = self
            .pending_grace_groups
            .drain(..)
            .map(|grace| grace_group_attachment_model(&grace))
            .collect();

        let events = chord
            .members
            .iter()
            .enumerate()
            .map(|(index, member)| {
                let octave = lowered_octave(&member.note)
                    .saturating_add(voice_octave_shift(&self.properties));
                let written_accidental = member.note.accidental.map(|accidental| accidental.sign);
                let (effective_accidental, accidental_source) = self.effective_accidental(
                    member.note.pitch.step,
                    octave,
                    written_accidental,
                    member.note.accidental.map(|accidental| accidental.span),
                );
                let member_multiplier =
                    length_multiplier(member.note.length.as_ref()).checked_mul(outer_multiplier);
                let mut attachments = attachment_bundle_model(&member.note.attachments);
                if index == 0 {
                    attachments.extend(attachment_bundle_model(&chord.attachments));
                    if !pending_graces.is_empty() {
                        let mut graces = std::mem::take(&mut pending_graces);
                        graces.append(&mut attachments.grace_groups);
                        attachments.grace_groups = graces;
                    }
                }
                (
                    LoweredEventAtom {
                        kind: LoweredEventAtomKind::Note {
                            step: member.note.pitch.step.to_ascii_uppercase(),
                            octave,
                            accidental: written_accidental,
                            effective_accidental,
                            accidental_source,
                            chord: index > 0,
                            span: member.note.span,
                        },
                        duration: self.unit.checked_mul(member_multiplier),
                    },
                    index == 0,
                    attachments,
                )
            })
            .collect();
        self.push_time_group(events, line_index, source_order);

        // Register chord-internal tie markers (`[DA-]`) as pending ties keyed to
        // the specific member that carried the `-`. This runs after the group is
        // pushed so the member indices exist and so the tie matches the *next*
        // group (not within the same chord). The just-pushed group's indices map
        // one-to-one to `chord.members` in order.
        if chord.members.iter().any(|member| member.tie.is_some())
            && let Some(group) = self.time_groups.last().cloned()
        {
            for (member, &event_index) in chord.members.iter().zip(group.iter()) {
                if let Some(marker) = member.tie {
                    self.register_pending_tie(event_index, marker);
                }
            }
        }
    }

    pub(crate) fn push_time_group(
        &mut self,
        events: Vec<(LoweredEventAtom, bool, EventAttachments)>,
        line_index: usize,
        source_order: u32,
    ) {
        if events.is_empty() {
            return;
        }

        self.finish_pending_tie_if_group_is_not_note(&events);
        let (group_multiplier, pending_broken) = self.consume_group_multiplier();
        let start_index = self.lowered.len();
        for (mut event, alignable, attachments) in events {
            event.duration = event.duration.checked_mul(group_multiplier);
            self.lowered.push(LoweredEvent::Timed(LoweredTimedEvent {
                event,
                line_index,
                source_order,
                alignable,
                attachments,
            }));
        }
        let group = (start_index..self.lowered.len()).collect::<Vec<_>>();
        if let Some(pending) = pending_broken {
            self.apply_pending_broken_rhythm(&pending, &group);
        }
        self.record_tuplet_group(&group);
        self.attach_pending_slur_starts(&group);
        self.finish_pending_tie_if_possible(&group);
        self.time_groups.push(group);
        // A timed note now exists in the current measure, so a following `>`/`<`
        // has a valid left operand (ABC 2.1 §4.4).
        self.broken_left_available = true;
    }

    fn consume_group_multiplier(&mut self) -> (Fraction, Option<PendingBrokenRhythm>) {
        let mut multiplier = Fraction::one();
        let pending_broken = self.pending_broken.take();
        if let Some(pending) = &pending_broken {
            multiplier = multiplier.checked_mul(pending.right_multiplier);
        }

        for tuplet in &self.active_tuplets {
            if tuplet.remaining > 0 {
                multiplier = multiplier.checked_mul(tuplet.multiplier);
            }
        }
        (multiplier, pending_broken)
    }

    pub(crate) fn apply_broken_rhythm(&mut self, marker: BrokenRhythmSyntax) {
        let (left_multiplier, right_multiplier) = broken_rhythm_multipliers(marker);
        // The left operand must belong to the *current* measure. After a barline
        // (or at the very start of the voice) there is no previous note for a
        // leading broken-rhythm sign, so it is void (ABC 2.1 §4.4).
        let group = match self.time_groups.last() {
            Some(group) if self.broken_left_available => group,
            _ => {
                self.diagnostics
                    .push(broken_rhythm_without_left_warning(marker.span));
                return;
            }
        };

        if self.pending_broken.is_some() {
            self.diagnostics
                .push(overlapping_broken_rhythm_warning(marker.span));
        }
        self.pending_broken = Some(PendingBrokenRhythm {
            span: marker.span,
            left_group: group.clone(),
            left_multiplier,
            right_multiplier,
        });
    }

    fn apply_pending_broken_rhythm(
        &mut self,
        pending: &PendingBrokenRhythm,
        right_group: &[usize],
    ) {
        for index in &pending.left_group {
            if let Some(LoweredEvent::Timed(timed)) = self.lowered.get_mut(*index) {
                timed.event.duration = timed.event.duration.checked_mul(pending.left_multiplier);
            }
        }
        if right_group.is_empty() {
            self.diagnostics
                .push(broken_rhythm_without_right_warning(pending.span));
        }
    }

    pub(crate) fn apply_slur(&mut self, slur: SlurSyntax) {
        match slur.direction {
            SlurDirection::Start => {
                let open = OpenSlur {
                    pair_id: self.next_slur_id,
                    marker: slur,
                };
                self.next_slur_id = self.next_slur_id.saturating_add(1);
                self.pending_slur_starts.push(open);
                self.open_slurs.push(open);
            }
            SlurDirection::End => {
                let open = if self
                    .open_slurs
                    .last()
                    .is_some_and(|open| open.marker.dotted != slur.dotted)
                    && let Some(position) = self
                        .open_slurs
                        .iter()
                        .rposition(|open| open.marker.dotted == slur.dotted)
                {
                    self.diagnostics.push(crossing_slur_warning(slur.span));
                    Some(self.open_slurs.remove(position))
                } else {
                    self.open_slurs.pop()
                };

                if let Some(open) = open {
                    if let Some(event_index) = self.last_note_event_index() {
                        self.attach_slur(event_index, open.pair_id, SlurRole::Stop, slur);
                    } else {
                        self.diagnostics.push(unmatched_slur_warning(slur.span));
                    }
                } else {
                    self.diagnostics.push(unmatched_slur_warning(slur.span));
                }
            }
        }
    }

    /// Install a (possibly mid-tune) key signature. A `K:` field is NOT a bar
    /// line: per ABC 2.1 §11.3 (`%%propagate-accidentals` default `pitch`) an
    /// explicit accidental applies to same-pitch notes until the end of the
    /// bar, so the measure accidental ledger is deliberately left intact.
    pub(crate) fn set_key(&mut self, key: Option<&KeySignature>) {
        self.key_accidentals = key_accidental_policy(key);
    }

    pub(crate) fn finish_pending_broken_at_boundary(&mut self) {
        if let Some(pending) = self.pending_broken.take() {
            self.diagnostics
                .push(broken_rhythm_without_right_warning(pending.span));
        }
        // No note from before the bar can serve as the left operand of a
        // broken-rhythm sign that appears after it (ABC 2.1 §4.4).
        self.broken_left_available = false;
    }

    fn attach_pending_slur_starts(&mut self, group: &[usize]) {
        if self.pending_slur_starts.is_empty() {
            return;
        }
        let Some(event_index) = group
            .iter()
            .copied()
            .find(|index| lowered_timed_note(self.lowered.get(*index)).is_some())
        else {
            return;
        };
        for slur in std::mem::take(&mut self.pending_slur_starts) {
            // `({grace}note)`: when the slur `(` opens BEFORE a leading grace
            // group of the first timed note, the grace is the first note of the
            // slurred series (ABC 2.1 §4.11 + §4.20 construct order), so the slur
            // starts on that grace note. Pick the earliest leading grace group
            // whose span follows the slur `(`. Otherwise (`{grace}(note)`, or no
            // grace) keep the slur on the timed note.
            if let Some(grace_index) = self.leading_grace_group_after(event_index, slur.marker.span)
            {
                self.attach_slur_to_grace(
                    event_index,
                    grace_index,
                    slur.pair_id,
                    SlurRole::Start,
                    slur.marker,
                );
            } else {
                self.attach_slur(event_index, slur.pair_id, SlurRole::Start, slur.marker);
            }
        }
    }

    /// Index, within the timed note's `grace_groups`, of the earliest leading
    /// grace group whose `span.start` follows the given slur-open span (i.e. the
    /// slur `(` was written before the grace `{`). `None` when no grace group
    /// qualifies.
    fn leading_grace_group_after(&self, event_index: usize, slur_span: Span) -> Option<usize> {
        let LoweredEvent::Timed(timed) = self.lowered.get(event_index)? else {
            return None;
        };
        timed
            .attachments
            .grace_groups
            .iter()
            .enumerate()
            .filter(|(_, grace)| grace.span.start > slur_span.start)
            .min_by_key(|(_, grace)| grace.span.start)
            .map(|(index, _)| index)
    }

    fn attach_slur_to_grace(
        &mut self,
        event_index: usize,
        grace_index: usize,
        pair_id: u32,
        role: SlurRole,
        marker: SlurSyntax,
    ) {
        if let Some(LoweredEvent::Timed(timed)) = self.lowered.get_mut(event_index)
            && let Some(grace) = timed.attachments.grace_groups.get_mut(grace_index)
        {
            grace.slurs.push(SlurAttachment {
                pair_id,
                role,
                span: marker.span,
                dotted: marker.dotted,
            });
        }
    }

    pub(crate) fn last_note_event_index(&self) -> Option<usize> {
        self.lowered
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, event)| lowered_timed_note(Some(event)).map(|_| index))
    }

    fn attach_slur(
        &mut self,
        event_index: usize,
        pair_id: u32,
        role: SlurRole,
        marker: SlurSyntax,
    ) {
        if let Some(LoweredEvent::Timed(timed)) = self.lowered.get_mut(event_index) {
            timed.attachments.slurs.push(SlurAttachment {
                pair_id,
                role,
                span: marker.span,
                dotted: marker.dotted,
            });
        }
    }

    pub(crate) fn finish_open_constructs(&mut self) {
        self.finish_pending_broken_at_boundary();
        self.finish_pending_tie_at_boundary(self.source_span);
        self.finish_open_tuplets_at_boundary();
        for slur in std::mem::take(&mut self.open_slurs) {
            self.diagnostics
                .push(unclosed_slur_warning(slur.marker.span));
        }
    }
}

pub(crate) fn lowered_timed_note(event: Option<&LoweredEvent>) -> Option<&LoweredTimedEvent> {
    match event {
        Some(LoweredEvent::Timed(timed))
            if matches!(timed.event.kind, LoweredEventAtomKind::Note { .. }) =>
        {
            Some(timed)
        }
        _ => None,
    }
}

pub(crate) fn is_note_atom(event: LoweredEventAtom) -> bool {
    matches!(event.kind, LoweredEventAtomKind::Note { .. })
}

pub(crate) fn note_signature(kind: LoweredEventAtomKind) -> Option<(char, i8)> {
    match kind {
        LoweredEventAtomKind::Note { step, octave, .. } => Some((step, octave)),
        LoweredEventAtomKind::Rest { .. } => None,
    }
}

fn grace_group_attachment_model(grace: &GraceGroupSyntax) -> GraceGroupAttachment {
    GraceGroupAttachment {
        span: grace.span,
        slash: grace.slash_span,
        note_count: grace
            .elements
            .iter()
            .filter(|element| {
                matches!(
                    element,
                    GraceElementSyntax::Note(_) | GraceElementSyntax::Chord(_)
                )
            })
            .count()
            .try_into()
            .unwrap_or(u32::MAX),
        events: grace
            .elements
            .iter()
            .filter_map(grace_event_model)
            .collect(),
        slurs: Vec::new(),
    }
}

fn attachment_bundle_model(bundle: &AttachmentBundle) -> EventAttachments {
    EventAttachments {
        grace_groups: bundle
            .grace_groups
            .iter()
            .map(grace_group_attachment_model)
            .collect(),
        chord_symbols: bundle
            .chord_symbols
            .iter()
            .map(|text| TextAttachment {
                text: text.text.clone(),
                span: text.span,
                placement: None,
            })
            .collect(),
        annotations: bundle
            .annotations
            .iter()
            .map(|text| TextAttachment {
                text: text.text.clone(),
                span: text.span,
                placement: match text.kind {
                    QuotedTextKind::Annotation(AnnotationPlacement::Above) => {
                        Some(AnnotationPlacementModel::Above)
                    }
                    QuotedTextKind::Annotation(AnnotationPlacement::Below) => {
                        Some(AnnotationPlacementModel::Below)
                    }
                    QuotedTextKind::Annotation(AnnotationPlacement::Left) => {
                        Some(AnnotationPlacementModel::Left)
                    }
                    QuotedTextKind::Annotation(AnnotationPlacement::Right) => {
                        Some(AnnotationPlacementModel::Right)
                    }
                    QuotedTextKind::Annotation(AnnotationPlacement::Free) => {
                        Some(AnnotationPlacementModel::Free)
                    }
                    QuotedTextKind::ChordSymbol => None,
                },
            })
            .collect(),
        decorations: bundle
            .decorations
            .iter()
            .map(|decoration| DecorationAttachment {
                name: decoration.name.clone(),
                span: decoration.span,
                source_kind: match decoration.kind {
                    DecorationKind::Named => DecorationSourceKind::Named,
                    DecorationKind::LegacyNamed => DecorationSourceKind::LegacyNamed,
                    DecorationKind::Shorthand => DecorationSourceKind::Shorthand,
                    DecorationKind::UserDefined => DecorationSourceKind::UserDefined,
                },
            })
            .collect(),
        lyrics: Vec::new(),
        symbols: Vec::new(),
        ties: Vec::new(),
        slurs: Vec::new(),
        tuplets: Vec::new(),
    }
}

fn grace_event_model(element: &GraceElementSyntax) -> Option<GraceEvent> {
    match element {
        GraceElementSyntax::Note(note) => Some(GraceEvent {
            source_span: note.span,
            kind: GraceEventKind::Note(grace_note_event_model(note)),
        }),
        GraceElementSyntax::Rest(rest) => Some(GraceEvent {
            source_span: rest.span,
            kind: GraceEventKind::Rest(RestEvent {
                visibility: rest.visibility,
            }),
        }),
        GraceElementSyntax::Chord(chord) => Some(GraceEvent {
            source_span: chord.span,
            kind: GraceEventKind::Chord(
                chord
                    .members
                    .iter()
                    .map(|member| grace_note_event_model(&member.note))
                    .collect(),
            ),
        }),
        GraceElementSyntax::Malformed(_) => None,
    }
}

fn grace_note_event_model(note: &NoteSyntax) -> GraceNoteEvent {
    let accidental = note.accidental.map(|accidental| accidental.sign);
    GraceNoteEvent {
        pitch: Pitch {
            step: note.pitch.step.to_ascii_uppercase(),
            alter: accidental.map(Accidental::alter).unwrap_or(0),
            octave: lowered_octave(note),
            spelling_source: note.pitch.span,
        },
        written_accidental: accidental.map(|kind| AccidentalMark {
            kind,
            explicit: true,
            courtesy: false,
            source: note
                .accidental
                .map(|accidental| accidental.span)
                .unwrap_or(note.span),
        }),
        length_multiplier: length_multiplier(note.length.as_ref()),
    }
}

fn lowered_octave(note: &NoteSyntax) -> i8 {
    let base_octave: i32 = if note.pitch.step.is_ascii_lowercase() {
        5
    } else {
        4
    };
    // Sum in i32 and clamp: an absurd run of `,`/`'` marks must not overflow
    // the i8 octave (debug panic) — the result saturates at the type bounds.
    let adjustment = note
        .octave_marks
        .iter()
        .map(|mark| match mark.mark {
            OctaveMark::Lower => -1,
            OctaveMark::Raise => 1,
        })
        .sum::<i32>();
    (base_octave + adjustment).clamp(i32::from(i8::MIN), i32::from(i8::MAX)) as i8
}

/// Octave displacement declared by a voice's clef octave suffix
/// (`clef=treble-8` → -1, `+8` → +1, `±15` → ±2), any explicit `octave=`
/// property, and any `middle=` clef modifier (which sets the pitch on the middle
/// staff line and so shifts the written→sounding octave). abc2xml writes the
/// note octaves shifted by the total amount (and marks the clef with a matching
/// `clef-octave-change` for the clef suffix part).
///
/// Oversized inputs clamp instead of overflowing: `octave=` clamps to ±9
/// (abc2xml's effective single-digit domain; malformed values stay ignored)
/// and the combined total clamps to ±12, keeping the later per-note
/// base+shift addition inside i8.
///
/// MUST stay value-for-value identical to the writer's mirrored copy in
/// `to_abc.rs` (which SUBTRACTS this shift to recover written octaves) or
/// every `octave=`/`clef±` voice breaks round-trip.
fn voice_octave_shift(properties: &VoicePropertiesModel) -> i8 {
    let mut shift: i32 = 0;
    if let Some(clef) = properties.clef.as_ref() {
        let clef = clef.text.as_str();
        if clef.contains("-15") {
            shift -= 2;
        } else if clef.contains("+15") {
            shift += 2;
        } else if clef.contains("-8") {
            shift -= 1;
        } else if clef.contains("+8") {
            shift += 1;
        }
    }
    if let Some(octave) = properties.octave.as_ref()
        && let Ok(value) = octave.text.trim().parse::<i64>()
    {
        shift += value.clamp(-9, 9) as i32;
    }
    if let Some(middle) = properties.middle.as_ref() {
        shift += i32::from(middle_octave_shift(middle.text.as_str()));
    }
    shift.clamp(-12, 12) as i8
}

/// Octave shift declared by a `middle=<pitch>` clef modifier, replicating
/// abc2xml's `gtrans` computation (a single pitch letter `[A-Ga-g]` optionally
/// followed by octave marks `,`/`'`). Returns 0 for malformed input.
pub(crate) fn middle_octave_shift(text: &str) -> i8 {
    let text = text.trim();
    let mut chars = text.chars();
    let Some(note) = chars.next() else {
        return 0;
    };
    if !note.is_ascii_alphabetic() || !matches!(note.to_ascii_uppercase(), 'A'..='G') {
        return 0;
    }
    let octstr = &text[note.len_utf8()..];
    if !octstr.chars().all(|ch| matches!(ch, ',' | '\'')) {
        return 0;
    }
    let n_up = note.to_ascii_uppercase();
    let base: i32 = if note.is_ascii_uppercase() { 4 } else { 5 };
    let marks = octstr.chars().count() as i32;
    let octnum = base + if octstr.contains('\'') { marks } else { -marks };
    let gtrans = (if matches!(n_up, 'A' | 'F' | 'D') {
        3
    } else {
        4
    }) - octnum;
    gtrans as i8
}

fn length_multiplier(length: Option<&LengthSyntax>) -> Fraction {
    length
        .map(|length| length.multiplier)
        .unwrap_or_else(Fraction::one)
}

fn broken_rhythm_multipliers(marker: BrokenRhythmSyntax) -> (Fraction, Fraction) {
    let shift = u32::from(marker.count).min(30);
    let denominator = 1u32.checked_shl(shift).unwrap_or(u32::MAX).max(1);
    let long = denominator
        .checked_mul(2)
        .and_then(|value| value.checked_sub(1))
        .unwrap_or(u32::MAX);
    match marker.direction {
        BrokenRhythmDirection::LeftShorter => (
            Fraction::new(1, denominator),
            Fraction::new(long, denominator),
        ),
        BrokenRhythmDirection::RightShorter => (
            Fraction::new(long, denominator),
            Fraction::new(1, denominator),
        ),
    }
}

fn variable_chord_duration_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.chord.variable_duration",
        "Chord members have different durations; members were preserved with their own durations",
        span,
    )
    .with_spec_reference(abc_chord_reference())
    .with_recovery_note(RecoveryNote::new(
        "ABC chord members should use a consistent duration within one chord group.",
    ))
}

fn broken_rhythm_without_left_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.broken_rhythm.missing_left",
        "Broken rhythm marker has no preceding time-bearing note group",
        span,
    )
    .with_spec_reference(abc_broken_rhythm_reference())
    .with_recovery_note(RecoveryNote::new(
        "The marker was preserved and applied only to the following note group when possible.",
    ))
}

fn broken_rhythm_without_right_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.broken_rhythm.missing_right",
        "Broken rhythm marker has no following time-bearing note group",
        span,
    )
    .with_spec_reference(abc_broken_rhythm_reference())
    .with_recovery_note(RecoveryNote::new(
        "The marker was preserved after applying the preceding-side duration change.",
    ))
}

fn overlapping_broken_rhythm_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.broken_rhythm.overlap",
        "Broken rhythm markers overlap before the next note group",
        span,
    )
    .with_spec_reference(abc_broken_rhythm_reference())
    .with_recovery_note(RecoveryNote::new(
        "The later marker determines the following-side duration change.",
    ))
}

fn unmatched_slur_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.unmatched_slur",
        "Slur end has no matching open slur",
        span,
    )
    .with_spec_reference(abc_slur_reference())
    .with_recovery_note(RecoveryNote::new(
        "The unmatched slur marker was preserved and skipped during lowering.",
    ))
}

fn crossing_slur_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.crossing_slur",
        "Slur close crosses another open slur",
        span,
    )
    .with_spec_reference(abc_slur_reference())
    .with_recovery_note(RecoveryNote::new(
        "The slur markers were preserved and paired by nearest compatible marker.",
    ))
}

fn unclosed_slur_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.unclosed_slur",
        "Slur start has no matching close slur",
        span,
    )
    .with_spec_reference(abc_slur_reference())
    .with_recovery_note(RecoveryNote::new(
        "The open slur marker was preserved and skipped during lowering.",
    ))
}

#[cfg(test)]
#[path = "voice_tests.rs"]
mod tests;
