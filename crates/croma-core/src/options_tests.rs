use super::*;

#[test]
fn export_options_expose_parser_options() {
    let options = ExportOptions {
        spec: AbcSpecVersion::V22Draft,
        parse_mode: ParseMode::Loose,
        diagnostics: DiagnosticOptions::default(),
        write: MusicXmlWriteOptions::default(),
    };

    assert_eq!(
        options.parse_options(),
        ParseOptions::new(AbcSpecVersion::V22Draft, ParseMode::Loose)
    );
}

#[test]
fn croma_carrier_warning_suppression_matches_private_namespace_and_midi() {
    let diagnostics = DiagnosticOptions::default().suppress_croma_carrier_warnings();

    assert!(!diagnostics.should_emit_croma_carrier_warning("croma-future"));
    assert!(!diagnostics.should_emit_croma_carrier_warning("CROMA-future"));
    assert!(!diagnostics.should_emit_croma_carrier_warning("MIDI"));
    assert!(!diagnostics.should_emit_croma_carrier_warning("midi"));
    assert!(diagnostics.should_emit_croma_carrier_warning("tuplets"));
    assert!(diagnostics.should_emit_croma_carrier_warning("not-croma-future"));
}
