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
use crate::lower::accidental::{accidental_from_field_sign, key_accidental_policy_from_model};
use crate::lower::align::{align_lyrics, align_symbols};
use crate::lower::semantic::semantic_voice_from_timeline;
use crate::lower::tempo::parse_tempo_model;
use std::collections::BTreeMap;

use crate::lower::timeline::build_voice_timeline;
use crate::model::{
    Accidental, AccidentalPolicy, AccidentalScope, AlignedLyric, BarlineKind, ClefChangeModel,
    Event, EventAttachments, Fraction, HarmonyKindText, KeyAccidentalModel, KeySignatureModel,
    LoweredEventAtom, LoweredEventAtomKind, LyricControl, MeterModel, MidiInstrumentModel,
    MusicXmlInstrumentRef, MusicXmlPartInstrumentModel, Part, PartId, PreservedDirective,
    RestVisibility, Score, ScoreDirectiveModel, ScoreDirectiveTokenKindModel,
    ScoreDirectiveTokenModel, ScoreMetadata, SlurRole, Staff, StaffId, StemDirectionModel,
    TempoBeat, TempoBeatRole, TempoModel, TextLine, TimelineEventKind, TupletRole, VoiceId,
    VoicePropertiesModel, VoiceTimeline, XVOICE_SLUR_PAIR_ID_BASE, lcm,
};
use crate::parse::ParseReport;
use crate::parse::field::{
    FieldState, KeyAccidental, KeyMode, KeySignature, KeyTonicAccidental, Meter, MeterKind,
    Spanned, StemDirection, UnitNoteLength, VoiceDefinition, VoiceProperties, parse_meter,
};
use crate::syntax::tune::ScoreLineBreak;
use crate::syntax::{
    AttachmentBundle, BarlineSyntax, InlineFieldSyntax, LyricLineSyntax, MusicFieldLine,
    MusicFieldLineKind, MusicItem, MusicLine, ParsedTuneMusic, PreservedDirectiveSyntax,
    ScoreDirectiveSyntax, SymbolLineSyntax,
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
        LoweredEvent::Untimed(_)
        | LoweredEvent::Overlay(_)
        | LoweredEvent::VariantEnding(_)
        | LoweredEvent::VariantEndingClose(_)
        | LoweredEvent::KeyChange(_)
        | LoweredEvent::MeterChange(_)
        | LoweredEvent::ClefChange(_)
        | LoweredEvent::TempoChange(_)
        | LoweredEvent::SectionLabel { .. }
        | LoweredEvent::MeasureNumber { .. } => divisions,
    });
    let events = all_lowered
        .into_iter()
        .filter_map(|event| match event {
            LoweredEvent::Timed(timed) => Some(timed.event.into_event(divisions)),
            LoweredEvent::Untimed(event) => Some(event.clone()),
            LoweredEvent::Overlay(_)
            | LoweredEvent::VariantEnding(_)
            | LoweredEvent::VariantEndingClose(_)
            | LoweredEvent::KeyChange(_)
            | LoweredEvent::MeterChange(_)
            | LoweredEvent::ClefChange(_)
            | LoweredEvent::TempoChange(_)
            | LoweredEvent::SectionLabel { .. }
            | LoweredEvent::MeasureNumber { .. } => None,
        })
        .collect();
    let meter_duration = lowering.meter_duration;
    let mut voices = lowering.into_voice_timelines(meter_duration, &mut diagnostics);
    align_lyrics(&mut voices, &lyric_lines, &mut diagnostics);
    align_symbols(&mut voices, &symbol_lines, &mut diagnostics);
    project_voice_midi(&mut voices, tune_music, field_state);
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
    /// Header key signature used to seed each voice when its stream first
    /// appears. Body `K:` changes are per-voice timeline events and must not
    /// rewrite this baseline for voices that have not reached the field.
    key: Option<KeySignature>,
    /// Verbatim header `K:`/`M:` display text — the dedupe baseline for
    /// no-op restatements.
    header_key_display: Option<String>,
    header_meter_display: Option<String>,
    /// Header meter duration used to seed each voice when its stream first
    /// appears. Body `M:` changes update only the current voice.
    meter_duration: Option<Fraction>,
    voices: Vec<LoweringState>,
    current_voice: String,
    source_order: u32,
    lyric_lines: Vec<VoicedLyricLine>,
    symbol_lines: Vec<VoicedSymbolLine>,
    diagnostics: Vec<Diagnostic>,
    /// Cross-voice slur carriers (`[I:croma-xvoice-slur pair=N role=..]`)
    /// re-pair across voices by their `pair=` id. This maps each carrier `pair`
    /// to the shared model `pair_id` allocated for that cross-voice slur, so the
    /// start (in one voice) and the stop (in another) land the SAME
    /// `SlurAttachment.pair_id` — the model shape a same-voice slur produces.
    /// Ids come from [`XVOICE_SLUR_PAIR_ID_BASE`] so they never collide with the
    /// small per-voice slur ids.
    xvoice_slur_pair_ids: Vec<(u32, u32)>,
    next_xvoice_slur_pair_id: u32,
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
            header_key_display: field_state.key.as_ref().map(|key| key.value.raw.clone()),
            header_meter_display: field_state
                .meter
                .as_ref()
                .map(|meter| meter.value.raw.clone()),
            meter_duration: field_state
                .meter
                .as_ref()
                .and_then(|meter| meter_duration(&meter.value)),
            voices: Vec::new(),
            current_voice: String::new(),
            source_order: 0,
            lyric_lines: Vec::new(),
            symbol_lines: Vec::new(),
            diagnostics: Vec::new(),
            xvoice_slur_pair_ids: Vec::new(),
            next_xvoice_slur_pair_id: XVOICE_SLUR_PAIR_ID_BASE,
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
            if key.value.compact_accidentals_ignored {
                lowering
                    .diagnostics
                    .push(compact_key_accidentals_ignored_warning(key.span));
            }
            if key.value.tonic_trailing_junk_ignored {
                lowering
                    .diagnostics
                    .push(key_tonic_trailing_junk_ignored_warning(key.span));
            }
            let key_props = key_clef_properties_model(&key.value.properties);
            if key_props != VoicePropertiesModel::default() {
                let state = lowering.current_state();
                merge_voice_properties(&mut state.initial_properties, key_props.clone());
                merge_voice_properties(&mut state.properties, key_props);
            }
        }
        lowering
    }

    fn apply_field(&mut self, field: &MusicFieldLine) {
        match &field.kind {
            MusicFieldLineKind::Meter(meter) => self.apply_meter_change(meter),
            MusicFieldLineKind::UnitNoteLength(unit) => self.apply_unit_change(unit),
            MusicFieldLineKind::Key(key) => self.apply_key_change(key),
            MusicFieldLineKind::Tempo(tempo) => self.apply_tempo_change(tempo),
            MusicFieldLineKind::Voice(voice) => self.switch_voice(voice.clone()),
            MusicFieldLineKind::Lyric(line) => self.lyric_lines.push(VoicedLyricLine {
                voice_id: self.current_voice.clone(),
                line: line.clone(),
            }),
            MusicFieldLineKind::Symbol(line) => self.symbol_lines.push(VoicedSymbolLine {
                voice_id: self.current_voice.clone(),
                line: line.clone(),
            }),
            MusicFieldLineKind::SectionLabel(label) => self.apply_section_label(label),
            MusicFieldLineKind::Unknown(value) => {
                if let Some(symbol) = parse_time_symbol_instruction(&value.value) {
                    self.current_state().pending_musicxml_time_symbol = Some(symbol);
                }
            }
            MusicFieldLineKind::PostTuneText(_)
            | MusicFieldLineKind::Score(_)
            | MusicFieldLineKind::Other => {}
        }
    }

    /// A body `P:` line or inline `[P:..]` (ABC 2.1 §4.3 section label): record a
    /// zero-duration section-label event at the current voice position so
    /// exporters reproduce the label in place (as a MusicXML `<rehearsal>`).
    fn apply_section_label(&mut self, label: &Spanned<String>) {
        self.current_state()
            .lowered
            .push(LoweredEvent::SectionLabel {
                label: label.value.clone(),
                span: label.span,
            });
    }

    fn apply_meter_change(&mut self, meter: &Spanned<Meter>) {
        self.apply_current_voice_meter_change(meter);
    }

    fn apply_current_voice_meter_change(&mut self, meter: &Spanned<Meter>) {
        let (preserve_restatement, time_symbol) = {
            let voice = self.current_state();
            let preserve = voice.pending_musicxml_meter_restatement;
            voice.pending_musicxml_meter_restatement = false;
            let time_symbol = voice.pending_musicxml_time_symbol.take();
            (preserve, time_symbol)
        };
        if !self.validate_meter_change(meter) {
            return;
        }
        // A meter change is NOT a bar line: per ABC 2.1 §11.3
        // (`%%propagate-accidentals` default `pitch`) an explicit accidental
        // applies to same-pitch notes until the end of the bar, so the
        // measure accidental ledger must survive a mid-tune `M:` field.
        let mut model = meter_model(meter);
        model.preserve_restatement = preserve_restatement;
        model.time_symbol = time_symbol;
        let has_time_symbol = model.time_symbol.is_some();
        let header = self.header_meter_display.clone();
        let duration = meter_duration(&meter.value);
        let voice = self.current_state();
        voice.finish_open_tuplets_at_boundary();
        voice.meter_duration = duration;
        // Record the change at the current voice's position so exporters can
        // reproduce it. A change to the voice's already-effective meter
        // (header included) records nothing: interleaved sources restate
        // `[M:..]` once per voice line, and re-applications would otherwise
        // stack duplicate events.
        if preserve_restatement
            || has_time_symbol
            || effective_meter_display(voice, header.as_deref()) != Some(model.display.as_str())
        {
            voice.lowered.push(LoweredEvent::MeterChange(model));
        }
    }

    /// Apply an inline `[M:..]` to the CURRENT voice only (matching abc2xml's
    /// per-voice scoping). Crucially, this makes the voice-blocked layout the
    /// ABC writer emits re-parse to the same per-voice events as interleaved
    /// sources: a global application would leak each voice's `[M:..]` token
    /// into every other voice at the wrong position.
    fn apply_inline_meter_change(&mut self, meter: &Spanned<Meter>) {
        self.apply_current_voice_meter_change(meter);
    }

    fn validate_meter_change(&mut self, meter: &Spanned<Meter>) -> bool {
        if meter_is_invalid_for_lowering(&meter.value) {
            self.diagnostics
                .push(invalid_meter_change_warning(meter.span));
            return false;
        }
        if matches!(meter.value.kind, MeterKind::Complex) && meter_duration(&meter.value).is_none()
        {
            self.diagnostics
                .push(unsupported_complex_meter_warning(meter.span));
        }
        true
    }

    /// A body `Q:` line or inline `[Q:..]` (ABC 2.1 §3.1.8 allows Q: in the
    /// tune body): record a zero-duration tempo-change event at the current
    /// voice position so exporters reproduce the change in place.
    fn apply_tempo_change(&mut self, tempo: &Spanned<String>) {
        let unit = self.unit;
        if crate::lower::tempo::has_unterminated_quote(&tempo.value) {
            self.diagnostics
                .push(unterminated_tempo_quote_warning(tempo.span));
        }
        if let Some(model) = parse_tempo_model(&tempo.value, tempo.span, unit) {
            self.current_state()
                .lowered
                .push(LoweredEvent::TempoChange(model));
        }
    }

    fn apply_unit_change(&mut self, unit: &Spanned<UnitNoteLength>) {
        // A body `L:` (or inline `[L:..]`) scopes to the CURRENT voice;
        // sibling voices and voices declared later keep the header unit
        // (`self.unit` stays the header value, which seeds new voices).
        // ABC 2.1 §7 is VOLATILE/silent on body-field voice scoping; the
        // abcm2ps/abc2midi/abc2xml convention — codified by ABC 2.2 — is
        // per-voice, and the old global broadcast silently under/over-filled
        // sibling voices' bars (tune_014637/005353).
        self.current_state().unit = unit.value.fraction.to_model_fraction();
    }

    fn apply_key_change(&mut self, key: &Spanned<KeySignature>) {
        self.apply_current_voice_key_change(key);
    }

    fn apply_current_voice_key_change(&mut self, key: &Spanned<KeySignature>) {
        // ABC 2.1 §3.1.14: a tonic-less `K:` carrying modifying accidentals
        // (`K:^F`) MODIFIES the prevailing key rather than restating it. Inherit
        // the active voice's tonic+mode and add the accidental, so `K:^F` over a
        // `K:Gmin` header becomes `K:Gm ^f` (the Bb/Eb base is kept and F# added),
        // not a tonic-less F#-only signature. A tonic-less `K:` with NO modifying
        // accidental is genuine junk (`K:???`) and still falls through to the
        // invalid-field reject below.
        if key.value.tonic.is_none()
            && !key.value.accidentals.is_empty()
            && let Some(prevailing) = self
                .current_state()
                .current_key
                .clone()
                .filter(|prev| prev.tonic.is_some())
        {
            let merged = Spanned::new(merge_key_modification(&prevailing, &key.value), key.span);
            self.apply_current_voice_key_change(&merged);
            return;
        }
        if key_is_invalid_for_lowering(&key.value) {
            self.diagnostics.push(invalid_key_change_warning(key.span));
            return;
        }
        self.warn_if_key_field_recovered(key);
        // A clef-only K: line (`K: clef=treble`) changes no key signature —
        // mirror the inline `[K:..]` guard: merge its clef properties into the
        // current voice but leave the key (and the recorded events) untouched.
        if !inline_key_changes_signature(&key.value) {
            self.apply_current_voice_key_properties(&key.value.properties);
            return;
        }
        let mut model = key_signature_model(key);
        let header = self.header_key_display.clone();
        {
            let voice = self.current_state();
            let preserve_restatement = voice.pending_musicxml_key_restatement;
            voice.pending_musicxml_key_restatement = false;
            model.preserve_restatement = preserve_restatement;
            voice.set_key(Some(&key.value));
            // Record the change at the current voice's position so exporters
            // can reproduce it. A change to the voice's already-effective key
            // records nothing (see the meter-change dedupe above; also
            // collapses no-op restatements identically on both generations,
            // keeping the round-trip stable). A MusicXML-origin restatement
            // carrier forces the no-op through so a foreign `<key>` survives.
            if preserve_restatement
                || effective_key_display(voice, header.as_deref()) != Some(model.display.as_str())
            {
                voice.lowered.push(LoweredEvent::KeyChange(model));
            }
        }
        // A K: field may carry clef/octave/middle/transpose modifiers
        // (ABC 2.1 §4.6, e.g. `K:C treble+8`, `K: Dm octave=1`). These scope to
        // the CURRENT voice only — exactly like abc2xml's doClef on K: fields —
        // and must not broadcast to the other voices. Merge them into the active
        // voice's properties so `voice_octave_shift` applies the resulting shift
        // to the notes that follow.
        self.apply_current_voice_key_properties(&key.value.properties);
    }

    /// Apply an inline `[K:..]` key change to the currently-active voice only.
    ///
    /// Per ABC 2.1 (§4.2, §7) an inline key change inside a voice line affects
    /// that voice from this point onward; the other voices keep their existing
    /// key signature. The active voice is the one `current_state()` resolves via
    /// `self.current_voice`.
    fn apply_inline_key_change(&mut self, key: &Spanned<KeySignature>) {
        self.apply_current_voice_key_change(key);
    }

    /// Merge clef/octave/middle/transpose modifiers carried on a `[K:..]` field
    /// into the current voice's properties (ABC 2.1 §4.6). Scoped to the active
    /// voice, like a whole-line K: change.
    fn apply_inline_key_clef_properties(&mut self, key: &KeySignature) {
        self.apply_current_voice_key_properties(&key.properties);
    }

    fn apply_current_voice_key_properties(&mut self, properties: &VoiceProperties) {
        let key_props = key_clef_properties_model(properties);
        if key_props == VoicePropertiesModel::default() {
            return;
        }
        let clef_change = key_clef_change_model(properties);
        let voice = self.current_state();
        if let Some(clef_change) = clef_change {
            let current_clef = voice
                .properties
                .clef
                .as_ref()
                .map(|clef| clef.text.as_str());
            if current_clef != Some(clef_change.clef.text.as_str()) {
                voice.lowered.push(LoweredEvent::ClefChange(clef_change));
            }
        }
        merge_voice_properties(&mut voice.properties, key_props);
    }

    /// Flag any lenient recovery the strict K: parser performed on a body/inline
    /// key field, so a recovered key never changes the signature silently:
    /// dropped no-space global accidentals (`K:D^f`) and discarded trailing tonic
    /// junk (`K:Bb,`). Both keep the valid base key; only the deviation is warned.
    fn warn_if_key_field_recovered(&mut self, key: &Spanned<KeySignature>) {
        if key.value.compact_accidentals_ignored {
            self.diagnostics
                .push(compact_key_accidentals_ignored_warning(key.span));
        }
        if key.value.tonic_trailing_junk_ignored {
            self.diagnostics
                .push(key_tonic_trailing_junk_ignored_warning(key.span));
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
                self.apply_inline_meter_change(&Spanned::new(meter, inline.value.span));
            }
            'L' => {
                if let Some(unit) = crate::parse::field::parse_unit_note_length(&inline.value.value)
                {
                    self.apply_unit_change(&Spanned::new(unit, inline.value.span));
                } else {
                    // An unparseable inline `[L:..]` keeps the previous unit note
                    // length; warn rather than drop it silently.
                    self.diagnostics
                        .push(invalid_unit_change_warning(inline.value.span));
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
                } else {
                    // Invalid inline `[K:..]` that names no signature and no clef:
                    // keep the prevailing key, but warn (mirrors the whole-line
                    // invalid-K: path; the strict parser never recovers silently).
                    self.diagnostics
                        .push(invalid_key_change_warning(inline.value.span));
                }
            }
            'Q' => {
                self.apply_tempo_change(&Spanned::new(
                    inline.value.value.clone(),
                    inline.value.span,
                ));
            }
            'P' => {
                // Inline `[P:..]` is a section label (ABC 2.1 §4.3): record a
                // zero-duration label event at the current voice position,
                // mirroring the body `P:` line and the inline `[Q:..]` path.
                self.apply_section_label(&Spanned::new(
                    inline.value.value.clone(),
                    inline.value.span,
                ));
            }
            // Inline `[I:..]` instruction field. This is the dispatch point for
            // croma's private `[I:croma-*]` CARRIERS — namespaced annotations that
            // round-trip MusicXML facts ABC 2.1 cannot natively express (per-note
            // instruments, functional harmony text, `<forward>` gaps, wide tuplets,
            // …). Each carrier has an emit builder in `to_abc.rs`, a
            // `parse_<thing>_instruction` below (`strip_prefix("croma-<name>")`), and
            // a re-emit site in `musicxml/`; the parsed fact is staged in a
            // `pending_musicxml_*` slot and drained onto the next event in
            // `lower/voice.rs`. The full namespace, syntax (incl. the `-hex=` rule for
            // `]`/`%`/control chars), and the per-carrier catalogue live in
            // `docs/carriers.md`. Each `if let Some(..) = parse_*` arm returns on a
            // match; an UNRECOGNISED `[I:..]` (incl. an unknown `croma-*`) falls
            // through to the display-directive tail below and is dropped with a
            // diagnostic — carriers are NOT preserved verbatim across croma versions.
            'I' => {
                if let Some(meter) =
                    parse_initial_meter_instruction(&inline.value.value, inline.value.span)
                {
                    let state = self.current_state();
                    state.meter_duration = meter.duration;
                    state.initial_meter = Some(meter);
                    return;
                }
                if let Some(key) =
                    parse_initial_key_instruction(&inline.value.value, inline.value.span)
                {
                    let state = self.current_state();
                    state.key_accidentals = key_accidental_policy_from_model(&key);
                    state.current_key = None;
                    state.initial_key = Some(key);
                    return;
                }
                if let Some(instrument) =
                    parse_note_instrument_instruction(&inline.value.value, inline.value.span)
                {
                    self.current_state().pending_musicxml_instrument = Some(instrument);
                    return;
                }
                if let Some(harmony_text) = parse_harmony_text_instruction(&inline.value.value) {
                    self.current_state().pending_musicxml_harmony_text = Some(harmony_text);
                    return;
                }
                if let Some(verse) = parse_lyric_extend_instruction(&inline.value.value) {
                    self.current_state()
                        .pending_musicxml_lyric_extends
                        .push(verse);
                    return;
                }
                if let Some(lyric) =
                    parse_lyric_duplicate_instruction(&inline.value.value, inline.value.span)
                {
                    self.current_state()
                        .pending_musicxml_lyric_duplicates
                        .push(lyric);
                    return;
                }
                if parse_musicxml_forward_instruction(&inline.value.value) {
                    self.current_state().pending_musicxml_forward = true;
                    return;
                }
                if let Some(duration) =
                    parse_musicxml_sequence_backup_instruction(&inline.value.value)
                {
                    self.current_state().pending_musicxml_sequence_backup = Some(duration);
                    return;
                }
                if let Some(tuplet) = parse_musicxml_tuplet_instruction(&inline.value.value) {
                    self.current_state().push_musicxml_tuplet(
                        tuplet.source_pair_id,
                        tuplet.actual_notes,
                        tuplet.normal_notes,
                        tuplet.role,
                        inline.value.span,
                    );
                    return;
                }
                if let Some(xvoice) = parse_xvoice_slur_instruction(&inline.value.value) {
                    // A slur whose ends are in different voices: `(`/`)` cannot
                    // span two `V:` streams, so each end rides a carrier. The
                    // shared `pair=` re-pairs the ends across voices onto ONE
                    // model `pair_id` (drained onto the next event as a
                    // `SlurAttachment`, the same shape a same-voice slur makes).
                    let pair_id = self.xvoice_slur_pair_id(xvoice.pair);
                    self.current_state().pending_xvoice_slurs.push((
                        pair_id,
                        xvoice.role,
                        inline.value.span,
                    ));
                    return;
                }
                if parse_musicxml_after_grace_instruction(&inline.value.value) {
                    self.current_state().pending_musicxml_after_grace = true;
                    return;
                }
                if let Some(clef) =
                    parse_clef_cursor_instruction(&inline.value.value, inline.value.span)
                {
                    self.current_state()
                        .lowered
                        .push(LoweredEvent::ClefChange(clef));
                    return;
                }
                if let Some(kind) = parse_barline_style_instruction(&inline.value.value) {
                    self.current_state().pending_musicxml_barline_kind = Some(kind);
                    return;
                }
                if let Some(tempo) = parse_tempo_instruction(&inline.value.value, inline.value.span)
                {
                    self.current_state()
                        .lowered
                        .push(LoweredEvent::TempoChange(tempo));
                    return;
                }
                if let Some(tempo) =
                    parse_sound_tempo_instruction(&inline.value.value, inline.value.span)
                {
                    self.current_state()
                        .lowered
                        .push(LoweredEvent::TempoChange(tempo));
                    return;
                }
                if parse_meter_restatement_instruction(&inline.value.value) {
                    self.current_state().pending_musicxml_meter_restatement = true;
                    return;
                }
                if parse_key_restatement_instruction(&inline.value.value) {
                    self.current_state().pending_musicxml_key_restatement = true;
                    return;
                }
                if let Some(symbol) = parse_time_symbol_instruction(&inline.value.value) {
                    self.current_state().pending_musicxml_time_symbol = Some(symbol);
                    return;
                }
                if let Some(display_number) = parse_measure_number_instruction(&inline.value.value)
                {
                    self.current_state()
                        .lowered
                        .push(LoweredEvent::MeasureNumber { display_number });
                    return;
                }
                if let Some(close) =
                    parse_ending_close_instruction(&inline.value.value, inline.value.span)
                {
                    self.current_state()
                        .lowered
                        .push(LoweredEvent::VariantEndingClose(close));
                    return;
                }
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
            // Any other inline field (`[w:]`, `[r:]`, `[N:]`, `[s:]`, `[U:]`,
            // ...) is a VALID field croma's lowering does not yet apply — an
            // unsupported-feature no-op, not a recovery from malformed input.
            // Per the 3-tier recovery policy it is silently skipped (KEEP), not
            // warned: a per-occurrence warning here is noise on well-formed input.
            _ => {}
        }
    }

    fn apply_music_line(&mut self, line: &MusicLine) {
        let mut previous_item_end = None;
        for item in &line.items {
            let item_span = item.span();
            let detached_from_previous = previous_item_end.is_some_and(|end| end < item_span.start);
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
                    let count = rest.count.map(|count| count.value).unwrap_or(1).max(1);
                    let voice_meter = self.current_state().meter_duration;
                    if let Some(meter_duration) = voice_meter {
                        let source_order = self.next_source_order();
                        // Multi-measure rests carry no syntax-level bundle, but
                        // flushed-ahead attachments (`"Dm"Z`) still bind to the
                        // first expanded rest measure.
                        let attachments = self
                            .current_state()
                            .take_timed_attachments(&AttachmentBundle::default());
                        for index in 0..count {
                            let rest_attachments = if index == 0 {
                                attachments.clone()
                            } else {
                                EventAttachments::default()
                            };
                            self.current_state().push_time_group(
                                vec![(
                                    LoweredEventAtom {
                                        kind: LoweredEventAtomKind::Rest {
                                            visibility: rest.visibility,
                                            multiple_rest: (index == 0
                                                && count > 1
                                                && rest.visibility == RestVisibility::Visible)
                                                .then_some(count),
                                            span: rest.span,
                                        },
                                        duration: meter_duration,
                                    },
                                    false,
                                    rest_attachments,
                                )],
                                line.line_index,
                                source_order,
                            );
                            if index + 1 < count {
                                self.push_implicit_regular_barline(rest.span);
                            }
                        }
                    } else {
                        self.current_state()
                            .diagnostics
                            .push(free_meter_multirest_warning(rest.span));
                        let duration = self.unit.checked_mul_u32(count);
                        let source_order = self.next_source_order();
                        let attachments = self
                            .current_state()
                            .take_timed_attachments(&AttachmentBundle::default());
                        self.current_state().push_time_group(
                            vec![(
                                LoweredEventAtom {
                                    kind: LoweredEventAtomKind::Rest {
                                        visibility: rest.visibility,
                                        multiple_rest: None,
                                        span: rest.span,
                                    },
                                    duration,
                                },
                                false,
                                attachments,
                            )],
                            line.line_index,
                            source_order,
                        );
                    }
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
                MusicItem::VariantEnding(ending) => {
                    if self.current_state().broken_left_available {
                        self.push_implicit_regular_barline(ending.span);
                    }
                    self.current_state()
                        .lowered
                        .push(LoweredEvent::VariantEnding(ending.clone()));
                }
                MusicItem::Barline(barline) => {
                    let pending_musicxml_barline_kind =
                        self.current_state().pending_musicxml_barline_kind.take();
                    let kind = if barline.kind == BarlineKind::Regular {
                        pending_musicxml_barline_kind.unwrap_or(barline.kind)
                    } else {
                        barline.kind
                    };
                    let source_order = self.next_source_order();
                    self.current_state().flush_pending_barline_directions(
                        line.line_index,
                        source_order,
                        barline.span,
                        kind,
                    );
                    if matches!(
                        kind,
                        BarlineKind::Dotted | BarlineKind::Dashed | BarlineKind::Invisible
                    ) {
                        self.current_state()
                            .diagnostics
                            .push(barline_export_policy_info(barline.span, kind));
                    }
                    self.current_state()
                        .attach_pending_grace_groups_to_previous_note();
                    self.current_state()
                        .attach_pending_grace_groups_to_previous_note_if_measure_complete();
                    self.current_state().finish_pending_broken_at_boundary();
                    self.current_state().finish_open_tuplets_at_boundary();
                    self.current_state().reset_measure_accidentals_at_barline();
                    // A grace flushed ahead of its note stays pending across the
                    // bar unless the boundary resolves it backward as an
                    // after-grace on a previous timed note. A leftover at the
                    // end of the voice surfaces as a dangling-grace diagnostic
                    // instead of a silent drop.
                    for kind in barline_lowering_kinds_with_kind(barline, kind) {
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
                    // flush). Buffer it until lowering can see whether it belongs
                    // to the next timed event as a leading grace or to the
                    // previous timed note as an after-grace at a boundary.
                    let state = self.current_state();
                    let force_after_previous = state.pending_musicxml_after_grace;
                    state.pending_musicxml_after_grace = false;
                    state.push_pending_grace_group(
                        grace.clone(),
                        detached_from_previous,
                        force_after_previous,
                    );
                }
                MusicItem::ChordSymbol(text) => {
                    // Flushed ahead of its note by a barline / line end / other
                    // boundary. ABC 2.1 §4.18 binds the symbol to the note it
                    // precedes, so buffer it for the next timed event (the
                    // boundary does not void it).
                    let state = self.current_state();
                    let harmony_text = state.pending_musicxml_harmony_text.take();
                    state.pending_chord_symbols.push(
                        crate::lower::voice::chord_symbol_attachment_model(text, harmony_text),
                    );
                }
                MusicItem::Annotation(text) => {
                    // Same flushed-ahead situation; §4.19 positions an
                    // annotation relative to the following note.
                    self.current_state().pending_annotations.push(text.clone());
                }
                MusicItem::Decoration(decoration) => {
                    // Flushed ahead of its symbol (§4.14); binds to the next
                    // timed event like the quoted-text cases above.
                    self.current_state()
                        .pending_decorations
                        .push(decoration.clone());
                }
                MusicItem::Unsupported(_) | MusicItem::Malformed(_) => {}
            }
            previous_item_end = Some(item_span.end);
        }
    }

    fn switch_voice(&mut self, voice: Spanned<VoiceDefinition>) {
        let existing = self
            .voices
            .iter()
            .position(|state| state.id.value == voice.value.id.value);
        let previous_clef = existing
            .and_then(|index| self.voices[index].properties.clef.as_ref())
            .map(|clef| clef.text.clone());
        let incoming = voice_properties_model(&voice.value);
        self.current_voice = voice.value.id.value.clone();
        let index = self.ensure_voice(voice);
        if existing.is_some()
            && let Some(clef) = incoming.clef
            && previous_clef.as_deref() != Some(clef.text.as_str())
        {
            self.voices[index]
                .lowered
                .push(LoweredEvent::ClefChange(ClefChangeModel {
                    source_span: clef.span,
                    clef,
                    musicxml_cursor_pre_backup: None,
                    musicxml_cursor_back: None,
                }));
        }
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
        let state = LoweringState::new(
            id,
            properties,
            self.unit,
            self.key.as_ref(),
            self.meter_duration,
        );
        self.voices.push(state);
        self.voices.len() - 1
    }

    /// The shared model `pair_id` for a cross-voice slur carrier `pair=`,
    /// allocating one on first sight so the matching end (in another voice,
    /// lowered separately) re-pairs onto the same id. Ids start at
    /// [`XVOICE_SLUR_PAIR_ID_BASE`] so they stay disjoint from per-voice slur ids.
    fn xvoice_slur_pair_id(&mut self, carrier_pair: u32) -> u32 {
        if let Some((_, pair_id)) = self
            .xvoice_slur_pair_ids
            .iter()
            .find(|(carrier, _)| *carrier == carrier_pair)
        {
            return *pair_id;
        }
        let pair_id = self.next_xvoice_slur_pair_id;
        self.next_xvoice_slur_pair_id = self.next_xvoice_slur_pair_id.saturating_add(1);
        self.xvoice_slur_pair_ids.push((carrier_pair, pair_id));
        pair_id
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

    fn push_implicit_regular_barline(&mut self, span: Span) {
        let voice = self.current_state();
        voice.attach_pending_grace_groups_to_previous_note();
        voice.attach_pending_grace_groups_to_previous_note_if_measure_complete();
        voice.finish_pending_broken_at_boundary();
        voice.finish_open_tuplets_at_boundary();
        voice.reset_measure_accidentals_at_barline();
        voice.lowered.push(LoweredEvent::Untimed(Event::Barline {
            kind: BarlineKind::Regular,
            span,
        }));
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

fn key_clef_change_model(properties: &VoiceProperties) -> Option<ClefChangeModel> {
    properties.clef.as_ref().map(|clef| ClefChangeModel {
        clef: text_line_from_spanned(clef),
        source_span: clef.span,
        musicxml_cursor_pre_backup: None,
        musicxml_cursor_back: None,
    })
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
    pub header_time_symbol: Option<String>,
    pub score_directives: &'a [ScoreDirectiveModel],
    pub preserved_directives: &'a [PreservedDirective],
    pub post_tune_lyrics: &'a [TextLine],
    pub diagnostics: &'a [Diagnostic],
    pub divisions: u32,
}

/// A brace `{ }` group is a grand staff (one multi-staff part) only when all its
/// member voices belong to a SINGLE part — i.e. they share the same base voice id
/// before croma's `#` continuation suffix (`P2`, `P2#2`). A brace over distinct
/// part ids (`{P1 P2}`, a `<part-group symbol="brace">`) is NOT a grand staff;
/// those parts stay separate. Requires ≥2 staves (a lone `{P2}` is not a grand
/// staff). This is what keeps a braced group of instruments from being collapsed
/// into one part while still merging a real piano grand staff.
fn brace_is_one_part(staves: &[Vec<usize>], voices: &[VoiceTimeline]) -> bool {
    if staves.len() < 2 {
        return false;
    }
    let base = |index: usize| voices[index].id.value.split('#').next().unwrap_or("");
    let mut members = staves.iter().flatten().copied();
    let Some(first) = members.next() else {
        return false;
    };
    let first_base = base(first);
    members.all(|index| base(index) == first_base)
}

/// Lay out voices into parts and staves from a `%%staves` / `%%score` directive.
/// Returns, per part, its staves — each a list of voice indices (in order):
/// - a parenthesis `( )` group merges its voices onto ONE staff of one part
///   (overlay voices on a shared staff);
/// - a brace `{ }` group is one part with SEPARATE staves — each direct member (a
///   bare voice, or a nested `( )` group) becomes its own staff (a piano grand
///   staff). This is what round-trips a multi-staff part's per-staff clefs;
/// - `[ ]` brackets and bare voices keep one single-staff part per voice in
///   directive order.
///
/// With no grouping directive, each voice is its own single-staff part.
fn part_layouts(
    directives: &[ScoreDirectiveModel],
    voices: &[VoiceTimeline],
) -> Vec<Vec<Vec<usize>>> {
    let one_per_voice = || {
        (0..voices.len())
            .map(|index| vec![vec![index]])
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
    let mut parts: Vec<Vec<Vec<usize>>> = Vec::new();
    let mut brace_staves: Vec<Vec<usize>> = Vec::new();
    let mut paren_voices: Vec<usize> = Vec::new();
    let mut in_brace = false;
    let mut in_paren = false;
    for token in &directive.tokens {
        match &token.kind {
            ScoreDirectiveTokenKindModel::GroupStart('{') => {
                in_brace = true;
                brace_staves = Vec::new();
            }
            ScoreDirectiveTokenKindModel::GroupEnd('}') => {
                if in_paren && !paren_voices.is_empty() {
                    brace_staves.push(std::mem::take(&mut paren_voices));
                }
                in_paren = false;
                if !brace_staves.is_empty() {
                    if brace_is_one_part(&brace_staves, voices) {
                        // Grand staff: every member is a voice of ONE part (shared
                        // base id, croma's `P2`/`P2#2` continuation naming) — one
                        // multi-staff part with a staff per member.
                        parts.push(std::mem::take(&mut brace_staves));
                    } else {
                        // A `<part-group>` brace (`{P1 P2}`): distinct part ids
                        // stay SEPARATE parts, exactly as before grand-staff
                        // support, so a braced group of instruments round-trips.
                        for staff in std::mem::take(&mut brace_staves) {
                            parts.push(vec![staff]);
                        }
                    }
                }
                in_brace = false;
            }
            ScoreDirectiveTokenKindModel::GroupStart('(') => {
                in_paren = true;
                paren_voices = Vec::new();
            }
            ScoreDirectiveTokenKindModel::GroupEnd(')') => {
                if in_paren && !paren_voices.is_empty() {
                    let staff = std::mem::take(&mut paren_voices);
                    if in_brace {
                        // A `( )` inside a `{ }` is one shared staff of the part.
                        brace_staves.push(staff);
                    } else {
                        // A bare `( )` is one single-staff part.
                        parts.push(vec![staff]);
                    }
                }
                in_paren = false;
            }
            ScoreDirectiveTokenKindModel::Voice(id) => {
                if let Some(index) = index_of(id) {
                    if in_paren {
                        paren_voices.push(index);
                    } else if in_brace {
                        // A bare voice inside a `{ }` is its own staff.
                        brace_staves.push(vec![index]);
                    } else {
                        parts.push(vec![vec![index]]);
                    }
                }
            }
            _ => {}
        }
    }
    // Flush an unterminated paren / brace defensively.
    if in_paren && !paren_voices.is_empty() {
        if in_brace {
            brace_staves.push(std::mem::take(&mut paren_voices));
        } else {
            parts.push(vec![std::mem::take(&mut paren_voices)]);
        }
    }
    if !brace_staves.is_empty() {
        if brace_is_one_part(&brace_staves, voices) {
            parts.push(std::mem::take(&mut brace_staves));
        } else {
            for staff in std::mem::take(&mut brace_staves) {
                parts.push(vec![staff]);
            }
        }
    }
    // Any voice the directive did not list still needs a part.
    let mentioned: std::collections::HashSet<usize> =
        parts.iter().flatten().flatten().copied().collect();
    for index in 0..voices.len() {
        if !mentioned.contains(&index) {
            parts.push(vec![vec![index]]);
        }
    }
    if parts.is_empty() {
        return one_per_voice();
    }
    parts
}

pub(crate) fn build_score_model(input: ScoreModelInput<'_>) -> Score {
    // Each ABC voice becomes its own MusicXML part, except that a `%%staves` /
    // `%%score` parenthesis group merges its voices into one multi-voice part.
    // A single-voice tune still yields exactly one part.
    let single_voice = input.voices.len() == 1;
    let layouts = part_layouts(input.score_directives, input.voices);
    let mut parts = layouts
        .iter()
        .enumerate()
        .map(|(part_index, staff_layout)| {
            // Build each staff's voices, assigning the voice its staff id. A
            // single-staff part has one staff (value 1) with every voice — the
            // shape every ABC tune lowered before grand-staff support; a `{ }`
            // brace part has one staff per layout entry (a piano grand staff).
            let mut semantic_voices: Vec<crate::model::Voice> = Vec::new();
            let staves = staff_layout
                .iter()
                .enumerate()
                .map(|(staff_index, staff_voice_indices)| {
                    let staff_id = StaffId {
                        value: u32::try_from(staff_index + 1).unwrap_or(1),
                        span: input.source_span,
                    };
                    let voice_ids = staff_voice_indices
                        .iter()
                        .map(|&voice_index| {
                            let voice = semantic_voice_from_timeline(
                                &input.voices[voice_index],
                                staff_id,
                                input.field_state,
                            );
                            let id = voice.id.clone();
                            semantic_voices.push(voice);
                            id
                        })
                        .collect::<Vec<_>>();
                    Staff {
                        id: staff_id,
                        voices: voice_ids,
                        source_span: input.source_span,
                    }
                })
                .collect::<Vec<_>>();
            let voice_indices: Vec<usize> =
                staff_layout.iter().flatten().copied().collect::<Vec<_>>();
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
            let instruments = part_musicxml_instruments(input.voices, &voice_indices);
            Part {
                id: PartId {
                    value: format!("P{}", part_index + 1),
                    span: input.source_span,
                },
                name,
                instruments,
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
            instruments: Vec::new(),
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
            meter: input.field_state.meter.as_ref().map(|meter| {
                let mut model = meter_model(meter);
                model.time_symbol = input.header_time_symbol.clone();
                model
            }),
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

fn part_musicxml_instruments(
    voices: &[VoiceTimeline],
    voice_indices: &[usize],
) -> Vec<MusicXmlPartInstrumentModel> {
    let mut instruments = Vec::new();
    for &voice_index in voice_indices {
        let Some(voice) = voices.get(voice_index) else {
            continue;
        };
        for instrument in &voice.musicxml_instruments {
            if !instruments
                .iter()
                .any(|existing: &MusicXmlPartInstrumentModel| existing.id == instrument.id)
            {
                instruments.push(instrument.clone());
            }
        }
    }
    instruments
}

fn meter_model(meter: &Spanned<Meter>) -> MeterModel {
    let duration = meter_duration(&meter.value);
    MeterModel {
        display: meter.value.raw.clone(),
        time_symbol: None,
        duration,
        free_meter: duration.is_none(),
        preserve_restatement: false,
        source_span: meter.span,
    }
}

/// The display text of the voice's effective key: the most recently recorded
/// key change, else the header key. A restatement of the effective key records
/// no event (both generations collapse it identically).
fn effective_key_display<'a>(voice: &'a LoweringState, header: Option<&'a str>) -> Option<&'a str> {
    voice
        .lowered
        .iter()
        .rev()
        .find_map(|event| match event {
            LoweredEvent::KeyChange(key) => Some(key.display.as_str()),
            _ => None,
        })
        .or(header)
}

/// The display text of the voice's effective meter (see `effective_key_display`).
fn effective_meter_display<'a>(
    voice: &'a LoweringState,
    header: Option<&'a str>,
) -> Option<&'a str> {
    voice
        .lowered
        .iter()
        .rev()
        .find_map(|event| match event {
            LoweredEvent::MeterChange(meter) => Some(meter.display.as_str()),
            _ => None,
        })
        .or(header)
}

fn key_signature_model(key: &Spanned<KeySignature>) -> KeySignatureModel {
    KeySignatureModel {
        // `display` is the VERBATIM source spelling, on purpose — it is load-bearing,
        // not a convenience. The ABC writer round-trips it (a `croma fmt` fixed point,
        // see `to_abc`), so deriving it from tonic/mode would lose the author's
        // spelling (`K:G minor` -> `K:Gm`, `K:C major` -> `K:C`). The mid-tune `<key>`
        // dedupe (`effective_key_display`) keys on it deliberately: it collapses only
        // exact-text no-op restatements, emitting one `<key>` per *distinct* source
        // declaration — which is what abc2xml does, so deduping by signature identity
        // instead would diverge (relative keys such as `K:G`/`K:Em` share `fifths`,
        // and source respellings abc2xml preserves would be dropped).
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
        // Set by the caller from a pending MusicXML restatement carrier; an ABC
        // `K:` field is never a forced restatement on its own.
        preserve_restatement: false,
        source_span: key.span,
    }
}

fn meter_is_invalid_for_lowering(meter: &Meter) -> bool {
    matches!(meter.kind, MeterKind::Complex)
        && !meter.raw.contains('/')
        && !meter.raw.contains('+')
        && !meter.raw.contains('(')
}

/// Whether a `K:` change carries no usable key information and so must be ignored
/// during lowering (the previous key stays in force, with an `abc.field.invalid_k`
/// warning). A `K:` is junk only when it has NO `A-G` tonic, NO clef/transpose
/// modifiers (ABC 2.1 §4.6), and is not one of the spec's tonic-less key *forms*:
///
/// - [`KeyMode::None`] — `K:` / `K:none`: explicitly "no key signature" (§3.1.14);
/// - [`KeyMode::HighlandPipes`] / [`KeyMode::HighlandPipesMarked`] — `K:HP` / `K:Hp`,
///   the bagpipe keys (§3.1.14);
/// - [`KeyMode::Explicit`] — `K:<tonic> exp <accidentals>`: an explicit accidental
///   list defines the signature even with no tonic step (§3.1.14).
///
/// A tonic-less `K:` that only *modifies* the prevailing key (`K:^F`) never reaches
/// here — `apply_current_voice_key_change` rewrites it to a tonic-ful key first.
///
/// The `KeyMode::None` arm subsumes the empty-`raw` and literal-`none` spellings
/// (`parse_key` maps both to that mode), so no separate `raw` string checks are
/// needed.
fn key_is_invalid_for_lowering(key: &KeySignature) -> bool {
    key.tonic.is_none()
        && key_clef_properties_model(&key.properties) == VoicePropertiesModel::default()
        && !matches!(
            key.mode,
            KeyMode::Explicit
                | KeyMode::None
                | KeyMode::HighlandPipes
                | KeyMode::HighlandPipesMarked
        )
}

/// Build the effective key for a tonic-less modifying `K:^F` (ABC 2.1 §3.1.14
/// "key signatures may be modified by adding accidentals"): inherit the
/// prevailing key's tonic+mode (so `key_fifths` keeps the prevailing base — the
/// Bb/Eb of `K:Gmin` — rather than collapsing to a tonic-less 0) and ADD the
/// body's accidentals, an accidental on the same note letter overriding the
/// inherited one. The result is equivalent to the tonic-ful spelling `K:Gm ^f`.
fn merge_key_modification(prevailing: &KeySignature, body: &KeySignature) -> KeySignature {
    let mut accidentals: Vec<KeyAccidental> = prevailing
        .accidentals
        .iter()
        .filter(|base| {
            !body
                .accidentals
                .iter()
                .any(|added| added.note.value.eq_ignore_ascii_case(&base.note.value))
        })
        .cloned()
        .collect();
    accidentals.extend(body.accidentals.iter().cloned());
    KeySignature {
        // Reads as "prevailing key + modifier" so its display differs from the
        // prevailing key and a mid-tune <key> is emitted (the dedupe keys on the
        // display); also the form `croma fmt --auto-fix` would sanitise to.
        raw: format!("{} {}", prevailing.raw.trim(), body.raw.trim()),
        tonic: prevailing.tonic,
        mode: prevailing.mode.clone(),
        accidentals,
        explicit: prevailing.explicit,
        compact_accidentals_ignored: false,
        tonic_trailing_junk_ignored: false,
        // The modifier carries its own clef/transpose props (usually none); the
        // prevailing clef stays active because applying empty props is a no-op.
        properties: body.properties.clone(),
    }
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

/// Forward-translate the score-meaningful `%%MIDI` sub-directives (`program` /
/// `channel` / `control` CC7-CC10 / `midi-unpitched` →
/// [`MidiInstrumentModel`]; `transpose` → `midi_transpose`) onto each timeline.
/// Both the line-start `%%MIDI ...` form
/// (in `preserved_directives`) and the inline `[I: MIDI=...]` form (a music-line
/// item) are projected, respecting per-voice scoping: a directive attaches to
/// the voice of the nearest preceding `V:` declaration (header or body) by
/// source position, or the first/default voice if it precedes every `V:`.
///
/// `%%MIDI` is an abc2midi convention, not part of ABC 2.1; only the
/// score-meaningful sub-directives are projected. `midi-unpitched` is croma's
/// MusicXML-origin carrier for percussion maps. Line-start directives still
/// survive verbatim in `preserved_directives` for round-trip and the formatter.
fn project_voice_midi(
    voices: &mut [VoiceTimeline],
    tune_music: &ParsedTuneMusic,
    field_state: &FieldState,
) {
    // `%%MIDI` directives live in `preserved_directives`; the inline
    // `[I: MIDI=...]` form lives in the music lines, so both are scanned below.
    if voices.is_empty() {
        return;
    }

    // Ordered voice-switch points (source position -> voice id) across the
    // header (`field_state.voices`) and the body (`V:` field lines).
    let mut switches: Vec<(usize, &str)> = field_state
        .voices
        .iter()
        .map(|voice| (voice.span.start, voice.value.id.value.as_str()))
        .collect();
    for field in &tune_music.body_fields {
        if let MusicFieldLineKind::Voice(voice) = &field.kind {
            switches.push((field.line_span.start, voice.value.id.value.as_str()));
        }
    }
    switches.sort_by_key(|(position, _)| *position);

    let default_voice = voices[0].id.value.clone();
    let voice_at = |position: usize| -> &str {
        switches
            .iter()
            .rev()
            .find(|(switch_position, _)| *switch_position <= position)
            .map_or(default_voice.as_str(), |(_, id)| *id)
    };

    // Collect every score-translatable MIDI directive as `(voice, args, span)`,
    // from both the line-start `%%MIDI ...` form (in `preserved_directives`) and
    // the inline `[I: MIDI=...]` form (a music-line item). `args` is the text
    // after the `MIDI` keyword, e.g. `program 41`.
    let mut directives: Vec<(&str, &str, Span)> = Vec::new();
    for directive in &tune_music.preserved_directives {
        if directive.name.value.eq_ignore_ascii_case("MIDI") {
            let voice_id = voice_at(directive.span.start);
            directives.push((voice_id, directive.value.value.as_str(), directive.span));
        }
    }
    for line in &tune_music.lines {
        for item in &line.items {
            let MusicItem::InlineField(inline) = item else {
                continue;
            };
            if inline.code != 'I' {
                continue;
            }
            let Some(rest) = inline.value.value.strip_prefix("MIDI") else {
                continue;
            };
            // Require `MIDI=`/`MIDI ` so `[I:MIDIfoo]` is not misread.
            if !rest.is_empty() && !rest.starts_with(['=', ' ', '\t']) {
                continue;
            }
            let args = rest.trim_start_matches(['=', ' ', '\t']);
            directives.push((voice_at(inline.span.start), args, inline.span));
        }
    }
    // Source order makes last-wins well-defined across both directive forms.
    directives.sort_by_key(|(_, _, span)| span.start);

    let mut instrument_by_voice: BTreeMap<&str, MidiInstrumentModel> = BTreeMap::new();
    let mut transpose_by_voice: BTreeMap<&str, i16> = BTreeMap::new();
    let mut musicxml_instruments_by_voice: BTreeMap<&str, Vec<MusicXmlPartInstrumentModel>> =
        BTreeMap::new();
    for directive in &tune_music.preserved_directives {
        if directive
            .name
            .value
            .eq_ignore_ascii_case("croma-musicxml-instrument")
            && let Some(instrument) =
                parse_musicxml_part_instrument_directive(&directive.value.value, directive.span)
        {
            let voice_id = voice_at(directive.span.start);
            let instruments = musicxml_instruments_by_voice.entry(voice_id).or_default();
            if let Some(existing) = instruments
                .iter()
                .position(|existing| existing.id == instrument.id)
            {
                instruments[existing] = instrument;
            } else {
                instruments.push(instrument);
            }
        }
    }
    for (voice_id, args, span) in directives {
        if let Some(transpose) = parse_midi_transpose(args) {
            transpose_by_voice.insert(voice_id, transpose);
            continue;
        }
        let model = instrument_by_voice
            .entry(voice_id)
            .or_insert(MidiInstrumentModel {
                program: None,
                channel: None,
                volume_cc: None,
                pan_cc: None,
                midi_unpitched: None,
                span,
            });
        apply_midi_instrument_directive(args, span, model);
    }

    for voice in voices.iter_mut() {
        if let Some(model) = instrument_by_voice
            .get(voice.id.value.as_str())
            .filter(|model| model.has_content())
        {
            voice.midi_instrument = Some(*model);
        }
        if let Some(transpose) = transpose_by_voice.get(voice.id.value.as_str()) {
            voice.midi_transpose = Some(*transpose);
        }
        if let Some(instruments) = musicxml_instruments_by_voice.get(voice.id.value.as_str()) {
            voice.musicxml_instruments = instruments.clone();
        }
    }
}

fn parse_musicxml_part_instrument_directive(
    value: &str,
    span: Span,
) -> Option<MusicXmlPartInstrumentModel> {
    let fields = parse_croma_key_values(value);
    let id = fields.get("id")?.trim();
    if id.is_empty() {
        return None;
    }
    let name = fields.get("name").map(|name| TextLine {
        text: name.clone(),
        span,
    });
    let midi = MidiInstrumentModel {
        program: parse_croma_u8(&fields, "program").filter(|value| *value <= 127),
        channel: parse_croma_u8(&fields, "channel").filter(|value| (1..=16).contains(value)),
        volume_cc: parse_croma_u8(&fields, "volume-cc")
            .or_else(|| parse_croma_u8(&fields, "volume"))
            .filter(|value| *value <= 127),
        pan_cc: parse_croma_u8(&fields, "pan-cc")
            .or_else(|| parse_croma_u8(&fields, "pan"))
            .filter(|value| *value <= 127),
        midi_unpitched: parse_croma_u8(&fields, "midi-unpitched")
            .filter(|value| (1..=128).contains(value)),
        span,
    };
    Some(MusicXmlPartInstrumentModel {
        id: id.to_owned(),
        name,
        midi: midi.has_content().then_some(midi),
        span,
    })
}

fn parse_note_instrument_instruction(value: &str, span: Span) -> Option<MusicXmlInstrumentRef> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-note-instrument")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    let id = fields.get("id")?.trim();
    (!id.is_empty()).then(|| MusicXmlInstrumentRef {
        id: id.to_owned(),
        span,
    })
}

fn parse_harmony_text_instruction(value: &str) -> Option<HarmonyKindText> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-harmony-text")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    if parse_croma_bool(&fields, "textless") {
        return Some(HarmonyKindText::Textless);
    }
    fields.get("text").cloned().map(HarmonyKindText::Text)
}

fn parse_lyric_extend_instruction(value: &str) -> Option<u32> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-lyric-extend")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    parse_croma_u32(&fields, "verse").filter(|verse| *verse > 0)
}

fn parse_lyric_duplicate_instruction(value: &str, span: Span) -> Option<AlignedLyric> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-lyric-duplicate")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    let verse = parse_croma_u32(&fields, "verse").filter(|verse| *verse > 0)?;
    let text = if let Some(hex) = fields.get("text-hex") {
        parse_croma_hex_utf8(hex)?
    } else {
        fields.get("text")?.clone()
    };
    Some(AlignedLyric {
        verse,
        text,
        span,
        control: LyricControl::Syllable,
        same_note_extend: parse_croma_bool(&fields, "extend"),
    })
}

fn parse_tempo_instruction(value: &str, span: Span) -> Option<TempoModel> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-tempo")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    let text = if let Some(hex) = fields.get("text-hex") {
        parse_croma_hex_utf8(hex)
    } else {
        fields.get("text").filter(|text| !text.is_empty()).cloned()
    };
    let beat = parse_croma_u32(&fields, "bpm")
        .or_else(|| parse_croma_u32(&fields, "tempo"))
        .map(|bpm| {
            let beat_numerator = parse_croma_u32(&fields, "beat-n")
                .or_else(|| parse_croma_u32(&fields, "beat-numerator"))
                .unwrap_or(1);
            let beat_denominator = parse_croma_u32(&fields, "beat-d")
                .or_else(|| parse_croma_u32(&fields, "beat-denominator"))
                .unwrap_or(4);
            (bpm, beat_numerator, beat_denominator)
        })
        .filter(|(_, beat_numerator, beat_denominator)| {
            *beat_numerator > 0 && *beat_denominator > 0
        })
        .map(|(bpm, beat_numerator, beat_denominator)| TempoBeat {
            beat_numerator,
            beat_denominator,
            bpm,
        });
    if text.is_none() && beat.is_none() {
        return None;
    }
    let beat_role = match fields.get("role").map(String::as_str) {
        Some("sound" | "playback" | "playback-sound-only") => TempoBeatRole::PlaybackSoundOnly,
        _ => TempoBeatRole::PrintedMetronome,
    };
    Some(TempoModel {
        text,
        beat,
        beat_role,
        source_span: span,
    })
}

