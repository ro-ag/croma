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
        Err(error) => {
            let diagnostics = if error.diagnostics().is_empty() {
                vec![Diagnostic::new(
                    croma_core::Severity::Error,
                    "export_error",
                    error.to_string(),
                    croma_core::Span::default(),
                )]
            } else {
                error.diagnostics().to_vec()
            };

            DocumentAnalysis {
                diagnostics,
                can_export_musicxml: false,
            }
        }
    }
}
