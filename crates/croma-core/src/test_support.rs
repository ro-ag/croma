use crate::{SourceText, Span};

pub(crate) fn span_of(source: &SourceText, needle: &str) -> Span {
    let start = source
        .as_str()
        .find(needle)
        .unwrap_or_else(|| panic!("expected source to contain {needle:?}"));
    Span::new(start, start + needle.len())
}

pub(crate) fn assert_span_text(source: &SourceText, span: Span, expected: &str) {
    assert_eq!(source.slice(span), Some(expected));
}
