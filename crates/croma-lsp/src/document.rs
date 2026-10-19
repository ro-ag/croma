//! The in-memory document store backing the server's incremental sync.
//!
//! To support incremental sync, the store keeps one `String` per open document
//! URI. Each `textDocument/didChange`
//! [`TextDocumentContentChangeEvent`] is applied against the **current** text:
//!
//! - with a `range`: the range is converted to byte offsets (clamped — never
//!   panicking) under the negotiated encoding and the affected slice is spliced
//!   out for the replacement text;
//! - without a `range`: the whole document is replaced (full-sync fallback).
//!
//! Every operation is **total**. Ranges past EOF, reversed ranges, mid-edit
//! truncations, and non-UTF-8-boundary offsets are all clamped, so no client
//! input — however malformed — can panic the store. This is the backbone of the
//! totality gate (leg C).

use std::collections::HashMap;

use croma_core::SourceText;
use lsp_types::{TextDocumentContentChangeEvent, Uri};

use crate::position::{PositionEncoding, position_to_byte};

/// A store of open documents keyed by URI.
#[derive(Debug, Default)]
pub struct DocumentStore {
    documents: HashMap<Uri, String>,
}

impl DocumentStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or replace) the full text of `uri`, as on `textDocument/didOpen`.
    pub fn open(&mut self, uri: Uri, text: String) {
        self.documents.insert(uri, text);
    }

    /// Remove `uri` from the store, as on `textDocument/didClose`. No-op if it
    /// was not open.
    pub fn close(&mut self, uri: &Uri) {
        self.documents.remove(uri);
    }

    /// The current text of `uri`, if open.
    pub fn get(&self, uri: &Uri) -> Option<&str> {
        self.documents.get(uri).map(String::as_str)
    }

    /// Number of open documents (test/diagnostic helper).
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Whether the store holds no documents.
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Apply a sequence of content changes to `uri` under `encoding`, returning
    /// the new full text (or `None` if `uri` is not open).
    ///
    /// Total: every change is clamped, so even a malformed mid-edit sequence
    /// leaves the store with valid UTF-8 and never panics.
    pub fn change(
        &mut self,
        uri: &Uri,
        changes: Vec<TextDocumentContentChangeEvent>,
        encoding: PositionEncoding,
    ) -> Option<&str> {
        let text = self.documents.get_mut(uri)?;
        for change in changes {
            apply_change(text, change, encoding);
        }
        Some(text.as_str())
    }
}

/// Apply a single content change to `text` in place under `encoding`.
fn apply_change(
    text: &mut String,
    change: TextDocumentContentChangeEvent,
    encoding: PositionEncoding,
) {
    let Some(range) = change.range else {
        // No range -> full-document replace (full-sync fallback).
        *text = change.text;
        return;
    };

    // Resolve the range against the *current* text, clamped to char boundaries.
    let source = SourceText::new(text.clone());
    let mut start = position_to_byte(&source, range.start, encoding);
    let mut end = position_to_byte(&source, range.end, encoding);
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }
    // Re-clamp onto char boundaries so `replace_range` can never split a scalar.
    start = crate::position::clamp_to_boundary(text, start);
    end = crate::position::clamp_to_boundary(text, end);

    text.replace_range(start..end, &change.text);
}

#[cfg(test)]
#[path = "document_tests.rs"]
mod tests;
