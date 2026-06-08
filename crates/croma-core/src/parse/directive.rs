//! Stylesheet directive parsing (`%%score`/`%%staves` and preserved directives).

use crate::diagnostic::Span;
use crate::parse::field::Spanned;
use crate::parse::music::trim_spanned_string;
use crate::source::SourceText;
use crate::syntax::tune::LineContext;
use crate::syntax::{PreservedDirectiveSyntax, ScoreDirectiveSyntax};

pub(super) fn parse_score_stylesheet_directive(
    source: &SourceText,
    line: &crate::syntax::tune::ClassifiedLine,
) -> Option<(usize, ScoreDirectiveSyntax)> {
    let tune_index = match line.context {
        LineContext::TuneHeader { tune_index } | LineContext::TuneBody { tune_index } => tune_index,
        LineContext::Preamble
        | LineContext::FileHeader
        | LineContext::BetweenBlocks
        | LineContext::FreeText
        | LineContext::TypesetText
        | LineContext::TuneTerminator { .. } => return None,
    };
    let text = source.slice(line.content_span)?;
    let rest = text.strip_prefix("%%")?;
    let name_start = line.content_span.start + 2;
    let trimmed_rest = rest.trim_start();
    let leading = rest.len() - trimmed_rest.len();
    let name_start = name_start + leading;
    let name_end_offset = trimmed_rest
        .find(char::is_whitespace)
        .unwrap_or(trimmed_rest.len());
    let name = &trimmed_rest[..name_end_offset];
    // `%%score` and `%%staves` share the same voice-grouping syntax.
    if !name.eq_ignore_ascii_case("score") && !name.eq_ignore_ascii_case("staves") {
        return None;
    }
    let name_span = Span::new(name_start, name_start + name.len());
    let value_start = name_span.end;
    let value_text = &text[value_start.saturating_sub(line.content_span.start)..];
    let value = trim_spanned_string(value_text, value_start);
    let directive = crate::parse::field::parse_score_directive(value.clone());
    Some((
        tune_index,
        ScoreDirectiveSyntax {
            line_index: line.index,
            span: line.content_span,
            marker_span: line
                .marker_span
                .unwrap_or_else(|| Span::new(line.content_span.start, line.content_span.start + 2)),
            name_span,
            value,
            directive,
        },
    ))
}

pub(super) fn parse_preserved_stylesheet_directive(
    source: &SourceText,
    line: &crate::syntax::tune::ClassifiedLine,
) -> Option<(usize, PreservedDirectiveSyntax)> {
    let tune_index = match line.context {
        LineContext::TuneHeader { tune_index } | LineContext::TuneBody { tune_index } => tune_index,
        LineContext::Preamble
        | LineContext::FileHeader
        | LineContext::BetweenBlocks
        | LineContext::FreeText
        | LineContext::TypesetText
        | LineContext::TuneTerminator { .. } => return None,
    };
    let text = source.slice(line.content_span)?;
    let rest = text.strip_prefix("%%")?;
    let name_start = line.content_span.start + 2;
    let trimmed_rest = rest.trim_start();
    let leading = rest.len() - trimmed_rest.len();
    let name_start = name_start + leading;
    let name_end_offset = trimmed_rest
        .find(char::is_whitespace)
        .unwrap_or(trimmed_rest.len());
    let name = &trimmed_rest[..name_end_offset];
    if name.is_empty() {
        return None;
    }
    let name_span = Span::new(name_start, name_start + name.len());
    let value_start = name_span.end;
    let value_text = &text[value_start.saturating_sub(line.content_span.start)..];
    Some((
        tune_index,
        PreservedDirectiveSyntax {
            line_index: line.index,
            span: line.content_span,
            marker_span: line
                .marker_span
                .unwrap_or_else(|| Span::new(line.content_span.start, line.content_span.start + 2)),
            name: Spanned::new(name.to_owned(), name_span),
            value: trim_spanned_string(value_text, value_start),
        },
    ))
}
