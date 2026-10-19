
use super::*;

fn in_bounds(source: &str, diags: &[lsp_types::Diagnostic]) {
    let text = SourceText::new(source);
    let line_count = text.line_count() as u32;
    for d in diags {
        assert!(
            d.range.start.line < line_count.max(1),
            "start line in bounds"
        );
        assert!(d.range.end.line < line_count.max(1), "end line in bounds");
        assert!(
            (d.range.start.line, d.range.start.character)
                <= (d.range.end.line, d.range.end.character),
            "range well-formed: {:?}",
            d.range
        );
    }
}

#[test]
fn well_formed_tune_has_no_diagnostics_or_only_in_bounds() {
    let source = "X:1\nT:Test\nK:C\nCDEF|\n";
    let diags = diagnostics(source, PositionEncoding::Utf8);
    in_bounds(source, &diags);
}

#[test]
fn malformed_source_emits_in_bounds_diagnostics_in_both_encodings() {
    // A header-less / broken body should surface at least one diagnostic;
    // whatever it is, its range must be in-bounds.
    let source = "this is not abc at all \u{e9}\n[[[\n";
    for enc in [PositionEncoding::Utf8, PositionEncoding::Utf16] {
        let diags = diagnostics(source, enc);
        in_bounds(source, &diags);
    }
}

#[test]
fn diagnostics_carry_croma_source_and_string_code() {
    // Force at least one diagnostic via an empty document (no tune).
    let source = "";
    let diags = diagnostics(source, PositionEncoding::Utf8);
    for d in &diags {
        assert_eq!(d.source.as_deref(), Some(DIAGNOSTIC_SOURCE));
        assert!(matches!(d.code, Some(NumberOrString::String(_))));
        assert!(d.severity.is_some());
    }
}

#[test]
fn severity_mapping_is_exhaustive() {
    assert_eq!(to_lsp_severity(Severity::Error), DiagnosticSeverity::ERROR);
    assert_eq!(
        to_lsp_severity(Severity::Warning),
        DiagnosticSeverity::WARNING
    );
    assert_eq!(
        to_lsp_severity(Severity::Info),
        DiagnosticSeverity::INFORMATION
    );
}
