//! Lyric (`w:`) and symbol (`s:`) line parsing.

use crate::diagnostic::Span;
use crate::parse::field::Spanned;
use crate::parse::music::{classify_quoted_text, is_escaped};
use crate::syntax::{
    LyricLineSyntax, LyricTokenKind, LyricTokenSyntax, QuotedTextKind, SymbolLineSyntax,
    SymbolTokenKind, SymbolTokenSyntax,
};

pub(super) fn parse_lyric_line(line_index: usize, span: Span, value: Spanned<String>) -> LyricLineSyntax {
    let tokens = parse_lyric_tokens(&value.value, value.span.start);
    LyricLineSyntax {
        line_index,
        span,
        value,
        tokens,
    }
}

fn parse_lyric_tokens(value: &str, offset: usize) -> Vec<LyricTokenSyntax> {
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut syllable_start = None;
    let mut syllable_text = String::new();

    while index < value.len() {
        let Some(ch) = value[index..].chars().next() else {
            break;
        };
        if is_lyric_separator(ch) {
            flush_lyric_syllable(
                &mut tokens,
                &mut syllable_start,
                &mut syllable_text,
                offset,
                index,
            );
            index += ch.len_utf8();
            continue;
        }

        match ch {
            '\\' => {
                let escape_start = index;
                index += ch.len_utf8();
                if let Some(next) = value[index..].chars().next() {
                    if syllable_start.is_none() {
                        syllable_start = Some(escape_start);
                    }
                    syllable_text.push(next);
                    index += next.len_utf8();
                }
            }
            '-' => {
                if syllable_start.is_some() {
                    flush_lyric_syllable(
                        &mut tokens,
                        &mut syllable_start,
                        &mut syllable_text,
                        offset,
                        index,
                    );
                    tokens.push(LyricTokenSyntax {
                        span: Span::new(offset + index, offset + index + 1),
                        text: "-".to_owned(),
                        kind: LyricTokenKind::Hyphen,
                    });
                } else {
                    // A hyphen preceded by a space or another hyphen is a
                    // "separate syllable" (ABC 2.1 section 5.1): it consumes a
                    // note with no sung text, e.g. `syll-a--ble` spans four
                    // notes with the third left blank. Emit a skip rather than a
                    // literal "-" so the held note carries no lyric text.
                    tokens.push(LyricTokenSyntax {
                        span: Span::new(offset + index, offset + index + 1),
                        text: String::new(),
                        kind: LyricTokenKind::Skip,
                    });
                }
                index += ch.len_utf8();
            }
            '_' => {
                flush_lyric_syllable(
                    &mut tokens,
                    &mut syllable_start,
                    &mut syllable_text,
                    offset,
                    index,
                );
                let start = index;
                while value[index..].starts_with('_') {
                    index += 1;
                    tokens.push(LyricTokenSyntax {
                        span: Span::new(offset + start, offset + index),
                        text: "_".to_owned(),
                        kind: LyricTokenKind::Extender,
                    });
                }
            }
            '*' => {
                flush_lyric_syllable(
                    &mut tokens,
                    &mut syllable_start,
                    &mut syllable_text,
                    offset,
                    index,
                );
                tokens.push(LyricTokenSyntax {
                    span: Span::new(offset + index, offset + index + 1),
                    text: String::new(),
                    kind: LyricTokenKind::Skip,
                });
                index += ch.len_utf8();
            }
            '~' => {
                if syllable_start.is_none() {
                    syllable_start = Some(index);
                }
                syllable_text.push(' ');
                index += ch.len_utf8();
            }
            '|' => {
                flush_lyric_syllable(
                    &mut tokens,
                    &mut syllable_start,
                    &mut syllable_text,
                    offset,
                    index,
                );
                tokens.push(LyricTokenSyntax {
                    span: Span::new(offset + index, offset + index + 1),
                    text: "|".to_owned(),
                    kind: LyricTokenKind::Bar,
                });
                index += ch.len_utf8();
            }
            _ => {
                if syllable_start.is_none() {
                    syllable_start = Some(index);
                }
                syllable_text.push(ch);
                index += ch.len_utf8();
            }
        }
    }

    flush_lyric_syllable(
        &mut tokens,
        &mut syllable_start,
        &mut syllable_text,
        offset,
        value.len(),
    );
    tokens
}

