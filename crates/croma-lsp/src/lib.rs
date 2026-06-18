//! croma's language-server support, split into a **transport-free analysis
//! layer** (this library) and a thin `lsp-server` shell (the `croma-lsp`
//! binary), per the LSP promotion spec
//! (`docs/superpowers/specs/2026-06-17-lsp-promotion.md`).
//!
//! The library exposes pure, synchronous, panic-free functions that map source
//! text to LSP payloads — currently [`diagnostics::diagnostics`] plus the
//! [`position`] mapping and the [`document::DocumentStore`]. The binary owns the
//! document store and the JSON-RPC loop and dispatches to these functions; it
//! contains no business logic. Because the analysis layer is transport-free, the
//! corpus totality gate (leg C) drives it in-process, mirroring the formatter's
//! `corpus_proof`.
//!
//! The LSP never diverges from the core: it adapts `croma-core`'s diagnostics
//! and spans, it never reparses (spec decision 5). Any LSP-vs-core mismatch is a
//! bug in this adapter, not a new spec.

// lsp-types 0.97's `Uri` wraps `fluent_uri::Uri`, which clippy flags as an
// interior-mutable map key. Its `Hash`/`Eq` are over the URI string (the inner
// cell only caches parsing), so keying `WorkspaceEdit.changes` and the
// `DocumentStore` on it is sound — the `mutable_key_type` lint is a false
// positive here.
#![allow(clippy::mutable_key_type)]

use croma_core::{Diagnostic, export_musicxml};

pub mod code_action;
pub mod completion;
pub mod diagnostics;
pub mod document;
pub mod formatting;
pub mod hover;
pub mod position;
pub mod structure;
pub mod tables;
pub mod tokens;

#[cfg(test)]
mod corpus_proof;

pub use code_action::code_actions;
pub use completion::completion;
pub use diagnostics::diagnostics;
pub use document::DocumentStore;
pub use formatting::formatting;
pub use hover::hover;
pub use position::{
    PositionEncoding, byte_to_position, position_to_byte, span_length, span_to_range,
};
pub use structure::{document_symbols, folding_ranges};
pub use tokens::{legend, semantic_tokens};

/// The result of analysing one document: the core diagnostics plus whether the
/// source lowered to MusicXML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentAnalysis {
    pub diagnostics: Vec<Diagnostic>,
    pub can_export_musicxml: bool,
}

/// Run croma-core's analysis over `source`, collecting its diagnostics.
///
/// This is the single seam the LSP layer adapts: it wraps `export_musicxml` and
/// normalises a hard error with no diagnostics into one synthetic `export_error`
/// diagnostic so the client always has something to show.
pub fn analyze_document(source: &str) -> DocumentAnalysis {
    match export_musicxml(source) {
        Ok(export) => DocumentAnalysis {
            diagnostics: export.diagnostics,
            can_export_musicxml: true,
        },
        Err(error) => {
            let diagnostics = if error.diagnostics().is_empty() {
                vec![Diagnostic::new(
                    croma_core::Severity::Error,
                    "export_error",
                    error.to_string(),
                    croma_core::Span::default(),
                )]
            } else {
                error.diagnostics().to_vec()
            };

            DocumentAnalysis {
                diagnostics,
                can_export_musicxml: false,
            }
        }
    }
}
