#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AbcSpecVersion {
    #[default]
    V21,
    V22Draft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParseMode {
    #[default]
    Strict,
    Loose,
    Recover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParseOptions {
    pub spec: AbcSpecVersion,
    pub mode: ParseMode,
}

impl ParseOptions {
    pub fn new(spec: AbcSpecVersion, mode: ParseMode) -> Self {
        Self { spec, mode }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExportOptions {
    pub spec: AbcSpecVersion,
    pub parse_mode: ParseMode,
}

impl ExportOptions {
    pub fn parse_options(self) -> ParseOptions {
        ParseOptions {
            spec: self.spec,
            mode: self.parse_mode,
        }
    }
}

impl From<ExportOptions> for ParseOptions {
    fn from(options: ExportOptions) -> Self {
        options.parse_options()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LowerOptions;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_options_expose_parser_options() {
        let options = ExportOptions {
            spec: AbcSpecVersion::V22Draft,
            parse_mode: ParseMode::Loose,
        };

        assert_eq!(
            options.parse_options(),
            ParseOptions::new(AbcSpecVersion::V22Draft, ParseMode::Loose)
        );
    }
}
