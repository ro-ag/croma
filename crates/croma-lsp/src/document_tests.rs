use super::*;
use lsp_types::{Position, Range};

fn uri() -> Uri {
    <Uri as std::str::FromStr>::from_str("file:///tune.abc").expect("valid test uri")
}

fn change_at(range: Option<Range>, text: &str) -> TextDocumentContentChangeEvent {
    TextDocumentContentChangeEvent {
        range,
        range_length: None,
        text: text.to_string(),
    }
}

fn full(text: &str) -> TextDocumentContentChangeEvent {
    change_at(None, text)
}

fn ranged(sl: u32, sc: u32, el: u32, ec: u32, text: &str) -> TextDocumentContentChangeEvent {
    change_at(
        Some(Range {
            start: Position {
                line: sl,
                character: sc,
            },
            end: Position {
                line: el,
                character: ec,
            },
        }),
        text,
    )
}

#[test]
fn open_get_close_round_trip() {
    let mut store = DocumentStore::new();
    store.open(uri(), "X:1\n".to_string());
    assert_eq!(store.get(&uri()), Some("X:1\n"));
    assert_eq!(store.len(), 1);
    store.close(&uri());
    assert_eq!(store.get(&uri()), None);
    assert!(store.is_empty());
}

#[test]
fn full_replace_when_no_range() {
    let mut store = DocumentStore::new();
    store.open(uri(), "old".to_string());
    let out = store
        .change(&uri(), vec![full("brand new")], PositionEncoding::Utf8)
        .map(str::to_string);
    assert_eq!(out.as_deref(), Some("brand new"));
}

#[test]
fn ranged_insert_at_line_start() {
    let mut store = DocumentStore::new();
    store.open(uri(), "X:1\nK:C\n".to_string());
    store.change(
        &uri(),
        vec![ranged(1, 0, 1, 0, "T:Hi\n")],
        PositionEncoding::Utf8,
    );
    assert_eq!(store.get(&uri()), Some("X:1\nT:Hi\nK:C\n"));
}

#[test]
fn ranged_replace_spanning_characters() {
    let mut store = DocumentStore::new();
    store.open(uri(), "abcdef\n".to_string());
    store.change(
        &uri(),
        vec![ranged(0, 2, 0, 4, "ZZZ")],
        PositionEncoding::Utf8,
    );
    assert_eq!(store.get(&uri()), Some("abZZZef\n"));
}

#[test]
fn ranged_delete_a_middle_line() {
    let mut store = DocumentStore::new();
    store.open(uri(), "a\nb\nc\n".to_string());
    store.change(&uri(), vec![ranged(1, 0, 2, 0, "")], PositionEncoding::Utf8);
    assert_eq!(store.get(&uri()), Some("a\nc\n"));
}

#[test]
fn range_past_eof_clamps_and_appends() {
    let mut store = DocumentStore::new();
    store.open(uri(), "abc".to_string());
    // Start and end both well past EOF -> clamp to len, i.e. append.
    store.change(
        &uri(),
        vec![ranged(99, 99, 99, 99, "XYZ")],
        PositionEncoding::Utf8,
    );
    assert_eq!(store.get(&uri()), Some("abcXYZ"));
}

#[test]
fn reversed_range_is_normalised() {
    let mut store = DocumentStore::new();
    store.open(uri(), "abcdef\n".to_string());
    // End before start; should still replace the [1,4) slice "bcd".
    store.change(
        &uri(),
        vec![ranged(0, 4, 0, 1, "_")],
        PositionEncoding::Utf8,
    );
    assert_eq!(store.get(&uri()), Some("a_ef\n"));
}

#[test]
fn multibyte_aware_edit_utf16() {
    let mut store = DocumentStore::new();
    store.open(uri(), "Café\n".to_string());
    // Under UTF-16, 'é' is 1 unit; replace it (cols 3..4) with "e".
    store.change(
        &uri(),
        vec![ranged(0, 3, 0, 4, "e")],
        PositionEncoding::Utf16,
    );
    assert_eq!(store.get(&uri()), Some("Cafe\n"));
}

#[test]
fn sequence_of_edits_applies_in_order() {
    let mut store = DocumentStore::new();
    store.open(uri(), "".to_string());
    // Simulate "type as you go": full-replace then incremental appends.
    store.change(
        &uri(),
        vec![
            full("X:1\n"),
            ranged(1, 0, 1, 0, "K:C\n"),
            ranged(2, 0, 2, 0, "CDEF|\n"),
        ],
        PositionEncoding::Utf8,
    );
    assert_eq!(store.get(&uri()), Some("X:1\nK:C\nCDEF|\n"));
}

#[test]
fn change_on_unknown_uri_returns_none() {
    let mut store = DocumentStore::new();
    assert!(
        store
            .change(&uri(), vec![full("x")], PositionEncoding::Utf8)
            .is_none()
    );
}

#[test]
fn clear_to_empty_then_garbage_never_panics() {
    let mut store = DocumentStore::new();
    store.open(uri(), "X:1\nK:C\n".to_string());
    store.change(&uri(), vec![full("")], PositionEncoding::Utf8);
    assert_eq!(store.get(&uri()), Some(""));
    // Garbage range against an empty doc -> clamps to (0,0).
    store.change(
        &uri(),
        vec![ranged(5, 5, 9, 9, "[[[")],
        PositionEncoding::Utf8,
    );
    assert_eq!(store.get(&uri()), Some("[[["));
}
