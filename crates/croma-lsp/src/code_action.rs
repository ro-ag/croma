//! `textDocument/codeAction`: a single "fix all" action wrapping
//! [`croma_fmt::auto_fix`].
//!
//! Per the promotion spec (decision 3, "codeAction = whole-document replace"),
//! `auto_fix` **formats first**, so its [`Change`](croma_fmt::Change) spans refer
//! to the *formatted* source, not the client's buffer — mapping them back is
//! fragile. Instead the code action is a **single [`TextEdit`] replacing the
//! whole document** with `auto_fix(src).output`, of kind
//! [`CodeActionKind::SOURCE_FIX_ALL`]. This is exact and matches the proven core
//! byte-for-byte (the same shape as `textDocument/formatting`).
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
use lsp_types::{CodeAction, CodeActionKind, Range, TextEdit, Url, WorkspaceEdit};

use crate::position::{PositionEncoding, span_to_range};

/// The fixed title prefix for the fix-all action.
const TITLE: &str = "Fix all auto-fixable problems (croma)";

/// Compute the code actions for `source` (the `range` selection is ignored: the
/// fix is whole-document) under `encoding`, addressed to `uri`.
///
/// Returns one [`CodeAction`] of kind `source.fixAll` when `auto_fix` produced
/// changes, else an empty list.
pub fn code_actions(
    uri: &Url,
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
mod tests {
    use super::*;
    use lsp_types::Position;

    fn uri() -> Url {
        Url::parse("file:///tune.abc").expect("valid uri")
    }

    fn whole_range() -> Range {
        Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 0,
            },
        }
    }

    /// Pull the single edit's new text out of a fix-all action.
    fn edit_text(action: &CodeAction) -> &str {
        let edit = action.edit.as_ref().expect("action has an edit");
        let changes = edit.changes.as_ref().expect("edit has changes");
        let edits = changes.get(&uri()).expect("edits for the uri");
        assert_eq!(edits.len(), 1, "exactly one whole-document edit");
        &edits[0].new_text
    }

    #[test]
    fn auto_fixable_source_offers_source_fix_all() {
        // `Q:320s` is a bare-tempo-suffix the auto-fixer strips (ABC 2.1 §10.1).
        let source = "X:1\nQ:320s\nK:C\nCDEF|\n";
        let fixed = auto_fix(source, FormatOptions::default());
        assert!(!fixed.changes.is_empty(), "fixture must be auto-fixable");

        let actions = code_actions(&uri(), source, whole_range(), PositionEncoding::Utf8);
        assert_eq!(actions.len(), 1, "one fix-all action");
        let action = &actions[0];
        assert_eq!(action.kind, Some(CodeActionKind::SOURCE_FIX_ALL));
        assert!(action.title.starts_with(TITLE), "title: {}", action.title);
        // The edit's new text equals auto_fix(src).output exactly.
        assert_eq!(
            edit_text(action),
            fixed.output,
            "edit text must equal auto_fix output"
        );
    }

    #[test]
    fn edit_replaces_the_whole_document() {
        let source = "X:1\nQ:320s\nK:C\nCDEF|\n";
        let actions = code_actions(&uri(), source, whole_range(), PositionEncoding::Utf8);
        let edit = actions[0].edit.as_ref().expect("action has an edit");
        let edits = &edit.changes.as_ref().expect("edit has changes")[&uri()];
        assert_eq!(
            edits[0].range.start,
            Position {
                line: 0,
                character: 0
            }
        );
    }

    #[test]
    fn title_lists_the_fix_kinds() {
        let source = "X:1\nQ:320s\nK:C\nCDEF|\n";
        let actions = code_actions(&uri(), source, whole_range(), PositionEncoding::Utf8);
        assert!(
            actions[0].title.contains("bare-tempo-suffix"),
            "title names the fix: {}",
            actions[0].title
        );
    }

    #[test]
    fn already_clean_source_offers_no_action() {
        // A canonical, already-formatted tune with nothing to fix.
        let source = croma_fmt::format("X:1\nT:T\nK:C\nCDEF|\n", FormatOptions::default());
        let actions = code_actions(&uri(), &source, whole_range(), PositionEncoding::Utf8);
        assert!(actions.is_empty(), "clean source yields no actions");
    }

    #[test]
    fn applying_the_edit_reproduces_auto_fix_output() {
        // Whole-document replace must reproduce auto_fix output byte-for-byte.
        let source = "X:1\nQ:1/4=1/4=160\nK:C\nC   D|\n";
        let fixed = auto_fix(source, FormatOptions::default());
        let actions = code_actions(&uri(), source, whole_range(), PositionEncoding::Utf8);
        if actions.is_empty() {
            // No changes -> nothing to assert (defensive; this fixture should fix).
            assert!(fixed.changes.is_empty());
            return;
        }
        let edit = &actions[0]
            .edit
            .as_ref()
            .expect("action has an edit")
            .changes
            .as_ref()
            .expect("edit has changes")[&uri()][0];
        let text = SourceText::new(source);
        let start =
            crate::position::position_to_byte(&text, edit.range.start, PositionEncoding::Utf8);
        let end = crate::position::position_to_byte(&text, edit.range.end, PositionEncoding::Utf8);
        let mut applied = source.to_string();
        applied.replace_range(start..end, &edit.new_text);
        assert_eq!(applied, fixed.output);
    }

    #[test]
    fn code_actions_never_panics_on_garbage() {
        for source in ["", "\n\n", "[[[\nK:\n)))\n", "not abc é\n", "Q:320s\n"] {
            for enc in [PositionEncoding::Utf8, PositionEncoding::Utf16] {
                let _ = code_actions(&uri(), source, whole_range(), enc);
            }
        }
    }
}
