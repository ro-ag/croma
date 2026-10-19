//! `textDocument/formatting` as a single whole-document replace.
//!
//! By design (whole-document replace, and promotion-bar leg B), the LSP never
//! formats independently: it calls the proven [`croma_fmt::format`] (idempotent
//! and lossless, 10000/0 over the corpus) and returns one full-document
//! [`lsp_types::TextEdit`] carrying its output.
//! If formatting is a no-op (`format(text) == text`) we return `vec![]` so the
//! client makes no edit. Because the single edit replaces the whole buffer with
//! `format`'s output verbatim, applying it reproduces `croma_fmt::format` exactly
//! — that byte-for-byte identity is leg B.

use croma_core::{SourceText, Span};
use croma_fmt::{FormatOptions, format};
use lsp_types::TextEdit;

use crate::position::{PositionEncoding, span_to_range};

/// Compute the formatting edits for `source` under `encoding`.
///
/// Returns an empty vector when `source` is already formatted; otherwise a
/// single [`TextEdit`] whose range spans the entire document (`0:0` to the
/// end-of-document position) and whose `new_text` is `croma_fmt::format(source)`.
pub fn formatting(source: &str, encoding: PositionEncoding) -> Vec<TextEdit> {
    let formatted = format(source, FormatOptions::default());
    if formatted == source {
        return Vec::new();
    }

    let text = SourceText::new(source);
    // The whole-document range: start of the document to the position just past
    // its last byte (encoding-aware via the R1 helper).
    let whole = span_to_range(&text, Span::new(0, text.len()), encoding);

    vec![TextEdit {
        range: whole,
        new_text: formatted,
    }]
}

#[cfg(test)]
#[path = "formatting_tests.rs"]
mod tests;
