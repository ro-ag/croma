//! Barline and repeat-ending spelling parsing.

use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span};
use crate::lower::abc_barline_reference;
use crate::model::BarlineKind;
use crate::parse::music::{MusicLineParser, is_barline_char, is_escaped};
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
            // A thick `]` ends the run UNLESS a thin `|` follows it: `]|`
            // (thick-thin), `]||`, `]|:` are one boundary, so keep scanning. A
            // `]` followed by anything else (`]`, a note, whitespace, `:`) closes
            // the run here. Splitting `]|` into `]` + a stray `|` would let the
            // stray bar steal the next measure's leading slot (tune_007014).
            if ch == Some(']') && self.peek_char() != Some('|') {
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
        // A `:` immediately followed by a bar glyph is the repeat-end dots of
        // that bar: `:|` (thin) and, under §4.8 liberal recognition, `:]` (the
        // `]` thick bar serving as the boundary, i.e. `:|]`). Consume the whole
        // run as one barline rather than splitting the `:` off as a stray dot.
        if matches!(self.peek_next_char(), Some('|') | Some(']')) {
            self.parse_barline(false);
            return;
        }
        // A `:` directly before a variant ending (`:[2`) is the end-of-repeat
        // dots of a bar whose `|` was dropped — §4.9's `:|2` shorthand family. It
        // closes the open ending and repeats backward; route it to a RepeatEnd
        // and let the following `[N` parse as the next ending. (Gated on a digit
        // after `[`, so a section transition `|]:[K:..]` keeps its glued-merge.)
        if self.peek_next_char() == Some('[')
            && self.text[self.index..]
                .chars()
                .nth(2)
                .is_some_and(|ch| ch.is_ascii_digit())
        {
            self.parse_bare_colon_repeat_end();
            return;
        }
        // A `:` glued onto the barline it follows (`|]:` before a new
        // section) is that bar's trailing repeat dots — extend the barline
        // instead of opening a phantom one-colon measure of its own.
        let glued_to_barline = matches!(
            self.items.last(),
            Some(MusicItem::Barline(previous)) if previous.span.end == self.line_offset + self.index
        );
        if glued_to_barline {
            let colon_start = self.index;
            self.bump_char();
            let new_end = self.line_offset + self.index;
            if let Some(MusicItem::Barline(previous)) = self.items.last_mut() {
                previous.raw.push(':');
                previous.span = Span::new(previous.span.start, new_end);
                let raw_without_dot = previous.raw.strip_prefix('.').unwrap_or(&previous.raw);
                previous.kind = barline_kind(raw_without_dot, previous.dotted);
            }
            let span = self.span(colon_start, self.index);
            self.push_token(MusicTokenKind::Barline, span);
            return;
        }
        let adjacent_before = self.text[..self.index]
            .chars()
            .next_back()
            .is_some_and(|ch| !ch.is_whitespace());
        let adjacent_after = self.peek_next_char().is_some_and(|ch| !ch.is_whitespace());
        // A single `:` at the START OF A LINE (nothing but whitespace before it
        // on this line) is a stripped repeat start for the following notes. This
        // shows up after section-ending `|]` lines in corpus files: treating it
        // as a liberal boundary opens a zero-event phantom measure before the
        // repeated section. A `:` that merely has whitespace *immediately*
        // before it but real content earlier on the line (`... A4 :D ...`,
        // tune_003603) is a stray mid-line repeat dot, NOT a section start — it
        // must not fabricate a forward repeat.
        let line_leading = self.text[..self.index].trim().is_empty();
        if line_leading && adjacent_after {
            self.parse_line_leading_colon_repeat_start();
            return;
        }
        // A lone `:` glued to a note group (`CDEF:GABc`) is still a bar-line
        // character run under §4.8's liberal-recognition guidance; dropping it
        // as malformed destroyed the measure boundary and cascaded
        // misalignment (71 corpus files). parse_barline classifies the bare
        // `:` as a Liberal boundary (no repeat glyph) with the
        // liberal-spelling warning. A free-floating `: ` with whitespace on
        // both sides stays a malformed stray repeat dot.
        if adjacent_before || adjacent_after {
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

    fn parse_line_leading_colon_repeat_start(&mut self) {
        self.flush_pending_attachments();
        let start = self.index;
        self.bump_char();
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Barline, span);
        // The recovery is justified (a line-leading `:` opening a section is the
        // author's `|:` with the pipe dropped) but must never be silent.
        self.diagnostics
            .push(line_leading_colon_repeat_warning(span));
        self.items.push(MusicItem::Barline(BarlineSyntax {
            span,
            kind: BarlineKind::RepeatStart,
            dotted: false,
            raw: ":".to_owned(),
        }));
    }

    fn parse_bare_colon_repeat_end(&mut self) {
        self.flush_pending_attachments();
        let start = self.index;
        self.bump_char();
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Barline, span);
        self.items.push(MusicItem::Barline(BarlineSyntax {
            span,
            kind: BarlineKind::RepeatEnd,
            dotted: false,
            raw: ":".to_owned(),
        }));
    }

    pub(super) fn parse_variant_ending(&mut self, shorthand: bool) {
        let start = self.index;
        if !shorthand {
            self.bump_char();
        }

        let mut endings = Vec::new();
        if !shorthand && self.peek_char() == Some('"') {
            if let Some(text) = self.parse_quoted_variant_ending_part() {
                endings.push(text);
            }
        } else {
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

    fn parse_quoted_variant_ending_part(&mut self) -> Option<VariantEndingPart> {
        let start = self.index;
        self.bump_char();
        let mut closed = false;
        while let Some(ch) = self.bump_char() {
            if ch == '"' && !is_escaped(self.text, self.index - ch.len_utf8()) {
                closed = true;
                break;
            }
        }
        if !closed {
            return None;
        }
        let span = self.span(start, self.index);
        let text = self
            .text
            .get(start + 1..self.index.saturating_sub(1))
            .unwrap_or("")
            .to_owned();
        Some(VariantEndingPart::Text { text, span })
    }
}

pub(in crate::parse) fn barline_kind(raw: &str, dotted: bool) -> BarlineKind {
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
        _ if raw.contains(']') && has_repeat_end(raw) && has_repeat_start(raw) => {
            BarlineKind::RepeatBoth
        }
        _ if raw.contains(']') && has_repeat_start(raw) => BarlineKind::RepeatStart,
        _ if raw.contains(']') && has_repeat_end(raw) => BarlineKind::RepeatEnd,
        _ if raw.starts_with("|]") => BarlineKind::Final,
        // Liberal runs (§4.8: "bar lines may have any shape, using a sequence
        // of |, [, ] and :") classify to their strongest component instead of
        // erasing their meaning. Reading order decides a `|:`-vs-`:|` clash
        // (`|:|` opens a repeat; `:|...|:` exact forms matched above).
        _ => {
            let forward = raw.find("|:");
            let backward = if raw.starts_with(':') && (raw.contains('|') || raw.contains(']')) {
                // Leading repeat dots over a `|` (`:|...`) or a `]` thick bar
                // (`:]` = `:|]`, §4.8 liberal) are a repeat-end boundary.
                Some(0)
            } else {
                raw.find(":|")
            };
            match (forward, backward) {
                (Some(start), Some(end)) if end < start => BarlineKind::RepeatBoth,
                (Some(_), Some(_)) | (Some(_), None) => BarlineKind::RepeatStart,
                (None, Some(_)) => BarlineKind::RepeatEnd,
                // A colon-less `]`-bearing run (`||]`, `]`) is a thin-thick
                // final bar; with stray colons (`[::]`) the shape is genuinely
                // ambiguous and stays a Liberal boundary.
                (None, None) if raw.contains(']') && !raw.contains(':') => BarlineKind::Final,
                (None, None) => BarlineKind::Liberal,
            }
        }
    }
}

fn has_repeat_start(raw: &str) -> bool {
    raw.ends_with(':') && raw.as_bytes()[..raw.len().saturating_sub(1)].contains(&b'|')
}

fn has_repeat_end(raw: &str) -> bool {
    raw.contains(":|")
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

fn line_leading_colon_repeat_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.barline.recovered_repeat",
        "A line-leading `:` was recovered as a repeat start (write `|:`)",
        span,
    )
    .with_spec_reference(abc_barline_reference())
    .with_recovery_note(RecoveryNote::new(
        "A repeat start is spelled `|:`; the bare `:` was treated as one.",
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
            BarlineKind::Dashed => "Dashed barline was preserved for MusicXML export policy",
            BarlineKind::Invisible => "Invisible barline was preserved for MusicXML export policy",
            _ => "Barline was preserved for MusicXML export policy",
        },
        span,
    )
    .with_spec_reference(abc_barline_reference())
}
