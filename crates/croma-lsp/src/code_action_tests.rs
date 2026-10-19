
use super::*;
use lsp_types::Position;

fn uri() -> Uri {
    <Uri as std::str::FromStr>::from_str("file:///tune.abc").expect("valid uri")
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
    let start = crate::position::position_to_byte(&text, edit.range.start, PositionEncoding::Utf8);
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
