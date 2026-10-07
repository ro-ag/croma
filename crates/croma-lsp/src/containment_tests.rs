//! Panic containment tests.

use super::{INTERNAL_ERROR_CODE, contain, internal_error_diagnostic};

#[test]
fn contain_passes_values_through_and_turns_panics_into_none() {
    assert_eq!(contain(|| 7), Some(7));
    let caught: Option<()> = contain(|| panic!("simulated core bug"));
    assert_eq!(caught, None);
}

#[test]
fn internal_error_diagnostic_is_an_error_at_the_document_start() {
    let diagnostic = internal_error_diagnostic();
    assert_eq!(
        diagnostic.severity,
        Some(lsp_types::DiagnosticSeverity::ERROR)
    );
    assert_eq!(
        diagnostic.code,
        Some(lsp_types::NumberOrString::String(
            INTERNAL_ERROR_CODE.to_string()
        ))
    );
    assert_eq!(diagnostic.range.start, lsp_types::Position::new(0, 0));
}
