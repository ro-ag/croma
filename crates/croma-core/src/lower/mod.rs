pub(crate) mod accidental;
mod align;
pub(crate) mod diagnostics;
mod semantic;
mod tempo;
pub(crate) mod tie;
pub(crate) mod timeline;
pub(crate) mod tuplet;
pub(crate) mod voice;

pub(crate) use crate::lower::diagnostics::*;
pub(crate) use crate::lower::voice::{
    ActiveTuplet, CompletedTuplet, LoweredEvent, LoweringState, PendingTie, is_note_atom,
    lowered_timed_note, note_signature,
};

use crate::diagnostic::{Diagnostic, Span};
use crate::lower::accidental::accidental_from_field_sign;
use crate::lower::align::{align_lyrics, align_symbols};
use crate::lower::semantic::semantic_voice_from_timeline;
use crate::lower::tempo::parse_tempo_model;
use crate::lower::timeline::build_voice_timeline;
use crate::model::{
    AccidentalPolicy, AccidentalScope, BarlineKind, Event, EventAttachments, Fraction,
    KeyAccidentalModel, KeySignatureModel, LoweredEventAtom, LoweredEventAtomKind, MeterModel,
    Part, PartId, PreservedDirective, Score, ScoreDirectiveModel, ScoreDirectiveTokenKindModel,
    ScoreDirectiveTokenModel, ScoreMetadata, Staff, StaffId, StemDirectionModel, TextLine,
    TimelineEventKind, VoiceId, VoicePropertiesModel, VoiceTimeline, lcm,
};
use crate::parse::ParseReport;
use crate::parse::field::{
    FieldState, KeyMode, KeySignature, KeyTonicAccidental, Meter, MeterKind, Spanned,
    StemDirection, UnitNoteLength, VoiceDefinition, VoiceProperties,
};
use crate::syntax::tune::ScoreLineBreak;
use crate::syntax::{
    BarlineSyntax, InlineFieldSyntax, LyricLineSyntax, MusicFieldLine, MusicFieldLineKind,
    MusicItem, MusicLine, ParsedTuneMusic, PreservedDirectiveSyntax, ScoreDirectiveSyntax,
    SymbolLineSyntax,
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

        // The tune header `K:` may carry clef/octave/middle/transpose modifiers
        // (ABC 2.1 §4.6, e.g. `K: Dm octave=1`). Unlike body `K:` lines, the
        // header key is seeded directly rather than via `apply_key_change`, so
        // merge its modifiers into the initial (current) voice here. In a
        // single-voice tune that is the whole tune; abc2xml does the same.
        if let Some(key) = field_state.key.as_ref() {
            let key_props = key_clef_properties_model(&key.value.properties);
            if key_props != VoicePropertiesModel::default() {
                merge_voice_properties(&mut lowering.current_state().properties, key_props);
            }
        }
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
        // A meter change is NOT a bar line: per ABC 2.1 §11.3
        // (`%%propagate-accidentals` default `pitch`) an explicit accidental
        // applies to same-pitch notes until the end of the bar, so the
        // measure accidental ledger must survive a mid-tune `M:` field.
        for voice in &mut self.voices {
            voice.finish_open_tuplets_at_boundary();
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
        // A K: field may carry clef/octave/middle/transpose modifiers
        // (ABC 2.1 §4.6, e.g. `K:C treble+8`, `K: Dm octave=1`). These scope to
        // the CURRENT voice only — exactly like abc2xml's doClef on K: fields —
        // and must not broadcast to the other voices. Merge them into the active
        // voice's properties so `voice_octave_shift` applies the resulting shift
        // to the notes that follow.
        let key_props = key_clef_properties_model(&key.value.properties);
        if key_props != VoicePropertiesModel::default() {
            merge_voice_properties(&mut self.current_state().properties, key_props);
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
        self.apply_inline_key_clef_properties(&key.value);
    }

    /// Merge clef/octave/middle/transpose modifiers carried on a `[K:..]` field
    /// into the current voice's properties (ABC 2.1 §4.6). Scoped to the active
    /// voice, like a whole-line K: change.
    fn apply_inline_key_clef_properties(&mut self, key: &KeySignature) {
        let key_props = key_clef_properties_model(&key.properties);
        if key_props != VoicePropertiesModel::default() {
            merge_voice_properties(&mut self.current_state().properties, key_props);
        }
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
                if let Some(unit) = crate::parse::field::parse_unit_note_length(&inline.value.value)
                {
                    self.apply_unit_change(&Spanned::new(unit, inline.value.span));
                }
            }
            'K' => {
                let key = crate::parse::field::parse_key(&inline.value.value, inline.value.span);
                // An inline `[K:..]` that carries no key information — e.g. a
                // clef-only change such as `[K:clef=bass]` — must leave the
                // current key signature untouched rather than reset it. Its
                // clef/octave/middle/transpose modifiers still apply, scoped to
                // the current voice (ABC 2.1 §4.6).
                if inline_key_changes_signature(&key) {
                    self.apply_inline_key_change(&Spanned::new(key, inline.value.span));
                } else if !key_is_invalid_for_lowering(&key) {
                    self.apply_inline_key_clef_properties(&key);
                }
            }
            'I' => {
                // `[I:...]` instructions are typeset/display directives (e.g.
                // abcm2ps `[I:setbarnb ...]`, `[I:tuplets ...]`) that do not
                // change how music lowers; abc2xml skips them too. Dropped,
                // but with a diagnostic — mirroring the header-line `I:` path.
                let directive = inline
                    .value
                    .value
                    .split_whitespace()
                    .next()
                    .unwrap_or_default();
                self.diagnostics
                    .push(inline_instruction_ignored_warning(directive, inline.span));
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
                    // A grace flushed ahead of its note but with no note before the
                    // bar is void (ABC 2.1 §4.20): drop the buffer at the boundary.
                    self.current_state().pending_grace_groups.clear();
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
                MusicItem::GraceGroup(grace) => {
                    // A grace group becomes a standalone item only when the parser
                    // flushed it ahead of its note (e.g. an intervening barline
                    // `{g}|` or inline field `{g}[M:3/4]c`; ties and overlays also
                    // flush). Per ABC 2.1 §4.20 the grace still attaches to the
                    // note it precedes, so buffer it and merge it into the next
                    // timed event. If a hard boundary (barline / voice switch /
                    // end of tune) arrives first, the buffer is dropped and the
                    // grace is voided.
                    self.current_state()
                        .pending_grace_groups
                        .push(grace.clone());
                }
                MusicItem::ChordSymbol(_)
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
        // A bar-line-only "phantom" measure is only safe to coalesce in a
        // single-voice tune. In multi-voice music a voice line that is just a
        // bar line is a legitimate *tacet* bar (the voice rests for that
        // measure) that abc2xml keeps so the voice stays measure-aligned with
        // its siblings, so it must not be collapsed.
        let single_voice = self.voices.len() == 1;
        let mut voices = self
            .voices
            .into_iter()
            .map(|voice| build_voice_timeline(voice, meter_duration, single_voice, diagnostics))
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
    if incoming.middle.is_some() {
        existing.middle = incoming.middle;
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
        middle: voice
            .parsed_properties
            .middle
            .as_ref()
            .map(text_line_from_spanned),
    }
}

/// Build a `VoicePropertiesModel` carrying only the clef/octave/middle/transpose
/// modifiers a `K:` field may declare (ABC 2.1 §4.6). All naming/stem fields are
/// left `None` so that merging into a voice never disturbs its name or stem — a
/// K: field never sets those.
fn key_clef_properties_model(properties: &VoiceProperties) -> VoicePropertiesModel {
    VoicePropertiesModel {
        clef: properties.clef.as_ref().map(text_line_from_spanned),
        octave: properties.octave.as_ref().map(text_line_from_spanned),
        transpose: properties.transpose.as_ref().map(text_line_from_spanned),
        middle: properties.middle.as_ref().map(text_line_from_spanned),
        ..VoicePropertiesModel::default()
    }
}

fn text_line_from_spanned(value: &Spanned<String>) -> TextLine {
    TextLine {
        text: value.value.clone(),
        span: value.span,
    }
}

/// Whether an inline `[K:..]` field actually specifies a key signature (and so
/// should be applied) rather than only carrying non-key information such as a
/// clef (`[K:clef=bass]`). A bare default — no tonic, no accidentals, plain
/// major mode — means "no key change", so it is left untouched.
fn inline_key_changes_signature(key: &KeySignature) -> bool {
    key.tonic.is_some() || !key.accidentals.is_empty() || key.mode != KeyMode::Major
}

pub(crate) fn extend_span(current: Span, next: Span) -> Span {
    if current.is_empty() {
        return next;
    }
    Span::new(current.start.min(next.start), current.end.max(next.end))
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

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
