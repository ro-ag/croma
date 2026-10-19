use super::*;
use crate::test_support::{assert_span_text, span_of};

#[test]
fn maps_utf8_byte_spans_to_line_columns() {
    let source = SourceText::new("T:Cafe\u{301}\nK:C\n");
    let accent = span_of(&source, "\u{301}");

    assert_eq!(accent, Span::new(6, 8));
    assert_eq!(source.slice(accent), Some("\u{301}"));
    assert_eq!(
        source.line_column(accent.start),
        Some(LineColumn::new(1, 7))
    );
    assert_eq!(
        source.byte_offset(LineColumn::new(1, 7)),
        Some(accent.start)
    );
    assert_span_text(&source, accent, "\u{301}");
}

#[test]
fn maps_multiline_spans_to_line_column_ranges() {
    let source = SourceText::new("X:1\nT:Cafe\u{301}\nK:C");
    let span = span_of(&source, "T:Cafe\u{301}\nK");

    assert_eq!(
        source.line_column_span(span),
        Some(LineColumnSpan {
            start: LineColumn::new(2, 1),
            end: LineColumn::new(3, 2),
        })
    );
}

#[test]
fn preserves_crlf_lf_and_cr_line_endings() {
    let source = SourceText::new("A\r\nB\nC\rD");

    assert_eq!(source.line_starts(), &[0, 3, 5, 7]);
    assert_eq!(source.line_ending(0), Some(LineEnding::Crlf));
    assert_eq!(source.line_ending(1), Some(LineEnding::Lf));
    assert_eq!(source.line_ending(2), Some(LineEnding::Cr));
    assert_eq!(source.line_ending(3), None);
    assert_eq!(source.line_with_ending(0), Some("A\r\n"));
    assert_eq!(source.line_with_ending(1), Some("B\n"));
    assert_eq!(source.line_with_ending(2), Some("C\r"));
    assert_eq!(source.line_text(3), Some("D"));
    assert_eq!(source.line_column(7), Some(LineColumn::new(4, 1)));
}

#[test]
fn ignores_bom_only_at_file_start() {
    let source = SourceText::new("\u{feff}X:1\nT:\u{feff}Title");

    assert!(source.has_leading_bom());
    assert_eq!(source.content_start(), 3);
    assert_eq!(source.line_starts(), &[3, 7]);
    assert_eq!(source.line_column(3), Some(LineColumn::new(1, 1)));
    assert_eq!(source.line_column(0), None);
    assert_eq!(source.line_text(0), Some("X:1"));
    assert_eq!(source.line_text(1), Some("T:\u{feff}Title"));

    let mid_file_bom = span_of(&source, "\u{feff}Title");
    assert_eq!(
        source.line_column(mid_file_bom.start),
        Some(LineColumn::new(2, 3))
    );
    assert_span_text(&source, mid_file_bom, "\u{feff}Title");
}

#[test]
fn can_carry_optional_file_name_for_diagnostics() {
    let source = SourceText::with_file_name("K:C\nC", "fixtures/minimal.abc");

    assert_eq!(source.file_name(), Some("fixtures/minimal.abc"));
}
