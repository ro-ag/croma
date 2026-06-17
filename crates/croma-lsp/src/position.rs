//! Position mapping between croma's UTF-8 **byte** `Span`s and LSP
//! [`lsp_types::Range`]s, parameterised by the negotiated position encoding.
//!
//! croma reports every span as a `[start, end)` pair of UTF-8 byte offsets into
//! the document. LSP, by contrast, addresses text as 0-based `line` + 0-based
//! `character`, where the unit of `character` is the **negotiated**
//! [`PositionEncoding`]:
//!
//! - [`PositionEncoding::Utf8`] — character = `byte − line_start_byte`. An exact,
//!   lossless match for our byte spans.
//! - [`PositionEncoding::Utf16`] — character = the sum of [`char::len_utf16`]
//!   over the line prefix `[line_start, byte)`. For pure-ASCII ABC (the vast
//!   majority of the corpus) this equals the byte delta.
//!
//! Every public function is **total**: out-of-range or non-char-boundary byte
//! offsets are clamped, never panicked on. This is a hard requirement of the
//! totality gate (leg C) — a malformed mid-edit byte offset must still yield an
//! in-bounds `Range`.

use croma_core::{SourceText, Span};
use lsp_types::{Position, PositionEncodingKind, Range};

/// The position encoding negotiated with the client during `initialize`.
///
/// We prefer [`Utf8`](PositionEncoding::Utf8) (offered via
/// `InitializeParams.capabilities.general.position_encodings` since LSP 3.17)
/// because it matches croma's native byte offsets exactly; otherwise we fall
/// back to the protocol default, [`Utf16`](PositionEncoding::Utf16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PositionEncoding {
    /// `character` counts UTF-8 bytes from the line start.
    Utf8,
    /// `character` counts UTF-16 code units from the line start (LSP default).
    #[default]
    Utf16,
}

impl PositionEncoding {
    /// The matching [`PositionEncodingKind`] to advertise in
    /// `ServerCapabilities.position_encoding`.
    pub fn to_kind(self) -> PositionEncodingKind {
        match self {
            PositionEncoding::Utf8 => PositionEncodingKind::UTF8,
            PositionEncoding::Utf16 => PositionEncodingKind::UTF16,
        }
    }
}

/// Clamp a byte `offset` into `[0, len]` and onto the nearest **lower** UTF-8
/// char boundary, so it is always a valid index for slicing `text`.
fn clamp_to_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Convert a single UTF-8 byte `offset` into an LSP [`Position`] under `encoding`.
///
/// Total: the offset is first clamped into `[0, len]` and onto a char boundary,
/// so this never panics regardless of the input. An offset at or past EOF maps
/// to the end of the last line.
pub fn byte_to_position(
    source: &SourceText,
    offset: usize,
    encoding: PositionEncoding,
) -> Position {
    let text = source.as_str();
    let offset = clamp_to_boundary(text, offset);

    // Locate the line whose start is the greatest line-start <= offset.
    let line_starts = source.line_starts();
    let line_index = match line_starts.binary_search(&offset) {
        Ok(index) => index,
        // `Err(0)` only happens when `offset` precedes the first line start
        // (i.e. inside a leading BOM); clamp to the first line, column 0.
        Err(0) => 0,
        Err(index) => index - 1,
    };

    let line_start = line_starts.get(line_index).copied().unwrap_or(0);
    let line_start = clamp_to_boundary(text, line_start);
    let prefix_start = line_start.min(offset);
    let prefix = text.get(prefix_start..offset).unwrap_or("");

    let character = match encoding {
        PositionEncoding::Utf8 => prefix.len(),
        PositionEncoding::Utf16 => prefix.chars().map(char::len_utf16).sum(),
    };

    Position {
        line: line_index as u32,
        character: character as u32,
    }
}