fn parse_sound_tempo_instruction(value: &str, span: Span) -> Option<TempoModel> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-sound-tempo")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    let bpm = parse_croma_u32(&fields, "bpm").or_else(|| parse_croma_u32(&fields, "tempo"))?;
    let beat_numerator = parse_croma_u32(&fields, "beat-n")
        .or_else(|| parse_croma_u32(&fields, "beat-numerator"))
        .unwrap_or(1);
    let beat_denominator = parse_croma_u32(&fields, "beat-d")
        .or_else(|| parse_croma_u32(&fields, "beat-denominator"))
        .unwrap_or(4);
    if beat_numerator == 0 || beat_denominator == 0 {
        return None;
    }
    let text = if let Some(hex) = fields.get("text-hex") {
        parse_croma_hex_utf8(hex)
    } else {
        fields.get("text").filter(|text| !text.is_empty()).cloned()
    };
    Some(TempoModel {
        text,
        beat: Some(TempoBeat {
            beat_numerator,
            beat_denominator,
            bpm,
        }),
        beat_role: TempoBeatRole::PlaybackSoundOnly,
        source_span: span,
    })
}

fn parse_meter_restatement_instruction(value: &str) -> bool {
    let value = value.trim();
    let Some(rest) = value.strip_prefix("croma-meter-restatement") else {
        return false;
    };
    rest.is_empty() || rest.starts_with(char::is_whitespace)
}

