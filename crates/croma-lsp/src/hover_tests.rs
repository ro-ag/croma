
use super::*;

fn pos(line: u32, character: u32) -> Position {
    Position { line, character }
}

/// Extract the Markdown body of a hover for assertions.
fn markup(hover: &Hover) -> &str {
    match &hover.contents {
        HoverContents::Markup(content) => &content.value,
        other => panic!("expected markup hover, got {other:?}"),
    }
}

#[test]
fn hover_on_key_field_returns_key_doc() {
    let source = "X:1\nK:C\nCDEF|\n";
    // Line 1 is "K:C"; hover on the 'K' (col 0).
    let hover = hover(source, pos(1, 0), PositionEncoding::Utf8).expect("hover on K");
    let md = markup(&hover);
    assert!(md.contains("key"), "K hover mentions key: {md}");
    assert!(md.contains("§3.1.14"), "K hover has the section ref");
    // The range covers the `K:` marker on line 1.
    let range = hover.range.expect("hover has a range");
    assert_eq!(range.start, pos(1, 0));
}

#[test]
fn hover_on_field_colon_still_returns_doc() {
    let source = "X:1\nT:My Tune\nK:C\nCDEF|\n";
    // The ':' of "T:" is at line 1, col 1 — still part of the marker.
    let hover = hover(source, pos(1, 1), PositionEncoding::Utf8).expect("hover on T:");
    assert!(markup(&hover).contains("title"));
}

#[test]
fn hover_on_field_value_returns_none() {
    let source = "X:1\nT:My Tune\nK:C\nCDEF|\n";
    // Hover on the title text ("My Tune"), not the key — no field-doc hover.
    assert!(hover(source, pos(1, 4), PositionEncoding::Utf8).is_none());
}

#[test]
fn hover_on_named_decoration_returns_meaning() {
    let source = "X:1\nK:C\n!trill!C|\n";
    // "!trill!" starts at line 2, col 0; hover inside it.
    let hover = hover(source, pos(2, 2), PositionEncoding::Utf8).expect("hover on !trill!");
    let md = markup(&hover);
    assert!(
        md.contains("Trill") || md.contains("trill"),
        "trill doc: {md}"
    );
    assert!(md.contains("§4.14"));
    let range = hover.range.expect("range");
    // Range spans "!trill!" (7 chars) on line 2.
    assert_eq!(range.start, pos(2, 0));
    assert_eq!(range.end, pos(2, 7));
}

#[test]
fn plus_delimited_decoration_is_malformed_under_strict_default() {
    // croma parses with strict ABC 2.1 by default, where `+...+` is NOT an
    // enabled decoration delimiter (only `!...!` is) — the parser yields a
    // Malformed token, so there is no decoration to hover. (The `+name+`
    // delimiter form is recognised only in non-strict dialects; see
    // `resolve_decoration_strips_plus_delimiters` for the inner-name logic.)
    let source = "X:1\nK:C\n+staccato+C|\n";
    assert!(hover(source, pos(2, 3), PositionEncoding::Utf8).is_none());
}

#[test]
fn resolve_decoration_strips_plus_delimiters() {
    // The inner-name resolver accepts both `!…!` and `+…+` delimited text, so
    // a `Decoration` token carrying a legacy `+name+` (as a non-strict dialect
    // emits) still resolves. Exercised directly via a synthetic source slice.
    let text = SourceText::new("+staccato+");
    let d = resolve_decoration(&text, Span::new(0, 10)).expect("resolves +staccato+");
    assert_eq!(d.name, "staccato");
    let bang = SourceText::new("!trill!");
    let t = resolve_decoration(&bang, Span::new(0, 7)).expect("resolves !trill!");
    assert_eq!(t.name, "trill");
}

#[test]
fn hover_on_shorthand_decoration_resolves() {
    // `T` before a note is the trill shorthand (ABC 2.1 §4.14).
    let source = "X:1\nK:C\nTC|\n";
    let hover = hover(source, pos(2, 0), PositionEncoding::Utf8).expect("hover on T shorthand");
    let md = markup(&hover);
    assert!(md.contains("rill"), "T shorthand resolves to trill: {md}");
}

#[test]
fn hover_on_note_returns_none() {
    let source = "X:1\nK:C\nCDEF|\n";
    // 'D' at line 2 col 1 — a pitch, not hoverable.
    assert!(hover(source, pos(2, 1), PositionEncoding::Utf8).is_none());
}

#[test]
fn hover_past_eof_returns_none() {
    let source = "X:1\nK:C\nCDEF|\n";
    assert!(hover(source, pos(999, 0), PositionEncoding::Utf8).is_none());
    assert!(hover(source, pos(2, 999), PositionEncoding::Utf8).is_none());
}

#[test]
fn hover_never_panics_on_garbage() {
    for source in ["", "\n\n", "[[[\nK:\n!!!\n", "not abc é\n", "+\n!+!\n"] {
        for enc in [PositionEncoding::Utf8, PositionEncoding::Utf16] {
            for line in 0..4u32 {
                for ch in 0..6u32 {
                    let _ = hover(source, pos(line, ch), enc);
                }
            }
        }
    }
}

#[test]
fn hover_handles_multibyte_lines_in_utf16() {
    // A title with a multi-byte char before the K: line; hover on K must
    // still land via the UTF-16 mapping.
    let source = "X:1\nT:Café\nK:C\nCDEF|\n";
    let hover = hover(source, pos(2, 0), PositionEncoding::Utf16).expect("hover on K");
    assert!(markup(&hover).contains("key"));
}
