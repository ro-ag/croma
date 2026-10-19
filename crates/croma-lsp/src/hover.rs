//! `textDocument/hover`: explain the field key or decoration under the cursor.
//!
//! By design hover and completion are static tables, so hover is pure
//! presentation over [`crate::tables`] — the ABC 2.1 §3.1 field set and the §4.14
//! decoration names croma recognises. It never reparses or invents
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
        // anywhere on it explains the field. Containment is inclusive of the
        // closing edge so the cursor at the colon's trailing edge still counts.
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
#[path = "hover_tests.rs"]
mod tests;
