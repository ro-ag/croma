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
