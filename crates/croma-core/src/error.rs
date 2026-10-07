use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::Diagnostic;

/// Why an ABC conversion failed.
///
/// Every failure today is [`CromaError::ParseFailed`], carrying the error
/// diagnostics; an empty file, a missing `K:` and a tune without music arrive
/// there with the codes `abc.file.empty`, `abc.file.missing_k` and
/// `abc.file.no_music`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CromaError {
    #[deprecated(
        since = "1.4.0",
        note = "never constructed; an empty source is ParseFailed with code abc.file.empty"
    )]
    EmptyInput,
    #[deprecated(
        since = "1.4.0",
        note = "never constructed; a missing K: is ParseFailed with code abc.file.missing_k"
    )]
    MissingKey,
    #[deprecated(
        since = "1.4.0",
        note = "never constructed; a tune without music is ParseFailed with code abc.file.no_music"
    )]
    NoMusic,
    /// The source has errors; the diagnostics say which and where.
    ParseFailed(Vec<Diagnostic>),
}

impl Display for CromaError {
    // The deprecated variants still need a message while they exist.
    #[allow(deprecated)]
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
