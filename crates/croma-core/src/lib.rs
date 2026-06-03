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
pub mod surface;

pub use diagnostic::{Diagnostic, Severity, Span};
pub use error::{CromaError, Result};
pub use model::{Event, Tune};
pub use options::{AbcSpecVersion, ExportOptions, ParseMode};

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

pub fn export_musicxml_with_options(
    source: &str,
    options: ExportOptions,
) -> Result<MusicXmlExport> {
    let surface = surface::analyze(source);
    let tune = parser::parse_tune(source, &surface, options)?;
    Ok(MusicXmlExport {
        musicxml: musicxml::write_score_partwise(&tune),
        diagnostics: Vec::new(),
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
    }
}
