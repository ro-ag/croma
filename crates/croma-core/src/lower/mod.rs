pub(crate) mod accidental;
mod align;
mod semantic;
mod tempo;
pub(crate) mod tie;
pub(crate) mod tuplet;
pub(crate) mod voice;

pub(crate) use crate::lower::voice::{
    is_note_atom, lowered_timed_note, note_signature, ActiveTuplet, CompletedTuplet, LoweredEvent,
    LoweredTimedEvent, LoweringState, PendingTie,
};

use crate::lower::accidental::{
    accidental_from_field_sign, key_accidental_policy, KeyAccidentalPolicy, MeasureAccidental,
};
use crate::lower::align::{align_lyrics, align_symbols};
use crate::lower::semantic::semantic_voice_from_timeline;
use crate::lower::tempo::parse_tempo_model;
use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
use crate::parse::field::{
    FieldState, KeyMode,
    KeySignature, KeyTonicAccidental, Meter, MeterKind, Spanned, StemDirection, UnitNoteLength, VoiceDefinition,
};
use crate::model::{
    Accidental, AccidentalMark, AccidentalPolicy, AccidentalScope,
    AlignedSymbolKind, AnnotationPlacementModel, BarlineKind, ChordEvent, ChordMemberEvent,
    DecorationAttachment, DecorationSourceKind, Event, EventAttachments, Fraction, GraceEvent,
    GraceEventKind, GraceGroupAttachment, GraceNoteEvent, KeyAccidentalModel, KeySignatureModel,
    LoweredEventAtom, LoweredEventAtomKind, LyricControl, Measure, MeasureBarline, MeasureId,
    MeterModel, NoteEvent, OverlaySegment, Part, PartId, Pitch, PreservedDirective,
    RepeatEndingModel, RepeatEndingPartModel, RestEvent, Score,
    ScoreDirectiveModel, ScoreDirectiveTokenKindModel, ScoreDirectiveTokenModel, ScoreMetadata,
    SlurAttachment, SlurRole, Staff, StaffId, StemDirectionModel,
    TextAttachment, TextLine, TieAttachment, TieRole, TimedEvent, TimedEventKind,
    TimelineEventKind, TupletAttachment, TupletRole, Voice, VoiceId, VoiceMeasureTimeline,
    VoicePropertiesModel, VoiceTimedEvent, VoiceTimeline, lcm,
};
use crate::parse::ParseReport;
use crate::syntax::tune::ScoreLineBreak;
use crate::syntax::{
    AnnotationPlacement, AttachmentBundle, BarlineSyntax, BrokenRhythmDirection,
    BrokenRhythmSyntax, ChordSyntax, DecorationKind,
    GraceElementSyntax, InlineFieldSyntax, LengthSyntax, LyricLineSyntax,
    MusicFieldLine, MusicFieldLineKind, MusicItem, MusicLine,
    NoteSyntax, OctaveMark, OverlaySyntax, ParsedTuneMusic, PreservedDirectiveSyntax, QuotedTextKind, RestSyntax,
    ScoreDirectiveSyntax, SlurDirection, SlurSyntax, SymbolLineSyntax,
    TieSyntax, TupletSyntax, VariantEndingPart, VariantEndingSyntax,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredMusic {
    pub events: Vec<Event>,
    pub divisions: u32,
    pub voices: Vec<VoiceTimeline>,
    pub score_directives: Vec<ScoreDirectiveModel>,
    pub preserved_directives: Vec<PreservedDirective>,
    pub post_tune_lyrics: Vec<TextLine>,
}


pub(crate) fn lower_tune_music(
    tune_music: &ParsedTuneMusic,
    field_state: &FieldState,
) -> ParseReport<LoweredMusic> {
    let unit = field_state.unit_note_length_fraction();
    let mut lowering = MultiVoiceLowering::new(unit, field_state, tune_music.span);
    let mut entries = Vec::new();
    for (index, line) in tune_music.lines.iter().enumerate() {
        entries.push((line.line_index, 1usize, index));
    }
    for (index, field) in tune_music.body_fields.iter().enumerate() {
        entries.push((field.line_index, 0usize, index));
    }
    entries.sort_unstable();

    for (_line_index, kind, index) in entries {
        if kind == 0 {
            lowering.apply_field(&tune_music.body_fields[index]);
        } else {
            lowering.apply_music_line(&tune_music.lines[index]);
        }
    }

    let lyric_lines = lowering.lyric_lines.clone();
    let symbol_lines = lowering.symbol_lines.clone();
    let mut diagnostics = lowering.finish_open_constructs();
    let all_lowered = lowering
        .voices
        .iter()
        .flat_map(|voice| voice.lowered.iter())
        .collect::<Vec<_>>();
    let divisions = all_lowered.iter().fold(8, |divisions, event| match event {
        LoweredEvent::Timed(timed) => lcm(divisions, timed.event.duration.divisions_requirement()),
        LoweredEvent::Untimed(_) | LoweredEvent::Overlay(_) | LoweredEvent::VariantEnding(_) => {
            divisions
        }
    });
    let events = all_lowered
        .into_iter()
        .filter_map(|event| match event {
            LoweredEvent::Timed(timed) => Some(timed.event.into_event(divisions)),
            LoweredEvent::Untimed(event) => Some(event.clone()),
            LoweredEvent::Overlay(_) | LoweredEvent::VariantEnding(_) => None,
        })
        .collect();
    let meter_duration = lowering.meter_duration;
    let mut voices = lowering.into_voice_timelines(meter_duration, &mut diagnostics);
    align_lyrics(&mut voices, &lyric_lines, &mut diagnostics);
    align_symbols(&mut voices, &symbol_lines, &mut diagnostics);
    let score_directives = tune_music
        .score_directives
        .iter()
        .map(score_directive_model)
        .collect();
    let preserved_directives = tune_music
        .preserved_directives
        .iter()
        .map(preserved_directive_model)
        .collect();
    let post_tune_lyrics = tune_music
        .body_fields
        .iter()
        .filter_map(|field| match &field.kind {
            MusicFieldLineKind::PostTuneText(text) => Some(TextLine {
                text: text.value.clone(),
                span: text.span,
            }),
            _ => None,
        })
        .collect();

    ParseReport::new(
        LoweredMusic {
            events,
            divisions,
            voices,
            score_directives,
            preserved_directives,
            post_tune_lyrics,
        },
        diagnostics,
    )
}

struct MultiVoiceLowering {
    unit: Fraction,
    key: Option<KeySignature>,
    meter: Option<Meter>,
    meter_duration: Option<Fraction>,
    voices: Vec<LoweringState>,
    current_voice: String,
    source_order: u32,
    lyric_lines: Vec<VoicedLyricLine>,
    symbol_lines: Vec<VoicedSymbolLine>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VoicedLyricLine {
    voice_id: String,
    line: LyricLineSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VoicedSymbolLine {
    voice_id: String,
    line: SymbolLineSyntax,
}

impl MultiVoiceLowering {
    fn new(unit: Fraction, field_state: &FieldState, fallback_span: Span) -> Self {
        let mut lowering = Self {
            unit,
            key: field_state.key.as_ref().map(|key| key.value.clone()),
            meter: field_state.meter.as_ref().map(|meter| meter.value.clone()),
            meter_duration: field_state
                .meter
                .as_ref()
                .and_then(|meter| meter_duration(&meter.value.kind)),
            voices: Vec::new(),
            current_voice: String::new(),
            source_order: 0,
            lyric_lines: Vec::new(),
            symbol_lines: Vec::new(),
            diagnostics: Vec::new(),
        };

        for voice in &field_state.voices {
            lowering.ensure_voice(voice.clone());
        }

        let initial = field_state
            .voices
            .first()
            .cloned()
            .or_else(|| field_state.voice.clone())
            .unwrap_or_else(|| default_voice_definition(fallback_span));
        lowering.current_voice = initial.value.id.value.clone();
        lowering.ensure_voice(initial);
        lowering
    }

    fn apply_field(&mut self, field: &MusicFieldLine) {
        match &field.kind {
            MusicFieldLineKind::Meter(meter) => self.apply_meter_change(meter),
            MusicFieldLineKind::UnitNoteLength(unit) => self.apply_unit_change(unit),
            MusicFieldLineKind::Key(key) => self.apply_key_change(key),
            MusicFieldLineKind::Voice(voice) => self.switch_voice(voice.clone()),
            MusicFieldLineKind::Lyric(line) => self.lyric_lines.push(VoicedLyricLine {
                voice_id: self.current_voice.clone(),
                line: line.clone(),
            }),
            MusicFieldLineKind::Symbol(line) => self.symbol_lines.push(VoicedSymbolLine {
                voice_id: self.current_voice.clone(),
                line: line.clone(),
            }),
            MusicFieldLineKind::PostTuneText(_)
            | MusicFieldLineKind::Score(_)
            | MusicFieldLineKind::Unknown(_)
            | MusicFieldLineKind::Other => {}
        }
    }

    fn apply_meter_change(&mut self, meter: &Spanned<Meter>) {
        if meter_is_invalid_for_lowering(&meter.value) {
            self.diagnostics
                .push(invalid_meter_change_warning(meter.span));
            return;
        }
        if matches!(meter.value.kind, MeterKind::Complex) {
            self.diagnostics
                .push(unsupported_complex_meter_warning(meter.span));
        }
        self.meter_duration = meter_duration(&meter.value.kind);
        self.meter = Some(meter.value.clone());
        for voice in &mut self.voices {
            voice.finish_open_tuplets_at_boundary();
            voice.reset_measure_accidentals();
        }
    }

    fn apply_unit_change(&mut self, unit: &Spanned<UnitNoteLength>) {
        self.unit = unit.value.fraction.to_model_fraction();
        for voice in &mut self.voices {
            voice.unit = self.unit;
        }
    }

    fn apply_key_change(&mut self, key: &Spanned<KeySignature>) {
        if key_is_invalid_for_lowering(&key.value) {
            self.diagnostics.push(invalid_key_change_warning(key.span));
            return;
        }
        self.key = Some(key.value.clone());
        for voice in &mut self.voices {
            voice.set_key(Some(&key.value));
        }
    }

    /// Apply an inline `[K:..]` key change to the currently-active voice only.
    ///
    /// Per ABC 2.1 (§4.2, §7) an inline key change inside a voice line affects
    /// that voice from this point onward; the other voices keep their existing
    /// key signature. The active voice is the one `current_state()` resolves via
    /// `self.current_voice`.
    fn apply_inline_key_change(&mut self, key: &Spanned<KeySignature>) {
        if key_is_invalid_for_lowering(&key.value) {
            self.diagnostics.push(invalid_key_change_warning(key.span));
            return;
        }
        self.current_state().set_key(Some(&key.value));
    }

    /// Apply an inline information field (`[M:..]`, `[K:..]`, `[L:..]`) that
    /// appears mid-line. Voice switches (`[V:..]`) are handled separately. The
    /// change takes effect from this point in the line, mirroring how the
    /// whole-line fields are applied via `apply_field`.
    fn apply_inline_field(&mut self, inline: &InlineFieldSyntax) {
        match inline.code {
            'M' => {
                let meter = crate::parse::field::parse_meter(&inline.value.value);
                self.apply_meter_change(&Spanned::new(meter, inline.value.span));
            }
            'L' => {
                if let Some(unit) = crate::parse::field::parse_unit_note_length(&inline.value.value) {
                    self.apply_unit_change(&Spanned::new(unit, inline.value.span));
                }
            }
            'K' => {
                let key = crate::parse::field::parse_key(&inline.value.value, inline.value.span);
                // An inline `[K:..]` that carries no key information — e.g. a
                // clef-only change such as `[K:clef=bass]` — must leave the
                // current key signature untouched rather than reset it.
                if inline_key_changes_signature(&key) {
                    self.apply_inline_key_change(&Spanned::new(key, inline.value.span));
                }
            }
            _ => {}
        }
    }

    fn apply_music_line(&mut self, line: &MusicLine) {
        for item in &line.items {
            match item {
                MusicItem::Note(note) => {
                    let source_order = self.next_source_order();
                    self.current_state()
                        .push_note_group(note, line.line_index, source_order);
                }
                MusicItem::Rest(rest) => {
                    let source_order = self.next_source_order();
                    self.current_state()
                        .push_rest_group(rest, line.line_index, source_order);
                }
                MusicItem::MultiMeasureRest(rest) => {
                    let count = rest.count.map(|count| count.value).unwrap_or(1);
                    let duration = if let Some(meter_duration) = self.meter_duration {
                        meter_duration.checked_mul_u32(count)
                    } else {
                        self.current_state()
                            .diagnostics
                            .push(free_meter_multirest_warning(rest.span));
                        self.unit.checked_mul_u32(count)
                    };
                    let source_order = self.next_source_order();
                    self.current_state().push_time_group(
                        vec![(
                            LoweredEventAtom {
                                kind: LoweredEventAtomKind::Rest {
                                    visibility: rest.visibility,
                                    span: rest.span,
                                },
                                duration,
                            },
                            false,
                            EventAttachments::default(),
                        )],
                        line.line_index,
                        source_order,
                    );
                }
                MusicItem::Spacer(spacer) => self
                    .current_state()
                    .lowered
                    .push(LoweredEvent::Untimed(Event::Spacer { span: spacer.span })),
                MusicItem::Chord(chord) => {
                    let source_order = self.next_source_order();
                    self.current_state()
                        .push_chord_group(chord, line.line_index, source_order);
                }
                MusicItem::BrokenRhythm(marker) => {
                    self.current_state().apply_broken_rhythm(*marker)
                }
                MusicItem::Tuplet(tuplet) => self.current_state().start_tuplet(tuplet),
                MusicItem::Slur(slur) => self.current_state().apply_slur(*slur),
                MusicItem::Tie(tie) => self.current_state().apply_tie(*tie),
                MusicItem::Overlay(overlay) => self
                    .current_state()
                    .lowered
                    .push(LoweredEvent::Overlay(*overlay)),
                MusicItem::VariantEnding(ending) => self
                    .current_state()
                    .lowered
                    .push(LoweredEvent::VariantEnding(ending.clone())),
                MusicItem::Barline(barline) => {
                    if matches!(barline.kind, BarlineKind::Dotted | BarlineKind::Invisible) {
                        self.current_state()
                            .diagnostics
                            .push(barline_export_policy_info(barline.span, barline.kind));
                    }
                    self.current_state().finish_pending_broken_at_boundary();
                    self.current_state().finish_open_tuplets_at_boundary();
                    self.current_state().reset_measure_accidentals_at_barline();
                    for kind in barline_lowering_kinds(barline) {
                        self.current_state()
                            .lowered
                            .push(LoweredEvent::Untimed(Event::Barline {
                                kind,
                                span: barline.span,
                            }));
                    }
                }
                MusicItem::InlineField(inline) if inline.code == 'V' => {
                    if let Some(voice) = &inline.voice {
                        self.switch_voice(voice.clone());
                    }
                }
                MusicItem::InlineField(inline) => self.apply_inline_field(inline),
                MusicItem::GraceGroup(_)
                | MusicItem::ChordSymbol(_)
                | MusicItem::Annotation(_)
                | MusicItem::Decoration(_)
                | MusicItem::Unsupported(_)
                | MusicItem::Malformed(_) => {}
            }
        }
    }

    fn switch_voice(&mut self, voice: Spanned<VoiceDefinition>) {
        self.current_voice = voice.value.id.value.clone();
        self.ensure_voice(voice);
    }

    fn next_source_order(&mut self) -> u32 {
        let order = self.source_order;
        self.source_order = self.source_order.saturating_add(1);
        order
    }

    fn ensure_voice(&mut self, voice: Spanned<VoiceDefinition>) -> usize {
        if let Some(index) = self
            .voices
            .iter()
            .position(|state| state.id.value == voice.value.id.value)
        {
            // A later voice reference (e.g. a bare `V:3` switch in the body) must
            // not wipe properties such as the clef declared when the voice was
            // first defined. Merge: new values win, otherwise keep the existing.
            let incoming = voice_properties_model(&voice.value);
            merge_voice_properties(&mut self.voices[index].properties, incoming);
            self.voices[index].source_span = voice.span;
            return index;
        }
        let id = VoiceId {
            value: voice.value.id.value.clone(),
            span: voice.value.id.span,
        };
        let properties = voice_properties_model(&voice.value);
        self.voices.push(LoweringState::new(
            id,
            properties,
            self.unit,
            self.key.as_ref(),
        ));
        self.voices.len() - 1
    }

    fn current_state(&mut self) -> &mut LoweringState {
        let index = self
            .voices
            .iter()
            .position(|state| state.id.value == self.current_voice)
            .unwrap_or_else(|| {
                let voice = default_voice_definition(Span::new(0, 0));
                self.current_voice = voice.value.id.value.clone();
                self.ensure_voice(voice)
            });
        &mut self.voices[index]
    }

    fn finish_open_constructs(&mut self) -> Vec<Diagnostic> {
        let mut diagnostics = std::mem::take(&mut self.diagnostics);
        for voice in &mut self.voices {
            voice.finish_open_constructs();
            diagnostics.extend(std::mem::take(&mut voice.diagnostics));
        }
        diagnostics
    }

    fn into_voice_timelines(
        self,
        meter_duration: Option<Fraction>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<VoiceTimeline> {
        let mut voices = self
            .voices
            .into_iter()
            .map(|voice| build_voice_timeline(voice, meter_duration, diagnostics))
            .collect::<Vec<_>>();
        if voices.len() > 1 {
            voices.retain(|voice| {
                voice_has_timeline_content(voice)
                    || voice.properties != VoicePropertiesModel::default()
                    || voice.id.value != "1"
            });
        }
        voices
    }
}

fn voice_has_timeline_content(voice: &VoiceTimeline) -> bool {
    voice.measures.iter().any(|measure| {
        !measure.overlays.is_empty()
            || measure.events.iter().any(|event| {
                matches!(
                    event.kind,
                    TimelineEventKind::Note { .. } | TimelineEventKind::Rest { .. }
                )
            })
    })
}

fn default_voice_definition(span: Span) -> Spanned<VoiceDefinition> {
    let id = Spanned::new("1".to_owned(), span);
    Spanned::new(
        VoiceDefinition {
            id: id.clone(),
            properties: Spanned::new(String::new(), Span::new(span.end, span.end)),
            parsed_properties: Default::default(),
        },
        span,
    )
}

/// Merge a later voice definition into the existing properties: any field set
/// by the new definition overrides, and unset fields keep their existing value
/// so a bare `V:` switch does not discard the original clef/name/etc.
fn merge_voice_properties(existing: &mut VoicePropertiesModel, incoming: VoicePropertiesModel) {
    if incoming.name.is_some() {
        existing.name = incoming.name;
    }
    if incoming.nm.is_some() {
        existing.nm = incoming.nm;
    }
    if incoming.subname.is_some() {
        existing.subname = incoming.subname;
    }
    if incoming.snm.is_some() {
        existing.snm = incoming.snm;
    }
    if incoming.clef.is_some() {
        existing.clef = incoming.clef;
    }
    if incoming.stem.is_some() {
        existing.stem = incoming.stem;
    }
    if incoming.octave.is_some() {
        existing.octave = incoming.octave;
    }
    if incoming.transpose.is_some() {
        existing.transpose = incoming.transpose;
    }
}

fn voice_properties_model(voice: &VoiceDefinition) -> VoicePropertiesModel {
    VoicePropertiesModel {
        name: voice
            .parsed_properties
            .name
            .as_ref()
            .map(text_line_from_spanned),
        nm: voice
            .parsed_properties
            .nm
            .as_ref()
            .map(text_line_from_spanned),
        subname: voice
            .parsed_properties
            .subname
            .as_ref()
            .map(text_line_from_spanned),
        snm: voice
            .parsed_properties
            .snm
            .as_ref()
            .map(text_line_from_spanned),
        clef: voice
            .parsed_properties
            .clef
            .as_ref()
            .map(text_line_from_spanned),
        stem: voice
            .parsed_properties
            .stem
            .as_ref()
            .map(|stem| match stem.value {
                StemDirection::Up => StemDirectionModel::Up,
                StemDirection::Down => StemDirectionModel::Down,
            }),
        octave: voice
            .parsed_properties
            .octave
            .as_ref()
            .map(text_line_from_spanned),
        transpose: voice
            .parsed_properties
            .transpose
            .as_ref()
            .map(text_line_from_spanned),
    }
}

fn text_line_from_spanned(value: &Spanned<String>) -> TextLine {
    TextLine {
        text: value.value.clone(),
        span: value.span,
    }
}

fn build_voice_timeline(
    voice: LoweringState,
    meter_duration: Option<Fraction>,
    diagnostics: &mut Vec<Diagnostic>,
) -> VoiceTimeline {
    let mut builder = VoiceTimelineBuilder::new(voice.id.clone(), meter_duration);
    for event in voice.lowered {
        builder.push(event, diagnostics);
    }
    let measures = builder.finish(diagnostics);
    VoiceTimeline {
        id: voice.id,
        properties: voice.properties,
        measures,
        source_span: voice.source_span,
    }
}

struct VoiceTimelineBuilder {
    voice_id: VoiceId,
    meter_duration: Option<Fraction>,
    measures: Vec<VoiceMeasureTimeline>,
    measure_index: u32,
    onset: Fraction,
    last_group_onset: Fraction,
    active_overlay: Option<OverlayBuilder>,
    overlay_count: u32,
}

impl VoiceTimelineBuilder {
    fn new(voice_id: VoiceId, meter_duration: Option<Fraction>) -> Self {
        Self {
            voice_id,
            meter_duration,
            measures: vec![VoiceMeasureTimeline {
                index: 0,
                span: Span::new(0, 0),
                events: Vec::new(),
                overlays: Vec::new(),
            }],
            measure_index: 0,
            onset: Fraction::zero(),
            last_group_onset: Fraction::zero(),
            active_overlay: None,
            overlay_count: 0,
        }
    }

    fn push(&mut self, event: LoweredEvent, diagnostics: &mut Vec<Diagnostic>) {
        match event {
            LoweredEvent::Timed(timed) => self.push_timed(timed),
            LoweredEvent::Untimed(Event::Barline { kind, span }) => {
                self.finish_overlay(diagnostics);
                let starts_current_measure = self.is_empty_measure_start()
                    && (starts_measure_barline(kind)
                        || (self.is_first_measure_start()
                            && starts_first_body_measure_barline(kind)))
                    || self.is_first_measure_combined_repeat_start(kind, span);
                self.push_barline(kind, span);
                if starts_current_measure {
                    return;
                }
                self.start_measure_after_barline(span);
            }
            LoweredEvent::Untimed(Event::Spacer { span }) => {
                let onset = self.onset;
                self.current_measure_mut().events.push(VoiceTimedEvent {
                    onset,
                    duration: Fraction::zero(),
                    span,
                    line_index: 0,
                    source_order: 0,
                    alignable: false,
                    kind: TimelineEventKind::Spacer,
                    lyrics: Vec::new(),
                    symbols: Vec::new(),
                    attachments: EventAttachments::default(),
                });
            }
            LoweredEvent::Untimed(Event::Note { .. } | Event::Rest { .. }) => {}
            LoweredEvent::Overlay(overlay) => {
                self.finish_overlay(diagnostics);
                let expected_duration = if self.onset == Fraction::zero() {
                    self.meter_duration.unwrap_or_else(Fraction::zero)
                } else {
                    self.onset
                };
                let overlay_id = VoiceId {
                    value: format!("{}.overlay{}", self.voice_id.value, self.overlay_count + 1),
                    span: overlay.span,
                };
                self.overlay_count = self.overlay_count.saturating_add(1);
                self.active_overlay = Some(OverlayBuilder {
                    id: overlay_id,
                    start_span: overlay.span,
                    span: overlay.span,
                    measure_index: self.measure_index,
                    expected_duration,
                    actual_duration: Fraction::zero(),
                    last_group_onset: Fraction::zero(),
                    events: Vec::new(),
                });
            }
            LoweredEvent::VariantEnding(ending) => {
                let onset = self.onset;
                let span = ending.span;
                self.current_measure_mut().events.push(VoiceTimedEvent {
                    onset,
                    duration: Fraction::zero(),
                    span,
                    line_index: 0,
                    source_order: 0,
                    alignable: false,
                    kind: TimelineEventKind::VariantEnding {
                        endings: repeat_ending_parts_model(&ending.endings),
                    },
                    attachments: EventAttachments::default(),
                    lyrics: Vec::new(),
                    symbols: Vec::new(),
                });
                self.current_measure_mut().span =
                    extend_span(self.current_measure_mut().span, span);
            }
        }
    }

    fn push_barline(&mut self, kind: BarlineKind, span: Span) {
        let onset = self.onset;
        let measure = self.current_measure_mut();
        measure.events.push(VoiceTimedEvent {
            onset,
            duration: Fraction::zero(),
            span,
            line_index: 0,
            source_order: 0,
            alignable: false,
            kind: TimelineEventKind::Barline { kind },
            lyrics: Vec::new(),
            symbols: Vec::new(),
            attachments: EventAttachments::default(),
        });
        measure.span = extend_span(measure.span, span);
    }

    fn is_empty_measure_start(&self) -> bool {
        self.onset == Fraction::zero()
            && self.active_overlay.is_none()
            && self
                .measures
                .last()
                .is_some_and(|measure| measure.events.is_empty() && measure.overlays.is_empty())
    }

    fn is_first_measure_start(&self) -> bool {
        self.measure_index == 0 && self.measures.len() == 1
    }

    fn is_first_measure_combined_repeat_start(&self, kind: BarlineKind, span: Span) -> bool {
        kind == BarlineKind::RepeatStart
            && self.onset == Fraction::zero()
            && self.active_overlay.is_none()
            && self.is_first_measure_start()
            && self.measures.last().is_some_and(|measure| {
                !measure.events.is_empty()
                    && measure.overlays.is_empty()
                    && measure.events.iter().all(|event| {
                        event.duration == Fraction::zero()
                            && event.span == span
                            && matches!(
                                event.kind,
                                TimelineEventKind::Barline {
                                    kind: BarlineKind::Double | BarlineKind::Initial
                                }
                            )
                    })
            })
    }

    fn start_measure_after_barline(&mut self, span: Span) {
        self.measure_index = self.measure_index.saturating_add(1);
        self.onset = Fraction::zero();
        self.last_group_onset = Fraction::zero();
        self.measures.push(VoiceMeasureTimeline {
            index: self.measure_index,
            span: Span::new(span.end, span.end),
            events: Vec::new(),
            overlays: Vec::new(),
        });
    }

    fn push_timed(&mut self, timed: LoweredTimedEvent) {
        let span = timed_span(timed.event);
        let chord_member = matches!(
            timed.event.kind,
            LoweredEventAtomKind::Note { chord: true, .. }
        );
        let onset = if let Some(overlay) = &self.active_overlay {
            if chord_member {
                overlay.last_group_onset
            } else {
                overlay.actual_duration
            }
        } else if chord_member {
            self.last_group_onset
        } else {
            self.onset
        };
        let event = VoiceTimedEvent {
            onset,
            duration: timed.event.duration,
            span,
            line_index: timed.line_index,
            source_order: timed.source_order,
            alignable: timed.alignable
                && matches!(timed.event.kind, LoweredEventAtomKind::Note { .. }),
            kind: timeline_event_kind(timed.event.kind),
            attachments: timed.attachments,
            lyrics: Vec::new(),
            symbols: Vec::new(),
        };
        if let Some(overlay) = &mut self.active_overlay {
            if !chord_member {
                overlay.last_group_onset = event.onset;
                overlay.actual_duration = overlay.actual_duration.checked_add(timed.event.duration);
            }
            overlay.span = extend_span(overlay.span, span);
            overlay.events.push(event);
        } else {
            if !chord_member {
                self.last_group_onset = event.onset;
                self.onset = self.onset.checked_add(timed.event.duration);
            }
            self.current_measure_mut().span = extend_span(self.current_measure_mut().span, span);
            self.current_measure_mut().events.push(event);
        }
    }

    fn finish(mut self, diagnostics: &mut Vec<Diagnostic>) -> Vec<VoiceMeasureTimeline> {
        self.finish_overlay(diagnostics);
        while self
            .measures
            .last()
            .is_some_and(|measure| measure.events.is_empty() && measure.overlays.is_empty())
            && self.measures.len() > 1
        {
            self.measures.pop();
        }
        self.measures
    }

    fn finish_overlay(&mut self, diagnostics: &mut Vec<Diagnostic>) {
        let Some(overlay) = self.active_overlay.take() else {
            return;
        };
        if overlay.actual_duration.less_than(overlay.expected_duration) {
            diagnostics.push(overlay_incomplete_measure_warning(
                overlay.start_span,
                overlay.actual_duration,
                overlay.expected_duration,
            ));
        } else if overlay.expected_duration.less_than(overlay.actual_duration) {
            diagnostics.push(overlay_overfull_measure_warning(
                overlay.start_span,
                overlay.actual_duration,
                overlay.expected_duration,
            ));
        }
        self.current_measure_mut().overlays.push(OverlaySegment {
            id: overlay.id,
            span: overlay.span,
            measure_index: overlay.measure_index,
            expected_duration: overlay.expected_duration,
            actual_duration: overlay.actual_duration,
            events: overlay.events,
        });
    }

    fn current_measure_mut(&mut self) -> &mut VoiceMeasureTimeline {
        self.measures
            .last_mut()
            .expect("timeline builder always has a current measure")
    }
}

/// Whether an inline `[K:..]` field actually specifies a key signature (and so
/// should be applied) rather than only carrying non-key information such as a
/// clef (`[K:clef=bass]`). A bare default — no tonic, no accidentals, plain
/// major mode — means "no key change", so it is left untouched.
fn inline_key_changes_signature(key: &KeySignature) -> bool {
    key.tonic.is_some() || !key.accidentals.is_empty() || key.mode != KeyMode::Major
}

fn starts_measure_barline(kind: BarlineKind) -> bool {
    matches!(
        kind,
        BarlineKind::Regular | BarlineKind::Initial | BarlineKind::RepeatStart
    )
}

fn starts_first_body_measure_barline(kind: BarlineKind) -> bool {
    matches!(
        kind,
        BarlineKind::Double | BarlineKind::Final | BarlineKind::Liberal
    )
}

struct OverlayBuilder {
    id: VoiceId,
    start_span: Span,
    span: Span,
    measure_index: u32,
    expected_duration: Fraction,
    actual_duration: Fraction,
    last_group_onset: Fraction,
    events: Vec<VoiceTimedEvent>,
}

fn timed_span(event: LoweredEventAtom) -> Span {
    match event.kind {
        LoweredEventAtomKind::Note { span, .. } | LoweredEventAtomKind::Rest { span, .. } => span,
    }
}

fn timeline_event_kind(kind: LoweredEventAtomKind) -> TimelineEventKind {
    match kind {
        LoweredEventAtomKind::Note {
            step,
            octave,
            accidental,
            effective_accidental,
            accidental_source,
            chord,
            ..
        } => TimelineEventKind::Note {
            step,
            octave,
            accidental,
            effective_accidental,
            accidental_source,
            chord,
        },
        LoweredEventAtomKind::Rest { visibility, .. } => TimelineEventKind::Rest { visibility },
    }
}

pub(crate) fn extend_span(current: Span, next: Span) -> Span {
    if current.is_empty() {
        return next;
    }
    Span::new(current.start.min(next.start), current.end.max(next.end))
}

fn repeat_ending_parts_model(parts: &[VariantEndingPart]) -> Vec<RepeatEndingPartModel> {
    parts
        .iter()
        .map(|part| match *part {
            VariantEndingPart::Single(number) => RepeatEndingPartModel::Single(number.value),
            VariantEndingPart::Range { start, end, .. } => RepeatEndingPartModel::Range {
                start: start.value,
                end: end.value,
            },
        })
        .collect()
}

pub(crate) struct ScoreModelInput<'a> {
    pub reference: TextLine,
    pub title: Option<TextLine>,
    pub composers: Vec<TextLine>,
    pub tempo: Option<TextLine>,
    pub source_span: Span,
    pub field_state: &'a FieldState,
    pub voices: &'a [VoiceTimeline],
    pub score_directives: &'a [ScoreDirectiveModel],
    pub preserved_directives: &'a [PreservedDirective],
    pub post_tune_lyrics: &'a [TextLine],
    pub diagnostics: &'a [Diagnostic],
    pub divisions: u32,
}

/// Group voice indices into parts according to a `%%staves` / `%%score`
/// directive. A parenthesis `( )` group merges its voices into one part
/// (overlay voices on a shared staff); `[ ]` brackets and `{ }` braces are
/// visual bracketing only and keep one part per voice. With no grouping
/// directive — or a directive that never merges voices — each voice is its own
/// part in voice-definition order (so the common bracket/brace cases keep the
/// existing ordering).
fn part_voice_groups(
    directives: &[ScoreDirectiveModel],
    voices: &[VoiceTimeline],
) -> Vec<Vec<usize>> {
    let one_per_voice = || {
        (0..voices.len())
            .map(|index| vec![index])
            .collect::<Vec<_>>()
    };
    let Some(directive) = directives.iter().rev().find(|directive| {
        directive
            .tokens
            .iter()
            .any(|token| matches!(token.kind, ScoreDirectiveTokenKindModel::Voice(_)))
    }) else {
        return one_per_voice();
    };

    let index_of = |id: &str| voices.iter().position(|voice| voice.id.value == id);
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut paren_depth = 0u32;
    let mut current: Vec<usize> = Vec::new();
    for token in &directive.tokens {
        match &token.kind {
            ScoreDirectiveTokenKindModel::GroupStart('(') => paren_depth += 1,
            ScoreDirectiveTokenKindModel::GroupEnd(')') => {
                paren_depth = paren_depth.saturating_sub(1);
                if paren_depth == 0 && !current.is_empty() {
                    groups.push(std::mem::take(&mut current));
                }
            }
            ScoreDirectiveTokenKindModel::Voice(id) => {
                if let Some(index) = index_of(id) {
                    if paren_depth > 0 {
                        current.push(index);
                    } else {
                        groups.push(vec![index]);
                    }
                }
            }
            _ => {}
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    // Any voice the directive did not list still needs a part.
    let mentioned: std::collections::HashSet<usize> = groups.iter().flatten().copied().collect();
    for index in 0..voices.len() {
        if !mentioned.contains(&index) {
            groups.push(vec![index]);
        }
    }
    // Only honour the directive ordering/grouping when it actually merges voices;
    // otherwise keep the simple one-part-per-voice order to avoid reordering the
    // common bracket/brace cases.
    if groups.is_empty() || groups.iter().all(|group| group.len() <= 1) {
        return one_per_voice();
    }
    groups
}

pub(crate) fn build_score_model(input: ScoreModelInput<'_>) -> Score {
    // Each ABC voice becomes its own MusicXML part, except that a `%%staves` /
    // `%%score` parenthesis group merges its voices into one multi-voice part.
    // A single-voice tune still yields exactly one part.
    let single_voice = input.voices.len() == 1;
    let groups = part_voice_groups(input.score_directives, input.voices);
    let mut parts = groups
        .iter()
        .enumerate()
        .map(|(part_index, voice_indices)| {
            let staff_id = StaffId {
                value: 1,
                span: input.source_span,
            };
            let semantic_voices = voice_indices
                .iter()
                .map(|&voice_index| {
                    semantic_voice_from_timeline(
                        &input.voices[voice_index],
                        staff_id,
                        input.field_state,
                    )
                })
                .collect::<Vec<_>>();
            let staves = vec![Staff {
                id: staff_id,
                voices: semantic_voices
                    .iter()
                    .map(|voice| voice.id.clone())
                    .collect(),
                source_span: input.source_span,
            }];
            // Prefer the (first) voice's own name; fall back to the tune title
            // only for a single-voice tune (a part name does not affect the
            // structural comparison).
            let name = semantic_voices
                .first()
                .and_then(|voice| {
                    voice
                        .properties
                        .name
                        .clone()
                        .or_else(|| voice.properties.nm.clone())
                })
                .or_else(|| single_voice.then(|| input.title.clone()).flatten());
            Part {
                id: PartId {
                    value: format!("P{}", part_index + 1),
                    span: input.source_span,
                },
                name,
                staves,
                voices: semantic_voices,
                source_span: input.source_span,
            }
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        // Defensive: a tune always exposes at least one (possibly empty) part.
        parts.push(Part {
            id: PartId {
                value: "P1".to_owned(),
                span: input.source_span,
            },
            name: input.title.clone(),
            staves: vec![Staff {
                id: StaffId {
                    value: 1,
                    span: input.source_span,
                },
                voices: Vec::new(),
                source_span: input.source_span,
            }],
            voices: Vec::new(),
            source_span: input.source_span,
        });
    }

    Score {
        metadata: ScoreMetadata {
            reference: input.reference,
            title: input.title.clone(),
            composers: input.composers,
            tempo_model: input.tempo.as_ref().and_then(|tempo| {
                parse_tempo_model(
                    &tempo.text,
                    tempo.span,
                    input.field_state.unit_note_length_fraction(),
                )
            }),
            tempo: input.tempo,
            meter: input.field_state.meter.as_ref().map(meter_model),
            key: input.field_state.key.as_ref().map(key_signature_model),
            directives: input.score_directives.to_vec(),
            preserved_directives: input.preserved_directives.to_vec(),
            post_tune_lyrics: input.post_tune_lyrics.to_vec(),
            source_span: input.source_span,
        },
        parts,
        diagnostics: input.diagnostics.to_vec(),
        divisions: input.divisions,
        source_span: input.source_span,
        accidental_policy: AccidentalPolicy {
            preserve_explicit_accidentals: true,
            reset_at_barlines: true,
            scope: AccidentalScope::PitchAndOctave,
            source_span: input.source_span,
        },
    }
}

fn meter_model(meter: &Spanned<Meter>) -> MeterModel {
    let duration = meter_duration(&meter.value.kind);
    MeterModel {
        display: meter.value.raw.clone(),
        duration,
        free_meter: duration.is_none(),
        source_span: meter.span,
    }
}

fn key_signature_model(key: &Spanned<KeySignature>) -> KeySignatureModel {
    KeySignatureModel {
        display: key.value.raw.clone(),
        fifths: key_fifths(&key.value),
        explicit_accidentals: key
            .value
            .accidentals
            .iter()
            .map(|accidental| KeyAccidentalModel {
                step: accidental.note.value.to_ascii_uppercase(),
                accidental: accidental_from_field_sign(accidental.sign),
                source_span: accidental.span,
            })
            .collect(),
        source_span: key.span,
    }
}

fn meter_is_invalid_for_lowering(meter: &Meter) -> bool {
    matches!(meter.kind, MeterKind::Complex)
        && !meter.raw.contains('/')
        && !meter.raw.contains('+')
        && !meter.raw.contains('(')
}

fn key_is_invalid_for_lowering(key: &KeySignature) -> bool {
    !key.raw.is_empty()
        && !key.raw.eq_ignore_ascii_case("none")
        && key.tonic.is_none()
        && !matches!(
            key.mode,
            KeyMode::Explicit
                | KeyMode::None
                | KeyMode::HighlandPipes
                | KeyMode::HighlandPipesMarked
        )
}

pub(crate) fn key_fifths(key: &KeySignature) -> i8 {
    let Some(tonic) = key.tonic else {
        return 0;
    };
    let base = match tonic.step {
        'C' => 0,
        'G' => 1,
        'D' => 2,
        'A' => 3,
        'E' => 4,
        'B' => 5,
        'F' => -1,
        _ => 0,
    };
    let accidental_shift = match tonic.accidental {
        Some(KeyTonicAccidental::Sharp) => 7,
        Some(KeyTonicAccidental::Flat) => -7,
        None => 0,
    };
    let mode_shift = match key.mode {
        KeyMode::Major | KeyMode::Ionian | KeyMode::Unknown(_) => 0,
        KeyMode::Mixolydian => -1,
        KeyMode::Dorian => -2,
        KeyMode::Minor | KeyMode::Aeolian => -3,
        KeyMode::Phrygian => -4,
        KeyMode::Locrian => -5,
        KeyMode::Lydian => 1,
        KeyMode::Explicit
        | KeyMode::None
        | KeyMode::HighlandPipes
        | KeyMode::HighlandPipesMarked => return 0,
    };
    (base + accidental_shift + mode_shift).clamp(-7, 7)
}

fn score_directive_model(syntax: &ScoreDirectiveSyntax) -> ScoreDirectiveModel {
    ScoreDirectiveModel {
        span: syntax.span,
        value: TextLine {
            text: syntax.value.value.clone(),
            span: syntax.value.span,
        },
        tokens: syntax
            .directive
            .tokens
            .iter()
            .map(|token| ScoreDirectiveTokenModel {
                span: token.span,
                kind: match &token.kind {
                    crate::parse::field::ScoreDirectiveTokenKind::Voice(id) => {
                        ScoreDirectiveTokenKindModel::Voice(id.clone())
                    }
                    crate::parse::field::ScoreDirectiveTokenKind::GroupStart(ch) => {
                        ScoreDirectiveTokenKindModel::GroupStart(*ch)
                    }
                    crate::parse::field::ScoreDirectiveTokenKind::GroupEnd(ch) => {
                        ScoreDirectiveTokenKindModel::GroupEnd(*ch)
                    }
                    crate::parse::field::ScoreDirectiveTokenKind::StaffSeparator => {
                        ScoreDirectiveTokenKindModel::StaffSeparator
                    }
                    crate::parse::field::ScoreDirectiveTokenKind::MeasureSeparator => {
                        ScoreDirectiveTokenKindModel::MeasureSeparator
                    }
                    crate::parse::field::ScoreDirectiveTokenKind::FloatingVoiceMarker => {
                        ScoreDirectiveTokenKindModel::FloatingVoiceMarker
                    }
                },
            })
            .collect(),
    }
}

fn preserved_directive_model(syntax: &PreservedDirectiveSyntax) -> PreservedDirective {
    PreservedDirective {
        name: TextLine {
            text: syntax.name.value.clone(),
            span: syntax.name.span,
        },
        value: TextLine {
            text: syntax.value.value.clone(),
            span: syntax.value.span,
        },
        span: syntax.span,
    }
}

pub(crate) fn music_code_span(line: &crate::syntax::tune::ClassifiedLine) -> Span {
    let mut end = line.text_span.end;
    if let Some(comment_span) = line.trailing_comment {
        end = end.min(comment_span.start);
    }
    if let ScoreLineBreak::Suppressed { marker_span } = line.score_line_break {
        end = end.min(marker_span.start);
    }
    Span::new(line.text_span.start, end)
}

pub(crate) fn meter_duration(kind: &MeterKind) -> Option<Fraction> {
    match kind {
        MeterKind::CommonTime => Some(Fraction::new(4, 4)),
        MeterKind::CutTime => Some(Fraction::new(2, 2)),
        MeterKind::Fraction {
            numerator,
            denominator,
        } => Some(Fraction::new(*numerator, *denominator)),
        MeterKind::None | MeterKind::Complex => None,
    }
}




fn barline_lowering_kinds(barline: &BarlineSyntax) -> Vec<BarlineKind> {
    let raw = barline.raw.strip_prefix('.').unwrap_or(&barline.raw);
    if barline.kind == BarlineKind::RepeatStart {
        if raw.starts_with("||") {
            return vec![BarlineKind::Double, BarlineKind::RepeatStart];
        }
        if raw.starts_with("[|") {
            return vec![BarlineKind::Initial, BarlineKind::RepeatStart];
        }
    }
    vec![barline.kind]
}








pub(crate) fn default_tuplet_q(p: u32) -> u32 {
    match p {
        2 | 4 | 8 => 3,
        _ => 2,
    }
}


pub(crate) fn invalid_tuplet_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.invalid_tuplet",
        "Tuplet specifier is outside the supported ABC range",
        span,
    )
    .with_spec_reference(abc_tuplet_reference())
    .with_recovery_note(RecoveryNote::new(
        "The tuplet syntax was preserved and ignored during lowering.",
    ))
}




fn barline_export_policy_info(span: Span, kind: BarlineKind) -> Diagnostic {
    Diagnostic::new(
        Severity::Info,
        "abc.musicxml.barline_policy",
        match kind {
            BarlineKind::Dotted => "Dotted barline is exported as a MusicXML dotted bar-style",
            BarlineKind::Invisible => "Invisible barline is exported as a MusicXML none bar-style",
            _ => "Barline export policy applied",
        },
        span,
    )
    .with_spec_reference(abc_barline_reference())
}

fn free_meter_multirest_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.multirest.free_meter",
        "Multi-measure rest in free meter has no measure duration; recovered using unit note length",
        span,
    )
    .with_spec_reference(abc_rest_reference())
    .with_recovery_note(RecoveryNote::new(
        "The rest count was preserved and each measure was lowered as one unit note length.",
    ))
}

fn overlay_incomplete_measure_warning(
    span: Span,
    actual: Fraction,
    expected: Fraction,
) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.voice.overlay_incomplete_measure",
        format!(
            "Overlay voice duration {}/{} is shorter than the measure-local duration {}/{}",
            actual.numerator, actual.denominator, expected.numerator, expected.denominator
        ),
        span,
    )
    .with_spec_reference(abc_overlay_reference())
    .with_recovery_note(RecoveryNote::new(
        "The overlay segment was preserved as a temporary measure-local voice.",
    ))
}

