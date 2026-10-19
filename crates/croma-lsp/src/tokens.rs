//! `textDocument/semanticTokens/full`: highlight the music token stream.
//!
//! The LSP walks the parser's flat, per-line `MusicToken` stream and maps each
//! token's [`MusicTokenKind`] to an index in a fixed [`SemanticTokensLegend`].
//! `Whitespace` tokens are skipped (they carry no highlight). The result is the
//! LSP delta-encoding: tokens ordered by (line, start), each stored as a delta
//! from its predecessor.
//!
//! **Non-overlap (a protocol requirement).** The flat stream is *not* strictly
//! non-overlapping: container constructs emit BOTH a span for the whole
//! construct AND spans for their inner elements — a grace group `{fg}` yields a
//! `GraceGroup` token over `{fg}` plus `Pitch` tokens for `f` and `g`; a chord
//! `[CEG]2` yields a `Chord` token over the whole thing plus inner `Pitch`/
//! `Length` tokens. LSP forbids overlapping semantic tokens, so we keep the
//! **outermost** token (the container, which starts first / spans widest) and
//! drop any later token nested inside it. Filtering is done on the exact byte
//! spans before position conversion, so it is precise regardless of encoding.
//! The emitted set therefore *covers* every highlighted byte the parser
//! produced (leg D (i), as coverage) with no overlaps (leg D (ii)).
//!
//! The legend and the `kind -> type` map are the single source of truth; the
//! server advertises the *same* [`legend()`] in its capabilities, so the indices
//! a client decodes match what it was told.
//!
//! Header-field highlighting is intentionally deferred: semantic tokens are
//! scoped to the music body, where the rich token stream lives.
//!
//! ## `MusicTokenKind` -> `SemanticTokenType` mapping
//!
//! Standard LSP token types are reused where the musical role has a natural
//! analogue; two custom types (`abcRest`, `abcError`) name roles the standard
//! set has no good fit for. The order below **is** the legend order (the index
//! advertised to the client):
//!
//! | index | SemanticTokenType | MusicTokenKind(s) |
//! |---|---|---|
//! | 0 | `variable`  | `Pitch` |
//! | 1 | `modifier`  | `Accidental`, `OctaveMark` |
//! | 2 | `number`    | `Length`, `Tuplet`, `BrokenRhythm` |
//! | 3 | `operator`  | `Barline`, `Slur`, `Tie`, `RepeatEnding`, `Overlay` |
//! | 4 | `string`    | `ChordSymbol`, `Annotation` |
//! | 5 | `decorator` | `Decoration` |
//! | 6 | `macro`     | `GraceGroup`, `Chord` |
//! | 7 | `keyword`   | `InlineField` |
//! | 8 | `comment`   | `Comment`, `Spacer`, `ScoreLineBreak` |
//! | 9 | `abcRest` (custom)  | `Rest`, `MultiMeasureRest` |
//! | 10 | `abcError` (custom) | `Malformed`, `Unsupported` |
//!
//! `Whitespace` maps to nothing (skipped). `Chord`/`GraceGroup` use `macro`
//! because they are container constructs; rests get a dedicated type so a theme
//! can distinguish silence from pitched notes.

use croma_core::syntax::{MusicToken, MusicTokenKind};
use croma_core::{ParseOptions, SourceText, parse_document};
use lsp_types::{SemanticToken, SemanticTokenType, SemanticTokens, SemanticTokensLegend};

use crate::position::{PositionEncoding, byte_to_position, span_length};

/// A custom token type for rests (silence), distinct from pitched notes.
const ABC_REST: SemanticTokenType = SemanticTokenType::new("abcRest");
/// A custom token type for malformed / unsupported spans, so clients can surface
/// them distinctly (e.g. a squiggly theme) without inventing a diagnostic.
const ABC_ERROR: SemanticTokenType = SemanticTokenType::new("abcError");

/// The legend advertised in `ServerCapabilities.semantic_tokens_provider` and
/// used to decode every token's `token_type`. The index of each entry is the
/// number stored in [`SemanticToken::token_type`]; keep it in sync with
/// [`token_type_index`].
pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::VARIABLE,  // 0
            SemanticTokenType::MODIFIER,  // 1
            SemanticTokenType::NUMBER,    // 2
            SemanticTokenType::OPERATOR,  // 3
            SemanticTokenType::STRING,    // 4
            SemanticTokenType::DECORATOR, // 5
            SemanticTokenType::MACRO,     // 6
            SemanticTokenType::KEYWORD,   // 7
            SemanticTokenType::COMMENT,   // 8
            ABC_REST,                     // 9
            ABC_ERROR,                    // 10
        ],
        token_modifiers: Vec::new(),
    }
}

/// Map a [`MusicTokenKind`] to its legend index, or `None` for kinds that emit
/// no highlight (only `Whitespace`). Exhaustive over all 25 variants so adding a
/// kind to the core is a compile error here, not a silent gap.
fn token_type_index(kind: MusicTokenKind) -> Option<u32> {
    use MusicTokenKind::*;
    Some(match kind {
        Whitespace => return None,
        Pitch => 0,
        Accidental | OctaveMark => 1,
        Length | Tuplet | BrokenRhythm => 2,
        Barline | Slur | Tie | RepeatEnding | Overlay => 3,
        ChordSymbol | Annotation => 4,
        Decoration => 5,
        GraceGroup | Chord => 6,
        InlineField => 7,
        Comment | Spacer | ScoreLineBreak => 8,
        Rest | MultiMeasureRest => 9,
        Malformed | Unsupported => 10,
    })
}

