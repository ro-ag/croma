//! `textDocument/documentSymbol` and `textDocument/foldingRange`: one entry per
//! tune.
//!
//! Both views are derived from the parser's tune extents
//! ([`ParsedTuneMusic::span`]) plus the header fields:
//!
//! - **Document symbols** — one [`DocumentSymbol`] per tune, named after the
//!   tune's `T:` title (falling back to `X:<n>` and then `tune <index>`), with
//!   `range` = the whole-tune span and `selection_range` = the title (or `X:`)
//!   span. The header fields of each tune appear as child symbols, so the
//!   outline mirrors the tune structure.
//! - **Folding ranges** — one [`FoldingRange`] per tune, from the tune's first
//!   to its last line, `kind = Region`.
//!
//! Note on extents: [`ParsedTuneMusic::span`] covers only the **body music**,
//! not the header (the header lives in [`ParsedTuneFields::span`]). For a useful
//! outline and fold — and so the header-field child symbols stay *contained* in
//! their parent (an LSP requirement) — we use the **whole-tune extent** (the
//! union of the tune-fields span and the music span). See [`tune_extent`].
//!
//! Both are pure and total: no tunes yields an empty result, and malformed input
//! is handled by the panic-free parser.

use croma_core::parse::field::ParsedFieldKind;
use croma_core::{
    AbcDocument, ParseOptions, ParsedField, ParsedTuneMusic, SourceText, Span, parse_document,
};
use lsp_types::{DocumentSymbol, FoldingRange, FoldingRangeKind, Range, SymbolKind};

use crate::position::{PositionEncoding, byte_to_position, span_to_range};

/// Compute the document symbols for `source` under `encoding`: a nested outline
/// with one [`DocumentSymbol`] per tune (header fields as children).
pub fn document_symbols(source: &str, encoding: PositionEncoding) -> Vec<DocumentSymbol> {
    let report = parse_document(source, ParseOptions::default());
    let document = &report.value;
    let text = &document.source;

    document
        .music
        .tunes
        .iter()
        .map(|tune| tune_symbol(document, text, tune, encoding))
        .collect()
}

/// Build the symbol for one tune.
#[allow(deprecated)] // `DocumentSymbol.deprecated` is a required (deprecated) field.
fn tune_symbol(
    document: &AbcDocument,
    text: &SourceText,
    tune: &ParsedTuneMusic,
    encoding: PositionEncoding,
) -> DocumentSymbol {
    let header_fields = header_fields(document, tune.tune_index);
    let (name, selection_span) = name_and_selection(&header_fields, tune);
    let range = span_to_range(text, tune_extent(document, tune), encoding);
    let selection_range = clamp_within(span_to_range(text, selection_span, encoding), range);

    let children: Vec<DocumentSymbol> = header_fields
        .iter()
        .map(|field| field_symbol(text, field, encoding))
        .collect();

    DocumentSymbol {
        name,
        detail: None,
        kind: SymbolKind::MODULE,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    }
}

/// A child symbol for one header field (e.g. `T:`, `K:`, `M:`).
#[allow(deprecated)]
fn field_symbol(
    text: &SourceText,
    field: &ParsedField,
    encoding: PositionEncoding,
) -> DocumentSymbol {
    let range = span_to_range(text, field.line_span, encoding);
    let name = format!("{}:", field.code);
    DocumentSymbol {
        name,
        detail: text
            .slice(field.value_span)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        kind: SymbolKind::FIELD,
        tags: None,
        deprecated: None,
        range,
        // A field's selection is its marker (e.g. the `T` of `T:`).
        selection_range: clamp_within(span_to_range(text, field.marker_span, encoding), range),
        children: None,
    }
}

