//! Decoration, shorthand, annotation, and quoted-text parsing.

use crate::diagnostic::Span;
use crate::options::ParseMode;
use crate::parse::field::DecorationDelimiter;
use crate::parse::music::{
    MusicLineParser, classify_quoted_text, is_barline_char, is_escaped, shorthand_canonical_name,
    user_symbol_canonical_name,
};
use crate::syntax::{
    DecorationKind, DecorationSyntax, MalformedSyntaxKind, MusicItem, MusicTokenKind,
    OverlaySyntax, QuotedTextKind, QuotedTextSyntax, SlurDirection, SlurSyntax, TieSyntax,
};

impl<'line> MusicLineParser<'line> {
    pub(super) fn parse_quoted_text(&mut self) {
        let start = self.index;
        self.bump_char();
        let mut closed = false;
        while let Some(ch) = self.bump_char() {
            if ch == '"' && !is_escaped(self.text, self.index - ch.len_utf8()) {
                closed = true;
                break;
            }
        }
        let span = self.span(start, self.index);
        if closed {
            let content_span = Span::new(span.start + 1, span.end.saturating_sub(1));
            let text = self
                .text
                .get(start + 1..self.index.saturating_sub(1))
                .unwrap_or("")
                .to_owned();
            let quoted = QuotedTextSyntax {
                span,
                content_span,
                kind: classify_quoted_text(&text),
                text,
            };
            self.push_token(
                match quoted.kind {
                    QuotedTextKind::ChordSymbol => MusicTokenKind::ChordSymbol,
                    QuotedTextKind::Annotation(_) => MusicTokenKind::Annotation,
                },
                span,
            );
            self.pending_attachments.push_quoted_text(quoted);
        } else {
            self.push_token(MusicTokenKind::Malformed, span);
            self.push_malformed(
                span,
                MalformedSyntaxKind::UnclosedQuotedText,
                "abc.music.unclosed_quoted_text",
                "Quoted text was preserved and skipped",
            );
        }
    }

    pub(super) fn parse_decoration(&mut self, delimiter: char) {
        let allowed = match delimiter {
            '!' => self.dialect.decoration_delimiter == DecorationDelimiter::Bang,
            '+' => {
                self.dialect.decoration_delimiter == DecorationDelimiter::Plus
                    || self.dialect.mode != ParseMode::Strict
            }
            _ => false,
        };
        if !allowed {
            self.flush_pending_attachments();
            self.parse_invalid_decoration(delimiter);
            return;
        }

        let start = self.index;
        self.bump_char();
        let name_start = self.index;
        let mut closed = false;
        while let Some(ch) = self.peek_char() {
            if ch == delimiter {
                self.bump_char();
                closed = true;
                break;
            }
            if ch.is_whitespace() || is_barline_char(ch) {
                break;
            }
            self.bump_char();
        }
        let span = self.span(start, self.index);
        if !closed {
            // An unclosed `!` (a stray, or a deprecated line-break before notes
            // such as `!f2e2f2`) must not swallow the following notes as its
            // name. Recover by keeping only the delimiter as malformed and
            // rewinding so the rest parses normally.
            self.index = name_start;
            let delimiter_span = self.span(start, name_start);
            self.push_token(MusicTokenKind::Malformed, delimiter_span);
            self.push_malformed(
                delimiter_span,
                MalformedSyntaxKind::UnclosedDecoration,
                "abc.music.unclosed_decoration",
                "Decoration delimiter was preserved and skipped",
            );
            return;
        }

        let name_end = self.index.saturating_sub(delimiter.len_utf8());
        let name_span = self.span(name_start, name_end);
        self.push_token(MusicTokenKind::Decoration, span);
        self.pending_attachments.decorations.push(DecorationSyntax {
            span,
            name_span,
            name: self.text[name_start..name_end].to_owned(),
            kind: if delimiter == '+' {
                DecorationKind::LegacyNamed
            } else {
                DecorationKind::Named
            },
        });
    }

    pub(super) fn parse_invalid_decoration(&mut self, delimiter: char) {
        let start = self.index;
        self.bump_char();
        while let Some(ch) = self.peek_char() {
            self.bump_char();
            if ch == delimiter || ch.is_whitespace() || is_barline_char(ch) {
                break;
            }
        }
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Malformed, span);
        self.push_malformed(
            span,
            MalformedSyntaxKind::InvalidDecoration,
            "abc.music.invalid_decoration",
            "Decoration delimiter is not enabled by the current ABC dialect state",
        );
    }

    pub(super) fn parse_shorthand_decoration(&mut self) {
        let start = self.index;
        let Some(symbol) = self.bump_char() else {
            return;
        };
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Decoration, span);
        let (name, kind) = if let Some(replacement) = self.user_symbol_replacement(symbol) {
            // A `U:`-defined symbol expands to its replacement so it maps through
            // the same canonical decoration path as the long-form name. If the
            // replacement is not a resolvable `!...!` decoration, fall back to
            // the raw letter (the exporter keeps the existing words behavior).
            (
                user_symbol_canonical_name(&replacement).unwrap_or_else(|| symbol.to_string()),
                DecorationKind::UserDefined,
            )
        } else {
            // Standard single-char shorthand: normalize to the canonical name so
            // all existing notation/symbol/dynamic emission logic just works.
            (
                shorthand_canonical_name(symbol).unwrap_or_else(|| symbol.to_string()),
                DecorationKind::Shorthand,
            )
        };
        self.pending_attachments.decorations.push(DecorationSyntax {
            span,
            name_span: span,
            name,
            kind,
        });
    }

    pub(super) fn parse_overlay(&mut self) {
        self.flush_pending_attachments();
        let start = self.index;
        self.bump_char();
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Overlay, span);
        self.items.push(MusicItem::Overlay(OverlaySyntax { span }));
    }

    pub(super) fn parse_tie(&mut self, dotted: bool) {
        self.flush_pending_attachments();
        let start = self.index;
        if dotted {
            self.bump_char();
        }
        self.bump_char();
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Tie, span);
        self.items.push(MusicItem::Tie(TieSyntax { span, dotted }));
    }

    pub(super) fn parse_slur(&mut self, direction: SlurDirection, dotted: bool) {
        self.flush_pending_attachments();
        let start = self.index;
        if dotted {
            self.bump_char();
        }
        self.bump_char();
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Slur, span);
        self.items.push(MusicItem::Slur(SlurSyntax {
            span,
            dotted,
            direction,
        }));
    }
}
