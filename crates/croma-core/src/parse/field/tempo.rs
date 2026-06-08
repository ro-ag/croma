//! Interpretation directive field parsing: line-break (`I:linebreak`),
//! decoration delimiter (`I:decoration`), user symbols (`U:`), and macros
//! (`m:`).

use super::misc::split_assignment;
use super::*;
use crate::diagnostic::Span;

pub(super) fn parse_line_break_mode(value: &str) -> Option<LineBreakMode> {
    let mut mode = LineBreakMode::none();
    let mut saw_token = false;
    for token in value.split_whitespace() {
        saw_token = true;
        match token.to_ascii_lowercase().as_str() {
            "<eol>" => mode.end_of_line = true,
            "<none>" => mode = LineBreakMode::none(),
            "$" => mode.dollar = true,
            "!" => mode.bang = true,
            _ => return None,
        }
    }
    saw_token.then_some(mode)
}

pub(super) fn parse_decoration_delimiter(value: &str) -> Option<DecorationDelimiter> {
    match value.trim() {
        "!" => Some(DecorationDelimiter::Bang),
        "+" => Some(DecorationDelimiter::Plus),
        _ => None,
    }
}

pub(super) fn parse_user_symbol(value: Spanned<String>) -> UserSymbol {
    let (left, right) = split_assignment(value);
    let symbol = left.value.chars().next().map(|symbol| {
        Spanned::new(
            symbol,
            Span::new(left.span.start, left.span.start + symbol.len_utf8()),
        )
    });
    UserSymbol {
        symbol,
        replacement: right,
    }
}

pub(super) fn parse_macro(value: Spanned<String>) -> MacroDefinition {
    let (left, right) = split_assignment(value);
    MacroDefinition {
        name: left,
        replacement: right,
    }
}
