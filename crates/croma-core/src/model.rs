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
        duration: u32,
    },
    Rest {
        duration: u32,
    },
    Bar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fraction {
    pub numerator: u32,
    pub denominator: u32,
}

impl Fraction {
    pub(crate) fn new(numerator: u32, denominator: u32) -> Self {
        Self {
            numerator,
            denominator: denominator.max(1),
        }
    }
}