fn is_lyric_separator(ch: char) -> bool {
    matches!(ch, ' ' | '\t')
}

fn flush_lyric_syllable(
    tokens: &mut Vec<LyricTokenSyntax>,
    syllable_start: &mut Option<usize>,
    syllable_text: &mut String,
    offset: usize,
    end: usize,
) {
    let Some(start) = syllable_start.take() else {
        return;
    };
    tokens.push(LyricTokenSyntax {
        span: Span::new(offset + start, offset + end),
        text: std::mem::take(syllable_text),
        kind: LyricTokenKind::Syllable,
    });
}

pub(super) fn parse_symbol_line(line_index: usize, span: Span, value: Spanned<String>) -> SymbolLineSyntax {
    let tokens = parse_symbol_tokens(&value.value, value.span.start);
    SymbolLineSyntax {
        line_index,
        span,
        value,
        tokens,
    }
}

fn parse_symbol_tokens(value: &str, offset: usize) -> Vec<SymbolTokenSyntax> {
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < value.len() {
        let Some(ch) = value[index..].chars().next() else {
            break;
        };
        if ch.is_whitespace() {
            index += ch.len_utf8();
            continue;
        }
        match ch {
            '*' => {
                tokens.push(SymbolTokenSyntax {
                    span: Span::new(offset + index, offset + index + 1),
                    text: String::new(),
                    kind: SymbolTokenKind::Skip,
                });
                index += ch.len_utf8();
            }
            '|' => {
                tokens.push(SymbolTokenSyntax {
                    span: Span::new(offset + index, offset + index + 1),
                    text: "|".to_owned(),
                    kind: SymbolTokenKind::Bar,
                });
                index += ch.len_utf8();
            }
            '"' => {
                let start = index;
                index += 1;
                while index < value.len() {
                    let Some(ch) = value[index..].chars().next() else {
                        break;
                    };
                    index += ch.len_utf8();
                    if ch == '"' && !is_escaped(value, index - ch.len_utf8()) {
                        break;
                    }
                }
                let raw = &value[start..index];
                let text = raw
                    .strip_prefix('"')
                    .and_then(|text| text.strip_suffix('"'))
                    .unwrap_or(raw)
                    .to_owned();
                let kind = match classify_quoted_text(&text) {
                    QuotedTextKind::ChordSymbol => SymbolTokenKind::ChordSymbol,
                    QuotedTextKind::Annotation(_) => SymbolTokenKind::Annotation,
                };
                tokens.push(SymbolTokenSyntax {
                    span: Span::new(offset + start, offset + index),
                    text,
                    kind,
                });
            }
            '!' | '+' => {
                let delimiter = ch;
                let start = index;
                index += delimiter.len_utf8();
                while index < value.len() {
                    let Some(ch) = value[index..].chars().next() else {
                        break;
                    };
                    index += ch.len_utf8();
                    if ch == delimiter {
                        break;
                    }
                }
                let raw = &value[start..index];
                let text = raw
                    .strip_prefix(delimiter)
                    .and_then(|text| text.strip_suffix(delimiter))
                    .unwrap_or(raw)
                    .to_owned();
                tokens.push(SymbolTokenSyntax {
                    span: Span::new(offset + start, offset + index),
                    text,
                    kind: SymbolTokenKind::Decoration,
                });
            }
            _ => {
                let start = index;
                while index < value.len() {
                    let Some(ch) = value[index..].chars().next() else {
                        break;
                    };
                    if ch.is_whitespace() || matches!(ch, '*' | '|') {
                        break;
                    }
                    index += ch.len_utf8();
                }
                tokens.push(SymbolTokenSyntax {
                    span: Span::new(offset + start, offset + index),
                    text: value[start..index].to_owned(),
                    kind: SymbolTokenKind::Raw,
                });
            }
        }
    }
    tokens
}
