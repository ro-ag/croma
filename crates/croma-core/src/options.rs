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
pub struct DiagnosticOptions {
    pub suppress_croma_carrier_warnings: bool,
}

impl DiagnosticOptions {
    pub fn suppress_croma_carrier_warnings(mut self) -> Self {
        self.suppress_croma_carrier_warnings = true;
        self
    }

    pub(crate) fn should_emit_croma_carrier_warning(self, name: &str) -> bool {
        !self.suppress_croma_carrier_warnings || !is_croma_carrier_name(name)
    }
}

pub fn is_croma_carrier_name(name: &str) -> bool {
    name.trim_start()
        .get(.."croma-".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("croma-"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParseOptions {
    pub spec: AbcSpecVersion,
    pub mode: ParseMode,
    pub diagnostics: DiagnosticOptions,
}

impl ParseOptions {
    pub fn new(spec: AbcSpecVersion, mode: ParseMode) -> Self {
        Self {
            spec,
            mode,
            diagnostics: DiagnosticOptions::default(),
        }
    }

    pub fn suppress_croma_carrier_warnings(mut self) -> Self {
        self.diagnostics = self.diagnostics.suppress_croma_carrier_warnings();
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExportOptions {
    pub spec: AbcSpecVersion,
    pub parse_mode: ParseMode,
    pub diagnostics: DiagnosticOptions,
}

impl ExportOptions {
    pub fn parse_options(self) -> ParseOptions {
        ParseOptions {
            spec: self.spec,
            mode: self.parse_mode,
            diagnostics: self.diagnostics,
        }
    }

    pub fn suppress_croma_carrier_warnings(mut self) -> Self {
        self.diagnostics = self.diagnostics.suppress_croma_carrier_warnings();
        self
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
            diagnostics: DiagnosticOptions::default(),
        };

        assert_eq!(
            options.parse_options(),
            ParseOptions::new(AbcSpecVersion::V22Draft, ParseMode::Loose)
        );
    }

    #[test]
    fn croma_carrier_warning_suppression_matches_private_namespace_only() {
        let diagnostics = DiagnosticOptions::default().suppress_croma_carrier_warnings();

        assert!(!diagnostics.should_emit_croma_carrier_warning("croma-future"));
        assert!(!diagnostics.should_emit_croma_carrier_warning("CROMA-future"));
        assert!(diagnostics.should_emit_croma_carrier_warning("tuplets"));
        assert!(diagnostics.should_emit_croma_carrier_warning("not-croma-future"));
    }
}
