//! `textDocument/completion`: offer field keys and decoration names.
//!
//! Per the promotion spec (decision 4, "hover/completion are static tables"),
//! completion is pure presentation over [`crate::tables`]. It is **context-aware
//! but deliberately simple and deterministic** — robust to half-typed, mid-edit
//! buffers (the totality gate drives it through "type as you go" keystrokes), so
//! it works from the current line's text rather than a full structural parse:
//!
//! - **At the start of a line** (only whitespace before the cursor) that is a
//!   header line — or anywhere in an empty / not-yet-started document — offer the
//!   ABC 2.1 §3.1 **field keys** (`X:`, `T:`, `M:`, `L:`, `K:`, …).
//! - **After a `!` or `+`** (a decoration being typed), or otherwise **inside a
//!   music body line**, offer the §4.14 **decoration names** (`trill`,
//!   `staccato`, …).
//! - Otherwise (e.g. inside a field value, mid-word in a header) return an empty
//!   list — no noise.
//!
//! Results are de-duplicated and emitted in a **stable order** (the table order),
//! with `sort_text` set so the client preserves it.
//!
//! Total: the position is clamped to a byte offset via the R1
//! [`position_to_byte`](crate::position::position_to_byte); a position past EOF
//! resolves to the document end and is handled like any other.

use croma_core::SourceText;
use lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind, Position,
};

use crate::position::{PositionEncoding, position_to_byte};
use crate::tables::{DECORATIONS, FIELD_KEYS};

/// The completion context inferred from the cursor's line and prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    /// Offer header field keys (`X:`/`T:`/`K:`/…).
    FieldKeys,
    /// Offer decoration names (`trill`/`staccato`/…).
    Decorations,
    /// Nothing sensible to offer.
    None,
}

/// Compute the completion items for `source` at `pos` under `encoding`.
pub fn completion(source: &str, pos: Position, encoding: PositionEncoding) -> Vec<CompletionItem> {
    let text = SourceText::new(source);
    let offset = position_to_byte(&text, pos, encoding);

    match infer_context(source, &text, offset) {
        Context::FieldKeys => field_key_items(),
        Context::Decorations => decoration_items(),
        Context::None => Vec::new(),
    }
}

/// Infer what to complete from the current line's prefix `[line_start, offset)`.
fn infer_context(source: &str, text: &SourceText, offset: usize) -> Context {
    // The line containing the cursor and the prefix typed so far.
    let pos = crate::position::byte_to_position(text, offset, PositionEncoding::Utf8);
    let line_index = pos.line as usize;
    let line_start = text.line(line_index).map(|l| l.start()).unwrap_or(0);
    let prefix = source.get(line_start..offset).unwrap_or("");

    // A decoration is being typed: an unmatched `!` or `+` earlier on the line,
    // with no whitespace since (decoration names are whitespace-free). This wins
    // even at column > 0 inside a music line.
    if in_open_decoration(prefix) {
        return Context::Decorations;
    }

    let trimmed = prefix.trim_start();

    // Line start (nothing but whitespace before the cursor).
    if trimmed.is_empty() {
        // At the very start of a line, offer field keys when this could be a
        // header line: an empty document, or a line that is not clearly inside a
        // music body. We keep it permissive (header keys are always safe to
        // suggest at a line start) but avoid offering them mid-music-line.
        return if line_is_music_body(text, line_index) {
            Context::Decorations
        } else {
            Context::FieldKeys
        };
    }

    // A single field letter typed but no colon yet (e.g. "K") — still completing
    // the field key.
    if trimmed.len() == 1
        && trimmed
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
    {
        return Context::FieldKeys;
    }

    // Inside what looks like a music body line (not an information field): offer
    // decorations. An information field line "X:…" is a value context -> nothing.
    if !prefix_is_information_field(prefix) && line_is_music_body(text, line_index) {
        return Context::Decorations;
    }

    Context::None
}

