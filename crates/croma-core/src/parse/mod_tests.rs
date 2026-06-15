use super::*;

#[test]
fn reports_empty_input_with_exact_span() {
    let report = parse_document("", ParseOptions::default());

    assert!(report.value.source.is_empty());
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "abc.file.empty");
    assert_eq!(report.diagnostics[0].span, Span::new(0, 0));
    assert!(report.diagnostics[0].spec_reference.is_some());
}

#[test]
fn reports_bom_only_input_as_empty_after_file_start_bom() {
    let report = parse_document("\u{feff}", ParseOptions::default());

    assert_eq!(report.value.source.content_start(), 3);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "abc.file.empty");
    assert_eq!(report.diagnostics[0].span, Span::new(3, 3));
}

#[test]
fn reports_comment_only_input_with_content_span() {
    let report = parse_document("% only\r\n  % comments\n", ParseOptions::default());

    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "abc.file.no_tune");
    assert_eq!(
        report.diagnostics[0].span,
        report.value.source.content_span()
    );
}

#[test]
fn does_not_ignore_mid_file_bom_as_empty_content() {
    let report = parse_document("% comment\n\u{feff}\n", ParseOptions::default());

    assert!(report.diagnostics.is_empty());
}

#[test]
fn multi_tune_missing_key_explains_the_cause() {
    // Two adjacent `X:` with no blank-line separator (ABC 2.1 §2.2) is malformed.
    // croma processes only the first tune (the K-less `X:1` shell) and rejects it
    // with `missing_k`; a recovery note clarifies that K: IS present on a later
    // tune, instead of the otherwise-confusing bare "missing K:" message.
    let document = parse_document("X:1\nX:2\nT:Foo\nK:C\nabc|\n", ParseOptions::default());
    let report = parse_tune_report_from_document(&document.value);
    let multi = report
        .diagnostics
        .iter()
        .find(|d| d.code == "abc.file.missing_k")
        .expect("expected missing_k for the K-less first tune");
    assert!(
        multi.recovery_note.is_some(),
        "multi-tune missing_k should carry a clarifying recovery note"
    );

    // A genuine single-tune missing K: stays a plain message (no extra note).
    let single = parse_document("X:1\nT:No Key\nabc\n", ParseOptions::default());
    let single_report = parse_tune_report_from_document(&single.value);
    let plain = single_report
        .diagnostics
        .iter()
        .find(|d| d.code == "abc.file.missing_k")
        .expect("expected missing_k for the single K-less tune");
    assert!(
        plain.recovery_note.is_none(),
        "single-tune missing_k stays plain"
    );
}

#[test]
fn reports_missing_key_at_eof() {
    let source = SourceText::new("X:1\nT:No Key\n");
    let surface = SurfaceMap::default();
    let report = parse_tune_report(&source, &surface, ParseOptions::default());

    assert!(report.value.is_none());
    assert!(report.has_errors());
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "abc.file.missing_k");
    assert_eq!(
        report.diagnostics[0].span,
        Span::new(source.len(), source.len())
    );
    assert_eq!(
        source.line_column_span(report.diagnostics[0].span),
        Some(crate::LineColumnSpan {
            start: crate::LineColumn::new(3, 1),
            end: crate::LineColumn::new(3, 1),
        })
    );
}

#[test]
fn reports_no_music_over_body_span() {
    let source = SourceText::new("X:1\nK:C\n|||\n");
    let surface = SurfaceMap::default();
    let report = parse_tune_report(&source, &surface, ParseOptions::default());

    assert!(report.value.is_none());
    assert!(report.has_errors());
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "abc.file.no_music")
        .expect("expected no-music diagnostic");
    assert_eq!(source.slice(diagnostic.span), Some("|||\n"));
}
