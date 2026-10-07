//! Panic containment for the server loop.
//!
//! The analysis layer is meant to be total, but it runs croma-core over
//! arbitrary editor text. If a core bug ever panics, the server must report it
//! and keep serving instead of exiting and taking the editor session with it.
//! A hang cannot be contained here; croma-core bounds its own work instead.

use std::panic::{AssertUnwindSafe, catch_unwind};

use lsp_types::{DiagnosticSeverity, NumberOrString, Position, Range};

use crate::diagnostics::DIAGNOSTIC_SOURCE;

/// The code of the one diagnostic the adapter emits on its own.
pub const INTERNAL_ERROR_CODE: &str = "croma.lsp.internal_error";

/// Run `f`, returning `None` if it panics. The default panic hook has already
/// written the panic message to stderr, which is the server's log channel.
pub fn contain<T>(f: impl FnOnce() -> T) -> Option<T> {
    // AssertUnwindSafe: callers pass closures that only read the document
    // store and text, so a panic cannot leave shared state half-updated.
    catch_unwind(AssertUnwindSafe(f)).ok()
}

/// What `publishDiagnostics` carries when analysing a document panicked. It is
/// the only diagnostic not adapted from croma-core, and it marks a core bug.
pub fn internal_error_diagnostic() -> lsp_types::Diagnostic {
    lsp_types::Diagnostic {
        range: Range::new(Position::new(0, 0), Position::new(0, 0)),
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String(INTERNAL_ERROR_CODE.to_string())),
        source: Some(DIAGNOSTIC_SOURCE.to_string()),
        message: "croma hit an internal error analysing this document. The server is still \
                  running; please report the document that caused it."
            .to_string(),
        ..Default::default()
    }
}

#[cfg(test)]
#[path = "containment_tests.rs"]
mod tests;
