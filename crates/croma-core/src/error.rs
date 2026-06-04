use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::Diagnostic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CromaError {
    EmptyInput,
    MissingKey,
    NoMusic,
    ParseFailed(Vec<Diagnostic>),
}

impl Display for CromaError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("ABC source is empty"),
            Self::MissingKey => formatter.write_str("ABC source is missing a K: field"),
            Self::NoMusic => formatter.write_str("ABC source does not contain body music"),
            Self::ParseFailed(diagnostics) => {
                if let Some(diagnostic) = diagnostics.first() {
                    formatter.write_str(&diagnostic.message)
                } else {
                    formatter.write_str("ABC parse failed")
                }
            }
        }
    }
}

impl Error for CromaError {}

impl CromaError {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            Self::ParseFailed(diagnostics) => diagnostics,
            _ => &[],
        }
    }

    pub(crate) fn from_diagnostics(diagnostics: Vec<Diagnostic>) -> Self {
        Self::ParseFailed(diagnostics)
    }
}

pub type Result<T> = std::result::Result<T, CromaError>;