fn overlay_overfull_measure_warning(
    span: Span,
    actual: Fraction,
    expected: Fraction,
) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.voice.overlay_overfull_measure",
        format!(
            "Overlay voice duration {}/{} is longer than the measure-local duration {}/{}",
            actual.numerator, actual.denominator, expected.numerator, expected.denominator
        ),
        span,
    )
    .with_spec_reference(abc_overlay_reference())
    .with_recovery_note(RecoveryNote::new(
        "The overlay segment was preserved as a temporary measure-local voice.",
    ))
}

pub(crate) fn lyric_syllable_count_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.lyric.syllable_count",
        "Lyric syllable count does not match the available notes",
        span,
    )
    .with_spec_reference(abc_lyric_reference())
    .with_recovery_note(RecoveryNote::new(
        "The excess lyric token was preserved but not aligned to a note.",
    ))
}

pub(crate) fn symbol_count_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.symbol.count",
        "Symbol line has more symbols than available notes",
        span,
    )
    .with_spec_reference(abc_symbol_reference())
    .with_recovery_note(RecoveryNote::new(
        "The excess symbol was preserved but not aligned to a note.",
    ))
}

fn invalid_meter_change_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.field.invalid_m",
        "Invalid M: field value was ignored during lowering",
        span,
    )
    .with_spec_reference(abc_field_reference())
    .with_recovery_note(RecoveryNote::new(
        "Lowering continued with the previous valid meter.",
    ))
}

