//! Conversion of croma-core [`Diagnostic`]s into LSP
//! [`lsp_types::Diagnostic`]s, parameterised by the negotiated position
//! encoding.
//!
//! This is a pure adapter: it calls the existing [`analyze_document`] (which
//! wraps `croma_core::export_musicxml`) and maps each core diagnostic's
//! `(severity, code, message, span)` onto the LSP shape. The byte `span` is
//! mapped to an in-bounds [`lsp_types::Range`] via
//! [`crate::position::span_to_range`]; the LSP layer never reparses or invents
//! diagnostics of its own (decision 5 in the promotion spec).

use croma_core::{Severity, SourceText};
use lsp_types::{DiagnosticSeverity, NumberOrString};

use crate::analyze_document;
use crate::position::{PositionEncoding, span_to_range};

/// The `source` field stamped on every LSP diagnostic we emit.
pub const DIAGNOSTIC_SOURCE: &str = "croma";

fn to_lsp_severity(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
    }
}

/// Analyse `source` and return the LSP diagnostics for it under `encoding`.
///
/// Pure and total: it builds a fresh [`SourceText`], runs the core analysis, and
/// maps each diagnostic. Every emitted [`lsp_types::Range`] is in-bounds for
/// `source` by construction of [`span_to_range`].
pub fn diagnostics(source: &str, encoding: PositionEncoding) -> Vec<lsp_types::Diagnostic> {
    let analysis = analyze_document(source);
    let text = SourceText::new(source);

    analysis
        .diagnostics
        .into_iter()
        .map(|diagnostic| lsp_types::Diagnostic {
            range: span_to_range(&text, diagnostic.span, encoding),
            severity: Some(to_lsp_severity(diagnostic.severity)),
            code: Some(NumberOrString::String(diagnostic.code.to_string())),
            source: Some(DIAGNOSTIC_SOURCE.to_string()),
            message: diagnostic.message,
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
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
}
