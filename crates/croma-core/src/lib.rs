//! Croma core library: the ABC 2.1 parser, score model, and ABC <-> MusicXML
//! conversion. The CLI, formatter, and language server call this library
//! rather than reparsing ABC.
//!
//! # Stability (1.x)
//!
//! Within 1.x, the conversion entry points ([`abc_to_musicxml`],
//! [`export_musicxml`], [`export_musicxml_with_options`], [`parse_document`],
//! [`lower_score`], [`write_musicxml`], [`write_abc`], and `read_musicxml` with
//! the `musicxml-reader` feature) and the builder methods on the option types
//! keep compiling. The public data types — the score model, syntax and parse
//! trees, diagnostics, and the option structs' fields — are not covered: a minor
//! release may add struct fields and enum variants. Build options from
//! `Default` plus the builder methods rather than struct literals, and give
//! `match`es on croma enums a wildcard arm. 2.0 will mark these types
//! `#[non_exhaustive]`.

pub mod agent;
pub mod diagnostic;
pub mod error;
mod escape;
mod hex;
mod lower;
pub mod model;
pub mod musicxml;
pub mod options;
pub mod parse;
pub mod source;
pub mod syntax;
mod text;
pub mod to_abc;

pub use agent::{AgentTopic, agent_topics, find_agent_topic};
pub use diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
pub use error::{CromaError, Result};
pub use model::{
    Accidental, AccidentalMark, AccidentalPolicy, AccidentalScope, BarlineKind, ChordEvent,
    ChordMemberEvent, Event, EventAttachments, Fraction, KeySignatureModel, Measure, MeasureId,
    MeterModel, NoteEvent, Part, Pitch, Rational, RestEvent, RestVisibility, Score, ScoreMetadata,
    Staff, StaffId, TimedEvent, TimedEventKind, Tune, TupletAttachment, TupletRole, Voice,
};
pub use options::{
    AbcSpecVersion, DiagnosticOptions, ExportOptions, LowerOptions, MusicXmlWriteOptions,
    ParseMode, ParseOptions, TupletDisplay,
};
pub use parse::field::{
    DecorationDelimiter, FieldState, LineBreakMode, ParsedAbcFields, ParsedField,
};
pub use parse::{AbcDocument, ParseReport};
pub use source::{LineColumn, LineColumnSpan, LineEnding, SourceLine, SourceText};
pub use syntax::{
    BarlineSyntax, LengthSyntax, MusicItem, MusicLine, MusicToken, MusicTokenKind,
    ParsedMusicDocument, ParsedTuneMusic,
};
pub use to_abc::{AbcWriteOptions, write_abc};

/// MusicXML -> [`Score`] reader (the inverse of [`write_musicxml`]).
///
/// Behind the `musicxml-reader` feature (on in the `croma` CLI); the default
/// library build never compiles it nor its sole optional dependency
/// (`roxmltree`). See
/// [`musicxml::read::read_musicxml`].
#[cfg(feature = "musicxml-reader")]
pub use musicxml::read::read_musicxml;

/// Complete a reader-built [`Score`] for the ABC projection only (synthesize
/// `voice.events` barline/ending events + a canonical key `display`). Applied on
/// the `croma read --format abc` / `musicxml2abc` paths, NOT on `--format xml`
/// (which must stay the pure `write_musicxml` inverse). See
/// [`musicxml::read::complete_score_for_abc`].
#[cfg(feature = "musicxml-reader")]
pub use musicxml::read::complete_score_for_abc;

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
    write_musicxml_with_options(score, MusicXmlWriteOptions::default())
}

/// Write MusicXML with render-oriented engraving hints (beams, stems, tuplet display,
/// ...). [`MusicXmlWriteOptions::default`] reproduces [`write_musicxml`] byte-for-byte.
pub fn write_musicxml_with_options(score: &Score, options: MusicXmlWriteOptions) -> MusicXmlExport {
    let report = musicxml::write_score_partwise_with_options(score, options);
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

    let write_report = musicxml::write_score_partwise_with_options(&tune.score, options.write);
    diagnostics.extend(write_report.diagnostics);

    Ok(MusicXmlExport {
        musicxml: write_report.value,
        diagnostics,
    })
}

#[cfg(test)]
#[path = "totality_tests.rs"]
mod totality_tests;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