/// Convert a croma byte [`Span`] into an LSP [`Range`] under `encoding`.
///
/// Total and order-preserving: a reversed span (`start > end`) is normalised so
/// the resulting range is well-formed. Every emitted `Range` is in-bounds for
/// `source`.
pub fn span_to_range(source: &SourceText, span: Span, encoding: PositionEncoding) -> Range {
    let (start, end) = if span.start <= span.end {
        (span.start, span.end)
    } else {
        (span.end, span.start)
    };
    Range {
        start: byte_to_position(source, start, encoding),
        end: byte_to_position(source, end, encoding),
    }
}

/// Measure a byte slice's width in the negotiated encoding's units (UTF-8 bytes
/// or UTF-16 code units).
///
/// This is the unit shared by a [`SemanticToken`](lsp_types::SemanticToken)'s
/// `length`/`delta_start` and by [`byte_to_position`]'s column: counting it the
/// same way is what keeps an emitted token's `length` consistent with the
/// `Range` its endpoints would map to. Total: a slice with no characters is 0.
pub fn measure(slice: &str, encoding: PositionEncoding) -> u32 {
    let units: usize = match encoding {
        PositionEncoding::Utf8 => slice.len(),
        PositionEncoding::Utf16 => slice.chars().map(char::len_utf16).sum(),
    };
    units as u32
}

/// The encoding-aware width of `span` within `source`, for a span that lies on a
/// single line (every ABC `MusicToken` does, by construction).
///
/// Total: an out-of-bounds or non-boundary span is clamped via [`SourceText`]'s
/// own boundary-checked [`slice`](SourceText::slice); anything unsliceable
/// measures 0. A reversed span (`start > end`) is normalised first.
pub fn span_length(source: &SourceText, span: Span, encoding: PositionEncoding) -> u32 {
    let text = source.as_str();
    let (start, end) = if span.start <= span.end {
        (span.start, span.end)
    } else {
        (span.end, span.start)
    };
    let start = clamp_to_boundary(text, start);
    let end = clamp_to_boundary(text, end);
    let slice = text.get(start..end).unwrap_or("");
    measure(slice, encoding)
}

