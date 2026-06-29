use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span};
use crate::lower::accidental::{KeyAccidentalPolicy, MeasureAccidental, key_accidental_policy};
use crate::lower::{abc_broken_rhythm_reference, abc_chord_reference, abc_slur_reference};
use crate::model::{
    Accidental, AccidentalMark, AlignedLyric, AnnotationPlacementModel, BarlineKind,
    DecorationAttachment, DecorationSourceKind, Event, EventAttachments, Fraction, GraceEvent,
    GraceEventKind, GraceGroupAttachment, GraceNoteEvent, HarmonyKindText, LoweredEventAtom,
    LoweredEventAtomKind, MusicXmlInstrumentRef, Pitch, RepeatEndingCloseModel, RestEvent,
    SlurAttachment, SlurRole, TextAttachment, TupletAttachment, TupletRole, VoiceId,
    VoicePropertiesModel,
};
use crate::parse::field::KeySignature;
use crate::syntax::{
    AnnotationPlacement, AttachmentBundle, BrokenRhythmDirection, BrokenRhythmSyntax, ChordSyntax,
    DecorationKind, DecorationSyntax, GraceElementSyntax, GraceGroupSyntax, LengthSyntax,
    NoteSyntax, OctaveMark, OverlaySyntax, QuotedTextKind, QuotedTextSyntax, RestSyntax,
    SlurDirection, SlurSyntax, TieSyntax, VariantEndingSyntax,
};

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum LoweredEvent {
    Timed(LoweredTimedEvent),
    Untimed(Event),
    Overlay(OverlaySyntax),
    VariantEnding(VariantEndingSyntax),
    VariantEndingClose(RepeatEndingCloseModel),
    KeyChange(crate::model::KeySignatureModel),
    MeterChange(crate::model::MeterModel),
    ClefChange(crate::model::ClefChangeModel),
    TempoChange(crate::model::TempoModel),
    /// A body/inline `P:` section label plus its source span. The span drives
    /// the writer's same-onset event ordering (it sorts zero-duration events by
    /// source position), so it must be the real `P:` position — a default span
    /// would sort the label before earlier same-onset changes and break the
    /// `<rehearsal>` round-trip.
    SectionLabel {
        label: String,
        span: Span,
    },
    /// MusicXML-origin source measure label for the current ABC measure.
    MeasureNumber {
        display_number: String,
    },
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
    pub(crate) initial_properties: VoicePropertiesModel,
    pub(crate) properties: VoicePropertiesModel,
    pub(crate) source_span: Span,
    pub(crate) initial_key: Option<crate::model::KeySignatureModel>,
    pub(crate) initial_meter: Option<crate::model::MeterModel>,
    pub(crate) unit: Fraction,
    /// The meter in effect for THIS voice (an inline `[M:..]` scopes to the
    /// current voice, like abc2xml; a standalone `M:` line updates every
    /// voice). Drives multi-measure-rest expansion.
    pub(crate) meter_duration: Option<Fraction>,
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
    /// The full prevailing key for this voice (tonic/mode/accidentals), kept so a
    /// tonic-less modifying `K:^F` (ABC 2.1 §3.1.14 "modified by adding
    /// accidentals") can inherit the current tonic+mode and add its accidental
    /// instead of being dropped. `key_accidentals` alone is the resolved policy
    /// and cannot reconstruct the tonic for `key_fifths`.
    pub(crate) current_key: Option<KeySignature>,
    pub(crate) accidental_state: Vec<MeasureAccidental>,
    pub(crate) pending_ties: Vec<PendingTie>,
    pub(crate) next_tie_id: u32,
    pub(crate) pending_slur_starts: Vec<OpenSlur>,
    pub(crate) open_slurs: Vec<OpenSlur>,
    pub(crate) pending_grace_slur_stops: Vec<PendingGraceSlurStop>,
    pub(crate) next_slur_id: u32,
    pub(crate) next_tuplet_id: u32,
    /// Grace groups flushed out of the parser's pending attachments by an
    /// intervening barline (`{g}|`), inline field (`{g}[M:3/4]c`), tie, overlay,
    /// or other flush trigger before their note was parsed (ABC 2.1 §4.20). The
    /// parser emits these as standalone `MusicItem::GraceGroup` items; we buffer
    /// them here and merge them into the next timed event's grace groups so the
    /// grace still attaches to the note it precedes. If a hard boundary follows
    /// and the current measure already has a previous timed note, the group is
    /// instead resolved as an after-grace/trill termination on that note.
    pub(crate) pending_grace_groups: Vec<PendingGraceGroup>,
    /// Quoted chord symbols flushed out of the parser's pending attachments by
    /// an intervening barline (`"F"| c`), line end, or other flush trigger
    /// before their note was parsed. ABC 2.1 §4.18 binds a chord symbol to the
    /// note it precedes and neither a barline nor a code line break voids it,
    /// so the symbol is buffered and merged into the next timed event. A
    /// leftover at the end of the voice surfaces as a
    /// `abc.music.dangling_quoted_text` warning, never a silent drop.
    pub(crate) pending_chord_symbols: Vec<TextAttachment>,
    /// Annotations in the same flushed-ahead situation as
    /// [`Self::pending_chord_symbols`] (ABC 2.1 §4.19: an annotation positions
    /// relative to the following note).
    pub(crate) pending_annotations: Vec<QuotedTextSyntax>,
    /// Decorations (`!f!`, `!trill!`, ...) in the same flushed-ahead situation
    /// (ABC 2.1 §4.14: a decoration precedes the symbol it decorates).
    pub(crate) pending_decorations: Vec<DecorationSyntax>,
    /// Croma MusicXML-origin `[I:croma-note-instrument ...]` carrier waiting for
    /// the next timed note/rest/chord event.
    pub(crate) pending_musicxml_instrument: Option<MusicXmlInstrumentRef>,
    /// Croma MusicXML-origin `[I:croma-harmony-text ...]` carrier waiting for
    /// the next chord-symbol attachment. `None` means no carrier is pending; a
    /// pending carrier decodes to a textless or explicit-text `<kind>` provenance.
    pub(crate) pending_musicxml_harmony_text: Option<HarmonyKindText>,
    /// Croma MusicXML-origin `[I:croma-lyric-extend ...]` carriers waiting for
    /// the next timed note/rest/chord event. `w:` lyric alignment applies them
    /// to the matching verse syllable after the music body has lowered.
    pub(crate) pending_musicxml_lyric_extends: Vec<u32>,
    /// Croma MusicXML-origin `[I:croma-lyric-duplicate ...]` carriers waiting
    /// for the next timed note/rest/chord event. `w:` carries the primary
    /// syllable; these carry same-note same-verse siblings ABC cannot spell.
    pub(crate) pending_musicxml_lyric_duplicates: Vec<AlignedLyric>,
    /// Croma MusicXML-origin `[I:croma-musicxml-forward]` carrier waiting for
    /// the next timed rest event. The rest advances ABC time; MusicXML emits it
    /// back as `<forward>` instead of `<note><rest>`.
    pub(crate) pending_musicxml_forward: bool,
    /// Croma MusicXML-origin tuplets that ABC cannot spell directly, waiting for
    /// the next timed event.
    pub(crate) pending_musicxml_tuplets: Vec<TupletAttachment>,
    /// Source carrier pair id -> lowering pair id for private MusicXML tuplets.
    pub(crate) musicxml_tuplet_pair_ids: Vec<(u32, u32)>,
    /// Croma MusicXML-origin source backup amount before this voice sequence.
    /// ABC cannot spell a MusicXML `<backup>` that overshoots the prior cursor,
    /// so the first timed event carries it back to the MusicXML writer.
    pub(crate) pending_musicxml_sequence_backup: Option<Fraction>,
    /// Croma MusicXML-origin `[I:croma-after-grace]` carrier waiting for the
    /// next flushed grace group. It disambiguates MusicXML after-grace notes
    /// from ABC leading graces that intentionally cross a barline.
    pub(crate) pending_musicxml_after_grace: bool,
    /// Croma MusicXML-origin `[I:croma-barline-style ...]` carrier waiting for
    /// the next parsed barline. Used for MusicXML bar styles ABC cannot spell
    /// natively, currently `dashed`.
    pub(crate) pending_musicxml_barline_kind: Option<BarlineKind>,
    /// Croma MusicXML-origin `[I:croma-meter-restatement]` carrier waiting for
    /// the next `[M:...]` field in this voice.
    pub(crate) pending_musicxml_meter_restatement: bool,
    /// Croma MusicXML-origin `[I:croma-key-restatement]` carrier waiting for the
    /// next `[K:...]` field in this voice.
    pub(crate) pending_musicxml_key_restatement: bool,
    /// Croma MusicXML-origin `[I:croma-time-symbol ...]` carrier waiting for the
    /// next `M:`/`[M:...]` field in this voice.
    pub(crate) pending_musicxml_time_symbol: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingGraceSlurStop {
    pub(crate) pair_id: u32,
    pub(crate) marker: SlurSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingGraceGroup {
    pub(crate) grace: GraceGroupSyntax,
    pub(crate) detached_from_previous: bool,
    pub(crate) force_after_previous: bool,
}

impl LoweringState {
    pub(crate) fn new(
        id: VoiceId,
        properties: VoicePropertiesModel,
        unit: Fraction,
        key: Option<&KeySignature>,
        meter_duration: Option<Fraction>,
    ) -> Self {
        let source_span = id.span;
        Self {
            id,
            initial_properties: properties.clone(),
            properties,
            source_span,
            initial_key: None,
            initial_meter: None,
            unit,
            meter_duration,
            lowered: Vec::new(),
            time_groups: Vec::new(),
            diagnostics: Vec::new(),
            active_tuplets: Vec::new(),
            pending_broken: None,
            broken_left_available: false,
            key_accidentals: key_accidental_policy(key),
            current_key: key.cloned(),
            accidental_state: Vec::new(),
            pending_ties: Vec::new(),
            next_tie_id: 1,
            pending_slur_starts: Vec::new(),
            open_slurs: Vec::new(),
            pending_grace_slur_stops: Vec::new(),
            next_slur_id: 1,
            next_tuplet_id: 1,
            pending_grace_groups: Vec::new(),
            pending_chord_symbols: Vec::new(),
            pending_annotations: Vec::new(),
            pending_decorations: Vec::new(),
            pending_musicxml_instrument: None,
            pending_musicxml_harmony_text: None,
            pending_musicxml_lyric_extends: Vec::new(),
            pending_musicxml_lyric_duplicates: Vec::new(),
            pending_musicxml_forward: false,
            pending_musicxml_tuplets: Vec::new(),
            musicxml_tuplet_pair_ids: Vec::new(),
            pending_musicxml_sequence_backup: None,
            pending_musicxml_after_grace: false,
            pending_musicxml_barline_kind: None,
            pending_musicxml_meter_restatement: false,
            pending_musicxml_key_restatement: false,
            pending_musicxml_time_symbol: None,
        }
    }

    /// Build the lowered attachment bundle for a timed event, prepending any
    /// grace groups, chord symbols, and annotations that the parser flushed
    /// ahead of their note (see `pending_grace_groups` /
    /// `pending_chord_symbols` / `pending_annotations`). Prepending preserves
    /// source order: a flushed attachment was written before the note's own
    /// attachments, and multiple flushed items keep their relative order. The
    /// buffers are drained once consumed.
    pub(crate) fn take_timed_attachments(&mut self, bundle: &AttachmentBundle) -> EventAttachments {
        let mut pending_graces = self.take_pending_grace_group_attachments();
        let mut attachments = attachment_bundle_model(bundle, self);
        if !pending_graces.is_empty() {
            pending_graces.append(&mut attachments.grace_groups);
            attachments.grace_groups = pending_graces;
        }
        if !self.pending_chord_symbols.is_empty() {
            let mut symbols: Vec<_> = self.pending_chord_symbols.drain(..).collect();
            symbols.append(&mut attachments.chord_symbols);
            attachments.chord_symbols = symbols;
        }
        if !self.pending_annotations.is_empty() {
            let mut annotations: Vec<_> = self
                .pending_annotations
                .drain(..)
                .map(|text| annotation_attachment_model(&text))
                .collect();
            annotations.append(&mut attachments.annotations);
            attachments.annotations = annotations;
        }
        if !self.pending_decorations.is_empty() {
            let mut decorations: Vec<_> = self
                .pending_decorations
                .drain(..)
                .map(|decoration| decoration_attachment_model(&decoration))
                .collect();
            decorations.append(&mut attachments.decorations);
            attachments.decorations = decorations;
        }
        if attachments.instrument.is_none() {
            attachments.instrument = self.pending_musicxml_instrument.take();
        }
        attachments
            .lyric_same_note_extends
            .append(&mut self.pending_musicxml_lyric_extends);
        attachments
            .lyric_same_note_duplicates
            .append(&mut self.pending_musicxml_lyric_duplicates);
        if self.pending_musicxml_forward {
            attachments.musicxml_forward = true;
            self.pending_musicxml_forward = false;
        }
        if attachments.musicxml_sequence_backup.is_none() {
            attachments.musicxml_sequence_backup = self.pending_musicxml_sequence_backup.take();
        }
        attachments
            .tuplets
            .append(&mut self.pending_musicxml_tuplets);
        attachments
    }

    pub(crate) fn push_musicxml_tuplet(
        &mut self,
        source_pair_id: u32,
        actual_notes: u32,
        normal_notes: u32,
        role: TupletRole,
        span: Span,
    ) {
        let pair_id = self.musicxml_tuplet_pair_id(source_pair_id);
        self.pending_musicxml_tuplets.push(TupletAttachment {
            pair_id,
            actual_notes,
            normal_notes,
            role,
            span,
        });
        if role == TupletRole::Stop {
            self.musicxml_tuplet_pair_ids
                .retain(|(source, _)| *source != source_pair_id);
        }
    }

    fn musicxml_tuplet_pair_id(&mut self, source_pair_id: u32) -> u32 {
        if let Some((_, pair_id)) = self
            .musicxml_tuplet_pair_ids
            .iter()
            .find(|(source, _)| *source == source_pair_id)
        {
            return *pair_id;
        }
        let pair_id = self.next_tuplet_id;
        self.next_tuplet_id = self.next_tuplet_id.saturating_add(1);
        self.musicxml_tuplet_pair_ids
            .push((source_pair_id, pair_id));
        pair_id
    }

    pub(crate) fn flush_pending_barline_directions(
        &mut self,
        line_index: usize,
        source_order: u32,
        span: Span,
        kind: BarlineKind,
    ) {
        let mut attachments = EventAttachments::default();
        let mut direction_span: Option<Span> = None;
        if !self.pending_annotations.is_empty() {
            attachments.annotations = self
                .pending_annotations
                .drain(..)
                .map(|text| {
                    direction_span = Some(merge_spans(direction_span, text.span));
                    annotation_attachment_model(&text)
                })
                .collect();
        }

        // A harmony candidate normally waits for the note it precedes, even
        // across a bar line (`"F"| c` binds F to c). But in a NOTE-LESS measure
        // — no note or rest since the last bar line: an orphan `"B"` between two
        // `||`, or a `y`-spacer measure carrying only an annotation — there is
        // no following note in this measure to host it, so per ABC 2.1
        // §4.18/§4.19 it anchors to this measure's bar line instead of deferring
        // across it. `broken_left_available` is the §4.4 "a timed note/rest
        // exists in the current measure" flag, still set here (the boundary
        // reset runs after this flush).
        let measure_is_note_less = !self.broken_left_available;
        let mut remaining_symbols = Vec::new();
        for text in self.pending_chord_symbols.drain(..) {
            // A final barline still has no following symbol; keep the pending
            // text for the end-of-voice dangling diagnostic path.
            let keep_pending = kind == BarlineKind::Final
                || (quoted_text_may_be_harmony(text.text.as_str()) && !measure_is_note_less);
            if keep_pending {
                remaining_symbols.push(text);
            } else {
                direction_span = Some(merge_spans(direction_span, text.span));
                // The chord-symbol channel renders a valid chord as `<harmony>`
                // (the orphan `"B"` between two `||`) and falls back to `<words>`
                // for non-harmony text (`"I"`, `"Final."`), matching how the same
                // symbol would render on a note. The annotations channel below is
                // words-only, so a flushed chord must not go there.
                attachments.chord_symbols.push(text);
            }
        }
        self.pending_chord_symbols = remaining_symbols;

        let mut remaining_decorations = Vec::new();
        for decoration in self.pending_decorations.drain(..) {
            if decoration_binds_to_barline(decoration.name.as_str()) {
                direction_span = Some(merge_spans(direction_span, decoration.span));
                attachments
                    .decorations
                    .push(decoration_attachment_model(&decoration));
            } else {
                remaining_decorations.push(decoration);
            }
        }
        self.pending_decorations = remaining_decorations;

        if attachments.annotations.is_empty()
            && attachments.decorations.is_empty()
            && attachments.chord_symbols.is_empty()
        {
            return;
        }

        self.lowered.push(LoweredEvent::Timed(LoweredTimedEvent {
            event: LoweredEventAtom {
                kind: LoweredEventAtomKind::Spacer {
                    span: direction_span.unwrap_or(span),
                },
                duration: Fraction::zero(),
            },
            line_index,
            source_order,
            alignable: false,
            attachments,
        }));
    }

    pub(crate) fn push_note_group(
        &mut self,
        note: &NoteSyntax,
        line_index: usize,
        source_order: u32,
    ) {
        let attachments = self.take_timed_attachments(&note.attachments);
        let octave = lowered_octave(note).saturating_add(voice_octave_shift(&self.properties));
        let written_accidental = note.accidental.map(|accidental| accidental.sign);
        let (effective_accidental, accidental_source) = self.effective_accidental(
            note.pitch.step,
            octave,
            written_accidental,
            note.accidental.map(|accidental| accidental.span),
        );
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
                        multiple_rest: None,
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

        // Attachments flushed ahead of this chord (graces, chord symbols,
        // annotations) attach to the chord as a whole (its first member),
        // mirroring chord-level attachments. They were written before the
        // chord, so lower them before any chord member resolves accidentals.
        let mut pending_graces = self.take_pending_grace_group_attachments();
        let mut pending_symbols: Vec<TextAttachment> =
            self.pending_chord_symbols.drain(..).collect();
        let mut pending_annotations: Vec<TextAttachment> = self
            .pending_annotations
            .drain(..)
            .map(|text| annotation_attachment_model(&text))
            .collect();
        let mut pending_decorations: Vec<DecorationAttachment> = self
            .pending_decorations
            .drain(..)
            .map(|decoration| decoration_attachment_model(&decoration))
            .collect();
        let chord_attachments = attachment_bundle_model(&chord.attachments, self);

        let mut events = Vec::with_capacity(chord.members.len());
        for (index, member) in chord.members.iter().enumerate() {
            let mut attachments = attachment_bundle_model(&member.note.attachments, self);
            if index == 0 {
                attachments.extend(chord_attachments.clone());
                if !pending_graces.is_empty() {
                    let mut graces = std::mem::take(&mut pending_graces);
                    graces.append(&mut attachments.grace_groups);
                    attachments.grace_groups = graces;
                }
                if !pending_symbols.is_empty() {
                    let mut symbols = std::mem::take(&mut pending_symbols);
                    symbols.append(&mut attachments.chord_symbols);
                    attachments.chord_symbols = symbols;
                }
                if !pending_annotations.is_empty() {
                    let mut annotations = std::mem::take(&mut pending_annotations);
                    annotations.append(&mut attachments.annotations);
                    attachments.annotations = annotations;
                }
                if !pending_decorations.is_empty() {
                    let mut decorations = std::mem::take(&mut pending_decorations);
                    decorations.append(&mut attachments.decorations);
                    attachments.decorations = decorations;
                }
                // Wide MusicXML tuplets ABC cannot spell ride in on private
                // `[I:croma-musicxml-tuplet ...]` carriers flushed ahead of this
                // chord. Like the note/rest path (`take_timed_attachments`), drain
                // them onto the chord head so the `<time-modification>`/`<tuplet>`
                // survive re-export; the writer mirrors normal chord tuplets,
                // which also carry on the head.
                attachments
                    .tuplets
                    .append(&mut self.pending_musicxml_tuplets);
                // Same for lyric carriers the reader flushes before a chord:
                // a same-note `<extend/>` melisma (`[I:croma-lyric-extend ...]`)
                // and a repeated same-verse `<lyric>` (`[I:croma-lyric-duplicate
                // ...]`) bind to the chord head, mirroring the note/rest path
                // (`take_timed_attachments`). Omitting this dropped the hold and
                // the duplicate credit lyrics on chord-led notes (PDMX lyric
                // cluster).
                attachments
                    .lyric_same_note_extends
                    .append(&mut self.pending_musicxml_lyric_extends);
                attachments
                    .lyric_same_note_duplicates
                    .append(&mut self.pending_musicxml_lyric_duplicates);
            }
            let octave =
                lowered_octave(&member.note).saturating_add(voice_octave_shift(&self.properties));
            let written_accidental = member.note.accidental.map(|accidental| accidental.sign);
            let (effective_accidental, accidental_source) = self.effective_accidental(
                member.note.pitch.step,
                octave,
                written_accidental,
                member.note.accidental.map(|accidental| accidental.span),
            );
            let member_multiplier =
                length_multiplier(member.note.length.as_ref()).checked_mul(outer_multiplier);
            events.push((
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
            ));
        }
        self.push_time_group(events, line_index, source_order);

        // Chord-internal slur markers bind to their OWN member (ABC 2.1 §4.11:
        // slurs "into, out of and between chords"), not the chord head/tail.
        // Run after the group is pushed so member indices exist (one-to-one with
        // `chord.members` in order), threading each marker through the voice-level
        // open-slur stack so slurs that cross the chord boundary still pair.
        if chord
            .members
            .iter()
            .any(|member| !member.slur_starts.is_empty() || !member.slur_ends.is_empty())
            && let Some(group) = self.time_groups.last().cloned()
        {
            for (member, &event_index) in chord.members.iter().zip(group.iter()) {
                for start in &member.slur_starts {
                    self.open_chord_member_slur(event_index, *start);
                }
                for end in &member.slur_ends {
                    self.close_chord_member_slur(event_index, *end);
                }
            }
        }

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
                    if self.pending_grace_group_between(open.marker.span, slur.span) {
                        self.pending_grace_slur_stops.push(PendingGraceSlurStop {
                            pair_id: open.pair_id,
                            marker: slur,
                        });
                        return;
                    }
                    let start_was_pending = self
                        .pending_slur_starts
                        .iter()
                        .any(|pending| pending.pair_id == open.pair_id);
                    if start_was_pending {
                        self.pending_slur_starts
                            .retain(|pending| pending.pair_id != open.pair_id);
                        self.diagnostics.push(unmatched_slur_warning(slur.span));
                        return;
                    }
                    if let Some(event_index) = self.last_note_event_index() {
                        // A voice-level `)` closing on a chord binds to the chord
                        // as a whole, which maps to its head note (ABC 2.1 §4.11;
                        // matches the slur start and foreign MusicXML / abc2xml).
                        // `last_note_event_index` returns the trailing chord
                        // member, so resolve it back to the head.
                        let event_index = self.chord_head_event_index(event_index);
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
        // An open tie must keep its sounding pitch across the change.
        self.preserve_tie_pitches_for_key_change();
        self.key_accidentals = key_accidental_policy(key);
        self.current_key = key.cloned();
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

    pub(crate) fn attach_pending_grace_groups_to_previous_note(&mut self) -> bool {
        if self.pending_grace_groups.is_empty() || !self.broken_left_available {
            return false;
        }
        let forced_after_grace = self
            .pending_grace_groups
            .iter()
            .all(|group| group.force_after_previous);
        let event_index = if forced_after_grace {
            self.previous_timed_note_or_chord_member_event_index()
        } else {
            self.previous_timed_note_event_index()
        };
        let Some(event_index) = event_index else {
            return false;
        };
        if !forced_after_grace
            && !self.timed_event_has_trill(event_index)
            && self.pending_grace_slur_stops.is_empty()
        {
            return false;
        }
        let graces = self.take_pending_grace_group_attachments();
        if let Some(LoweredEvent::Timed(timed)) = self.lowered.get_mut(event_index) {
            timed.attachments.after_grace_groups.extend(graces);
            return true;
        }
        false
    }

    pub(crate) fn attach_pending_grace_groups_to_previous_note_if_measure_complete(
        &mut self,
    ) -> bool {
        if self.pending_grace_groups.is_empty()
            || !self
                .pending_grace_groups
                .iter()
                .all(|group| group.detached_from_previous)
            || !self.current_measure_has_reached_meter()
        {
            return false;
        }
        let Some(event_index) = self.previous_timed_note_event_index() else {
            return false;
        };
        let graces = self.take_pending_grace_group_attachments();
        if let Some(LoweredEvent::Timed(timed)) = self.lowered.get_mut(event_index) {
            timed.attachments.after_grace_groups.extend(graces);
            return true;
        }
        false
    }

    pub(crate) fn push_pending_grace_group(
        &mut self,
        grace: GraceGroupSyntax,
        detached_from_previous: bool,
        force_after_previous: bool,
    ) {
        self.pending_grace_groups.push(PendingGraceGroup {
            grace,
            detached_from_previous,
            force_after_previous,
        });
    }

    fn attach_pending_slur_starts(&mut self, group: &[usize]) {
        if self.pending_slur_starts.is_empty() {
            return;
        }
        // The slur `(` binds to the immediately-following element, which may be a
        // rest (`(z/G/...`): a rest carries notations in MusicXML, so anchor the
        // start on the first timed note OR rest of the group (skipping spacers).
        // Earlier this looked for a note only, relocating the start onto the next
        // pitched note and skipping the rest (ABC 2.1 §4.11/§4.20; tune_008749).
        let Some(event_index) = group.iter().copied().find(|index| {
            matches!(
                self.lowered.get(*index),
                Some(LoweredEvent::Timed(timed))
                    if matches!(
                        timed.event.kind,
                        LoweredEventAtomKind::Note { .. } | LoweredEventAtomKind::Rest { .. }
                    )
            )
        }) else {
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

    fn pending_grace_group_between(&self, open_span: Span, close_span: Span) -> bool {
        self.pending_grace_groups.iter().any(|group| {
            group.grace.span.start > open_span.start
                && group.grace.span.end <= close_span.start
                && grace_contains_note_event(&group.grace)
        })
    }

    fn current_measure_has_reached_meter(&self) -> bool {
        let Some(expected) = self.meter_duration else {
            return false;
        };
        let elapsed = self
            .lowered
            .iter()
            .rev()
            .take_while(|event| !matches!(event, LoweredEvent::Untimed(Event::Barline { .. })))
            .fold(Fraction::zero(), |total, event| match event {
                LoweredEvent::Timed(timed) => total.checked_add(timed.event.duration),
                _ => total,
            });
        !elapsed.less_than(expected)
    }

    fn take_pending_grace_group_attachments(&mut self) -> Vec<GraceGroupAttachment> {
        let pending_grace_groups = self.pending_grace_groups.drain(..).collect::<Vec<_>>();
        if pending_grace_groups.is_empty() {
            return Vec::new();
        }

        let mut starts_by_group = vec![Vec::new(); pending_grace_groups.len()];
        let mut remaining_starts = Vec::new();
        for start in std::mem::take(&mut self.pending_slur_starts) {
            if let Some(index) = pending_grace_groups
                .iter()
                .enumerate()
                .filter(|(_, grace)| {
                    grace.grace.span.start > start.marker.span.start
                        && grace_contains_note_event(&grace.grace)
                })
                .min_by_key(|(_, grace)| grace.grace.span.start)
                .map(|(index, _)| index)
            {
                starts_by_group[index].push(start);
            } else {
                remaining_starts.push(start);
            }
        }
        self.pending_slur_starts = remaining_starts;

        let mut stops_by_group = vec![Vec::new(); pending_grace_groups.len()];
        let mut unapplied_stops = Vec::new();
        for stop in self.pending_grace_slur_stops.drain(..) {
            if let Some(index) = pending_grace_groups
                .iter()
                .enumerate()
                .filter(|(_, grace)| {
                    grace.grace.span.end <= stop.marker.span.start
                        && grace_contains_note_event(&grace.grace)
                })
                .map(|(index, _)| index)
                .next_back()
            {
                stops_by_group[index].push(stop);
            } else {
                unapplied_stops.push(stop);
            }
        }
        self.pending_grace_slur_stops = unapplied_stops;

        let mut attachments = Vec::with_capacity(pending_grace_groups.len());
        for ((grace, starts), stops) in pending_grace_groups
            .iter()
            .zip(starts_by_group)
            .zip(stops_by_group)
        {
            let mut attachment = grace_group_attachment_model(&grace.grace, self);
            for start in starts {
                attachment.slurs.push(SlurAttachment {
                    pair_id: start.pair_id,
                    role: SlurRole::Start,
                    span: start.marker.span,
                    dotted: start.marker.dotted,
                });
            }
            for stop in stops {
                if !attach_grace_group_slur_stop(&mut attachment, stop.pair_id, stop.marker) {
                    self.diagnostics
                        .push(unmatched_slur_warning(stop.marker.span));
                }
            }
            attachments.push(attachment);
        }
        attachments
    }

    pub(crate) fn last_note_event_index(&self) -> Option<usize> {
        self.lowered
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, event)| lowered_timed_note(Some(event)).map(|_| index))
    }

    /// Resolve a note event index to its chord head. If the event at `index` is
    /// a chord member (`chord: true`), walk back over the earlier members of the
    /// same time group — they share its `source_order`/`line_index` — to the
    /// head (the first note, written without `<chord/>`). A single note or the
    /// head itself returns `index` unchanged. Used so a voice-level slur stop
    /// closing on a chord lands on the head, not the trailing member.
    fn chord_head_event_index(&self, index: usize) -> usize {
        let Some(LoweredEvent::Timed(timed)) = self.lowered.get(index) else {
            return index;
        };
        if !matches!(
            timed.event.kind,
            LoweredEventAtomKind::Note { chord: true, .. }
        ) {
            return index;
        }
        let source_order = timed.source_order;
        let line_index = timed.line_index;
        let mut head = index;
        for earlier in (0..index).rev() {
            let Some(LoweredEvent::Timed(earlier_timed)) = self.lowered.get(earlier) else {
                break;
            };
            if earlier_timed.source_order == source_order
                && earlier_timed.line_index == line_index
                && matches!(earlier_timed.event.kind, LoweredEventAtomKind::Note { .. })
            {
                head = earlier;
            } else {
                break;
            }
        }
        head
    }

    fn previous_timed_note_event_index(&self) -> Option<usize> {
        for (index, event) in self.lowered.iter().enumerate().rev() {
            match event {
                LoweredEvent::Timed(timed) => {
                    return matches!(
                        &timed.event.kind,
                        LoweredEventAtomKind::Note { chord: false, .. }
                    )
                    .then_some(index);
                }
                _ => continue,
            }
        }
        None
    }

    fn previous_timed_note_or_chord_member_event_index(&self) -> Option<usize> {
        for (index, event) in self.lowered.iter().enumerate().rev() {
            match event {
                LoweredEvent::Timed(timed) => {
                    return match &timed.event.kind {
                        LoweredEventAtomKind::Note { chord: false, .. } => Some(index),
                        LoweredEventAtomKind::Note { chord: true, .. } => {
                            let source_order = timed.source_order;
                            let line_index = timed.line_index;
                            let mut first_index = index;
                            for earlier in (0..index).rev() {
                                let Some(LoweredEvent::Timed(earlier_timed)) =
                                    self.lowered.get(earlier)
                                else {
                                    break;
                                };
                                if earlier_timed.source_order == source_order
                                    && earlier_timed.line_index == line_index
                                    && matches!(
                                        &earlier_timed.event.kind,
                                        LoweredEventAtomKind::Note { chord: true, .. }
                                    )
                                {
                                    first_index = earlier;
                                } else {
                                    break;
                                }
                            }
                            Some(first_index)
                        }
                        _ => None,
                    };
                }
                _ => continue,
            }
        }
        None
    }

    fn timed_event_has_trill(&self, event_index: usize) -> bool {
        let Some(LoweredEvent::Timed(timed)) = self.lowered.get(event_index) else {
            return false;
        };
        timed
            .attachments
            .decorations
            .iter()
            .any(|decoration| decoration.name == "trill")
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

    /// Open a slur on a specific chord member: allocate a pair id, push it onto
    /// the voice-level open-slur stack (so a `)` outside the chord can still
    /// close it), and attach the start to that member's event (ABC 2.1 §4.11).
    fn open_chord_member_slur(&mut self, event_index: usize, marker: SlurSyntax) {
        let open = OpenSlur {
            pair_id: self.next_slur_id,
            marker,
        };
        self.next_slur_id = self.next_slur_id.saturating_add(1);
        self.open_slurs.push(open);
        self.attach_slur(event_index, open.pair_id, SlurRole::Start, marker);
    }

    /// Close a slur on a specific chord member. Mirrors `apply_slur`'s End
    /// matching (dotted-aware crossing recovery) but attaches the stop to this
    /// member rather than the last note overall.
    fn close_chord_member_slur(&mut self, event_index: usize, marker: SlurSyntax) {
        let open = if self
            .open_slurs
            .last()
            .is_some_and(|open| open.marker.dotted != marker.dotted)
            && let Some(position) = self
                .open_slurs
                .iter()
                .rposition(|open| open.marker.dotted == marker.dotted)
        {
            self.diagnostics.push(crossing_slur_warning(marker.span));
            Some(self.open_slurs.remove(position))
        } else {
            self.open_slurs.pop()
        };
        let Some(open) = open else {
            self.diagnostics.push(unmatched_slur_warning(marker.span));
            return;
        };
        let start_was_pending = self
            .pending_slur_starts
            .iter()
            .any(|pending| pending.pair_id == open.pair_id);
        if start_was_pending {
            self.pending_slur_starts
                .retain(|pending| pending.pair_id != open.pair_id);
            self.diagnostics.push(unmatched_slur_warning(marker.span));
            return;
        }
        self.attach_slur(event_index, open.pair_id, SlurRole::Stop, marker);
    }

    pub(crate) fn finish_open_constructs(&mut self) {
        self.finish_pending_broken_at_boundary();
        self.finish_pending_tie_at_boundary(self.source_span);
        self.finish_open_tuplets_at_boundary();
        for slur in std::mem::take(&mut self.open_slurs) {
            self.diagnostics
                .push(unclosed_slur_warning(slur.marker.span));
        }
        // Quoted text still pending at the end of the voice has no note to
        // bind to (ABC 2.1 §4.18/§4.19); surface the drop, never silent.
        for text in std::mem::take(&mut self.pending_chord_symbols) {
            self.diagnostics
                .push(dangling_quoted_text_warning(text.span));
        }
        for text in std::mem::take(&mut self.pending_annotations) {
            self.diagnostics
                .push(dangling_quoted_text_warning(text.span));
        }
        self.attach_pending_grace_groups_to_previous_note();
        // Same for a grace group with no following or previous note to decorate
        // (§4.12).
        for group in std::mem::take(&mut self.pending_grace_groups) {
            self.diagnostics
                .push(dangling_grace_group_warning(group.grace.span));
        }
        // And a decoration with no following symbol (§4.14): the dangling
        // text warning covers it (same no-silent-loss policy).
        for decoration in std::mem::take(&mut self.pending_decorations) {
            self.diagnostics
                .push(dangling_quoted_text_warning(decoration.span));
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
        LoweredEventAtomKind::Rest { .. } | LoweredEventAtomKind::Spacer { .. } => None,
    }
}

fn grace_group_attachment_model(
    grace: &GraceGroupSyntax,
    state: &mut LoweringState,
) -> GraceGroupAttachment {
    let mut events: Vec<GraceEvent> = Vec::new();
    let mut pending_slur_starts = Vec::new();
    let mut open_slurs = Vec::new();
    let mut last_note_event_index = None;

    for element in &grace.elements {
        match element {
            GraceElementSyntax::Slur(slur) => match slur.direction {
                SlurDirection::Start => {
                    let open = OpenSlur {
                        pair_id: state.next_slur_id,
                        marker: *slur,
                    };
                    state.next_slur_id = state.next_slur_id.saturating_add(1);
                    pending_slur_starts.push(open);
                    open_slurs.push(open);
                }
                SlurDirection::End => {
                    let Some(open) = open_slurs.pop() else {
                        state.diagnostics.push(unmatched_slur_warning(slur.span));
                        continue;
                    };
                    let start_was_pending = pending_slur_starts
                        .iter()
                        .any(|pending| pending.pair_id == open.pair_id);
                    pending_slur_starts.retain(|pending| pending.pair_id != open.pair_id);
                    if !start_was_pending && let Some(index) = last_note_event_index {
                        attach_grace_slur(&mut events[index], open.pair_id, SlurRole::Stop, *slur);
                    } else {
                        state.diagnostics.push(unmatched_slur_warning(slur.span));
                    }
                }
            },
            _ => {
                let Some(mut event) = grace_event_model(element, state) else {
                    continue;
                };
                if is_grace_note_event(&event) {
                    for open in pending_slur_starts.drain(..) {
                        attach_grace_slur(&mut event, open.pair_id, SlurRole::Start, open.marker);
                    }
                    last_note_event_index = Some(events.len());
                }
                events.push(event);
            }
        }
    }
    let unclosed_pair_ids = open_slurs
        .iter()
        .map(|open| open.pair_id)
        .collect::<Vec<_>>();
    for open in open_slurs {
        state
            .diagnostics
            .push(unclosed_slur_warning(open.marker.span));
    }
    if !unclosed_pair_ids.is_empty() {
        for event in &mut events {
            event
                .slurs
                .retain(|slur| !unclosed_pair_ids.contains(&slur.pair_id));
        }
    }

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
        events,
        slurs: Vec::new(),
    }
}

fn attach_grace_slur(event: &mut GraceEvent, pair_id: u32, role: SlurRole, marker: SlurSyntax) {
    event.slurs.push(SlurAttachment {
        pair_id,
        role,
        span: marker.span,
        dotted: marker.dotted,
    });
}

fn attach_grace_group_slur_stop(
    group: &mut GraceGroupAttachment,
    pair_id: u32,
    marker: SlurSyntax,
) -> bool {
    let Some(event) = group
        .events
        .iter_mut()
        .rev()
        .find(|event| is_grace_note_event(event))
    else {
        return false;
    };
    attach_grace_slur(event, pair_id, SlurRole::Stop, marker);
    true
}

fn grace_contains_note_event(grace: &GraceGroupSyntax) -> bool {
    grace.elements.iter().any(|element| {
        matches!(
            element,
            GraceElementSyntax::Note(_) | GraceElementSyntax::Chord(_)
        )
    })
}

fn is_grace_note_event(event: &GraceEvent) -> bool {
    matches!(
        event.kind,
        GraceEventKind::Note(_) | GraceEventKind::Chord(_)
    )
}

fn attachment_bundle_model(
    bundle: &AttachmentBundle,
    state: &mut LoweringState,
) -> EventAttachments {
    EventAttachments {
        grace_groups: bundle
            .grace_groups
            .iter()
            .map(|grace| grace_group_attachment_model(grace, state))
            .collect(),
        after_grace_groups: Vec::new(),
        chord_symbols: bundle
            .chord_symbols
            .iter()
            .map(|text| {
                chord_symbol_attachment_model(text, state.pending_musicxml_harmony_text.take())
            })
            .collect(),
        annotations: bundle
            .annotations
            .iter()
            .map(annotation_attachment_model)
            .collect(),
        decorations: bundle
            .decorations
            .iter()
            .map(decoration_attachment_model)
            .collect(),
        instrument: None,
        lyrics: Vec::new(),
        lyric_same_note_extends: Vec::new(),
        lyric_same_note_duplicates: Vec::new(),
        musicxml_forward: false,
        musicxml_sequence_backup: None,
        symbols: Vec::new(),
        ties: Vec::new(),
        slurs: Vec::new(),
        tuplets: Vec::new(),
    }
}

fn merge_spans(accumulator: Option<Span>, span: Span) -> Span {
    match accumulator {
        Some(current) => Span::new(current.start.min(span.start), current.end.max(span.end)),
        None => span,
    }
}

pub(crate) fn chord_symbol_attachment_model(
    text: &QuotedTextSyntax,
    musicxml_harmony_text: Option<HarmonyKindText>,
) -> TextAttachment {
    TextAttachment {
        text: text.text.clone(),
        span: text.span,
        placement: None,
        // No pending carrier means an ABC-native chord (no MusicXML provenance).
        musicxml_harmony_text: musicxml_harmony_text.unwrap_or(HarmonyKindText::AbcNative),
    }
}

pub(crate) fn annotation_attachment_model(text: &QuotedTextSyntax) -> TextAttachment {
    TextAttachment {
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
        musicxml_harmony_text: HarmonyKindText::AbcNative,
    }
}

fn quoted_text_may_be_harmony(text: &str) -> bool {
    matches!(text.trim_start().chars().next(), Some('A'..='G'))
}

fn decoration_binds_to_barline(name: &str) -> bool {
    !matches!(
        name,
        "." | "staccato"
            | ">"
            | "accent"
            | "emphasis"
            | "tenuto"
            | "wedge"
            | "marcato"
            | "breath"
            | "fermata"
            | "invertedfermata"
            | "trill"
            | "mordent"
            | "lowermordent"
            | "uppermordent"
            | "pralltriller"
            | "turn"
            | "invertedturn"
            | "upbow"
            | "downbow"
            | "open"
            | "thumb"
            | "snap"
            | "+"
            | "plus"
            | "0"
            | "1"
            | "2"
            | "3"
            | "4"
            | "5"
            | "arpeggio"
            | "slide"
            | "roll"
    )
}

pub(crate) fn decoration_attachment_model(decoration: &DecorationSyntax) -> DecorationAttachment {
    DecorationAttachment {
        name: decoration.name.clone(),
        span: decoration.span,
        source_kind: match decoration.kind {
            DecorationKind::Named => DecorationSourceKind::Named,
            DecorationKind::LegacyNamed => DecorationSourceKind::LegacyNamed,
            DecorationKind::Shorthand => DecorationSourceKind::Shorthand,
            DecorationKind::UserDefined => DecorationSourceKind::UserDefined,
        },
    }
}

fn grace_event_model(
    element: &GraceElementSyntax,
    state: &mut LoweringState,
) -> Option<GraceEvent> {
    match element {
        GraceElementSyntax::Note(note) => Some(GraceEvent {
            source_span: note.span,
            kind: GraceEventKind::Note(grace_note_event_model(note, state)),
            slurs: Vec::new(),
        }),
        GraceElementSyntax::Rest(rest) => Some(GraceEvent {
            source_span: rest.span,
            kind: GraceEventKind::Rest(RestEvent {
                visibility: rest.visibility,
            }),
            slurs: Vec::new(),
        }),
        GraceElementSyntax::Chord(chord) => Some(GraceEvent {
            source_span: chord.span,
            kind: GraceEventKind::Chord(
                chord
                    .members
                    .iter()
                    .map(|member| grace_note_event_model(&member.note, state))
                    .collect(),
            ),
            slurs: Vec::new(),
        }),
        GraceElementSyntax::Slur(_) | GraceElementSyntax::Malformed(_) => None,
    }
}

fn grace_note_event_model(note: &NoteSyntax, state: &mut LoweringState) -> GraceNoteEvent {
    let octave = lowered_octave(note);
    let ledger_octave = octave.saturating_add(voice_octave_shift(&state.properties));
    let accidental = note.accidental.map(|accidental| accidental.sign);
    let (effective_accidental, _) = state.effective_accidental(
        note.pitch.step,
        ledger_octave,
        accidental,
        note.accidental.map(|accidental| accidental.span),
    );
    GraceNoteEvent {
        pitch: Pitch {
            step: note.pitch.step.to_ascii_uppercase(),
            alter: effective_accidental.map(Accidental::alter).unwrap_or(0),
            octave,
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
        decorations: note
            .attachments
            .decorations
            .iter()
            .map(decoration_attachment_model)
            .collect(),
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
pub(crate) fn voice_octave_shift(properties: &VoicePropertiesModel) -> i8 {
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

fn dangling_quoted_text_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.dangling_quoted_text",
        "Quoted text has no following note to attach to",
        span,
    )
    .with_spec_reference(crate::lower::diagnostics::abc_annotation_reference())
    .with_recovery_note(RecoveryNote::new(
        "The quoted text was dropped at the end of the voice.",
    ))
}

fn dangling_grace_group_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.dangling_grace_group",
        "Grace group has no following note to decorate",
        span,
    )
    .with_spec_reference(crate::lower::diagnostics::abc_grace_reference())
    .with_recovery_note(RecoveryNote::new(
        "The grace group was dropped at the end of the voice.",
    ))
}

#[cfg(test)]
#[path = "voice_tests.rs"]
mod tests;
