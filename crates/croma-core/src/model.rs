use crate::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tune {
    pub reference: String,
    pub title: String,
    pub meter: String,
    pub key: String,
    pub divisions: u32,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Note {
        step: char,
        octave: i8,
        accidental: Option<Accidental>,
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
                span,
            } => Event::Note {
                step,
                octave,
                accidental,
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