fn unsupported_complex_meter_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.meter.unsupported_complex",
        "Complex meter is preserved but has no fixed measure duration yet",
        span,
    )
    .with_spec_reference(abc_field_reference())
    .with_recovery_note(RecoveryNote::new(
        "Measure construction continued as free meter until a supported meter appears.",
    ))
}

fn invalid_key_change_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.field.invalid_k",
        "Invalid K: field value was ignored during lowering",
        span,
    )
    .with_spec_reference(abc_field_reference())
    .with_recovery_note(RecoveryNote::new(
        "Lowering continued with the previous valid key signature.",
    ))
}



pub(crate) fn abc_barline_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.8 repeat/bar symbols")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_rest_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.5 rests")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_chord_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.11 chords")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_tuplet_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.13 tuplets")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_broken_rhythm_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.7 broken rhythm")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_slur_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.10 ties and slurs")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_overlay_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 7.4 voice overlay")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_lyric_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 5.1 lyrics alignment")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_symbol_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.15 symbol lines")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_field_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 information fields")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RestVisibility;
    use crate::options::ParseOptions;
    use crate::parse::{parse_document, parse_tune_report_from_document};
    use crate::syntax::{MalformedSyntax, MalformedSyntaxKind};

    fn events_for(source: &str) -> (Vec<Event>, Vec<Diagnostic>) {
        let document = parse_document(source, ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);
        (
            report.value.expect("expected tune").events,
            report.diagnostics,
        )
    }

    fn count_diagnostics(diagnostics: &[Diagnostic], code: &'static str) -> usize {
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == code)
            .count()
    }

    #[test]
    fn normalizes_pitch_case_and_mixed_octave_marks() {
        let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\nC C' c C,',\n");

        assert!(diagnostics.is_empty());
        let octaves = events
            .iter()
            .filter_map(|event| match event {
                Event::Note { octave, .. } => Some(*octave),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(octaves, vec![4, 5, 5, 3]);
    }

    #[test]
    fn recovers_standalone_octave_marks_without_attaching_to_neighbor_notes() {
        let document_report = parse_document("X:1\nL:1/8\nK:C\n' , C\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(&document_report.diagnostics, "abc.music.malformed_octave"),
            2
        );

        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let malformed = tune_music.lines[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MusicItem::Malformed(item) => Some(item),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(malformed.len(), 2);
        assert!(malformed.iter().all(|item| !item.span.is_empty()));

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let events = tune_report.value.expect("expected tune").events;
        assert!(matches!(
            events.as_slice(),
            [Event::Note {
                step: 'C',
                octave: 4,
                accidental: None,
                ..
            }]
        ));
    }

    #[test]
    fn preserves_explicit_accidentals_in_semantic_events() {
        let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\n^C _D =E ^^F __G\n");

        assert!(diagnostics.is_empty());
        let accidentals = events
            .iter()
            .filter_map(|event| match event {
                Event::Note { accidental, .. } => Some(*accidental),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            accidentals,
            vec![
                Some(Accidental::Sharp),
                Some(Accidental::Flat),
                Some(Accidental::Natural),
                Some(Accidental::DoubleSharp),
                Some(Accidental::DoubleFlat),
            ]
        );
    }

    #[test]
    fn recovers_dangling_accidentals_without_leaking_into_later_notes() {
        let document_report = parse_document("X:1\nL:1/8\nK:C\n^ _ = C\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(
                &document_report.diagnostics,
                "abc.music.malformed_accidental"
            ),
            3
        );

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let events = tune_report.value.expect("expected tune").events;
        assert!(matches!(
            events.as_slice(),
            [Event::Note {
                step: 'C',
                accidental: None,
                ..
            }]
        ));
    }

    #[test]
    fn lowers_fractional_lengths_and_slash_shorthand() {
        let document = parse_document(
            "X:1\nL:1/8\nK:C\nA2 A/ A// A3/2 A/4\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert!(report.diagnostics.is_empty());
        assert_eq!(tune.divisions, 8);
        let durations = tune
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Note { duration, .. } => Some(*duration),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(durations, vec![8, 2, 1, 6, 1]);
    }

    #[test]
    fn recovers_malformed_lengths_and_preserves_valid_neighbors() {
        let document_report =
            parse_document("X:1\nL:1/8\nK:C\nA0 B/0 C 3 / D\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(&document_report.diagnostics, "abc.music.malformed_length"),
            4
        );

        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let malformed_spans = tune_music.lines[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MusicItem::Malformed(item) => Some(item.span),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(malformed_spans.len(), 2);
        assert!(
            malformed_spans
                .iter()
                .all(|span| document_report.value.source.slice(*span).is_some())
        );

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let tune = tune_report.value.expect("expected tune");
        let durations = tune
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Note { duration, .. } => Some(*duration),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(durations, vec![4, 4, 4, 4]);
    }

    #[test]
    fn lowers_multi_measure_rests_in_known_and_free_meter() {
        let (known_events, known_diagnostics) = events_for("X:1\nM:2/4\nL:1/8\nK:C\nZ2 X\n");
        assert!(known_diagnostics.is_empty());
        let known_durations = known_events
            .iter()
            .filter_map(|event| match event {
                Event::Rest {
                    duration,
                    visibility,
                    ..
                } => Some((*duration, *visibility)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            known_durations,
            vec![
                (32, RestVisibility::Visible),
                (16, RestVisibility::Invisible),
            ]
        );

        let document =
            parse_document("X:1\nM:none\nL:1/8\nK:C\nZ3\n", ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.multirest.free_meter")
        );
        assert_eq!(tune.events.len(), 1);
        assert!(matches!(tune.events[0], Event::Rest { duration: 12, .. }));
    }

    #[test]
    fn lowers_visible_invisible_rests_and_spacers() {
        let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\nz x y C\n");

        assert!(diagnostics.is_empty());
        let rests = events
            .iter()
            .filter_map(|event| match event {
                Event::Rest {
                    visibility,
                    duration,
                    ..
                } => Some((*visibility, *duration)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rests,
            vec![(RestVisibility::Visible, 4), (RestVisibility::Invisible, 4),]
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, Event::Spacer { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn malformed_rest_lengths_recover_to_safe_durations() {
        let document_report = parse_document("X:1\nL:1/8\nK:C\nz0 x/0\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(&document_report.diagnostics, "abc.music.malformed_length"),
            2
        );

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let rests = tune_report
            .value
            .expect("expected tune")
            .events
            .into_iter()
            .filter_map(|event| match event {
                Event::Rest {
                    visibility,
                    duration,
                    ..
                } => Some((visibility, duration)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rests,
            vec![(RestVisibility::Visible, 4), (RestVisibility::Invisible, 4),]
        );
    }

    #[test]
    fn lowers_basic_double_and_repeat_barlines() {
        let (events, diagnostics) = events_for("X:1\nK:C\nC|D||E|:F:|G::A[|B|]c\n");

        assert!(diagnostics.is_empty());
        let barlines = events
            .iter()
            .filter_map(|event| match event {
                Event::Barline { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            barlines,
            vec![
                BarlineKind::Regular,
                BarlineKind::Double,
                BarlineKind::RepeatStart,
                BarlineKind::RepeatEnd,
                BarlineKind::RepeatBoth,
                BarlineKind::Initial,
                BarlineKind::Final,
            ]
        );
    }

    #[test]
    fn recovers_invalid_barline_fragments_as_skipped_malformed_items() {
        let document_report = parse_document("X:1\nK:C\nC : D\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(&document_report.diagnostics, "abc.music.invalid_barline"),
            1
        );

        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        assert!(tune_music.lines[0].items.iter().any(|item| matches!(
            item,
            MusicItem::Malformed(MalformedSyntax {
                kind: MalformedSyntaxKind::InvalidBarline,
                ..
            })
        )));

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let notes = tune_report
            .value
            .expect("expected tune")
            .events
            .into_iter()
            .filter(|event| matches!(event, Event::Note { .. }))
            .count();
        assert_eq!(notes, 2);
    }

    #[test]
    fn parses_liberal_dotted_and_invisible_barlines_with_diagnostics() {
        let report = parse_document("X:1\nK:C\nC |[| D .| E [|] F\n", ParseOptions::default());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.barline.liberal")
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.barline.policy")
        );

        let tune_report = parse_tune_report_from_document(&report.value);
        assert!(
            tune_report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.musicxml.barline_policy")
        );
    }

    #[test]
    fn unclosed_inline_fields_groups_and_strings_are_recoverable_syntax() {
        let document_report = parse_document(
            "X:1\nK:C\nC [M:3/4\nD {ef\nE \"Am\nF [CE\nG\n",
            ParseOptions::default(),
        );

        for code in [
            "abc.music.unclosed_inline_field",
            "abc.music.unclosed_grace",
            "abc.music.unclosed_quoted_text",
            "abc.music.unclosed_chord",
        ] {
            assert!(
                document_report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == code),
                "expected diagnostic {code}"
            );
        }

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let notes = tune_report
            .value
            .expect("expected tune")
            .events
            .into_iter()
            .filter_map(|event| match event {
                Event::Note { step, .. } => Some(step),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(notes, vec!['C', 'D', 'E', 'F', 'G']);
    }

    #[test]
    fn parses_spec_attachment_order_around_note_group() {
        let document_report =
            parse_document("X:1\nL:1/8\nK:C\n\"Gm7\"v.=G,2\n", ParseOptions::default());
        assert!(document_report.diagnostics.is_empty());
        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let note = tune_music.lines[0]
            .items
            .iter()
            .find_map(|item| match item {
                MusicItem::Note(note) => Some(note),
                _ => None,
            })
            .expect("expected note");

        assert_eq!(note.attachments.chord_symbols[0].text, "Gm7");
        assert_eq!(
            note.attachments
                .decorations
                .iter()
                .map(|decoration| decoration.name.as_str())
                .collect::<Vec<_>>(),
            // `v` (down-bow shorthand) normalizes to its canonical decoration
            // name; `.` (staccato) is handled separately and stays as-is.
            vec!["downbow", "."]
        );
        assert_eq!(
            note.accidental.map(|accidental| accidental.sign),
            Some(Accidental::Natural)
        );
        assert_eq!(note.octave_marks[0].mark, OctaveMark::Lower);
        assert_eq!(
            note.length.as_ref().map(|length| length.raw.as_str()),
            Some("2")
        );
    }

    #[test]
    fn classifies_quoted_chord_symbols_and_annotations() {
        let document_report = parse_document(
            "X:1\nL:1/8\nK:C\n\"Am7\"C \"^above\"D \"_below\"E \"<left\"F \">right\"G \"@free\"A\n",
            ParseOptions::default(),
        );
        assert!(document_report.diagnostics.is_empty());
        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let notes = tune_music.lines[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MusicItem::Note(note) => Some(note),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(notes[0].attachments.chord_symbols[0].text, "Am7");
        let placements = notes[1..]
            .iter()
            .map(|note| note.attachments.annotations[0].kind)
            .collect::<Vec<_>>();
        assert_eq!(
            placements,
            vec![
                QuotedTextKind::Annotation(AnnotationPlacement::Above),
                QuotedTextKind::Annotation(AnnotationPlacement::Below),
                QuotedTextKind::Annotation(AnnotationPlacement::Left),
                QuotedTextKind::Annotation(AnnotationPlacement::Right),
                QuotedTextKind::Annotation(AnnotationPlacement::Free),
            ]
        );
    }

    #[test]
    fn parses_user_defined_and_legacy_decoration_symbols_from_dialect_state() {
        let user_symbol = parse_document("X:1\nU:W=!trill!\nK:C\nWC\n", ParseOptions::default());
        assert!(user_symbol.diagnostics.is_empty());
        let tune_music = user_symbol
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let note = tune_music.lines[0]
            .items
            .iter()
            .find_map(|item| match item {
                MusicItem::Note(note) => Some(note),
                _ => None,
            })
            .expect("expected note");
        assert_eq!(
            note.attachments.decorations[0].kind,
            DecorationKind::UserDefined
        );
        // The `U:`-defined symbol expands to its canonical decoration name so it
        // maps through the same export path as the long-form `!trill!`.
        assert_eq!(note.attachments.decorations[0].name, "trill");

        let legacy_allowed = parse_document(
            "X:1\nI:decoration +\nK:C\n+trill+C\n",
            ParseOptions::default(),
        );
        assert!(legacy_allowed.diagnostics.is_empty());
        let tune_music = legacy_allowed
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let note = tune_music.lines[0]
            .items
            .iter()
            .find_map(|item| match item {
                MusicItem::Note(note) => Some(note),
                _ => None,
            })
            .expect("expected note");
        assert_eq!(
            note.attachments.decorations[0].kind,
            DecorationKind::LegacyNamed
        );

        let legacy_rejected = parse_document("X:1\nK:C\n+trill+C\n", ParseOptions::default());
        assert!(
            legacy_rejected
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.invalid_decoration")
        );
    }

    #[test]
    fn parses_chord_with_inside_and_outside_decorations() {
        let document_report =
            parse_document("X:1\nL:1/8\nK:C\n!trill![.CEG]\n", ParseOptions::default());
        assert!(document_report.diagnostics.is_empty());
        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let chord = tune_music.lines[0]
            .items
            .iter()
            .find_map(|item| match item {
                MusicItem::Chord(chord) => Some(chord),
                _ => None,
            })
            .expect("expected chord");

        assert_eq!(chord.attachments.decorations[0].name, "trill");
        assert_eq!(chord.members.len(), 3);
        assert_eq!(chord.members[0].note.attachments.decorations[0].name, ".");
    }

    #[test]
    fn lowers_chord_member_and_outer_duration_multipliers() {
        let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\n[C2E2G2]3\n");
        assert!(diagnostics.is_empty());
        let notes = events
            .iter()
            .filter_map(|event| match event {
                Event::Note {
                    step,
                    duration,
                    chord,
                    ..
                } => Some((*step, *duration, *chord)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            notes,
            vec![('C', 24, false), ('E', 24, true), ('G', 24, true)]
        );
    }

    #[test]
    fn variable_duration_chord_members_emit_diagnostic() {
        let document = parse_document("X:1\nL:1/8\nK:C\n[E2G,6]\n", ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.chord.variable_duration")
        );
    }

    #[test]
    fn broken_rhythm_is_transparent_across_grace_groups() {
        let (left_events, left_diagnostics) = events_for("X:1\nL:1/8\nK:C\nA<{g}A\n");
        let (right_events, right_diagnostics) = events_for("X:1\nL:1/8\nK:C\nA{g}<A\n");

        assert!(left_diagnostics.is_empty());
        assert!(right_diagnostics.is_empty());
        let durations = |events: Vec<Event>| {
            events
                .into_iter()
                .filter_map(|event| match event {
                    Event::Note { duration, .. } => Some(duration),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(durations(left_events), durations(right_events));
    }

    #[test]
    fn parses_staccato_triplet_without_spaces() {
        let document_report =
            parse_document("X:1\nL:1/8\nK:C\n(3.a.b.c\n", ParseOptions::default());
        assert!(document_report.diagnostics.is_empty());
        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");

        assert!(matches!(tune_music.lines[0].items[0], MusicItem::Tuplet(_)));
        let staccato_count = tune_music.lines[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MusicItem::Note(note) => Some(&note.attachments.decorations),
                _ => None,
            })
            .filter(|decorations| decorations.iter().any(|decoration| decoration.name == "."))
            .count();
        assert_eq!(staccato_count, 3);
    }

    #[test]
    fn parses_adjacent_repeat_endings_after_barlines() {
        let document_report = parse_document("X:1\nK:C\n:|2 C|1D A:|2B\n", ParseOptions::default());
        assert!(document_report.diagnostics.is_empty());
        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");

        let endings = tune_music.lines[0]
            .items
            .iter()
            .filter(|item| matches!(item, MusicItem::VariantEnding(_)))
            .count();
        let repeat_ends = tune_music.lines[0]
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    MusicItem::Barline(BarlineSyntax {
                        kind: BarlineKind::RepeatEnd,
                        ..
                    })
                )
            })
            .count();
        assert_eq!(endings, 3);
        assert_eq!(repeat_ends, 2);
    }

    #[test]
    fn parses_bracketed_variant_ending_lists_and_ranges() {
        let document_report = parse_document(
            "X:1\nK:C\n[1 C | [2 D | [1,3] E | [1-3] F | [1,3,5-7] G\n",
            ParseOptions::default(),
        );
        assert!(document_report.diagnostics.is_empty());
        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let endings = tune_music.lines[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MusicItem::VariantEnding(ending) => Some(ending),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(endings.len(), 5);
        assert_eq!(endings[0].endings.len(), 1);
        assert_eq!(endings[2].endings.len(), 2);
        assert!(matches!(
            endings[3].endings[0],
            VariantEndingPart::Range { .. }
        ));
        assert_eq!(endings[4].endings.len(), 3);
    }

    #[test]
    fn repeat_ending_shorthand_must_be_adjacent() {
        let legal = parse_document("X:1\nK:C\nC| [1D\n", ParseOptions::default());
        assert!(
            !legal
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.invalid_repeat_ending")
        );

        let spaced = parse_document("X:1\nK:C\nC| 1D\n", ParseOptions::default());
        assert!(
            spaced
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.invalid_repeat_ending")
        );
    }

    #[test]
    fn unclosed_slurs_are_recoverable_in_lowering() {
        let document = parse_document("X:1\nK:C\n(C D\n", ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.unclosed_slur")
        );
        assert_eq!(
            report
                .value
                .expect("expected tune")
                .events
                .iter()
                .filter(|event| matches!(event, Event::Note { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn non_music_lines_and_chords_do_not_leak_comments_or_directives() {
        let document_report = parse_document(
            "X:1\nT:ABC\n+:DEF\nK:C\n%%text GAB\n[CDE] C % FED\n",
            ParseOptions::default(),
        );
        let report = parse_tune_report_from_document(&document_report.value);
        let events = report.value.expect("expected tune").events;

        let notes = events
            .iter()
            .filter(|event| matches!(event, Event::Note { .. }))
            .count();
        assert_eq!(notes, 4);
    }

    #[test]
    fn lowers_sequential_body_voice_blocks_to_explicit_timelines() {
        let document =
            parse_document("X:1\nK:C\nV:1\nC D|\nV:2\nE F|\n", ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert_eq!(
            tune.voices
                .iter()
                .map(|voice| voice.id.value.as_str())
                .collect::<Vec<_>>(),
            vec!["1", "2"]
        );
        let note_counts = tune
            .voices
            .iter()
            .map(|voice| {
                voice
                    .measures
                    .iter()
                    .flat_map(|measure| &measure.events)
                    .filter(|event| matches!(event.kind, TimelineEventKind::Note { .. }))
                    .count()
            })
            .collect::<Vec<_>>();
        assert_eq!(note_counts, vec![2, 2]);
    }

    #[test]
    fn lowers_inline_voice_switches_to_interleaved_timelines() {
        let document = parse_document(
            "X:1\nK:C\n[V:T1] C D| [V:T2] E F|\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert_eq!(tune.voices.len(), 2);
        assert!(tune.voices.iter().any(|voice| voice.id.value == "T1"));
        assert!(tune.voices.iter().any(|voice| voice.id.value == "T2"));
        let inline_voice = document
            .music
            .tune(0)
            .expect("expected parsed tune music")
            .lines
            .first()
            .expect("expected music line")
            .items
            .iter()
            .find_map(|item| match item {
                MusicItem::InlineField(field) if field.code == 'V' => Some(field),
                _ => None,
            })
            .expect("expected inline V field");
        assert_eq!(inline_voice.value.value, "T1");
    }

    #[test]
    fn inline_field_after_barline_is_not_swallowed_into_a_liberal_barline() {
        // `|[M:3/8]` must parse as a plain barline followed by an inline field,
        // not as a liberal `|[` combined barline. The old greedy barline scan
        // ate the `[`, mangling the field and inserting a spurious empty bar.
        let document = parse_document(
            "X:1\nL:1/4\nM:6/8\nK:C\nC3|[M:3/8]E2E|[M:6/8]F2G|\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        let non_empty: Vec<usize> = tune.voices[0]
            .measures
            .iter()
            .map(|measure| {
                measure
                    .events
                    .iter()
                    .filter(|event| event.alignable)
                    .count()
            })
            .collect();
        assert_eq!(
            non_empty,
            vec![1, 2, 2],
            "no spurious empty measures: {non_empty:?}"
        );

        let line = document
            .music
            .tune(0)
            .expect("expected parsed tune music")
            .lines
            .first()
            .expect("expected music line");
        let inline_codes: Vec<char> = line
            .items
            .iter()
            .filter_map(|item| match item {
                MusicItem::InlineField(field) => Some(field.code),
                _ => None,
            })
            .collect();
        assert_eq!(inline_codes, vec!['M', 'M']);
    }

    #[test]
    fn aligns_postponed_and_adjacent_lyrics_under_abc21_cursor_rules() {
        let document = parse_document(
            "X:1\nK:C\nC D E F|\nG A B c|\nw: doh re mi fa sol la ti doh\nw: alt verse words here more text ok done\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let lyrics = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .flat_map(|event| &event.lyrics)
            .filter(|lyric| lyric.control == LyricControl::Syllable)
            .map(|lyric| (lyric.verse, lyric.text.as_str()))
            .collect::<Vec<_>>();

        assert!(lyrics.contains(&(1, "doh")));
        assert!(lyrics.contains(&(1, "doh")));
        assert!(lyrics.contains(&(2, "alt")));
    }

    #[test]
    fn empty_lyric_line_consumes_notes_and_later_lyrics_start_after_them() {
        let document = parse_document(
            "X:1\nK:C\nC D E F|\nw:\nG A B c|\nw: sol la ti doh\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let lyrics = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .filter_map(|event| {
                event
                    .lyrics
                    .iter()
                    .find(|lyric| lyric.control == LyricControl::Syllable)
                    .map(|lyric| lyric.text.as_str())
            })
            .collect::<Vec<_>>();

        assert_eq!(lyrics, vec!["sol", "la", "ti", "doh"]);
    }

    #[test]
    fn lyrics_skip_rests_spacers_grace_notes_and_bar_marker_advances() {
        let document = parse_document(
            "X:1\nK:C\nC z y {g}D|E F|\nw: one | two three\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let aligned = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .filter_map(|event| {
                event
                    .lyrics
                    .iter()
                    .find(|lyric| lyric.control == LyricControl::Syllable)
                    .map(|lyric| lyric.text.as_str())
            })
            .collect::<Vec<_>>();

        assert_eq!(aligned, vec!["one", "two", "three"]);
    }

    #[test]
    fn overlay_rewinds_to_previous_barline_and_warns_when_incomplete() {
        let document = parse_document("X:1\nL:1/8\nK:C\nC D & E|\n", ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.voice.overlay_incomplete_measure")
        );
        let overlay = &tune.voices[0].measures[0].overlays[0];
        assert_eq!(overlay.events.len(), 1);
        assert_eq!(overlay.events[0].onset, Fraction::zero());
    }

    #[test]
    fn symbol_lines_align_to_notes_and_preserve_symbol_kinds() {
        let document = parse_document(
            "X:1\nK:C\nC z D E F|\ns: \"C\" * !>! \"^slow\"\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let symbols = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .flat_map(|event| &event.symbols)
            .map(|symbol| (symbol.text.as_str(), symbol.kind))
            .collect::<Vec<_>>();

        assert_eq!(
            symbols,
            vec![
                ("C", AlignedSymbolKind::ChordSymbol),
                (">", AlignedSymbolKind::Decoration),
                ("^slow", AlignedSymbolKind::Annotation),
            ]
        );
    }

    #[test]
    fn preserves_score_directives_and_post_tune_words_in_lowered_tune() {
        let document = parse_document(
            "X:1\nK:C\n%%score (T1 T2)\nC\nW:after words\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert_eq!(tune.score_directives.len(), 1);
        assert_eq!(tune.score_directives[0].value.text, "(T1 T2)");
        assert_eq!(tune.post_tune_lyrics[0].text, "after words");
    }

    #[test]
    fn preserves_header_score_directive_and_header_words() {
        let document = parse_document(
            "X:1\nI:score (A B)\nW:header words\nK:C\nC\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert_eq!(tune.score_directives.len(), 1);
        assert_eq!(tune.score_directives[0].value.text, "(A B)");
        assert_eq!(tune.post_tune_lyrics[0].text, "header words");
    }

    #[test]
    fn body_voice_properties_override_header_definition_in_timeline() {
        let document = parse_document(
            "X:1\nV:1 name=Header\nK:C\nV:1 name=Body stem=down\nC\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert_eq!(
            tune.voices[0]
                .properties
                .name
                .as_ref()
                .map(|name| name.text.as_str()),
            Some("Body")
        );
        assert_eq!(
            tune.voices[0].properties.stem,
            Some(StemDirectionModel::Down)
        );
    }

    #[test]
    fn lyrics_controls_preserve_text_and_diagnose_excess_tokens() {
        let source = "X:1\nK:C\nC D E F G A|\nw: time__ of~the \\-dash * extra too\n";
        let document = parse_document(source, ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let note_lyrics = tune.voices[0].measures[0]
            .events
            .iter()
            .filter(|event| event.alignable)
            .map(|event| {
                event
                    .lyrics
                    .iter()
                    .map(|lyric| (lyric.control, lyric.text.as_str()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        assert_eq!(note_lyrics[0], vec![(LyricControl::Syllable, "time")]);
        assert_eq!(note_lyrics[1], vec![(LyricControl::Extender, "")]);
        assert_eq!(note_lyrics[2], vec![(LyricControl::Extender, "")]);
        assert_eq!(note_lyrics[3], vec![(LyricControl::Syllable, "of the")]);
        assert_eq!(note_lyrics[4], vec![(LyricControl::Syllable, "-dash")]);
        assert!(note_lyrics[5].is_empty());
        assert_eq!(
            count_diagnostics(&report.diagnostics, "abc.lyric.syllable_count"),
            2
        );
    }

    #[test]
    fn lyric_bar_marker_is_ignored_when_cursor_is_already_at_measure_boundary() {
        let document = parse_document(
            "X:1\nK:C\nC D|E F|\nw: one two | three four\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let aligned = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .filter_map(|event| {
                event
                    .lyrics
                    .iter()
                    .find(|lyric| lyric.control == LyricControl::Syllable)
                    .map(|lyric| lyric.text.as_str())
            })
            .collect::<Vec<_>>();

        assert_eq!(aligned, vec!["one", "two", "three", "four"]);
    }

    fn syllables_per_measure(source: &str) -> Vec<Vec<String>> {
        let document = parse_document(source, ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        tune.voices[0]
            .measures
            .iter()
            .map(|measure| {
                measure
                    .events
                    .iter()
                    .filter(|event| event.alignable)
                    .map(|event| {
                        event
                            .lyrics
                            .iter()
                            .find(|lyric| {
                                matches!(
                                    lyric.control,
                                    LyricControl::Syllable | LyricControl::Extender
                                )
                            })
                            .map(|lyric| lyric.text.clone())
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    }

    #[test]
    fn leading_bar_marker_advances_past_a_filled_first_measure() {
        // Per ABC 2.1 section 5.1 a `|` "advances to the next bar". When a lyric
        // line opens with `|`, the notes of the first measure carry no syllable
        // and the first word lands on the downbeat of the second measure. The
        // earlier cursor model wrongly treated the line-start boundary as an
        // already-synced barline and kept the word on the pickup note.
        let per_measure = syllables_per_measure("X:1\nL:1/4\nK:C\nG z|c d|e f|\nw: |Oh well\n");
        assert_eq!(per_measure[0], vec!["".to_owned()]);
        assert_eq!(per_measure[1], vec!["Oh".to_owned(), "well".to_owned()]);
    }

    #[test]
    fn consecutive_bar_markers_each_advance_one_measure() {
        // `|||` must skip three bars, not collapse into a single no-op at the
        // first boundary. The first word therefore lands in the fourth measure.
        let per_measure =
            syllables_per_measure("X:1\nL:1/4\nK:C\nG|c d|e f|g a|b c|\nw:|||All done\n");
        assert!(per_measure[0].iter().all(|text| text.is_empty()));
        assert!(per_measure[1].iter().all(|text| text.is_empty()));
        assert!(per_measure[2].iter().all(|text| text.is_empty()));
        assert_eq!(per_measure[3], vec!["All".to_owned(), "done".to_owned()]);
    }

    #[test]
    fn double_hyphen_holds_a_blank_note_instead_of_a_literal_dash() {
        // `tri--umph` spans three notes with the middle one blank (ABC 2.1
        // section 5.1: a hyphen preceded by another hyphen is a separate, empty
        // syllable). The middle note must not export the literal "-" text.
        let document = parse_document(
            "X:1\nL:1/4\nK:C\nc d e|\nw: tri--umph\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let texts = tune.voices[0].measures[0]
            .events
            .iter()
            .filter(|event| event.alignable)
            .map(|event| {
                event
                    .lyrics
                    .iter()
                    .find(|lyric| lyric.control == LyricControl::Syllable)
                    .map(|lyric| lyric.text.clone())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            texts,
            vec![Some("tri".to_owned()), None, Some("umph".to_owned())]
        );
    }

    #[test]
    fn symbol_bar_boundary_and_excess_symbol_are_diagnosed_without_realigning() {
        let document = parse_document(
            "X:1\nK:C\nC D|E F|\ns: \"C\" !>! | \"^slow\" !fermata! !extra!\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let aligned = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .flat_map(|event| &event.symbols)
            .map(|symbol| (symbol.text.as_str(), symbol.kind))
            .collect::<Vec<_>>();

        assert_eq!(
            aligned,
            vec![
                ("C", AlignedSymbolKind::ChordSymbol),
                (">", AlignedSymbolKind::Decoration),
                ("^slow", AlignedSymbolKind::Annotation),
                ("fermata", AlignedSymbolKind::Decoration),
            ]
        );
        assert_eq!(
            count_diagnostics(&report.diagnostics, "abc.symbol.count"),
            1
        );
    }

    #[test]
    fn lyrics_use_the_current_voice_cursor_in_interleaved_body_fields() {
        let document = parse_document(
            "X:1\nK:C\nV:1\nC D|\nw: one two\nV:2\nE F|\nw: three four\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let lyrics_for_voice = |voice_id: &str| {
            tune.voices
                .iter()
                .find(|voice| voice.id.value == voice_id)
                .expect("expected voice")
                .measures
                .iter()
                .flat_map(|measure| &measure.events)
                .flat_map(|event| &event.lyrics)
                .filter(|lyric| lyric.control == LyricControl::Syllable)
                .map(|lyric| lyric.text.as_str())
                .collect::<Vec<_>>()
        };

        assert_eq!(lyrics_for_voice("1"), vec!["one", "two"]);
        assert_eq!(lyrics_for_voice("2"), vec!["three", "four"]);
    }

    #[test]
    fn body_voice_field_can_switch_voice_and_carry_same_line_music() {
        let document = parse_document(
            "X:1\nL:1/8\nK:C\nV:1 C D|\nV:2 E F|\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        assert!(!report.has_errors());
        let tune = report
            .value
            .as_ref()
            .expect("expected same-line voice music");
        let notes_for_voice = |voice_id: &str| {
            tune.voices
                .iter()
                .find(|voice| voice.id.value == voice_id)
                .expect("expected voice")
                .measures
                .iter()
                .flat_map(|measure| &measure.events)
                .filter_map(|event| match &event.kind {
                    TimelineEventKind::Note { step, .. } => Some(*step),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };

        assert_eq!(notes_for_voice("1"), vec!['C', 'D']);
        assert_eq!(notes_for_voice("2"), vec!['E', 'F']);
    }

    #[test]
    fn body_voice_properties_are_not_treated_as_same_line_music() {
        let document =
            parse_document("X:1\nK:C\nV:1 clef=treble\nC D|\n", ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected following music line");
        let voice = tune
            .voices
            .iter()
            .find(|voice| voice.id.value == "1")
            .expect("expected voice");

        assert_eq!(
            voice
                .properties
                .clef
                .as_ref()
                .map(|clef| clef.text.as_str()),
            Some("treble")
        );
        assert_eq!(
            voice.measures[0]
                .events
                .iter()
                .filter(|event| matches!(event.kind, TimelineEventKind::Note { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn body_voice_property_words_are_not_treated_as_same_line_music() {
        let document = parse_document(
            "X:1\nK:C\nV:1 Program 1 110 alto\nC D|\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected following music line");
        let voice = tune
            .voices
            .iter()
            .find(|voice| voice.id.value == "1")
            .expect("expected voice");

        assert_eq!(
            count_diagnostics(&report.diagnostics, "abc.music.unknown_token"),
            0
        );
        assert_eq!(
            voice.measures[0]
                .events
                .iter()
                .filter(|event| matches!(event.kind, TimelineEventKind::Note { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn overfull_overlay_measure_duration_emits_diagnostic() {
        let document = parse_document("X:1\nL:1/8\nK:C\nC & D E|\n", ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);

        assert_eq!(
            count_diagnostics(&report.diagnostics, "abc.voice.overlay_overfull_measure"),
            1
        );
    }

    #[test]
    fn ampersand_in_lyric_and_symbol_lines_is_not_music_overlay_syntax() {
        let document = parse_document(
            "X:1\nK:C\nC D E|\nw: Tom & Jerry\ns: & * !>!\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let lyrics = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .flat_map(|event| &event.lyrics)
            .filter(|lyric| lyric.control == LyricControl::Syllable)
            .map(|lyric| lyric.text.as_str())
            .collect::<Vec<_>>();
        let symbols = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .flat_map(|event| &event.symbols)
            .map(|symbol| (symbol.text.as_str(), symbol.kind))
            .collect::<Vec<_>>();

        assert_eq!(lyrics, vec!["Tom", "&", "Jerry"]);
        assert_eq!(
            symbols,
            vec![
                ("&", AlignedSymbolKind::Raw),
                (">", AlignedSymbolKind::Decoration)
            ]
        );
        assert!(tune.voices[0].measures[0].overlays.is_empty());
    }

    fn tune_for(source: &str) -> (crate::model::Tune, Vec<Diagnostic>) {
        let document = parse_document(source, ParseOptions::default());
        let mut diagnostics = document.diagnostics;
        let report = parse_tune_report_from_document(&document.value);
        diagnostics.extend(report.diagnostics);
        (report.value.expect("expected tune"), diagnostics)
    }

    fn diagnostic_span<'a>(
        source: &'a str,
        diagnostics: &'a [Diagnostic],
        code: &'static str,
    ) -> &'a str {
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == code)
            .expect("expected diagnostic");
        &source[diagnostic.span.start..diagnostic.span.end]
    }

    fn semantic_note_events(tune: &crate::model::Tune) -> Vec<&TimedEvent> {
        tune.score.parts[0].voices[0]
            .events
            .iter()
            .filter(|event| matches!(event.kind, TimedEventKind::Note(_)))
            .collect()
    }

    #[test]
    fn semantic_score_marks_pickup_and_keeps_fixed_measure_numbers() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\nC|D E F G|A B c d|\n";
        let (tune, diagnostics) = tune_for(source);

        assert!(diagnostics.is_empty());
        let measures = &tune.score.parts[0].voices[0].measures;
        assert_eq!(measures[0].expected_duration, Some(Fraction::new(4, 4)));
        assert_eq!(measures[0].actual_duration, Fraction::new(1, 4));
        assert!(measures[0].pickup);
        assert_eq!(
            measures
                .iter()
                .map(|measure| measure.id.number)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(measures[1].actual_duration, Fraction::new(4, 4));
        assert!(measures[1].complete);
    }

    #[test]
    fn leading_repeat_start_stays_on_first_measure() {
        let source = "X:1\nM:3/4\nL:1/4\nK:C\n|: G c d | E D C |\n";
        let (tune, diagnostics) = tune_for(source);

        assert!(diagnostics.is_empty());
        let measures = &tune.score.parts[0].voices[0].measures;
        assert_eq!(measures.len(), 2);
        assert_eq!(measures[0].actual_duration, Fraction::new(3, 4));
        assert_eq!(measures[1].actual_duration, Fraction::new(3, 4));
        assert!(
            measures[0]
                .barlines
                .iter()
                .any(|barline| barline.kind == BarlineKind::RepeatStart)
        );
        let first_measure_steps = semantic_note_events(&tune)
            .into_iter()
            .take(3)
            .map(|event| match &event.kind {
                TimedEventKind::Note(note) => note.pitch.step,
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert_eq!(first_measure_steps, vec!['G', 'C', 'D']);
    }

    #[test]
    fn plain_leading_barline_does_not_create_empty_pickup_measure() {
        let source = "X:1\nM:4/4\nL:1/4\nK:C\n| E | F G A B |\n";
        let (tune, diagnostics) = tune_for(source);

        assert!(diagnostics.is_empty());
        let measures = &tune.score.parts[0].voices[0].measures;
        assert_eq!(measures.len(), 2);
        assert_eq!(measures[0].actual_duration, Fraction::new(1, 4));
        assert!(measures[0].pickup);
        assert_eq!(measures[1].actual_duration, Fraction::new(4, 4));
    }

    #[test]
    fn semantic_accidentals_propagate_within_measure_and_reset_at_barline() {
        let source = "X:1\nL:1/8\nK:C\n^F F|F\n";
        let (tune, diagnostics) = tune_for(source);

        assert!(diagnostics.is_empty());
        let alters = semantic_note_events(&tune)
            .into_iter()
            .map(|event| match &event.kind {
                TimedEventKind::Note(note) => note.pitch.alter,
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert_eq!(alters, vec![1, 1, 0]);
        assert!(tune.score.accidental_policy.reset_at_barlines);
    }

    #[test]
    fn semantic_tuplets_and_broken_rhythm_keep_rational_durations() {
        let source = "X:1\nL:1/8\nK:C\n(3CDE F>G\n";
        let (tune, diagnostics) = tune_for(source);

        assert!(diagnostics.is_empty());
        let durations = semantic_note_events(&tune)
            .into_iter()
            .map(|event| event.duration)
            .collect::<Vec<_>>();
        assert_eq!(
            durations,
            vec![
                Fraction::new(1, 12),
                Fraction::new(1, 12),
                Fraction::new(1, 12),
                Fraction::new(3, 16),
                Fraction::new(1, 16),
            ]
        );
    }

    #[test]
    fn semantic_voices_have_explicit_onsets_and_durations() {
        let source = "X:1\nL:1/4\nK:C\nV:1\nC D|\nV:2\nE2 F|\n";
        let (tune, diagnostics) = tune_for(source);

        assert!(diagnostics.is_empty());
        let voice = tune
            .score
            .parts
            .iter()
            .flat_map(|part| &part.voices)
            .find(|voice| voice.id.value == "2")
            .expect("expected voice 2");
        let notes = voice
            .events
            .iter()
            .filter(|event| matches!(event.kind, TimedEventKind::Note(_)))
            .map(|event| (event.onset, event.duration))
            .collect::<Vec<_>>();
        assert_eq!(
            notes,
            vec![
                (Fraction::zero(), Fraction::new(2, 4)),
                (Fraction::new(2, 4), Fraction::new(1, 4)),
            ]
        );
    }

    #[test]
    fn semantic_lyrics_symbols_and_prefix_attachments_stay_on_intended_event() {
        let source = "X:1\nL:1/8\nK:C\n\"Am\"!trill!C D\nw: one two\ns: \"C\" !>!\n";
        let (tune, diagnostics) = tune_for(source);

        assert!(diagnostics.is_empty());
        let notes = semantic_note_events(&tune);
        let first = notes[0];
        assert_eq!(first.attachments.chord_symbols[0].text, "Am");
        assert_eq!(first.attachments.decorations[0].name, "trill");
        assert_eq!(first.attachments.lyrics[0].text, "one");
        assert_eq!(first.attachments.symbols[0].text, "C");
    }

    #[test]
    fn broken_rhythm_without_neighbors_diagnoses_and_keeps_timing_stable() {
        let source = "X:1\nL:1/8\nK:C\n< C D >|\n";
        let (tune, diagnostics) = tune_for(source);

        assert_eq!(
            diagnostic_span(source, &diagnostics, "abc.music.broken_rhythm.missing_left"),
            "<"
        );
        assert_eq!(
            diagnostic_span(
                source,
                &diagnostics,
                "abc.music.broken_rhythm.missing_right"
            ),
            ">"
        );
        let durations = semantic_note_events(&tune)
            .into_iter()
            .map(|event| event.duration)
            .collect::<Vec<_>>();
        assert_eq!(durations, vec![Fraction::new(1, 8), Fraction::new(1, 8)]);
        assert_eq!(
            tune.score.parts[0].voices[0].measures[0].actual_duration,
            Fraction::new(2, 8)
        );
    }

    #[test]
    fn short_tuplet_does_not_consume_notes_after_barline() {
        let source = "X:1\nL:1/8\nK:C\n(3C|D E|\n";
        let (tune, diagnostics) = tune_for(source);

        assert_eq!(
            diagnostic_span(source, &diagnostics, "abc.music.tuplet.too_few_notes"),
            "(3"
        );
        let durations = semantic_note_events(&tune)
            .into_iter()
            .map(|event| event.duration)
            .collect::<Vec<_>>();
        assert_eq!(
            durations,
            vec![
                Fraction::new(1, 12),
                Fraction::new(1, 8),
                Fraction::new(1, 8)
            ]
        );
        assert_eq!(
            tune.score.parts[0].voices[0].measures[1].actual_duration,
            Fraction::new(2, 8)
        );
    }

    #[test]
    fn unmatched_tie_preserves_unmerged_note_events() {
        let source = "X:1\nL:1/8\nK:C\nC- D E\n";
        let (tune, diagnostics) = tune_for(source);

        assert_eq!(
            diagnostic_span(source, &diagnostics, "abc.music.unmatched_tie"),
            "-"
        );
        let notes = semantic_note_events(&tune);
        assert_eq!(notes.len(), 3);
        assert!(notes.iter().all(|event| event.attachments.ties.is_empty()));
        assert_eq!(
            notes.iter().map(|event| event.duration).collect::<Vec<_>>(),
            vec![
                Fraction::new(1, 8),
                Fraction::new(1, 8),
                Fraction::new(1, 8)
            ]
        );
    }

    #[test]
    fn ties_resolve_across_barlines_without_changing_measure_timing() {
        let source = "X:1\nM:2/4\nL:1/4\nK:C\nC- | C D |\n";
        let (tune, diagnostics) = tune_for(source);

        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.unmatched_tie")
        );
        let notes = semantic_note_events(&tune);
        assert_eq!(notes.len(), 3);
        assert_eq!(
            notes
                .iter()
                .map(|event| event.attachments.ties.first().map(|tie| tie.role))
                .collect::<Vec<_>>(),
            vec![Some(TieRole::Start), Some(TieRole::Stop), None]
        );
        assert_eq!(
            notes[0].attachments.ties[0].pair_id,
            notes[1].attachments.ties[0].pair_id
        );
        assert_eq!(
            notes.iter().map(|event| event.duration).collect::<Vec<_>>(),
            vec![
                Fraction::new(1, 4),
                Fraction::new(1, 4),
                Fraction::new(1, 4)
            ]
        );
        let measures = &tune.score.parts[0].voices[0].measures;
        assert_eq!(
            measures
                .iter()
                .map(|measure| measure.actual_duration)
                .collect::<Vec<_>>(),
            vec![Fraction::new(1, 4), Fraction::new(2, 4)]
        );
        assert!(measures[0].pickup);
        assert!(measures[1].complete);
    }

    #[test]
    fn crossing_slurs_diagnose_but_preserve_notes() {
        let source = "X:1\nL:1/8\nK:C\n.(C (D .) E)\n";
        let (tune, diagnostics) = tune_for(source);

        assert_eq!(
            diagnostic_span(source, &diagnostics, "abc.music.crossing_slur"),
            ".)"
        );
        assert_eq!(semantic_note_events(&tune).len(), 3);
        assert_eq!(
            semantic_note_events(&tune)
                .into_iter()
                .map(|event| event.duration)
                .collect::<Vec<_>>(),
            vec![
                Fraction::new(1, 8),
                Fraction::new(1, 8),
                Fraction::new(1, 8)
            ]
        );
    }

    #[test]
    fn variable_chord_members_preserve_chord_shape_and_following_timing() {
        let source = "X:1\nL:1/8\nK:C\n[E2G,6] C\n";
        let (tune, diagnostics) = tune_for(source);

        assert_eq!(
            diagnostic_span(source, &diagnostics, "abc.music.chord.variable_duration"),
            "[E2G,6]"
        );
        let events = &tune.score.parts[0].voices[0].events;
        let chord = events
            .iter()
            .find_map(|event| match &event.kind {
                TimedEventKind::Chord(chord) => Some(chord),
                _ => None,
            })
            .expect("expected chord");
        assert_eq!(chord.members.len(), 2);
        assert_eq!(
            chord
                .members
                .iter()
                .map(|member| member.duration)
                .collect::<Vec<_>>(),
            vec![Fraction::new(2, 8), Fraction::new(6, 8)]
        );
        let note_after = events
            .iter()
            .find(|event| {
                matches!(event.kind, TimedEventKind::Note(_)) && event.onset == Fraction::new(2, 8)
            })
            .expect("expected following note");
        assert_eq!(note_after.onset, Fraction::new(2, 8));
    }

    #[test]
    fn overlay_incomplete_duration_does_not_shift_base_timeline() {
        let source = "X:1\nL:1/8\nK:C\nC D & E|\n";
        let (tune, diagnostics) = tune_for(source);

        assert_eq!(
            diagnostic_span(source, &diagnostics, "abc.voice.overlay_incomplete_measure"),
            "&"
        );
        let base_notes = semantic_note_events(&tune)
            .into_iter()
            .map(|event| (event.onset, event.duration))
            .collect::<Vec<_>>();
        assert_eq!(
            base_notes,
            vec![
                (Fraction::zero(), Fraction::new(1, 8)),
                (Fraction::new(1, 8), Fraction::new(1, 8)),
            ]
        );
    }

    #[test]
    fn lyric_count_mismatches_attach_valid_syllables_only() {
        let source = "X:1\nL:1/8\nK:C\nC D E F|\nw: one two\nw: a b c d e\n";
        let (tune, diagnostics) = tune_for(source);

        assert_eq!(
            count_diagnostics(&diagnostics, "abc.lyric.syllable_count"),
            2
        );
        let lyrics = semantic_note_events(&tune)
            .into_iter()
            .flat_map(|event| event.attachments.lyrics.iter())
            .filter(|lyric| lyric.control == LyricControl::Syllable)
            .map(|lyric| lyric.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(lyrics, vec!["one", "a", "two", "b", "c", "d"]);
        assert_eq!(semantic_note_events(&tune).len(), 4);
    }

    #[test]
    fn invalid_state_changes_keep_previous_state_and_valid_music() {
        let source = "X:1\nM:2/4\nL:1/8\nK:C\nC D|\nM:bad\nK:???\nL:bad\nE F|\n";
        let document = parse_document(source, ParseOptions::default());
        assert_eq!(
            diagnostic_span(source, &document.diagnostics, "abc.field.invalid_l"),
            "bad"
        );
        let report = parse_tune_report_from_document(&document.value);
        let tune = report.value.expect("expected tune");

        assert_eq!(
            diagnostic_span(source, &report.diagnostics, "abc.field.invalid_m"),
            "bad"
        );
        assert_eq!(
            diagnostic_span(source, &report.diagnostics, "abc.field.invalid_k"),
            "???"
        );
        let notes = semantic_note_events(&tune);
        assert_eq!(notes.len(), 4);
        assert!(
            notes
                .iter()
                .all(|event| event.duration == Fraction::new(1, 8))
        );
        assert_eq!(
            tune.score.parts[0].voices[0].measures[1].expected_duration,
            Some(Fraction::new(2, 4))
        );
    }

    #[test]
    fn malformed_repeat_ending_keeps_barline_timing_intact() {
        let source = "X:1\nL:1/8\nK:C\nC|[1- D|E|\n";
        let document = parse_document(source, ParseOptions::default());
        assert_eq!(
            diagnostic_span(
                source,
                &document.diagnostics,
                "abc.music.invalid_repeat_ending"
            ),
            // `|[1-` parses as a barline plus the `[1` variant ending, so the
            // malformed-ending diagnostic spans the whole `[1-`.
            "[1-"
        );
        let report = parse_tune_report_from_document(&document.value);
        let tune = report.value.expect("expected tune");

        assert_eq!(semantic_note_events(&tune).len(), 3);
        assert_eq!(
            tune.score.parts[0].voices[0]
                .measures
                .iter()
                .map(|measure| measure.actual_duration)
                .collect::<Vec<_>>(),
            vec![
                Fraction::new(1, 8),
                Fraction::new(1, 8),
                Fraction::new(1, 8)
            ]
        );
    }

    #[test]
    fn unsupported_directive_is_metadata_not_music() {
        let source = "X:1\nK:C\n%%foo bar\nC D|\n";
        let (tune, diagnostics) = tune_for(source);

        assert_eq!(
            diagnostic_span(source, &diagnostics, "abc.directive.unsupported"),
            "foo"
        );
        assert_eq!(tune.score.metadata.preserved_directives[0].name.text, "foo");
        assert_eq!(semantic_note_events(&tune).len(), 2);
        assert_eq!(
            tune.score.parts[0].voices[0].measures[0].actual_duration,
            Fraction::new(2, 8)
        );
    }
}