/// Convert an LSP [`Position`] back to a UTF-8 byte offset under `encoding`.
///
/// Total: a line past EOF clamps to the document length; a character past the
/// end of its line clamps to the line's end (including its newline run, so an
/// end-exclusive edit range can address the line break). Used by the document
/// store to apply incremental [`Range`] edits against the current text.
pub fn position_to_byte(
    source: &SourceText,
    position: Position,
    encoding: PositionEncoding,
) -> usize {
    let text = source.as_str();
    let line_index = position.line as usize;

    let Some(line) = source.line(line_index) else {
        // Line past EOF -> clamp to end of document.
        return text.len();
    };

    // The editable extent of a line includes its terminator, so that a range end
    // of `(line, very_large)` can land on the line break (LSP convention).
    let line_start = line.start();
    let line_limit = line.end();
    let want = position.character as usize;

    let mut byte = line_start;
    let mut counted = 0usize;
    let line_slice = text.get(line_start..line_limit).unwrap_or("");
    for ch in line_slice.chars() {
        if counted >= want {
            break;
        }
        let units = match encoding {
            PositionEncoding::Utf8 => ch.len_utf8(),
            PositionEncoding::Utf16 => ch.len_utf16(),
        };
        counted += units;
        byte += ch.len_utf8();
    }
    byte.min(line_limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn ascii_byte_offsets_map_identically_in_both_encodings() {
        let src = SourceText::new("X:1\nK:C\nabc\n");
        // 'b' on line 2 (0-based) is at byte 9; line 2 starts at byte 8.
        let span = Span::new(9, 10);
        for enc in [PositionEncoding::Utf8, PositionEncoding::Utf16] {
            let range = span_to_range(&src, span, enc);
            assert_eq!(range.start, pos(2, 1), "{enc:?}");
            assert_eq!(range.end, pos(2, 2), "{enc:?}");
        }
    }

    #[test]
    fn multibyte_char_differs_between_utf8_and_utf16() {
        // "Café" — 'é' (U+00E9) is 2 UTF-8 bytes, 1 UTF-16 unit.
        let src = SourceText::new("T:Café\n");
        // Field text: T : C a f é  -> bytes 0 1 2 3 4 5..7
        // The 'é' occupies bytes [5, 7); a span just past it is byte 7.
        let after_e = Span::new(7, 7);

        let utf8 = byte_to_position(&src, after_e.start, PositionEncoding::Utf8);
        // UTF-8 character = byte delta = 7 - 0 = 7.
        assert_eq!(utf8, pos(0, 7));

        let utf16 = byte_to_position(&src, after_e.start, PositionEncoding::Utf16);
        // UTF-16: T,:,C,a,f each 1 unit (5) + é 1 unit = 6.
        assert_eq!(utf16, pos(0, 6));
    }

    #[test]
    fn combining_char_counts_as_its_own_scalar() {
        // "e" + U+0301 combining acute. The combining mark is 2 UTF-8 bytes,
        // 1 UTF-16 unit, and a distinct Unicode scalar.
        let src = SourceText::new("we\u{301}\n");
        // bytes: w(0) e(1) ́(2..4) \n(4)
        let after_combining = 4usize;
        assert_eq!(
            byte_to_position(&src, after_combining, PositionEncoding::Utf8),
            pos(0, 4)
        );
        assert_eq!(
            byte_to_position(&src, after_combining, PositionEncoding::Utf16),
            // w(1) e(1) combining(1) = 3 UTF-16 units.
            pos(0, 3)
        );
    }

    #[test]
    fn bom_prefixed_file_anchors_first_line_at_content_start() {
        // A leading BOM is content_start = 3; the first line still starts there.
        let src = SourceText::new("\u{feff}X:1\nK:C\n");
        // 'X' is the first content byte at offset 3.
        let x = byte_to_position(&src, 3, PositionEncoding::Utf8);
        assert_eq!(x, pos(0, 0));
        // An offset *inside* the BOM (0) clamps to the first line, column 0.
        let in_bom = byte_to_position(&src, 0, PositionEncoding::Utf8);
        assert_eq!(in_bom, pos(0, 0));
    }

    #[test]
    fn offset_at_end_of_file_is_in_bounds() {
        let src = SourceText::new("X:1\nK:C");
        let len = src.as_str().len();
        let at_eof = byte_to_position(&src, len, PositionEncoding::Utf8);
        // Last line "K:C" is line 1 (0-based), 3 chars wide.
        assert_eq!(at_eof, pos(1, 3));
        // Past EOF clamps to the same place — never panics.
        let past_eof = byte_to_position(&src, len + 100, PositionEncoding::Utf8);
        assert_eq!(past_eof, pos(1, 3));
    }

    #[test]
    fn offset_on_non_char_boundary_clamps_down() {
        let src = SourceText::new("é\n");
        // byte 1 is mid-'é'; clamp to boundary 0 -> column 0.
        let mid = byte_to_position(&src, 1, PositionEncoding::Utf8);
        assert_eq!(mid, pos(0, 0));
    }

    #[test]
    fn reversed_span_is_normalised() {
        let src = SourceText::new("abcdef\n");
        let range = span_to_range(&src, Span::new(4, 1), PositionEncoding::Utf8);
        assert_eq!(range.start, pos(0, 1));
        assert_eq!(range.end, pos(0, 4));
    }

    #[test]
    fn empty_document_maps_origin() {
        let src = SourceText::new("");
        let range = span_to_range(&src, Span::new(0, 0), PositionEncoding::Utf8);
        assert_eq!(range.start, pos(0, 0));
        assert_eq!(range.end, pos(0, 0));
    }

    #[test]
    fn position_round_trips_to_byte_ascii() {
        let src = SourceText::new("X:1\nK:C\nabc\n");
        for (byte, _) in src.as_str().char_indices() {
            let p = byte_to_position(&src, byte, PositionEncoding::Utf8);
            let back = position_to_byte(&src, p, PositionEncoding::Utf8);
            assert_eq!(back, byte, "round trip at byte {byte}");
        }
    }

    #[test]
    fn position_round_trips_to_byte_utf16_multibyte() {
        let src = SourceText::new("T:Café\nK:C\n");
        // Start-of-each-char positions must round-trip under UTF-16.
        for (byte, _) in src.as_str().char_indices() {
            let p = byte_to_position(&src, byte, PositionEncoding::Utf16);
            let back = position_to_byte(&src, p, PositionEncoding::Utf16);
            assert_eq!(back, byte, "utf16 round trip at byte {byte}");
        }
    }

    #[test]
    fn position_to_byte_clamps_line_past_eof() {
        let src = SourceText::new("X:1\nK:C\n");
        let p = pos(999, 0);
        assert_eq!(
            position_to_byte(&src, p, PositionEncoding::Utf8),
            src.as_str().len()
        );
    }

    #[test]
    fn span_length_counts_encoding_units() {
        // "T:Café" — the span over "Café" is 4 bytes for "Caf" + 2 for 'é' = 5
        // UTF-8 bytes, but 4 UTF-16 units.
        let src = SourceText::new("T:Café\n");
        let cafe = Span::new(2, 7); // "Café"
        assert_eq!(span_length(&src, cafe, PositionEncoding::Utf8), 5);
        assert_eq!(span_length(&src, cafe, PositionEncoding::Utf16), 4);
    }

    #[test]
    fn span_length_is_total_on_garbage_spans() {
        let src = SourceText::new("abc\n");
        // Past EOF clamps end to len: slice [2,4) = "c\n" -> 2.
        assert_eq!(
            span_length(&src, Span::new(2, 99), PositionEncoding::Utf8),
            2
        );
        // Reversed normalises to [1,3) = "bc" -> 2.
        assert_eq!(
            span_length(&src, Span::new(3, 1), PositionEncoding::Utf8),
            2
        );
        let multi = SourceText::new("é\n");
        // 'é' is bytes [0,2); [1,2) has start mid-char (clamps to 0) and end on a
        // boundary, so the measured slice is [0,2) = "é" -> 2 UTF-8 bytes.
        assert_eq!(
            span_length(&multi, Span::new(1, 2), PositionEncoding::Utf8),
            2
        );
        // A span entirely inside a single multi-byte char measures 0: [1,1).
        assert_eq!(
            span_length(&multi, Span::new(1, 1), PositionEncoding::Utf8),
            0
        );
    }

    #[test]
    fn measure_matches_byte_to_position_column_on_one_line() {
        // The measure of a single-line prefix from the line start must equal the
        // column byte_to_position reports at its end — the invariant tokens rely
        // on. Stay on line 0 (bytes <= 8, before the newline at byte 8).
        let src = SourceText::new("CDEFé|\n");
        // Bytes: C0 D1 E2 F3 é4..6 |6 \n7. Line 0 text is bytes [0,7).
        for end in [0usize, 1, 4, 6, 7] {
            let pos = byte_to_position(&src, end, PositionEncoding::Utf16);
            assert_eq!(pos.line, 0, "byte {end} stays on line 0");
            let prefix = src.as_str().get(0..end).unwrap_or("");
            let len = measure(prefix, PositionEncoding::Utf16);
            assert_eq!(pos.character, len, "at byte {end}");
        }
    }

    #[test]
    fn position_to_byte_clamps_character_past_line_end() {
        let src = SourceText::new("ab\ncd\n");
        // Line 0 "ab" + newline: a huge character clamps to the line's end
        // (its terminator), i.e. byte 3.
        let p = pos(0, 999);
        assert_eq!(position_to_byte(&src, p, PositionEncoding::Utf8), 3);
    }
}
