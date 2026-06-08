//! Croma core library.
//!
//! The first stable product surface is ABC -> MusicXML. CLI, formatter, and
//! language-server crates should call this library rather than reparsing ABC.

pub mod diagnostic;
pub mod error;
pub mod fields;
pub mod model;
pub mod music;
pub mod musicxml;
pub mod options;
pub mod parse;
pub mod source;
pub mod syntax;

pub use diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
pub use error::{CromaError, Result};
pub use fields::{DecorationDelimiter, FieldState, LineBreakMode, ParsedAbcFields, ParsedField};
pub use model::{
    Accidental, AccidentalMark, AccidentalPolicy, AccidentalScope, BarlineKind, ChordEvent,
    ChordMemberEvent, Event, EventAttachments, Fraction, KeySignatureModel, Measure, MeasureId,
    MeterModel, NoteEvent, Part, Pitch, Rational, RestEvent, RestVisibility, Score, ScoreMetadata,
    Staff, StaffId, TimedEvent, TimedEventKind, Tune, TupletAttachment, TupletRole, Voice,
};
pub use syntax::{
    BarlineSyntax, LengthSyntax, MusicItem, MusicLine, MusicToken, MusicTokenKind,
    ParsedMusicDocument, ParsedTuneMusic,
};
pub use options::{AbcSpecVersion, ExportOptions, LowerOptions, ParseMode, ParseOptions};
pub use parse::{AbcDocument, ParseReport};
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
    parse::parse_document(source, options)
}

pub fn lower_score(document: &AbcDocument, _options: LowerOptions) -> ParseReport<Option<Score>> {
    let report = parse::parse_tune_report_from_document(document);
    ParseReport {
        value: report.value.map(|tune| tune.score),
        diagnostics: report.diagnostics,
    }
}

pub fn write_musicxml(score: &Score) -> MusicXmlExport {
    let report = musicxml::write_score_partwise(score);
    MusicXmlExport {
        musicxml: report.value,
        diagnostics: report.diagnostics,
    }
}

pub fn export_musicxml_with_options(
    source: &str,
    options: ExportOptions,
) -> Result<MusicXmlExport> {
    let parse_report = parse_document(source, options.parse_options());
    if parse_report.has_errors() {
        return Err(CromaError::from_diagnostics(parse_report.diagnostics));
    }

    let ParseReport {
        value: document,
        mut diagnostics,
    } = parse_report;
    let tune_report = parse::parse_tune_report_from_document(&document);
    diagnostics.extend(tune_report.diagnostics);

    let Some(tune) = tune_report.value else {
        return Err(CromaError::from_diagnostics(diagnostics));
    };

    let write_report = musicxml::write_score_partwise(&tune.score);
    diagnostics.extend(write_report.diagnostics);

    Ok(MusicXmlExport {
        musicxml: write_report.value,
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
        assert!(xml.contains("<step>C</step>"));
        assert!(xml.contains("<octave>4</octave>"));
        assert!(xml.contains("<octave>5</octave>"));
        assert!(xml.contains("<type>eighth</type>"));
        assert!(!xml.contains("<measure number=\"3\">"));
    }

    #[test]
    fn exports_explicit_accidentals_to_musicxml() {
        let export = export_musicxml("X:1\nT:Accidentals\nL:1/8\nK:C\n^C =D __E\n")
            .expect("explicit accidentals should export");

        assert!(export.diagnostics.is_empty());
        assert!(export.musicxml.contains("<alter>1</alter>"));
        assert!(export.musicxml.contains("<accidental>sharp</accidental>"));
        assert!(export.musicxml.contains("<accidental>natural</accidental>"));
        assert!(export.musicxml.contains("<alter>-2</alter>"));
        assert!(
            export
                .musicxml
                .contains("<accidental>flat-flat</accidental>")
        );
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
