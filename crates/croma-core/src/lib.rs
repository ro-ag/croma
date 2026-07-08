//! Croma core library.
//!
//! The first stable product surface is ABC -> MusicXML. CLI, formatter, and
//! language-server crates should call this library rather than reparsing ABC.

pub mod agent;
pub mod diagnostic;
pub mod error;
mod lower;
pub mod model;
pub mod musicxml;
pub mod options;
pub mod parse;
pub mod source;
pub mod syntax;
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

/// Experimental MusicXML -> [`Score`] reader (the inverse of [`write_musicxml`]).
///
/// Feature-gated behind `musicxml-reader`; the default build never compiles it
/// nor its sole optional dependency (`roxmltree`). See
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
mod tests {
    use super::*;

    fn engrave(source: &str) -> String {
        let options = ExportOptions::default().engrave();
        export_musicxml_with_options(source, options)
            .expect("engraving export should succeed")
            .musicxml
    }

    #[test]
    fn engrave_emits_beam_grouping() {
        // Eight eighth notes in 4/4 → two beamed groups of four.
        let xml = engrave("X:1\nL:1/8\nM:4/4\nK:C\nCDEF GABc|\n");
        let begins = xml.matches("<beam number=\"1\">begin</beam>").count();
        let ends = xml.matches("<beam number=\"1\">end</beam>").count();
        let continues = xml.matches("<beam number=\"1\">continue</beam>").count();
        assert_eq!(begins, 2, "two beam groups\n{xml}");
        assert_eq!(ends, 2);
        assert_eq!(continues, 4);
    }

    #[test]
    fn engrave_hides_bracket_on_beamed_triplet() {
        // A fully-beamed eighth-note triplet → number only, no bracket.
        let xml = engrave("X:1\nL:1/8\nM:4/4\nK:C\n(3CDE (3FGA|\n");
        assert!(
            xml.contains("<tuplet type=\"start\" number=\"1\" bracket=\"no\"/>"),
            "beamed triplet should be number-only\n{xml}"
        );
        assert!(xml.contains("<beam number=\"1\">begin</beam>"));
    }

    #[test]
    fn default_export_keeps_bare_tuplet() {
        let xml =
            abc_to_musicxml("X:1\nL:1/8\nM:4/4\nK:C\n(3CDE (3FGA|\n").expect("default export");
        assert!(xml.contains("<tuplet type=\"start\" number=\"1\"/>"));
        assert!(!xml.contains("bracket="));
    }

    #[test]
    fn engrave_emits_stem_direction() {
        // Quarter notes: C4 sits below the treble middle line → stem up; the higher
        // notes → stem down.
        let xml = engrave("X:1\nL:1/4\nM:4/4\nK:C\nC e c g|\n");
        assert!(xml.contains("<stem>up</stem>"), "{xml}");
        assert!(xml.contains("<stem>down</stem>"));
    }

    #[test]
    fn engrave_multivoice_rest_placement_and_parity() {
        // Overlay = two voices on one staff: upper voice stems up with its rest raised,
        // lower voice stems down with its rest lowered.
        let xml = engrave("X:1\nM:4/4\nL:1/4\nK:C\nc2 z2 & E2 z2|\n");
        assert!(xml.contains("<display-step>D</display-step>"), "{xml}");
        assert!(xml.contains("<display-octave>5</display-octave>"));
        assert!(xml.contains("<display-step>G</display-step>"));
        assert!(xml.contains("<stem>up</stem>"));
        assert!(xml.contains("<stem>down</stem>"));
    }

    #[test]
    fn engrave_slur_and_tie_placement() {
        let slur = engrave("X:1\nL:1/8\nM:4/4\nK:C\n(CD) z4|\n");
        assert!(
            slur.contains("<slur type=\"start\" number=\"1\" placement=\"below\"/>"),
            "{slur}"
        );
        let tie = engrave("X:1\nL:1/4\nM:4/4\nK:C\nC4-|C4|\n");
        assert!(tie.contains("orientation=\"over\""), "{tie}");
    }

    #[test]
    fn default_export_emits_no_stems_or_placement() {
        let xml = abc_to_musicxml("X:1\nL:1/4\nM:4/4\nK:C\nC e c g|\n").expect("default export");
        assert!(!xml.contains("<stem>"));
        assert!(!xml.contains("placement="));
        assert!(!xml.contains("orientation="));
        assert!(!xml.contains("display-step"));
    }

    #[test]
    fn default_export_emits_no_beams() {
        let xml = abc_to_musicxml("X:1\nL:1/8\nM:4/4\nK:C\nCDEF GABc|\n")
            .expect("default export should succeed");
        assert!(!xml.contains("<beam"), "default output must carry no beams");
    }

    #[test]
    fn engrave_default_matches_plain_export_byte_for_byte() {
        // The engraving profile must be strictly additive: with every hint off it is
        // byte-identical to the plain writer, protecting the round-trip gates.
        let sources = [
            "X:1\nT:Guard\nL:1/8\nM:4/4\nK:C\nCDEF GABc|G4 z4|\n",
            "X:1\nL:1/8\nM:6/8\nK:G\n(3ABc def|[CEG]2 z|\n", // tuplet + chord + rest
            "X:1\nM:4/4\nL:1/4\nK:C\nc2 z2 & E2 z2|\n",      // multi-voice overlay
            "X:1\nL:1/4\nM:4/4\nK:C\n(C4-|C4)|\n",           // tie + slur
        ];
        for source in sources {
            let plain = abc_to_musicxml(source).expect("plain export");
            let same = export_musicxml_with_options(source, ExportOptions::default())
                .expect("options export")
                .musicxml;
            assert_eq!(
                plain, same,
                "default options must match plain for\n{source}"
            );
        }
    }

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