fn parse_key_restatement_instruction(value: &str) -> bool {
    let value = value.trim();
    let Some(rest) = value.strip_prefix("croma-key-restatement") else {
        return false;
    };
    rest.is_empty() || rest.starts_with(char::is_whitespace)
}

fn parse_time_symbol_instruction(value: &str) -> Option<String> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-time-symbol")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    let symbol = fields.get("symbol")?;
    matches!(symbol.as_str(), "common" | "cut").then(|| symbol.clone())
}

fn parse_musicxml_forward_instruction(value: &str) -> bool {
    let value = value.trim();
    let Some(rest) = value.strip_prefix("croma-musicxml-forward") else {
        return false;
    };
    rest.is_empty() || rest.starts_with(char::is_whitespace)
}

fn parse_musicxml_sequence_backup_instruction(value: &str) -> Option<Fraction> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-musicxml-sequence-backup")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    let numerator = parse_croma_u32(&fields, "n")
        .or_else(|| parse_croma_u32(&fields, "numerator"))
        .filter(|value| *value > 0)?;
    let denominator = parse_croma_u32(&fields, "d")
        .or_else(|| parse_croma_u32(&fields, "denominator"))
        .filter(|value| *value > 0)?;
    Some(Fraction::new(numerator, denominator))
}

struct MusicXmlTupletInstruction {
    source_pair_id: u32,
    actual_notes: u32,
    normal_notes: u32,
    role: TupletRole,
}

