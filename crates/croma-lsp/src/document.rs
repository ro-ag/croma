//! The in-memory document store backing the server's incremental sync.
//!
//! Per the promotion spec (decision: incremental sync), the store keeps one
//! `String` per open document URI. Each `textDocument/didChange`
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
use lsp_types::{TextDocumentContentChangeEvent, Url};

use crate::position::{PositionEncoding, position_to_byte};

/// A store of open documents keyed by URI.
#[derive(Debug, Default)]
pub struct DocumentStore {
    documents: HashMap<Url, String>,
}

impl DocumentStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or replace) the full text of `uri`, as on `textDocument/didOpen`.
    pub fn open(&mut self, uri: Url, text: String) {
        self.documents.insert(uri, text);
    }

    /// Remove `uri` from the store, as on `textDocument/didClose`. No-op if it
    /// was not open.
    pub fn close(&mut self, uri: &Url) {
        self.documents.remove(uri);
    }

    /// The current text of `uri`, if open.
    pub fn get(&self, uri: &Url) -> Option<&str> {
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
        uri: &Url,
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
    start = clamp_to_boundary(text, start);
    end = clamp_to_boundary(text, end);

    text.replace_range(start..end, &change.text);
}

/// Clamp a byte `offset` into `[0, len]` and onto the nearest lower char
/// boundary, so `replace_range` can never split a multi-byte scalar.
fn clamp_to_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range};

    fn uri() -> Url {
        Url::parse("file:///tune.abc").expect("valid test uri")
    }

    fn change_at(range: Option<Range>, text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range,
            range_length: None,
            text: text.to_string(),
        }
    }

    fn full(text: &str) -> TextDocumentContentChangeEvent {
        change_at(None, text)
    }

    fn ranged(sl: u32, sc: u32, el: u32, ec: u32, text: &str) -> TextDocumentContentChangeEvent {
        change_at(
            Some(Range {
                start: Position {
                    line: sl,
                    character: sc,
                },
                end: Position {
                    line: el,
                    character: ec,
                },
            }),
            text,
        )
    }

    #[test]
    fn open_get_close_round_trip() {
        let mut store = DocumentStore::new();
        store.open(uri(), "X:1\n".to_string());
        assert_eq!(store.get(&uri()), Some("X:1\n"));
        assert_eq!(store.len(), 1);
        store.close(&uri());
        assert_eq!(store.get(&uri()), None);
        assert!(store.is_empty());
    }

    #[test]
    fn full_replace_when_no_range() {
        let mut store = DocumentStore::new();
        store.open(uri(), "old".to_string());
        let out = store
            .change(&uri(), vec![full("brand new")], PositionEncoding::Utf8)
            .map(str::to_string);
        assert_eq!(out.as_deref(), Some("brand new"));
    }

    #[test]
    fn ranged_insert_at_line_start() {
        let mut store = DocumentStore::new();
        store.open(uri(), "X:1\nK:C\n".to_string());
        // Insert "T:Hi\n" at start of line 1.
        store.change(
            &uri(),
            vec![ranged(1, 0, 1, 0, "T:Hi\n")],
            PositionEncoding::Utf8,
        );
        assert_eq!(store.get(&uri()), Some("X:1\nT:Hi\nK:C\n"));
    }

    #[test]
    fn ranged_replace_spanning_characters() {
        let mut store = DocumentStore::new();
        store.open(uri(), "abcdef\n".to_string());
        // Replace "cd" (cols 2..4 on line 0) with "ZZZ".
        store.change(
            &uri(),
            vec![ranged(0, 2, 0, 4, "ZZZ")],
            PositionEncoding::Utf8,
        );
        assert_eq!(store.get(&uri()), Some("abZZZef\n"));
    }

    #[test]
    fn ranged_delete_a_middle_line() {
        let mut store = DocumentStore::new();
        store.open(uri(), "a\nb\nc\n".to_string());
        // Delete line 1 entirely: from (1,0) to (2,0).
        store.change(&uri(), vec![ranged(1, 0, 2, 0, "")], PositionEncoding::Utf8);
        assert_eq!(store.get(&uri()), Some("a\nc\n"));
    }

    #[test]
    fn range_past_eof_clamps_and_appends() {
        let mut store = DocumentStore::new();
        store.open(uri(), "abc".to_string());
        // Start and end both well past EOF -> clamp to len, i.e. append.
        store.change(
            &uri(),
            vec![ranged(99, 99, 99, 99, "XYZ")],
            PositionEncoding::Utf8,
        );
        assert_eq!(store.get(&uri()), Some("abcXYZ"));
    }

    #[test]
    fn reversed_range_is_normalised() {
        let mut store = DocumentStore::new();
        store.open(uri(), "abcdef\n".to_string());
        // End before start; should still replace the [1,4) slice "bcd".
        store.change(
            &uri(),
            vec![ranged(0, 4, 0, 1, "_")],
            PositionEncoding::Utf8,
        );
        assert_eq!(store.get(&uri()), Some("a_ef\n"));
    }

    #[test]
    fn multibyte_aware_edit_utf16() {
        let mut store = DocumentStore::new();
        store.open(uri(), "Café\n".to_string());
        // Under UTF-16, 'é' is 1 unit; replace it (cols 3..4) with "e".
        store.change(
            &uri(),
            vec![ranged(0, 3, 0, 4, "e")],
            PositionEncoding::Utf16,
        );
        assert_eq!(store.get(&uri()), Some("Cafe\n"));
    }

    #[test]
    fn sequence_of_edits_applies_in_order() {
        let mut store = DocumentStore::new();
        store.open(uri(), "".to_string());
        // Simulate "type as you go": full-replace then incremental appends.
        store.change(
            &uri(),
            vec![
                full("X:1\n"),
                ranged(1, 0, 1, 0, "K:C\n"),
                ranged(2, 0, 2, 0, "CDEF|\n"),
            ],
            PositionEncoding::Utf8,
        );
        assert_eq!(store.get(&uri()), Some("X:1\nK:C\nCDEF|\n"));
    }

    #[test]
    fn change_on_unknown_uri_returns_none() {
        let mut store = DocumentStore::new();
        assert!(
            store
                .change(&uri(), vec![full("x")], PositionEncoding::Utf8)
                .is_none()
        );
    }

    #[test]
    fn clear_to_empty_then_garbage_never_panics() {
        let mut store = DocumentStore::new();
        store.open(uri(), "X:1\nK:C\n".to_string());
        store.change(&uri(), vec![full("")], PositionEncoding::Utf8);
        assert_eq!(store.get(&uri()), Some(""));
        // Garbage range against an empty doc -> clamps to (0,0).
        store.change(
            &uri(),
            vec![ranged(5, 5, 9, 9, "[[[")],
            PositionEncoding::Utf8,
        );
        assert_eq!(store.get(&uri()), Some("[[["));
    }
}
