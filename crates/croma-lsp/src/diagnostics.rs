//! Conversion of croma-core [`Diagnostic`]s into LSP
//! [`lsp_types::Diagnostic`]s, parameterised by the negotiated position
//! encoding.
//!
//! This is a pure adapter: it calls the existing [`analyze_document`] (which
//! wraps `croma_core::export_musicxml`) and maps each core diagnostic's
//! `(severity, code, message, span)` onto the LSP shape. The byte `span` is
//! mapped to an in-bounds [`lsp_types::Range`] via
//! [`crate::position::span_to_range`]; the LSP layer never reparses or invents
//! diagnostics of its own, by design.

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
#[path = "diagnostics_tests.rs"]
mod tests;
