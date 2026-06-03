use croma_core::{Diagnostic, export_musicxml};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentAnalysis {
    pub diagnostics: Vec<Diagnostic>,
    pub can_export_musicxml: bool,
}

pub fn analyze_document(source: &str) -> DocumentAnalysis {
    match export_musicxml(source) {
        Ok(export) => DocumentAnalysis {
            diagnostics: export.diagnostics,
            can_export_musicxml: true,
        },
        Err(error) => DocumentAnalysis {
            diagnostics: vec![Diagnostic {
                severity: croma_core::Severity::Error,
                code: "export_error",
                message: error.to_string(),
                span: croma_core::Span::default(),
            }],
            can_export_musicxml: false,
        },
    }
}