/// Whether the line prefix has an open (unclosed) decoration delimiter: the last
/// `!` or `+` on the line is unmatched and nothing but decoration-name characters
/// follow it (no whitespace), so the user is mid-name.
fn in_open_decoration(prefix: &str) -> bool {
    // Find the last delimiter; a decoration name has no spaces, so any whitespace
    // after it means we are no longer inside the decoration.
    let Some(delim) = prefix.rfind(['!', '+']) else {
        return false;
    };
    let after = &prefix[delim + 1..];
    if after.chars().any(char::is_whitespace) {
        return false;
    }
    // Count the delimiter char before the cursor: an even count means the last
    // one closed a pair (not open); odd means it is open.
    let delim_char = prefix.as_bytes()[delim];
    let count = prefix.bytes().filter(|&b| b == delim_char).count();
    count % 2 == 1
}

/// A loose check that `prefix` is the start of an information field line: a
/// single letter (or `%%`-style directive) followed by a colon, i.e. the cursor
/// is in the field's value, not its key. Used to avoid offering decorations in a
/// field value.
fn prefix_is_information_field(prefix: &str) -> bool {
    is_information_field_line(prefix.trim_start())
}

/// Heuristically decide whether `line_index` is a music-body line: it is inside a
/// tune (a preceding `X:` exists) and is not itself an information field or a
/// directive/blank line. Robust to mid-edit buffers — purely textual.
fn line_is_music_body(text: &SourceText, line_index: usize) -> bool {
    // Must be after an `X:` reference line (a tune has started).
    let mut started = false;
    let mut in_header = false;
    for idx in 0..=line_index {
        let line = text.line_text(idx).unwrap_or("").trim_start();
        if is_field_line(line, 'X') {
            started = true;
            in_header = true;
            continue;
        }
        if started && is_field_line(line, 'K') {
            // The K: line ends the header; subsequent lines are body.
            in_header = false;
        }
    }
    // The cursor's own line:
    let this = text.line_text(line_index).unwrap_or("").trim_start();
    if this.starts_with('%') {
        return false; // a comment / directive line
    }
    if is_information_field_line(this) {
        return false; // an information field, not music
    }
    started && !in_header
}

/// Whether `line` (already left-trimmed) is the field `letter` line (`letter:`).
fn is_field_line(line: &str, letter: char) -> bool {
    let mut chars = line.chars();
    chars.next() == Some(letter) && chars.next() == Some(':')
}

/// Whether `line` (already left-trimmed) is any single-letter information field.
fn is_information_field_line(line: &str) -> bool {
    let mut chars = line.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic()) && matches!(chars.next(), Some(':'))
}

/// All field-key completion items, in spec order.
fn field_key_items() -> Vec<CompletionItem> {
    FIELD_KEYS
        .iter()
        .enumerate()
        .map(|(rank, field)| CompletionItem {
            label: field.insert_text(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(field.name.to_string()),
            documentation: Some(markup(field.markdown())),
            insert_text: Some(field.insert_text()),
            // Preserve table order regardless of the client's own sort.
            sort_text: Some(format!("{rank:03}")),
            ..Default::default()
        })
        .collect()
}

/// All decoration-name completion items, in table order. Names are inserted
/// **without** delimiters (the user has typically just typed the opening `!`/`+`,
/// or will wrap them); the label shows the canonical name.
fn decoration_items() -> Vec<CompletionItem> {
    DECORATIONS
        .iter()
        .enumerate()
        .map(|(rank, decoration)| {
            let detail = match decoration.shorthand {
                Some(symbol) => format!("decoration (shorthand {symbol})"),
                None => "decoration".to_string(),
            };
            CompletionItem {
                label: decoration.name.to_string(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some(detail),
                documentation: Some(markup(decoration.markdown())),
                insert_text: Some(decoration.name.to_string()),
                sort_text: Some(format!("{rank:03}")),
                ..Default::default()
            }
        })
        .collect()
}

/// Wrap a Markdown string as `CompletionItem` documentation.
fn markup(value: String) -> Documentation {
    Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value,
    })
}

#[cfg(test)]
mod tests {
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
}
