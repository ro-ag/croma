//! `textDocument/formatting` as a single whole-document replace.
//!
//! Per the promotion spec (decision 3, "codeAction = whole-document replace")
//! and promotion-bar leg B, the LSP never formats independently: it calls the
//! proven [`croma_fmt::format`] (idempotent + lossless, 10000/0 over the corpus)
//! and returns one full-document [`lsp_types::TextEdit`] carrying its output.
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
mod tests {
    use super::*;
    use lsp_types::Position;

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn already_formatted_source_is_a_no_op() {
        // A canonical, fully-formatted tune should be a `format` fixed point, so
        // formatting returns no edits.
        let source = format("X:1\nT:T\nK:C\nCDEF|\n", FormatOptions::default());
        let edits = formatting(&source, PositionEncoding::Utf8);
        assert!(
            edits.is_empty(),
            "fixed point should yield no edits: {edits:?}"
        );
    }

    #[test]
    fn unformatted_source_yields_one_whole_document_edit() {
        // Loose whitespace that the formatter normalises -> exactly one edit.
        let source = "X:1\nK:C\nC   D   E   F|\n";
        let formatted = format(source, FormatOptions::default());
        assert_ne!(
            formatted, source,
            "fixture must actually change under format"
        );

        let edits = formatting(source, PositionEncoding::Utf8);
        assert_eq!(edits.len(), 1, "must be a single full-document edit");
        let edit = &edits[0];
        assert_eq!(
            edit.new_text, formatted,
            "edit text must equal croma_fmt::format"
        );
        assert_eq!(
            edit.range.start,
            pos(0, 0),
            "edit must start at document origin"
        );
    }

    #[test]
    fn applying_the_edit_reproduces_format_byte_for_byte() {
        // Leg B in miniature: replacing the whole-document range with the edit's
        // text yields exactly `format(source)`.
        let source = "X:1\nK:C\nC2D2|]\n\n\nE2F2|\n";
        let formatted = format(source, FormatOptions::default());
        let edits = formatting(source, PositionEncoding::Utf8);
        let applied = apply(source, &edits, PositionEncoding::Utf8);
        assert_eq!(applied, formatted);
    }

    #[test]
    fn end_position_covers_the_whole_document_under_utf16() {
        // A multi-byte char must not desync the end position from the buffer.
        let source = "T:Café\nK:C\nC   D|\n"; // unformatted body forces an edit
        let formatted = format(source, FormatOptions::default());
        if formatted == source {
            return; // formatter chose not to touch it; nothing to assert
        }
        let edits = formatting(source, PositionEncoding::Utf16);
        assert_eq!(edits.len(), 1);
        let applied = apply(source, &edits, PositionEncoding::Utf16);
        assert_eq!(applied, formatted, "utf16 whole-doc replace is exact");
    }

    /// Apply a full-document edit set the way a conformant client would: resolve
    /// the (single) edit's range to byte offsets and splice. Used to prove the
    /// round-trip identity locally; the corpus harness does the same at scale.
    fn apply(source: &str, edits: &[TextEdit], encoding: PositionEncoding) -> String {
        if edits.is_empty() {
            return source.to_string();
        }
        let text = SourceText::new(source);
        let edit = &edits[0];
        let start = crate::position::position_to_byte(&text, edit.range.start, encoding);
        let end = crate::position::position_to_byte(&text, edit.range.end, encoding);
        let mut out = source.to_string();
        out.replace_range(start..end, &edit.new_text);
        out
    }
}
