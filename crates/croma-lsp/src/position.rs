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
pub(crate) fn clamp_to_boundary(text: &str, offset: usize) -> usize {
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
#[path = "position_tests.rs"]
mod tests;
