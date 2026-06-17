//! `textDocument/hover`: explain the field key or decoration under the cursor.
//!
//! Per the promotion spec (decision 4, "hover/completion are static tables"),
//! hover is pure presentation over [`crate::tables`] — the ABC 2.1 §3.1 field set
//! and the §4.14 decoration names croma recognises. It never reparses or invents
//! meaning: it locates the token under the position in the parsed document and
//! looks its doc up in the static table.
//!
//! Two things are hoverable:
//!
//! - **A header field key.** When the position lands on a header information
//!   field's key (the `K` or the `:` of a `K:` line, before the value), return
//!   the field's name + doc + §ref, with `range` set to the field's marker.
//! - **A decoration.** When the position lands on a `Decoration` music token
//!   (`!trill!`, `+staccato+`, or a single-char shorthand like `T`), return the
//!   decoration's meaning, with `range` set to the token span.
//!
//! Everything else (notes, barlines, field values, free text) yields `None`.
//!
//! Total: the position is converted to a byte offset via the R1
//! [`position_to_byte`](crate::position::position_to_byte) (clamped, never
//! panics); a position past EOF or on whitespace simply finds no token and
//! returns `None`.

use croma_core::syntax::MusicTokenKind;
use croma_core::{ParseOptions, ParsedField, SourceText, Span, parse_document};
use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::position::{PositionEncoding, position_to_byte, span_to_range};
use crate::tables::{Decoration, FieldKey, decoration, decoration_for_shorthand, field_key};

/// Compute the hover for `source` at `pos` under `encoding`.
///
/// Returns the field-key doc when `pos` is on a header field's key, the
/// decoration doc when `pos` is on a decoration token, else `None`.
pub fn hover(source: &str, pos: Position, encoding: PositionEncoding) -> Option<Hover> {
    let report = parse_document(source, ParseOptions::default());
    let document = report.value;
    let text = &document.source;
    let offset = position_to_byte(text, pos, encoding);

    // 1. A header field key (the marker region of an information field).
    if let Some((field, key)) = field_key_at(&document.fields, offset) {
        return Some(make_hover(
            key.markdown(),
            span_to_range(text, field.marker_span, encoding),
        ));
    }

    // 2. A decoration token in a music line.
    if let Some((span, decoration)) = decoration_at(&document, text, offset) {
        return Some(make_hover(
            decoration.markdown(),
            span_to_range(text, span, encoding),
        ));
    }

    None
}

/// Find the header field whose key marker contains `offset`, returning the field
/// and its static-table entry. Only the field's **key** (marker span, e.g. the
/// `K:` of a `K:C` line) is hoverable — hovering the value is not a field-doc
/// gesture.
fn field_key_at(
    fields: &croma_core::parse::field::ParsedAbcFields,
    offset: usize,
) -> Option<(&ParsedField, &'static FieldKey)> {
    for field in &fields.fields {
        // The marker span covers the field letter and its colon (`K:`); hovering
        // anywhere on it explains the field. Use a half-open containment so the
        // cursor at the colon's trailing edge still counts.
        if span_contains_inclusive(field.marker_span, offset)
            && let Some(key) = field_key(field.code)
        {
            return Some((field, key));
        }
    }
    None
}

/// Find a `Decoration` music token containing `offset` and resolve it to a
/// table entry. Handles both delimited names (`!trill!`, `+staccato+`) and the
/// single-char shorthands (`T`, `~`).
fn decoration_at(
    document: &croma_core::AbcDocument,
    text: &SourceText,
    offset: usize,
) -> Option<(Span, &'static Decoration)> {
    for tune in &document.music.tunes {
        for line in &tune.lines {
            for token in &line.tokens {
                if token.kind != MusicTokenKind::Decoration {
                    continue;
                }
                if !span_contains_inclusive(token.span, offset) {
                    continue;
                }
                if let Some(decoration) = resolve_decoration(text, token.span) {
                    return Some((token.span, decoration));
                }
            }
        }
    }
    None
}

/// Resolve the decoration a token's source text denotes. A delimited token
/// (`!name!` / `+name+`) is looked up by its inner name; a bare single-char token
/// is resolved as an ABC 2.1 §4.14 shorthand.
fn resolve_decoration(text: &SourceText, span: Span) -> Option<&'static Decoration> {
    let raw = text.slice(span)?.trim();
    // Delimited form: strip a matching `!…!` or `+…+` pair.
    if let Some(inner) = raw
        .strip_prefix('!')
        .and_then(|rest| rest.strip_suffix('!'))
        .or_else(|| {
            raw.strip_prefix('+')
                .and_then(|rest| rest.strip_suffix('+'))
        })
    {
        return decoration(inner);
    }
    // Shorthand form: a single char (e.g. `T`, `~`).
    let mut chars = raw.chars();
    let first = chars.next()?;
    if chars.next().is_none() {
        return decoration_for_shorthand(first);
    }
    None
}

/// Whether `span` contains `offset`, treating the closing edge inclusively so a
/// cursor resting at the end of the token still hovers it. A degenerate (empty)
/// span matches only its single point.
fn span_contains_inclusive(span: Span, offset: usize) -> bool {
    let (start, end) = if span.start <= span.end {
        (span.start, span.end)
    } else {
        (span.end, span.start)
    };
    offset >= start && offset <= end
}

/// Build a Markdown hover with a range.
fn make_hover(markdown: String, range: lsp_types::Range) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range: Some(range),
    }
}

#[cfg(test)]
mod tests {
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
}