fn parse_musicxml_tuplet_instruction(value: &str) -> Option<MusicXmlTupletInstruction> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-musicxml-tuplet")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    let source_pair_id = parse_croma_u32(&fields, "id")
        .or_else(|| parse_croma_u32(&fields, "pair"))
        .or_else(|| parse_croma_u32(&fields, "pair-id"))?;
    let actual_notes = parse_croma_u32(&fields, "actual")
        .or_else(|| parse_croma_u32(&fields, "actual-notes"))
        .or_else(|| parse_croma_u32(&fields, "a"))
        .filter(|value| *value > 0)?;
    let normal_notes = parse_croma_u32(&fields, "normal")
        .or_else(|| parse_croma_u32(&fields, "normal-notes"))
        .or_else(|| parse_croma_u32(&fields, "n"))
        .filter(|value| *value > 0)?;
    let role = match fields.get("role").map(|value| value.trim()) {
        Some("start") => TupletRole::Start,
        Some("continue") => TupletRole::Continue,
        Some("stop") => TupletRole::Stop,
        _ => return None,
    };
    Some(MusicXmlTupletInstruction {
        source_pair_id,
        actual_notes,
        normal_notes,
        role,
    })
}

struct XvoiceSlurInstruction {
    pair: u32,
    role: SlurRole,
}

