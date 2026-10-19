use super::*;

#[test]
fn diagnostics_carry_spec_and_recovery_metadata() {
    let diagnostic = Diagnostic::new(
        Severity::Warning,
        "abc.test.recovered",
        "test recovery",
        Span::new(2, 5),
    )
    .with_spec_reference(
        SpecReference::new("ABC 2.1 section 2")
            .with_url("https://abcnotation.com/wiki/abc:standard:v2.1"),
    )
    .with_recovery_note(RecoveryNote::new("continued after the malformed token"));

    assert_eq!(diagnostic.code, "abc.test.recovered");
    assert_eq!(
        diagnostic
            .spec_reference
            .as_ref()
            .map(|spec| spec.label.as_str()),
        Some("ABC 2.1 section 2")
    );
    assert_eq!(
        diagnostic
            .recovery_note
            .as_ref()
            .map(|note| note.message.as_str()),
        Some("continued after the malformed token")
    );
}

#[test]
fn span_helpers_are_byte_based() {
    let span = Span::new(2, 6);

    assert_eq!(span.len(), 4);
    assert!(!span.is_empty());
    assert!(span.contains(2));
    assert!(span.contains(5));
    assert!(!span.contains(6));
    assert!(span.contains_span(Span::new(3, 5)));
}
