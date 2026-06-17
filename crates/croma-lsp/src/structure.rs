//! `textDocument/documentSymbol` and `textDocument/foldingRange`: one entry per
//! tune.
//!
//! Per the promotion spec (R2 scope), both views are derived from the parser's
//! tune extents ([`ParsedTuneMusic::span`]) plus the header fields:
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
mod tests {
    use super::*;

    #[test]
    fn two_tune_document_yields_two_symbols_named_by_title() {
        let source = "\
X:1
T:First Tune
K:C
CDEF|

X:2
T:Second Tune
K:G
GABc|
";
        let symbols = document_symbols(source, PositionEncoding::Utf8);
        assert_eq!(symbols.len(), 2, "one symbol per tune");
        assert_eq!(symbols[0].name, "First Tune");
        assert_eq!(symbols[1].name, "Second Tune");
        for sym in &symbols {
            assert_eq!(sym.kind, SymbolKind::MODULE);
            // selection_range must be inside range.
            assert!(
                (sym.range.start.line, sym.range.start.character)
                    <= (
                        sym.selection_range.start.line,
                        sym.selection_range.start.character
                    )
            );
            assert!(
                (
                    sym.selection_range.end.line,
                    sym.selection_range.end.character
                ) <= (sym.range.end.line, sym.range.end.character)
            );
            assert!(sym.children.as_ref().is_some_and(|c| !c.is_empty()));
        }
    }

    #[test]
    fn title_less_tune_falls_back_to_reference_then_positional() {
        // No T: -> "X:7".
        let with_x = "X:7\nK:C\nCDEF|\n";
        let s = document_symbols(with_x, PositionEncoding::Utf8);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].name, "X:7");
    }

    #[test]
    fn no_tunes_yields_no_symbols_or_folds() {
        for source in ["", "% just a comment\n", "\n\n"] {
            assert!(document_symbols(source, PositionEncoding::Utf8).is_empty());
            assert!(folding_ranges(source, PositionEncoding::Utf8).is_empty());
        }
    }

    #[test]
    fn folding_range_spans_a_multiline_tune() {
        let source = "X:1\nT:T\nK:C\nCDEF|\nGABc|\n";
        let folds = folding_ranges(source, PositionEncoding::Utf8);
        assert_eq!(folds.len(), 1, "one fold per tune");
        let fold = &folds[0];
        assert_eq!(fold.start_line, 0, "fold starts at the tune's first line");
        assert!(
            fold.end_line > fold.start_line,
            "fold covers multiple lines"
        );
        assert_eq!(fold.kind, Some(FoldingRangeKind::Region));
    }

    #[test]
    fn malformed_source_never_panics() {
        for source in ["[[[\nK:C\n", "X:\nT:\nK:\n)))(((\n", "not abc é\n"] {
            for enc in [PositionEncoding::Utf8, PositionEncoding::Utf16] {
                let _ = document_symbols(source, enc);
                let _ = folding_ranges(source, enc);
            }
        }
    }

    #[test]
    fn symbol_ranges_are_in_bounds() {
        let source = "X:1\nT:Café Tune\nK:C\nC2D2|\n";
        let text = SourceText::new(source);
        let line_count = text.line_count() as u32;
        for sym in document_symbols(source, PositionEncoding::Utf16) {
            assert!(sym.range.end.line < line_count.max(1));
            for child in sym.children.unwrap_or_default() {
                assert!(child.range.end.line < line_count.max(1));
            }
        }
    }
}