/// Parse `[I:croma-xvoice-slur pair=N role=start|stop]` — one end of a slur
/// whose start and stop sit in different voices (`(`/`)` cannot span `V:`
/// lines). `pair` re-pairs the two ends across voices; `role` is which end.
fn parse_xvoice_slur_instruction(value: &str) -> Option<XvoiceSlurInstruction> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-xvoice-slur")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    let pair = parse_croma_u32(&fields, "pair")
        .or_else(|| parse_croma_u32(&fields, "pair-id"))
        .or_else(|| parse_croma_u32(&fields, "id"))?;
    let role = match fields.get("role").map(|value| value.trim()) {
        Some("start") => SlurRole::Start,
        Some("stop") => SlurRole::Stop,
        _ => return None,
    };
    Some(XvoiceSlurInstruction { pair, role })
}

fn parse_musicxml_after_grace_instruction(value: &str) -> bool {
    let value = value.trim();
    let Some(rest) = value.strip_prefix("croma-after-grace") else {
        return false;
    };
    rest.is_empty() || rest.starts_with(char::is_whitespace)
}

fn parse_clef_cursor_instruction(value: &str, span: Span) -> Option<ClefChangeModel> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-clef-cursor")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    let clef = if let Some(hex) = fields.get("clef-hex") {
        parse_croma_hex_utf8(hex)?
    } else {
        fields.get("clef")?.clone()
    };
    let back_n = parse_croma_u32(&fields, "back-n").filter(|value| *value > 0)?;
    let back_d = parse_croma_u32(&fields, "back-d").filter(|value| *value > 0)?;
    let pre_backup = parse_croma_u32(&fields, "pre-back-n")
        .zip(parse_croma_u32(&fields, "pre-back-d"))
        .filter(|(_, denominator)| *denominator > 0)
        .map(|(numerator, denominator)| Fraction::new(numerator, denominator))
        .unwrap_or_else(|| Fraction::new(back_n, back_d));
    (!clef.trim().is_empty()).then(|| ClefChangeModel {
        clef: TextLine { text: clef, span },
        source_span: span,
        musicxml_cursor_pre_backup: Some(pre_backup),
        musicxml_cursor_back: Some(Fraction::new(back_n, back_d)),
    })
}

