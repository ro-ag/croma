//! `textDocument/codeAction`: a single "fix all" action wrapping
//! [`croma_fmt::auto_fix`].
//!
//! By design the code action is a whole-document replace. `auto_fix` **formats
//! first**, so its [`Change`](croma_fmt::Change) spans refer to the *formatted*
//! source, not the client's buffer — mapping them back is fragile. Instead the
//! code action is a **single [`TextEdit`] replacing the whole document** with
//! `auto_fix(src).output`, of kind [`CodeActionKind::SOURCE_FIX_ALL`]. This is
//! exact and matches the proven core byte-for-byte (the same shape as
//! `textDocument/formatting`).
//!
//! The action is offered **only when `auto_fix` actually changed something**
//! (`!changes.is_empty()`); a clean document yields an empty list. The title
//! summarises the distinct fix kinds applied.
//!
//! Total: builds a fresh [`SourceText`], runs the panic-free core, and emits one
//! whole-document edit. The `range` parameter (the client's selection) is
//! irrelevant to a whole-document fix and is ignored.

use std::collections::BTreeSet;

use croma_core::{SourceText, Span};
use croma_fmt::{Change, FormatOptions, auto_fix};
use lsp_types::{CodeAction, CodeActionKind, Range, TextEdit, Uri, WorkspaceEdit};

use crate::position::{PositionEncoding, span_to_range};

/// The fixed title prefix for the fix-all action.
const TITLE: &str = "Fix all auto-fixable problems (croma)";

/// Compute the code actions for `source` (the `range` selection is ignored: the
/// fix is whole-document) under `encoding`, addressed to `uri`.
///
/// Returns one [`CodeAction`] of kind `source.fixAll` when `auto_fix` produced
/// changes, else an empty list.
pub fn code_actions(
    uri: &Uri,
    source: &str,
    _range: Range,
    encoding: PositionEncoding,
) -> Vec<CodeAction> {
    let fixed = auto_fix(source, FormatOptions::default());
    if fixed.changes.is_empty() {
        return Vec::new();
    }

    let text = SourceText::new(source);
    let whole = span_to_range(&text, Span::new(0, text.len()), encoding);
    let edit = TextEdit {
        range: whole,
        new_text: fixed.output,
    };

    let workspace = WorkspaceEdit {
        changes: Some([(uri.clone(), vec![edit])].into_iter().collect()),
        ..Default::default()
    };

    vec![CodeAction {
        title: title_for(&fixed.changes),
        kind: Some(CodeActionKind::SOURCE_FIX_ALL),
        diagnostics: None,
        edit: Some(workspace),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    }]
}

/// Summarise the applied fix kinds into the action title, e.g.
/// `"Fix all auto-fixable problems (croma): bare-tempo-suffix, field-spacing"`.
fn title_for(changes: &[Change]) -> String {
    // Distinct kinds, in a stable (sorted-label) order.
    let kinds: BTreeSet<&'static str> = changes.iter().map(|c| c.kind.label()).collect();
    if kinds.is_empty() {
        return TITLE.to_string();
    }
    let list = kinds.into_iter().collect::<Vec<_>>().join(", ");
    format!("{TITLE}: {list}")
}

#[cfg(test)]
#[path = "code_action_tests.rs"]
mod tests;
