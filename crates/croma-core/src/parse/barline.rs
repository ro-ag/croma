//! Barline and repeat-ending spelling parsing.

use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span};
use crate::model::BarlineKind;
use crate::music::abc_barline_reference;
use crate::parse::music::{MusicLineParser, is_barline_char};
use crate::syntax::{
    BarlineSyntax, MalformedSyntaxKind, MusicItem, MusicTokenKind, VariantEndingPart,
    VariantEndingSyntax,
};

impl<'line> MusicLineParser<'line> {
    pub(super) fn parse_barline(&mut self, dotted: bool) {
        self.flush_pending_attachments();
        let start = self.index;
        if dotted {
            self.bump_char();
        }
        let scan_start = self.index;
        while self.peek_char().is_some_and(is_barline_char) {
            // `[` is a barline character (it leads `[|`, `[::]`), but a `[`
            // encountered partway through the scan opens a separate construct —
            // a chord (`|[G2C,2]`), variant ending (`|[1`) or inline field
            // (`|[M:3/8]`) — unless it continues a `[|`. Stop there so that `[`
            // is parsed on its own instead of being swallowed into the barline.
            if self.peek_char() == Some('[')
                && self.index != scan_start
                && self.peek_next_char() != Some('|')
            {
                break;
            }
            let ch = self.bump_char();
            // `]` always closes a barline spelling (`|]`, `:|]`, `[|]`, `[::]`).
            // Anything after it — e.g. the `|` in `|]|` — begins a new barline,
            // so stop here instead of swallowing it into a single Liberal run
            // and losing the section/final barline (ABC 2.1 §6).
            if ch == Some(']') {
                break;
            }
        }
        let span = self.span(start, self.index);
        let raw = self.text[start..self.index].to_owned();
        let raw_without_dot = raw.strip_prefix('.').unwrap_or(&raw);
        let kind = barline_kind(raw_without_dot, dotted);

        self.push_token(MusicTokenKind::Barline, span);
        if kind == BarlineKind::Liberal {
            self.diagnostics.push(liberal_barline_warning(span, &raw));
        } else if kind == BarlineKind::Dotted || kind == BarlineKind::Invisible {
            self.diagnostics
                .push(barline_syntax_policy_info(span, kind));
        }
        self.items.push(MusicItem::Barline(BarlineSyntax {
            span,
            kind,
            dotted,
            raw,
        }));

        if self.peek_char().is_some_and(|ch| ch.is_ascii_digit()) {
            self.parse_variant_ending(true);
        }
    }


    pub(super) fn parse_colon(&mut self) {
        if self.starts_with("::") {
            self.parse_barline(false);
            return;
        }
        if self.peek_next_char() == Some('|') {
            self.parse_barline(false);
            return;
        }
        self.flush_pending_attachments();
        self.parse_malformed_single(
            MalformedSyntaxKind::InvalidBarline,
            "abc.music.invalid_barline",
            "A repeat dot must be part of a barline spelling",
        );
    }


    pub(super) fn parse_variant_ending(&mut self, shorthand: bool) {
        let start = self.index;
        if !shorthand {
            self.bump_char();
        }

        let mut endings = Vec::new();
        while let Some(first) = self.parse_number_token() {
            if self.peek_char() == Some('-') {
                self.bump_char();
                if let Some(second) = self.parse_number_token() {
                    endings.push(VariantEndingPart::Range {
                        start: first,
                        end: second,
                        span: Span::new(first.span.start, second.span.end),
                    });
                } else {
                    endings.push(VariantEndingPart::Single(first));
                    self.diagnostics
                        .push(invalid_repeat_ending_warning(self.span(start, self.index)));
                    break;
                }
            } else {
                endings.push(VariantEndingPart::Single(first));
            }

            if self.peek_char() == Some(',') {
                self.bump_char();
                continue;
            }
            break;
        }

        if !shorthand && self.peek_char() == Some(']') {
            self.bump_char();
        }

        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::RepeatEnding, span);
        if endings.is_empty() {
            self.push_malformed(
                span,
                MalformedSyntaxKind::InvalidRepeatEnding,
                "abc.music.invalid_repeat_ending",
                "Repeat ending did not contain an ending number",
            );
        } else {
            self.items
                .push(MusicItem::VariantEnding(VariantEndingSyntax {
                    span,
                    shorthand,
                    endings,
                }));
        }
    }

}

fn barline_kind(raw: &str, dotted: bool) -> BarlineKind {
    if dotted {
        return BarlineKind::Dotted;
    }

    match raw {
        "|" => BarlineKind::Regular,
        "||" => BarlineKind::Double,
        "|]" => BarlineKind::Final,
        "[|" => BarlineKind::Initial,
        "|:" | "|::" | "||:" | "[|:" => BarlineKind::RepeatStart,
        ":|" | "::|" | ":||" | ":|]" => BarlineKind::RepeatEnd,
        "::" | ":|:" | ":||:" => BarlineKind::RepeatBoth,
        "[|]" => BarlineKind::Invisible,
        _ => BarlineKind::Liberal,
    }
}


fn invalid_repeat_ending_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.invalid_repeat_ending",
        "Repeat ending range is malformed",
        span,
    )
    .with_spec_reference(abc_barline_reference())
    .with_recovery_note(RecoveryNote::new(
        "The repeat ending syntax was preserved and skipped.",
    ))
}


fn liberal_barline_warning(span: Span, raw: &str) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.barline.liberal",
        format!("Liberal barline spelling `{raw}` was normalized as a measure boundary"),
        span,
    )
    .with_spec_reference(abc_barline_reference())
    .with_recovery_note(RecoveryNote::new(
        "The exact spelling is preserved in syntax; lowering uses a regular barline.",
    ))
}


fn barline_syntax_policy_info(span: Span, kind: BarlineKind) -> Diagnostic {
    Diagnostic::new(
        Severity::Info,
        "abc.music.barline.policy",
        match kind {
            BarlineKind::Dotted => "Dotted barline was preserved for MusicXML export policy",
            BarlineKind::Invisible => "Invisible barline was preserved for MusicXML export policy",
            _ => "Barline was preserved for MusicXML export policy",
        },
        span,
    )
    .with_spec_reference(abc_barline_reference())
}