fn parse_barline_style_instruction(value: &str) -> Option<BarlineKind> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-barline-style")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    match fields.get("style").map(String::as_str) {
        Some("dashed") => Some(BarlineKind::Dashed),
        _ => None,
    }
}

fn parse_measure_number_instruction(value: &str) -> Option<String> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-measure-number")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    let display_number = if let Some(hex) = fields
        .get("n-hex")
        .or_else(|| fields.get("number-hex"))
        .or_else(|| fields.get("label-hex"))
    {
        parse_croma_hex_utf8(hex)?
    } else {
        fields
            .get("n")
            .or_else(|| fields.get("number"))
            .or_else(|| fields.get("label"))?
            .clone()
    };
    (!display_number.trim().is_empty()).then_some(display_number)
}

fn parse_ending_close_instruction(
    value: &str,
    span: Span,
) -> Option<crate::model::RepeatEndingCloseModel> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-ending-close")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    let close_type = match fields.get("type").map(String::as_str) {
        Some("stop") => crate::model::RepeatEndingCloseType::Stop,
        Some("discontinue") => crate::model::RepeatEndingCloseType::Discontinue,
        _ => return None,
    };
    let location = match fields.get("location").map(String::as_str) {
        Some("left") => crate::model::RepeatEndingCloseLocation::Left,
        Some("right") | None => crate::model::RepeatEndingCloseLocation::Right,
        _ => return None,
    };
    let number = fields.get("number").or_else(|| fields.get("n"))?;
    let endings = parse_repeat_ending_parts(number)?;
    Some(crate::model::RepeatEndingCloseModel {
        span,
        close_type,
        location,
        endings,
    })
}

