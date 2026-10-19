
use super::*;

fn pos(line: u32, character: u32) -> Position {
    Position { line, character }
}

fn labels(items: &[CompletionItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

#[test]
fn header_line_start_offers_field_keys() {
    // A blank line in a tune header (after X:, before K:) -> field keys.
    let source = "X:1\n\nK:C\nCDEF|\n";
    let items = completion(source, pos(1, 0), PositionEncoding::Utf8);
    let labels = labels(&items);
    assert!(labels.contains(&"T:"), "offers T: ; got {labels:?}");
    assert!(labels.contains(&"K:"), "offers K: ; got {labels:?}");
    assert!(labels.contains(&"M:"), "offers M:");
    assert!(
        items
            .iter()
            .all(|i| i.kind == Some(CompletionItemKind::FIELD))
    );
}

#[test]
fn empty_document_offers_field_keys() {
    let items = completion("", pos(0, 0), PositionEncoding::Utf8);
    assert!(labels(&items).contains(&"X:"), "empty doc offers X:");
}

#[test]
fn after_bang_offers_decoration_names() {
    // Cursor right after the '!' in a music line.
    let source = "X:1\nK:C\nC!\n";
    // Line 2 is "C!"; cursor at col 2 (just after '!').
    let items = completion(source, pos(2, 2), PositionEncoding::Utf8);
    let labels = labels(&items);
    assert!(labels.contains(&"trill"), "offers trill; got {labels:?}");
    assert!(labels.contains(&"staccato"), "offers staccato");
    assert!(
        items
            .iter()
            .all(|i| i.kind == Some(CompletionItemKind::ENUM_MEMBER))
    );
}

#[test]
fn after_plus_offers_decoration_names() {
    let source = "X:1\nK:C\nC+\n";
    let items = completion(source, pos(2, 2), PositionEncoding::Utf8);
    assert!(labels(&items).contains(&"accent"), "offers accent after +");
}

#[test]
fn partially_typed_decoration_still_offers_names() {
    // "!tr" — still inside an open decoration.
    let source = "X:1\nK:C\nC!tr\n";
    let items = completion(source, pos(2, 4), PositionEncoding::Utf8);
    assert!(labels(&items).contains(&"trill"));
}

#[test]
fn music_body_line_start_offers_decorations_not_fields() {
    // A fresh music line start inside the body: decorations, not field keys.
    let source = "X:1\nK:C\nCDEF|\n\n";
    // Line 3 is blank, but it is in the body (after K:). At col 0 we are at a
    // music line start -> decorations.
    let items = completion(source, pos(3, 0), PositionEncoding::Utf8);
    let labels = labels(&items);
    assert!(labels.contains(&"trill"), "body line offers decorations");
    assert!(
        !labels.contains(&"K:"),
        "body line does not offer field keys"
    );
}

#[test]
fn inside_field_value_offers_nothing() {
    // Cursor in the middle of a title value -> no completion noise.
    let source = "X:1\nT:My Tune\nK:C\nCDEF|\n";
    let items = completion(source, pos(1, 5), PositionEncoding::Utf8);
    assert!(
        items.is_empty(),
        "field value yields nothing: {:?}",
        labels(&items)
    );
}

#[test]
fn closed_decoration_pair_is_not_open() {
    // "!trill!" already closed; the cursor after it on the same line is not in
    // an open decoration, and after a closed decoration we are still in a
    // music body line, so decorations are still on offer (a new one could
    // start) — but field keys must NOT appear.
    let source = "X:1\nK:C\n!trill!C\n";
    let items = completion(source, pos(2, 8), PositionEncoding::Utf8);
    assert!(!labels(&items).contains(&"K:"), "no field keys mid-music");
}

#[test]
fn results_are_stable_and_deduped() {
    let a = completion("", pos(0, 0), PositionEncoding::Utf8);
    let b = completion("", pos(0, 0), PositionEncoding::Utf8);
    assert_eq!(labels(&a), labels(&b), "deterministic order");
    let mut seen = std::collections::HashSet::new();
    for item in &a {
        assert!(seen.insert(item.label.clone()), "dup {}", item.label);
    }
    let sorts: Vec<&str> = a.iter().filter_map(|i| i.sort_text.as_deref()).collect();
    let mut sorted = sorts.clone();
    sorted.sort_unstable();
    assert_eq!(sorts, sorted, "sort_text preserves table order");
}

#[test]
fn completion_never_panics_on_garbage() {
    for source in ["", "\n\n", "[[[\nK:\n!!!\n", "not abc é\n", "C+\n!\n"] {
        for enc in [PositionEncoding::Utf8, PositionEncoding::Utf16] {
            for line in 0..4u32 {
                for ch in 0..6u32 {
                    let _ = completion(source, pos(line, ch), enc);
                }
            }
        }
    }
}
