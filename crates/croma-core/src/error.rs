use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CromaError {
    EmptyInput,
    MissingKey,
    NoMusic,
}

impl Display for CromaError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("ABC source is empty"),
            Self::MissingKey => formatter.write_str("ABC source is missing a K: field"),
            Self::NoMusic => formatter.write_str("ABC source does not contain body music"),
        }
    }
}

impl Error for CromaError {}

pub type Result<T> = std::result::Result<T, CromaError>;