fn parse_repeat_ending_parts(value: &str) -> Option<Vec<crate::model::RepeatEndingPartModel>> {
    let mut endings = Vec::new();
    for token in value.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some((start, end)) = token.split_once('-') {
            let start = start.trim().parse::<u32>().ok()?;
            let end = end.trim().parse::<u32>().ok()?;
            endings.push(crate::model::RepeatEndingPartModel::Range { start, end });
        } else {
            endings.push(crate::model::RepeatEndingPartModel::Single(
                token.parse::<u32>().ok()?,
            ));
        }
    }
    (!endings.is_empty()).then_some(endings)
}

fn parse_initial_key_instruction(value: &str, span: Span) -> Option<KeySignatureModel> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-initial-key")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    let fifths = parse_croma_i8(&fields, "fifths")?;
    let explicit_accidentals = fields
        .get("accidentals")
        .map(|value| parse_initial_key_accidentals(value, span))
        .unwrap_or_default();
    Some(KeySignatureModel {
        display: String::new(),
        fifths,
        explicit_accidentals,
        preserve_restatement: false,
        source_span: span,
    })
}

fn parse_initial_meter_instruction(value: &str, span: Span) -> Option<MeterModel> {
    let value = value.trim();
    let rest = value.strip_prefix("croma-initial-meter")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let fields = parse_croma_key_values(rest);
    let display = fields.get("display")?.trim();
    if display.is_empty() {
        return None;
    }
    let parsed = parse_meter(display);
    let mut meter = meter_model(&Spanned::new(parsed, span));
    meter.time_symbol = fields
        .get("symbol")
        .filter(|symbol| matches!(symbol.as_str(), "common" | "cut"))
        .cloned();
    Some(meter)
}

fn parse_initial_key_accidentals(value: &str, span: Span) -> Vec<KeyAccidentalModel> {
    value
        .split(',')
        .filter_map(|item| {
            let (step, alter) = item.split_once(':')?;
            let step = step.trim().chars().next()?.to_ascii_uppercase();
            let alter = alter.trim().parse::<i8>().ok()?;
            let accidental = accidental_from_alter(alter)?;
            Some(KeyAccidentalModel {
                step,
                accidental,
                source_span: span,
            })
        })
        .collect()
}

fn accidental_from_alter(alter: i8) -> Option<Accidental> {
    match alter {
        -2 => Some(Accidental::DoubleFlat),
        -1 => Some(Accidental::Flat),
        0 => Some(Accidental::Natural),
        1 => Some(Accidental::Sharp),
        2 => Some(Accidental::DoubleSharp),
        _ => None,
    }
}

