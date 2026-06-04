use crate::Span;

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
    pub post_tune_lyrics: Vec<TextLine>,
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
    pub properties: VoicePropertiesModel,
    pub measures: Vec<VoiceMeasureTimeline>,
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
    pub lyrics: Vec<AlignedLyric>,
    pub symbols: Vec<AlignedSymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineEventKind {
    Note {
        step: char,
        octave: i8,
        accidental: Option<Accidental>,
        chord: bool,
    },
    Rest {
        visibility: RestVisibility,
    },
    Spacer,
    Barline {
        kind: BarlineKind,
    },
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
pub(crate) struct TimedEvent {
    pub kind: TimedEventKind,
    pub duration: Fraction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimedEventKind {
    Note {
        step: char,
        octave: i8,
        accidental: Option<Accidental>,
        chord: bool,
        span: Span,
    },
    Rest {
        visibility: RestVisibility,
        span: Span,
    },
}

impl TimedEvent {
    pub(crate) fn into_event(self, divisions: u32) -> Event {
        let duration = self.duration.to_divisions(divisions);
        match self.kind {
            TimedEventKind::Note {
                step,
                octave,
                accidental,
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
            TimedEventKind::Rest { visibility, span } => Event::Rest {
                visibility,
                duration,
                span,
            },
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
