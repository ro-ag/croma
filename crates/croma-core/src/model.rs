use crate::{Diagnostic, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tune {
    pub reference: String,
    pub title: String,
    pub meter: String,
    pub key: String,
    pub divisions: u32,
    pub events: Vec<Event>,
    pub voices: Vec<VoiceTimeline>,
    pub score_directives: Vec<ScoreDirectiveModel>,
    pub preserved_directives: Vec<PreservedDirective>,
    pub post_tune_lyrics: Vec<TextLine>,
    pub score: Score,
}

/// Rational duration used by semantic lowering before MusicXML divisions are
/// selected.
pub type Rational = Fraction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Score {
    pub metadata: ScoreMetadata,
    pub parts: Vec<Part>,
    pub diagnostics: Vec<Diagnostic>,
    pub divisions: u32,
    pub source_span: Span,
    pub accidental_policy: AccidentalPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreMetadata {
    pub reference: TextLine,
    pub title: Option<TextLine>,
    pub composers: Vec<TextLine>,
    pub tempo: Option<TextLine>,
    /// Structured interpretation of [`Self::tempo`] for `<metronome>` export.
    pub tempo_model: Option<TempoModel>,
    pub meter: Option<MeterModel>,
    pub key: Option<KeySignatureModel>,
    pub directives: Vec<ScoreDirectiveModel>,
    pub preserved_directives: Vec<PreservedDirective>,
    pub post_tune_lyrics: Vec<TextLine>,
    pub source_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreservedDirective {
    pub name: TextLine,
    pub value: TextLine,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeterModel {
    pub display: String,
    pub duration: Option<Rational>,
    pub free_meter: bool,
    pub source_span: Span,
}

/// A parsed ABC `Q:` tempo field, structured for MusicXML `<metronome>` export.
///
/// The raw field text is preserved in [`ScoreMetadata::tempo`]; this model adds
/// the interpreted beat unit, beats-per-minute and any leading quoted text so
/// the writer can emit a real metronome direction rather than plain words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempoModel {
    /// Optional quoted text accompanying the tempo (e.g. `"Allegro"`).
    pub text: Option<String>,
    /// Beat unit and bpm, when the field carries a numeric tempo.
    pub beat: Option<TempoBeat>,
    /// Whether the numeric tempo was a visible metronome or playback-only
    /// MusicXML `<sound tempo>`. ABC `Q:` creates visible metronomes; imported
    /// playback-only tempo uses a croma carrier so the round trip does not print
    /// a metronome that was absent from the source.
    pub beat_role: TempoBeatRole,
    pub source_span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempoBeatRole {
    PrintedMetronome,
    PlaybackSoundOnly,
}

/// The numeric component of a tempo: a beat-unit fraction and its bpm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TempoBeat {
    /// Beat-unit fraction numerator (e.g. `1` for `1/4`, `3` for `3/8`).
    pub beat_numerator: u32,
    /// Beat-unit fraction denominator (e.g. `4` for `1/4`, `8` for `3/8`).
    pub beat_denominator: u32,
    /// Beats per minute.
    pub bpm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySignatureModel {
    pub display: String,
    pub fifths: i8,
    pub explicit_accidentals: Vec<KeyAccidentalModel>,
    pub source_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClefChangeModel {
    pub clef: TextLine,
    pub source_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyAccidentalModel {
    pub step: char,
    pub accidental: Accidental,
    pub source_span: Span,
}

/// ABC accidentals are source-significant. Croma records the written accidental
/// exactly when present, applies it to following matching pitches in the same
/// measure, and clears that measure-local state at each barline. When the key
/// signature or syntax leaves behavior ambiguous, diagnostics keep the policy
/// decision source-spanned instead of silently changing notation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccidentalPolicy {
    pub preserve_explicit_accidentals: bool,
    pub reset_at_barlines: bool,
    pub scope: AccidentalScope,
    pub source_span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccidentalScope {
    PitchAndOctave,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub id: PartId,
    pub name: Option<TextLine>,
    pub instruments: Vec<MusicXmlPartInstrumentModel>,
    pub staves: Vec<Staff>,
    pub voices: Vec<Voice>,
    pub source_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartId {
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staff {
    pub id: StaffId,
    pub voices: Vec<VoiceId>,
    pub source_span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaffId {
    pub value: u32,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voice {
    pub id: VoiceId,
    pub staff: StaffId,
    pub initial_properties: VoicePropertiesModel,
    pub properties: VoicePropertiesModel,
    pub measures: Vec<Measure>,
    pub events: Vec<TimedEvent>,
    /// Score-level MIDI instrument projected from this voice's `%%MIDI program`
    /// / `%%MIDI channel` directives (see [`MidiInstrumentModel`]). `None` when
    /// the voice declared no score-translatable MIDI instrument.
    pub midi_instrument: Option<MidiInstrumentModel>,
    /// Chromatic transposition (semitones) projected from this voice's
    /// `%%MIDI transpose <n>` directive, emitted as MusicXML
    /// `<attributes><transpose><chromatic>`. Distinct from the ABC `transpose=`
    /// voice property (which is carried in [`VoicePropertiesModel`] and takes
    /// precedence when both are present). `None` when no `%%MIDI transpose` was
    /// declared.
    pub midi_transpose: Option<i16>,
    pub source_span: Span,
}

/// A score-level MIDI instrument projected from a voice's `%%MIDI program` /
/// `channel` / `control` / `midi-unpitched` directives.
///
/// `%%MIDI` is an abc2midi convention, not part of ABC 2.1; the score-meaningful
/// fields (instrument identity, channel, CC7 volume, CC10 pan, unpitched
/// percussion key) are
/// forward-translated to MusicXML `<score-instrument>` / `<midi-instrument>`
/// while the raw directive text stays in [`ScoreMetadata::preserved_directives`]
/// for round-trip / formatter use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiInstrumentModel {
    /// General MIDI program number exactly as written in the directive
    /// (0-based, abc2midi/GM convention). The MusicXML `<midi-program>` is this
    /// value plus one (1-based). `None` when no program was declared.
    pub program: Option<u8>,
    /// MIDI channel (1-16) from the two-integer `program <chan> <prog>` form or
    /// a sibling `%%MIDI channel` directive in the same voice. `None` when no
    /// channel was declared.
    pub channel: Option<u8>,
    /// MIDI CC7 (channel volume) value 0-127 from `%%MIDI control 7 <n>`,
    /// rendered as `<midi-instrument><volume>` (`n / 1.27`). `None` when unset.
    pub volume_cc: Option<u8>,
    /// MIDI CC10 (pan) value 0-127 from `%%MIDI control 10 <n>`, rendered as
    /// `<midi-instrument><pan>` (`n / 127 * 180 - 90`). `None` when unset.
    pub pan_cc: Option<u8>,
    /// MusicXML `<midi-unpitched>` value for unpitched percussion playback
    /// (1-128). `None` when the instrument is pitched or unspecified.
    pub midi_unpitched: Option<u8>,
    /// Source span of the directive that last set this instrument.
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicXmlPartInstrumentModel {
    pub id: String,
    pub name: Option<TextLine>,
    pub midi: Option<MidiInstrumentModel>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicXmlInstrumentRef {
    pub id: String,
    pub span: Span,
}

impl MidiInstrumentModel {
    /// True when this projection carries at least one field worth emitting as a
    /// MusicXML `<score-instrument>` / `<midi-instrument>`.
    pub fn has_content(&self) -> bool {
        self.program.is_some()
            || self.channel.is_some()
            || self.volume_cc.is_some()
            || self.pan_cc.is_some()
            || self.midi_unpitched.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeasureId {
    pub index: u32,
    pub number: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measure {
    pub id: MeasureId,
    pub source_span: Span,
    pub expected_duration: Option<Rational>,
    pub actual_duration: Rational,
    /// Display hint for the first measure of an expanded ABC `Zn` rest run.
    /// The measures still exist individually; this only requests the compact
    /// MusicXML multi-rest glyph.
    pub multiple_rest: Option<u32>,
    pub pickup: bool,
    pub complete: bool,
    pub barlines: Vec<MeasureBarline>,
    pub repeat_endings: Vec<RepeatEndingModel>,
    pub overlays: Vec<OverlaySegment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeasureBarline {
    pub kind: BarlineKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatEndingModel {
    pub span: Span,
    pub endings: Vec<RepeatEndingPartModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepeatEndingPartModel {
    Single(u32),
    Range { start: u32, end: u32 },
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedEvent {
    pub measure: MeasureId,
    pub onset: Rational,
    pub duration: Rational,
    pub source: Span,
    pub kind: TimedEventKind,
    pub attachments: EventAttachments,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimedEventKind {
    Note(NoteEvent),
    Chord(ChordEvent),
    Rest(RestEvent),
    Spacer,
    Barline(MeasureBarline),
    RepeatEnding(RepeatEndingModel),
    /// A mid-tune key change (`[K:..]` or a body `K:` line). Zero duration;
    /// the new key's alters are already baked into later pitches — the event
    /// records WHERE the change was written so exporters can reproduce it.
    KeyChange(KeySignatureModel),
    /// A mid-tune meter change (`[M:..]` or a body `M:` line). Zero duration.
    MeterChange(MeterModel),
    /// A mid-tune clef change carried by `[K:..]` or a body `K:` line. Zero
    /// duration; following pitches are already lowered with the new voice
    /// octave shift.
    ClefChange(ClefChangeModel),
    /// A mid-tune tempo change (`[Q:..]` or a body `Q:` line). Zero duration.
    TempoChange(TempoModel),
    /// A body/inline `P:` section label (ABC 2.1 §4.3), e.g. `P:A` or `[P:B]`.
    /// Zero duration; emitted as a MusicXML `<rehearsal>`. The header `P:ABAB`
    /// play-order macro is a different construct and never produces this.
    SectionLabel(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteEvent {
    pub pitch: Pitch,
    pub written_accidental: Option<AccidentalMark>,
    pub chord_member: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChordEvent {
    pub members: Vec<ChordMemberEvent>,
    pub source_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChordMemberEvent {
    pub pitch: Pitch,
    pub duration: Rational,
    pub written_accidental: Option<AccidentalMark>,
    pub source_span: Span,
    pub attachments: EventAttachments,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestEvent {
    pub visibility: RestVisibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pitch {
    pub step: char,
    pub alter: i8,
    pub octave: i8,
    pub spelling_source: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccidentalMark {
    pub kind: Accidental,
    pub explicit: bool,
    pub courtesy: bool,
    pub source: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventAttachments {
    pub grace_groups: Vec<GraceGroupAttachment>,
    pub after_grace_groups: Vec<GraceGroupAttachment>,
    pub chord_symbols: Vec<TextAttachment>,
    pub annotations: Vec<TextAttachment>,
    pub decorations: Vec<DecorationAttachment>,
    pub instrument: Option<MusicXmlInstrumentRef>,
    pub lyrics: Vec<AlignedLyric>,
    pub symbols: Vec<AlignedSymbol>,
    pub ties: Vec<TieAttachment>,
    pub slurs: Vec<SlurAttachment>,
    pub tuplets: Vec<TupletAttachment>,
}

impl EventAttachments {
    pub fn is_empty(&self) -> bool {
        self.grace_groups.is_empty()
            && self.after_grace_groups.is_empty()
            && self.chord_symbols.is_empty()
            && self.annotations.is_empty()
            && self.decorations.is_empty()
            && self.instrument.is_none()
            && self.lyrics.is_empty()
            && self.symbols.is_empty()
            && self.ties.is_empty()
            && self.slurs.is_empty()
            && self.tuplets.is_empty()
    }

    pub(crate) fn extend(&mut self, other: EventAttachments) {
        self.grace_groups.extend(other.grace_groups);
        self.after_grace_groups.extend(other.after_grace_groups);
        self.chord_symbols.extend(other.chord_symbols);
        self.annotations.extend(other.annotations);
        self.decorations.extend(other.decorations);
        if self.instrument.is_none() {
            self.instrument = other.instrument;
        }
        self.lyrics.extend(other.lyrics);
        self.symbols.extend(other.symbols);
        self.ties.extend(other.ties);
        self.slurs.extend(other.slurs);
        self.tuplets.extend(other.tuplets);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraceGroupAttachment {
    pub span: Span,
    pub slash: Option<Span>,
    pub note_count: u32,
    pub events: Vec<GraceEvent>,
    /// Slurs that bind to the FIRST grace note of this group, e.g. the `(` in
    /// `({grace}note)` opens before the grace group, so the slur starts on the
    /// grace note rather than the following main note (ABC 2.1 §4.11 + §4.20).
    pub slurs: Vec<SlurAttachment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraceEvent {
    pub source_span: Span,
    pub kind: GraceEventKind,
    pub slurs: Vec<SlurAttachment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraceEventKind {
    Note(GraceNoteEvent),
    Rest(RestEvent),
    Chord(Vec<GraceNoteEvent>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraceNoteEvent {
    pub pitch: Pitch,
    pub written_accidental: Option<AccidentalMark>,
    /// Written length modifier of the grace note relative to the grace base
    /// unit (`/` -> 1/2, `2` -> 2, etc.; `1` when no modifier is written).
    pub length_multiplier: Fraction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextAttachment {
    pub text: String,
    pub span: Span,
    pub placement: Option<AnnotationPlacementModel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationPlacementModel {
    Above,
    Below,
    Left,
    Right,
    Free,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecorationAttachment {
    pub name: String,
    pub span: Span,
    pub source_kind: DecorationSourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationSourceKind {
    Named,
    LegacyNamed,
    Shorthand,
    UserDefined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TieAttachment {
    pub pair_id: u32,
    pub role: TieRole,
    pub span: Span,
    pub dotted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TieRole {
    Start,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlurAttachment {
    pub pair_id: u32,
    pub role: SlurRole,
    pub span: Span,
    pub dotted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlurRole {
    Start,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TupletAttachment {
    pub pair_id: u32,
    pub actual_notes: u32,
    pub normal_notes: u32,
    pub role: TupletRole,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TupletRole {
    Start,
    Continue,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Note {
        step: char,
        octave: i8,
        accidental: Option<Accidental>,
        chord: bool,
        duration: u32,
        span: Span,
    },
    Rest {
        visibility: RestVisibility,
        duration: u32,
        span: Span,
    },
    Spacer {
        span: Span,
    },
    Barline {
        kind: BarlineKind,
        span: Span,
    },
}

impl Event {
    pub fn span(&self) -> Span {
        match self {
            Self::Note { span, .. }
            | Self::Rest { span, .. }
            | Self::Spacer { span }
            | Self::Barline { span, .. } => *span,
        }
    }

    pub(crate) fn is_time_bearing(&self) -> bool {
        matches!(self, Self::Note { .. } | Self::Rest { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceTimeline {
    pub id: VoiceId,
    pub initial_properties: VoicePropertiesModel,
    pub properties: VoicePropertiesModel,
    pub measures: Vec<VoiceMeasureTimeline>,
    /// MusicXML-origin part-list instruments carried through ABC as croma
    /// `%%MIDI instrument-def` directives.
    pub musicxml_instruments: Vec<MusicXmlPartInstrumentModel>,
    /// Score-level MIDI instrument projected from this voice's `%%MIDI` program
    /// / channel directives, carried through to the semantic [`Voice`].
    pub midi_instrument: Option<MidiInstrumentModel>,
    /// Chromatic transposition (semitones) projected from this voice's
    /// `%%MIDI transpose` directive, carried through to the semantic [`Voice`].
    pub midi_transpose: Option<i16>,
    pub source_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceId {
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VoicePropertiesModel {
    pub name: Option<TextLine>,
    pub nm: Option<TextLine>,
    pub subname: Option<TextLine>,
    pub snm: Option<TextLine>,
    pub clef: Option<TextLine>,
    pub stem: Option<StemDirectionModel>,
    pub octave: Option<TextLine>,
    pub transpose: Option<TextLine>,
    pub middle: Option<TextLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StemDirectionModel {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceMeasureTimeline {
    pub index: u32,
    pub span: Span,
    pub events: Vec<VoiceTimedEvent>,
    pub overlays: Vec<OverlaySegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceTimedEvent {
    pub onset: Fraction,
    pub duration: Fraction,
    pub span: Span,
    pub line_index: usize,
    pub source_order: u32,
    pub alignable: bool,
    pub kind: TimelineEventKind,
    pub attachments: EventAttachments,
    pub lyrics: Vec<AlignedLyric>,
    pub symbols: Vec<AlignedSymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineEventKind {
    Note {
        step: char,
        octave: i8,
        accidental: Option<Accidental>,
        effective_accidental: Option<Accidental>,
        accidental_source: Option<Span>,
        chord: bool,
    },
    Rest {
        visibility: RestVisibility,
        multiple_rest: Option<u32>,
    },
    Spacer,
    Barline {
        kind: BarlineKind,
    },
    VariantEnding {
        endings: Vec<RepeatEndingPartModel>,
    },
    KeyChange(KeySignatureModel),
    MeterChange(MeterModel),
    ClefChange(ClefChangeModel),
    TempoChange(TempoModel),
    /// A body/inline `P:` section label, mirrored from [`TimedEventKind`] so the
    /// per-voice alignment timeline carries it (zero duration, non-alignable).
    SectionLabel(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlaySegment {
    pub id: VoiceId,
    pub span: Span,
    pub measure_index: u32,
    pub expected_duration: Fraction,
    pub actual_duration: Fraction,
    pub events: Vec<VoiceTimedEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignedLyric {
    pub verse: u32,
    pub text: String,
    pub span: Span,
    pub control: LyricControl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LyricControl {
    Syllable,
    Hyphen,
    Extender,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignedSymbol {
    pub layer: u32,
    pub text: String,
    pub span: Span,
    pub kind: AlignedSymbolKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignedSymbolKind {
    Decoration,
    ChordSymbol,
    Annotation,
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreDirectiveModel {
    pub span: Span,
    pub value: TextLine,
    pub tokens: Vec<ScoreDirectiveTokenModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreDirectiveTokenModel {
    pub span: Span,
    pub kind: ScoreDirectiveTokenKindModel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreDirectiveTokenKindModel {
    Voice(String),
    GroupStart(char),
    GroupEnd(char),
    StaffSeparator,
    MeasureSeparator,
    FloatingVoiceMarker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextLine {
    pub text: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestVisibility {
    Visible,
    Invisible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accidental {
    DoubleFlat,
    Flat,
    Natural,
    Sharp,
    DoubleSharp,
}

impl Accidental {
    pub(crate) fn alter(self) -> i8 {
        match self {
            Self::DoubleFlat => -2,
            Self::Flat => -1,
            Self::Natural => 0,
            Self::Sharp => 1,
            Self::DoubleSharp => 2,
        }
    }

    pub(crate) fn musicxml_name(self) -> &'static str {
        match self {
            Self::DoubleFlat => "flat-flat",
            Self::Flat => "flat",
            Self::Natural => "natural",
            Self::Sharp => "sharp",
            Self::DoubleSharp => "double-sharp",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarlineKind {
    Regular,
    Double,
    Final,
    Initial,
    RepeatStart,
    RepeatEnd,
    RepeatBoth,
    Dotted,
    Invisible,
    Liberal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LoweredEventAtom {
    pub kind: LoweredEventAtomKind,
    pub duration: Fraction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoweredEventAtomKind {
    Note {
        step: char,
        octave: i8,
        accidental: Option<Accidental>,
        effective_accidental: Option<Accidental>,
        accidental_source: Option<Span>,
        chord: bool,
        span: Span,
    },
    Rest {
        visibility: RestVisibility,
        multiple_rest: Option<u32>,
        span: Span,
    },
    Spacer {
        span: Span,
    },
}

impl LoweredEventAtom {
    pub(crate) fn into_event(self, divisions: u32) -> Event {
        let duration = self.duration.to_divisions(divisions);
        match self.kind {
            LoweredEventAtomKind::Note {
                step,
                octave,
                accidental,
                effective_accidental: _,
                accidental_source: _,
                chord,
                span,
            } => Event::Note {
                step,
                octave,
                accidental,
                chord,
                duration,
                span,
            },
            LoweredEventAtomKind::Rest {
                visibility, span, ..
            } => Event::Rest {
                visibility,
                duration,
                span,
            },
            LoweredEventAtomKind::Spacer { span } => Event::Spacer { span },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fraction {
    pub numerator: u32,
    pub denominator: u32,
}

impl Fraction {
    pub fn zero() -> Self {
        Self {
            numerator: 0,
            denominator: 1,
        }
    }

    pub fn new(numerator: u32, denominator: u32) -> Self {
        let denominator = denominator.max(1);
        let gcd = gcd(numerator, denominator);
        Self {
            numerator: numerator / gcd,
            denominator: denominator / gcd,
        }
    }

    pub fn one() -> Self {
        Self {
            numerator: 1,
            denominator: 1,
        }
    }

    pub(crate) fn checked_mul(self, other: Self) -> Self {
        Self::new(
            self.numerator.saturating_mul(other.numerator),
            self.denominator.saturating_mul(other.denominator),
        )
    }

    pub(crate) fn checked_mul_u32(self, value: u32) -> Self {
        Self::new(self.numerator.saturating_mul(value), self.denominator)
    }

    pub(crate) fn checked_add(self, other: Self) -> Self {
        let numerator = self
            .numerator
            .saturating_mul(other.denominator)
            .saturating_add(other.numerator.saturating_mul(self.denominator));
        let denominator = self.denominator.saturating_mul(other.denominator);
        Self::new(numerator, denominator)
    }

    pub(crate) fn less_than(self, other: Self) -> bool {
        u64::from(self.numerator) * u64::from(other.denominator)
            < u64::from(other.numerator) * u64::from(self.denominator)
    }

    pub(crate) fn divisions_requirement(self) -> u32 {
        let denominator = u64::from(self.denominator);
        let scaled_numerator = u64::from(self.numerator) * 4;
        let gcd = gcd_u64(denominator, scaled_numerator);
        u32::try_from(denominator / gcd).unwrap_or(u32::MAX)
    }

    pub(crate) fn to_divisions(self, divisions: u32) -> u32 {
        let numerator = u64::from(self.numerator) * 4 * u64::from(divisions);
        let denominator = u64::from(self.denominator);
        let value = numerator / denominator;
        u32::try_from(value.max(1)).unwrap_or(u32::MAX)
    }
}

pub(crate) fn lcm(left: u32, right: u32) -> u32 {
    if left == 0 || right == 0 {
        return left.max(right).max(1);
    }
    (left / gcd(left, right)).saturating_mul(right)
}

fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}

fn gcd_u64(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}
