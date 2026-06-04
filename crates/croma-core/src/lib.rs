//! Croma core library.
//!
//! The first stable product surface is ABC -> MusicXML. CLI, formatter, and
//! language-server crates should call this library rather than reparsing ABC.

pub mod diagnostic;
pub mod error;
pub mod model;
pub mod musicxml;
pub mod options;
pub mod parser;
pub mod source;
pub mod surface;

pub use diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
pub use error::{CromaError, Result};
pub use model::{Event, Tune};
pub use options::{AbcSpecVersion, ExportOptions, ParseMode, ParseOptions};
pub use parser::{AbcDocument, ParseReport};
pub use source::{LineColumn, LineColumnSpan, LineEnding, SourceLine, SourceText};

#[cfg(test)]
pub(crate) mod test_support;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicXmlExport {
    pub musicxml: String,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn abc_to_musicxml(source: &str) -> Result<String> {
    export_musicxml(source).map(|export| export.musicxml)
}

pub fn export_musicxml(source: &str) -> Result<MusicXmlExport> {
    export_musicxml_with_options(source, ExportOptions::default())
}

pub fn parse_document(source: &str, options: ParseOptions) -> ParseReport<AbcDocument> {
    parser::parse_document(source, options)
}

pub fn export_musicxml_with_options(
    source: &str,
    options: ExportOptions,
) -> Result<MusicXmlExport> {
    let parse_report = parse_document(source, options.parse_options());
    if parse_report.has_errors() {
        return Err(CromaError::from_diagnostics(parse_report.diagnostics));
    }

    let surface = surface::analyze_source(&parse_report.value.source);
    let tune_report = parser::parse_tune_report(
        &parse_report.value.source,
        &surface,
        options.parse_options(),
    );
    let mut diagnostics = parse_report.diagnostics;
    diagnostics.extend(tune_report.diagnostics);

    let Some(tune) = tune_report.value else {
        return Err(CromaError::from_diagnostics(diagnostics));
    };

    Ok(MusicXmlExport {
        musicxml: musicxml::write_score_partwise(&tune),
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_basic_abc_to_musicxml() {
        let xml = abc_to_musicxml("X:1\nT:Scale\nM:4/4\nL:1/8\nK:C\nC D E F|G A B c|\n")
            .expect("basic ABC should export");

        assert!(xml.contains("<score-partwise version=\"4.0\">"));
        assert!(xml.contains("<part-name>Scale</part-name>"));
        assert!(xml.contains("<step>C</step><octave>4</octave>"));
        assert!(xml.contains("<step>C</step><octave>5</octave>"));
        assert!(xml.contains("<type>eighth</type>"));
        assert!(!xml.contains("<measure number=\"3\">"));
    }

    #[test]
    fn defaults_to_abc_21() {
        assert_eq!(ExportOptions::default().spec, AbcSpecVersion::V21);
        assert_eq!(ParseOptions::default().spec, AbcSpecVersion::V21);
    }

    #[test]
    fn export_errors_expose_exact_empty_input_diagnostic_span() {
        let error = export_musicxml("").expect_err("empty input should fail");
        let diagnostic = error
            .diagnostics()
            .first()
            .expect("expected parse diagnostic");

        assert_eq!(diagnostic.code, "abc.file.empty");
        assert_eq!(diagnostic.span, Span::new(0, 0));
    }

    #[test]
    fn export_errors_expose_exact_missing_key_diagnostic_span() {
        let source = SourceText::new("X:1\nT:No Key\n");
        let error = export_musicxml(source.as_str()).expect_err("missing key should fail");
        let diagnostic = error
            .diagnostics()
            .first()
            .expect("expected parse diagnostic");

        assert_eq!(diagnostic.code, "abc.file.missing_k");
        assert_eq!(diagnostic.span, Span::new(source.len(), source.len()));
    }
}