fn parse_croma_i8(fields: &BTreeMap<String, String>, key: &str) -> Option<i8> {
    fields.get(key)?.trim().parse::<i8>().ok()
}

fn parse_croma_u32(fields: &BTreeMap<String, String>, key: &str) -> Option<u32> {
    fields.get(key)?.trim().parse::<u32>().ok()
}

fn parse_croma_bool(fields: &BTreeMap<String, String>, key: &str) -> bool {
    matches!(
        fields.get(key).map(|value| value.trim()),
        Some("1" | "true" | "yes" | "on")
    )
}

fn parse_croma_u8(fields: &BTreeMap<String, String>, key: &str) -> Option<u8> {
    fields
        .get(key)?
        .trim()
        .parse::<u16>()
        .ok()
        .and_then(|value| {
            if value <= u16::from(u8::MAX) {
                Some(value as u8)
            } else {
                None
            }
        })
}

fn parse_croma_hex_utf8(value: &str) -> Option<String> {
    let value = value.trim();
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let mut chars = value.bytes();
    while let (Some(hi), Some(lo)) = (chars.next(), chars.next()) {
        let hi = hex_nibble(hi)?;
        let lo = hex_nibble(lo)?;
        bytes.push((hi << 4) | lo);
    }
    String::from_utf8(bytes).ok()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_croma_key_values(value: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let mut chars = value.char_indices().peekable();
    while let Some((_, ch)) = chars.peek().copied() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        let key_start = chars.peek().map(|(index, _)| *index).unwrap_or(value.len());
        while let Some((_, ch)) = chars.peek().copied() {
            if ch == '=' || ch.is_whitespace() {
                break;
            }
            chars.next();
        }
        let key_end = chars.peek().map(|(index, _)| *index).unwrap_or(value.len());
        while let Some((_, ch)) = chars.peek().copied() {
            if !ch.is_whitespace() {
                break;
            }
            chars.next();
        }
        if chars.next_if(|(_, ch)| *ch == '=').is_none() {
            while let Some((_, ch)) = chars.peek().copied() {
                if ch.is_whitespace() {
                    break;
                }
                chars.next();
            }
            continue;
        }
        while let Some((_, ch)) = chars.peek().copied() {
            if !ch.is_whitespace() {
                break;
            }
            chars.next();
        }
        let mut field_value = String::new();
        if chars.next_if(|(_, ch)| *ch == '"').is_some() {
            let mut escaped = false;
            for (_, ch) in chars.by_ref() {
                if escaped {
                    field_value.push(ch);
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    break;
                } else {
                    field_value.push(ch);
                }
            }
        } else {
            while let Some((_, ch)) = chars.peek().copied() {
                if ch.is_whitespace() {
                    break;
                }
                field_value.push(ch);
                chars.next();
            }
        }
        let key = value[key_start..key_end].to_ascii_lowercase();
        if !key.is_empty() {
            fields.insert(key, field_value);
        }
    }
    fields
}

/// Update `model` from one `%%MIDI` directive value, parsing the
/// score-translatable `program` / `channel` / `control` (CC7 volume, CC10 pan) /
/// `midi-unpitched` sub-directives. A trailing `%` comment and any non-numeric
/// trailing words are ignored, matching abc2midi's lenient leading-integer
/// parse; out-of-range values are skipped. Last write wins, so a later directive
/// in the same voice overrides an earlier one.
fn apply_midi_instrument_directive(value: &str, span: Span, model: &mut MidiInstrumentModel) {
    let head = value.split('%').next().unwrap_or(value);
    let mut tokens = head.split_whitespace();
    let Some(sub_directive) = tokens.next() else {
        return;
    };
    match sub_directive {
        // `program <prog>` or `program <channel> <prog>` (both 0-based GM for
        // the program, 1-16 for the channel).
        "program" => {
            let integers: Vec<u8> = tokens.map_while(leading_u8).collect();
            let (channel, program) = match integers.as_slice() {
                [program] => (None, Some(*program)),
                [channel, program, ..] => (Some(*channel), Some(*program)),
                [] => (None, None),
            };
            if let Some(program) = program.filter(|program| *program <= 127) {
                model.program = Some(program);
                if let Some(channel) = channel.filter(|channel| (1..=16).contains(channel)) {
                    model.channel = Some(channel);
                }
                model.span = span;
            }
        }
        // Standalone `channel <n>`; the program (instrument identity) may be set
        // by a sibling `program` directive in the same voice.
        "channel" => {
            if let Some(channel) = tokens
                .next()
                .and_then(leading_u8)
                .filter(|channel| (1..=16).contains(channel))
            {
                model.channel = Some(channel);
                model.span = span;
            }
        }
        // `control <cc> <value>`: only CC7 (channel volume) and CC10 (pan) carry
        // score-relevant sound metadata; every other controller is playback-only.
        "control" => {
            let integers: Vec<u8> = tokens.map_while(leading_u8).collect();
            if let [controller, value, ..] = integers.as_slice() {
                match controller {
                    7 => {
                        model.volume_cc = Some(*value);
                        model.span = span;
                    }
                    10 => {
                        model.pan_cc = Some(*value);
                        model.span = span;
                    }
                    _ => {}
                }
            }
        }
        // Croma extension for MusicXML-origin percussion: carry the
        // MusicXML 1-based MIDI unpitched key through the ABC leg.
        "midi-unpitched" => {
            if let Some(value) = tokens
                .next()
                .and_then(leading_u8)
                .filter(|value| (1..=128).contains(value))
            {
                model.midi_unpitched = Some(value);
                model.span = span;
            }
        }
        // Every other sub-directive is playback-only: not score-translated.
        _ => {}
    }
}

/// Parse the leading ASCII-digit run of `token` as a `u8` (abc2midi accepts
/// e.g. `program 0/`); returns `None` when there is no leading digit or it
/// overflows a `u8`.
fn leading_u8(token: &str) -> Option<u8> {
    let digits: String = token.chars().take_while(char::is_ascii_digit).collect();
    digits.parse::<u8>().ok()
}

/// Parse `%%MIDI transpose <n>` into a chromatic semitone count, ignoring a
/// trailing `% comment`. Returns `None` for any other sub-directive or a
/// non-numeric / out-of-`i16`-range argument. (`%%MIDI transpose` is an abc2midi
/// playback transpose that abc2xml maps to MusicXML `<transpose><chromatic>`.)
fn parse_midi_transpose(value: &str) -> Option<i16> {
    let head = value.split('%').next().unwrap_or(value);
    let mut tokens = head.split_whitespace();
    if tokens.next()? != "transpose" {
        return None;
    }
    tokens.next()?.parse::<i16>().ok()
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

pub(crate) fn meter_duration(meter: &Meter) -> Option<Fraction> {
    match &meter.kind {
        MeterKind::CommonTime => Some(Fraction::new(4, 4)),
        MeterKind::CutTime => Some(Fraction::new(2, 2)),
        MeterKind::Fraction {
            numerator,
            denominator,
        } => Some(Fraction::new(*numerator, *denominator)),
        MeterKind::None => None,
        MeterKind::Complex => complex_meter_duration(&meter.raw),
    }
}

fn complex_meter_duration(raw: &str) -> Option<Fraction> {
    let value = raw.trim();
    if value.contains('+') && !value.trim_start().starts_with('(') {
        let mut total = Fraction::zero();
        let mut saw_part = false;
        for part in value.split('+') {
            let (numerator, denominator) = part.trim().split_once('/')?;
            total = total.checked_add(Fraction::new(
                numerator.trim().parse().ok()?,
                denominator.trim().parse().ok()?,
            ));
            saw_part = true;
        }
        return saw_part.then_some(total);
    }

    if let Some((beats, beat_type)) = value.split_once('/') {
        let beats = beats
            .trim()
            .strip_prefix('(')
            .and_then(|beats| beats.strip_suffix(')'))
            .unwrap_or_else(|| beats.trim());
        let denominator = beat_type.trim().parse::<u32>().ok()?;
        let numerator = additive_u32(beats)?;
        return Some(Fraction::new(numerator, denominator));
    }
    None
}

fn additive_u32(value: &str) -> Option<u32> {
    let mut total = 0u32;
    let mut saw_part = false;
    for part in value.split('+') {
        total = total.checked_add(part.trim().parse::<u32>().ok()?)?;
        saw_part = true;
    }
    saw_part.then_some(total)
}

fn barline_lowering_kinds_with_kind(
    barline: &BarlineSyntax,
    kind: BarlineKind,
) -> Vec<BarlineKind> {
    let raw = barline.raw.strip_prefix('.').unwrap_or(&barline.raw);
    if kind == BarlineKind::RepeatStart {
        // A fused closer+repeat-start run (`|]:`, `]||:`, `||]:`): the thick `]`
        // bar closes the current measure (light-heavy) while the trailing `:`
        // opens the next section's repeat. Split like `||:`/`[|:` so the closer
        // lands on this measure's right and the forward repeat leads the next
        // measure — otherwise the `]` final bar is silently dropped (ABC 2.1
        // §4.8). Checked before the `||`/`[|` arms because `||]:` starts with
        // `||` yet must close light-heavy, not light-light.
        if raw.contains(']') {
            return vec![BarlineKind::Final, BarlineKind::RepeatStart];
        }
        if raw.starts_with("||") {
            return vec![BarlineKind::Double, BarlineKind::RepeatStart];
        }
        if raw.starts_with("[|") {
            return vec![BarlineKind::Initial, BarlineKind::RepeatStart];
        }
    }
    vec![kind]
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