/// The whole-tune byte extent: the union of the tune-fields span (header +
/// body) and the music span. `ParsedTuneMusic::span` alone is body-only, so on
/// its own it would exclude the header and leave header-field child symbols
/// outside the parent range. Falls back to the music span when there are no
/// parsed fields for the tune.
fn tune_extent(document: &AbcDocument, tune: &ParsedTuneMusic) -> Span {
    match document.fields.tune(tune.tune_index) {
        Some(fields) => Span::new(
            fields.span.start.min(tune.span.start),
            fields.span.end.max(tune.span.end),
        ),
        None => tune.span,
    }
}

/// The header fields for `tune_index`, in source order.
fn header_fields(document: &AbcDocument, tune_index: usize) -> Vec<ParsedField> {
    let Some(tune_fields) = document.fields.tune(tune_index) else {
        return Vec::new();
    };
    tune_fields
        .header_field_indices
        .iter()
        .filter_map(|index| document.fields.field(*index))
        .cloned()
        .collect()
}

/// Resolve the symbol name and its selection span: prefer the `T:` title, then
/// the `X:` reference (`X:<n>`), then a positional `tune <index>` fallback. The
/// selection span is the title/reference field span, or the tune's start.
fn name_and_selection(header_fields: &[ParsedField], tune: &ParsedTuneMusic) -> (String, Span) {
    // Title first.
    for field in header_fields {
        if let ParsedFieldKind::Title(value) = &field.kind {
            let title = value.value.trim();
            if !title.is_empty() {
                return (title.to_string(), value.span);
            }
        }
    }
    // Then the reference number as "X:<n>".
    for field in header_fields {
        if let ParsedFieldKind::Reference(value) = &field.kind {
            let reference = value.value.trim();
            if !reference.is_empty() {
                return (format!("X:{reference}"), value.span);
            }
        }
    }
    // Positional fallback (1-based for a human-friendly label).
    let label = format!("tune {}", tune.tune_index + 1);
    let start = Span::new(tune.span.start, tune.span.start);
    (label, start)
}

/// Clamp `inner` so it stays within `outer` (LSP requires `selection_range` to
/// be contained in `range`). If the inner range is degenerate or outside, fall
/// back to the start of `outer`.
fn clamp_within(inner: Range, outer: Range) -> Range {
    let in_bounds = (outer.start.line, outer.start.character)
        <= (inner.start.line, inner.start.character)
        && (inner.end.line, inner.end.character) <= (outer.end.line, outer.end.character)
        && (inner.start.line, inner.start.character) <= (inner.end.line, inner.end.character);
    if in_bounds {
        inner
    } else {
        Range {
            start: outer.start,
            end: outer.start,
        }
    }
}

/// Compute the folding ranges for `source` under `encoding`: one `Region` fold
/// per tune, spanning the tune's first to last line.
pub fn folding_ranges(source: &str, encoding: PositionEncoding) -> Vec<FoldingRange> {
    let report = parse_document(source, ParseOptions::default());
    let document = &report.value;
    let text = &document.source;

    document
        .music
        .tunes
        .iter()
        .filter_map(|tune| tune_fold(document, text, tune, encoding))
        .collect()
}

/// A fold for one tune (whole-tune extent), or `None` when it occupies a single
/// line (nothing to fold).
fn tune_fold(
    document: &AbcDocument,
    text: &SourceText,
    tune: &ParsedTuneMusic,
    encoding: PositionEncoding,
) -> Option<FoldingRange> {
    let extent = tune_extent(document, tune);
    let start_pos = byte_to_position(text, extent.start, encoding);
    // Use the last in-bounds byte of the span for the end line, so a trailing
    // newline doesn't push the fold onto the next (empty) line.
    let end_byte = extent.end.saturating_sub(1).max(extent.start);
    let end_pos = byte_to_position(text, end_byte, encoding);
    if end_pos.line <= start_pos.line {
        return None;
    }
    Some(FoldingRange {
        start_line: start_pos.line,
        start_character: None,
        end_line: end_pos.line,
        end_character: None,
        kind: Some(FoldingRangeKind::Region),
        collapsed_text: None,
    })
}

#[cfg(test)]
#[path = "structure_tests.rs"]
mod tests;