/// One absolute (pre-delta) token: its 0-based line, its start character in the
/// negotiated encoding, its length, and its legend type index.
struct AbsToken {
    line: u32,
    start: u32,
    length: u32,
    token_type: u32,
}

/// Compute the semantic tokens for `source` under `encoding`.
///
/// Pure and total: it parses (the parser is panic-free, recovering malformed
/// input), walks every tune's per-line token stream, skips `Whitespace`, clamps
/// each token to its own line, and emits the LSP delta-encoded stream ordered by
/// (line, start). Multi-byte characters are measured in the negotiated encoding.
pub fn semantic_tokens(source: &str, encoding: PositionEncoding) -> SemanticTokens {
    let report = parse_document(source, ParseOptions::default());
    let document = report.value;
    let text = &document.source;

    let mut absolute: Vec<AbsToken> = Vec::new();
    for tune in &document.music.tunes {
        for line in &tune.lines {
            // Filter the line's tokens to a non-overlapping set on byte spans,
            // keeping the outermost token where a container encloses inner ones.
            for (span, token_type) in non_overlapping(&line.tokens) {
                if let Some(abs) = absolute_token(text, span, token_type, encoding) {
                    absolute.push(abs);
                }
            }
        }
    }

    // The filter yields tokens in source (start) order per line, lines in order,
    // so the stream is already sorted by (line, start); sort defensively anyway
    // so the delta encoding is always monotonic even if that ever changes.
    absolute.sort_by_key(|t| (t.line, t.start));

    SemanticTokens {
        result_id: None,
        data: delta_encode(&absolute),
    }
}

/// Reduce one line's tokens to a non-overlapping `(span, token_type)` sequence
/// in start order, skipping `Whitespace` and dropping any highlightable token
/// that overlaps a wider one already kept.
///
/// The flat stream overlaps in two ways: a container (`Chord`/`GraceGroup`)
/// spans its inner element tokens, and a container span can even reach back over
/// a preceding sibling decoration (`.[FA]2` — the `Chord` span begins at the
/// `.`). To produce a clean cover, we sort by `(start asc, end desc)` so the
/// **widest** token at each start wins, then greedily keep a token only when it
/// starts at or after the running covered-end. Because every dropped token is
/// contained in a kept wider one, the union of kept spans equals the union of
/// all non-whitespace token spans (leg D coverage), with no overlaps.
fn non_overlapping(tokens: &[MusicToken]) -> Vec<(croma_core::Span, u32)> {
    let mut candidates: Vec<(croma_core::Span, u32)> = tokens
        .iter()
        .filter_map(|token| {
            let token_type = token_type_index(token.kind)?;
            let span = if token.span.start <= token.span.end {
                token.span
            } else {
                croma_core::Span::new(token.span.end, token.span.start)
            };
            Some((span, token_type))
        })
        .collect();
    // Widest-first at each start so a container outranks the tokens it encloses
    // (and any sibling its span reaches over).
    candidates.sort_by(|a, b| a.0.start.cmp(&b.0.start).then(b.0.end.cmp(&a.0.end)));

    let mut out: Vec<(croma_core::Span, u32)> = Vec::new();
    let mut covered_end = 0usize;
    for (span, token_type) in candidates {
        if span.start < covered_end {
            continue; // overlaps a wider token already kept
        }
        covered_end = covered_end.max(span.end);
        out.push((span, token_type));
    }
    out
}

/// Resolve one token span to an absolute token, clamped to a single line.
///
/// A `MusicToken` never spans a line break by construction; we still clamp the
/// measured length so the token cannot extend past its start line's content
/// (defensive against any future multi-line span). A zero-length span (after
/// clamping) is dropped — there is nothing to highlight.
fn absolute_token(
    text: &SourceText,
    span: croma_core::Span,
    token_type: u32,
    encoding: PositionEncoding,
) -> Option<AbsToken> {
    let start_pos = byte_to_position(text, span.start, encoding);
    let line_index = start_pos.line as usize;

    // Clamp the span's end to the end of the start line's text so a token never
    // crosses a line boundary.
    let line_text_end = text
        .line(line_index)
        .map(|line| line.text_end())
        .unwrap_or_else(|| text.len());
    let clamped_end = span.end.min(line_text_end);
    let length = span_length(
        text,
        croma_core::Span::new(span.start, clamped_end),
        encoding,
    );
    if length == 0 {
        return None;
    }

    Some(AbsToken {
        line: start_pos.line,
        start: start_pos.character,
        length,
        token_type,
    })
}

/// Delta-encode a (line, start)-sorted absolute token list into the LSP wire
/// form: each token's line/start is stored relative to its predecessor.
fn delta_encode(absolute: &[AbsToken]) -> Vec<SemanticToken> {
    let mut data = Vec::with_capacity(absolute.len());
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;
    for token in absolute {
        let delta_line = token.line - prev_line;
        let delta_start = if delta_line == 0 {
            token.start - prev_start
        } else {
            token.start
        };
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length: token.length,
            token_type: token.token_type,
            token_modifiers_bitset: 0,
        });
        prev_line = token.line;
        prev_start = token.start;
    }
    data
}

#[cfg(test)]
#[path = "tokens_tests.rs"]
mod tests;
