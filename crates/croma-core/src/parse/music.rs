//! Music-line parser: text -> surface music AST.
//!
//! The lowering half (text-AST -> model) remains in `crate::lower`.

use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
use crate::parse::field::{
    DialectState, InterpretationField, ParsedAbcFields, ParsedFieldKind,
    ScoreDirective, Spanned, parse_voice_for_music,
};
use crate::model::RestVisibility;
use crate::parse::ParseReport;
use crate::source::SourceText;
use crate::syntax::tune::{LineContext, LineKind, ScoreLineBreak, SurfaceMap};
use crate::syntax::{
    AnnotationPlacement, AttachmentBundle, InlineFieldSyntax, MalformedSyntax, MalformedSyntaxKind,
    MusicFieldLine, MusicFieldLineKind, MusicItem, MusicLine, MusicToken, MusicTokenKind, ParsedMusicDocument, ParsedTuneMusic, QuotedTextKind,
    ScoreDirectiveSyntax, SlurDirection, SpacerSyntax, UnsupportedSyntax,
    UnsupportedSyntaxKind,
};
use crate::lower::{
    abc_field_reference, music_code_span,
};
use crate::parse::directive::{
    parse_preserved_stylesheet_directive, parse_score_stylesheet_directive,
};
use crate::parse::lyric::{parse_lyric_line, parse_symbol_line};

pub(crate) fn parse_music_document(
    source: &SourceText,
    surface: &SurfaceMap,
    fields: &ParsedAbcFields,
) -> ParseReport<ParsedMusicDocument> {
    let mut diagnostics = Vec::new();
    let mut tunes = surface
        .line_map
        .tunes
        .iter()
        .map(|tune| ParsedTuneMusic {
            tune_index: tune.index,
            span: tune.body_span,
            lines: Vec::new(),
            body_fields: Vec::new(),
            lyric_lines: Vec::new(),
            symbol_lines: Vec::new(),
            score_directives: Vec::new(),
            preserved_directives: Vec::new(),
        })
        .collect::<Vec<_>>();

    for line in &surface.line_map.lines {
        let LineContext::TuneBody { tune_index } = line.context else {
            if matches!(line.context, LineContext::TuneHeader { .. })
                && line.kind == LineKind::InformationField
                && let Some(field_line) = music_field_for_line(fields, line)
                && let Some(tune_index) = tune_index_for_line_context(line.context)
                && let Some(tune) = tunes.iter_mut().find(|tune| tune.tune_index == tune_index)
            {
                match &field_line.kind {
                    MusicFieldLineKind::Score(score) => {
                        tune.score_directives
                            .push(score_directive_syntax_from_field(&field_line, score));
                    }
                    MusicFieldLineKind::Meter(_)
                    | MusicFieldLineKind::UnitNoteLength(_)
                    | MusicFieldLineKind::Key(_)
                    | MusicFieldLineKind::Unknown(_)
                    | MusicFieldLineKind::Other => {}
                    MusicFieldLineKind::PostTuneText(_) => tune.body_fields.push(field_line),
                    MusicFieldLineKind::Voice(_)
                    | MusicFieldLineKind::Lyric(_)
                    | MusicFieldLineKind::Symbol(_) => {}
                }
            }
            if matches!(
                line.context,
                LineContext::TuneHeader { .. } | LineContext::TuneBody { .. }
            ) && line.kind == LineKind::StylesheetDirective
            {
                if let Some((tune_index, directive)) =
                    parse_score_stylesheet_directive(source, line)
                    && let Some(tune) = tunes.iter_mut().find(|tune| tune.tune_index == tune_index)
                {
                    tune.score_directives.push(directive);
                } else if let Some((tune_index, directive)) =
                    parse_preserved_stylesheet_directive(source, line)
                    && let Some(tune) = tunes.iter_mut().find(|tune| tune.tune_index == tune_index)
                {
                    diagnostics.push(unsupported_directive_warning(directive.name.span));
                    tune.preserved_directives.push(directive);
                }
            }
            continue;
        };

        let Some(tune) = tunes.iter_mut().find(|tune| tune.tune_index == tune_index) else {
            continue;
        };

        if line.kind == LineKind::InformationField {
            if let Some(field_line) = music_field_for_line(fields, line) {
                let same_line_voice_music = same_line_voice_music(fields, line);
                match &field_line.kind {
                    MusicFieldLineKind::Lyric(lyric) => {
                        tune.lyric_lines.push(lyric.clone());
                    }
                    MusicFieldLineKind::Symbol(symbol) => {
                        tune.symbol_lines.push(symbol.clone());
                    }
                    MusicFieldLineKind::Score(score) => {
                        tune.score_directives
                            .push(score_directive_syntax_from_field(&field_line, score));
                    }
                    MusicFieldLineKind::Meter(_)
                    | MusicFieldLineKind::UnitNoteLength(_)
                    | MusicFieldLineKind::Key(_)
                    | MusicFieldLineKind::Unknown(_)
                    | MusicFieldLineKind::Voice(_)
                    | MusicFieldLineKind::PostTuneText(_)
                    | MusicFieldLineKind::Other => {}
                }
                if let Some((voice_field, code_span)) = same_line_voice_music {
                    tune.body_fields.push(voice_field);
                    if let Some(parsed_line) =
                        parse_music_code_line(source, fields, tune_index, line, code_span)
                    {
                        diagnostics.extend(parsed_line.diagnostics);
                        tune.lines.push(parsed_line.line);
                    }
                } else {
                    tune.body_fields.push(field_line);
                }
            }
            continue;
        }

        if line.kind == LineKind::StylesheetDirective {
            if let Some((_, directive)) = parse_score_stylesheet_directive(source, line) {
                tune.score_directives.push(directive);
            } else if let Some((_, directive)) = parse_preserved_stylesheet_directive(source, line)
            {
                diagnostics.push(unsupported_directive_warning(directive.name.span));
                tune.preserved_directives.push(directive);
            }
            continue;
        }

        if line.kind != LineKind::MusicCode {
            continue;
        }
        let code_span = music_code_span(line);
        if let Some(parsed_line) =
            parse_music_code_line(source, fields, tune_index, line, code_span)
        {
            diagnostics.extend(parsed_line.diagnostics);
            tune.lines.push(parsed_line.line);
        }
    }

    ParseReport::new(ParsedMusicDocument { tunes }, diagnostics)
}

struct ParsedMusicLineWithDiagnostics {
    line: MusicLine,
    diagnostics: Vec<Diagnostic>,
}

fn parse_music_code_line(
    source: &SourceText,
    fields: &ParsedAbcFields,
    tune_index: usize,
    line: &crate::syntax::tune::ClassifiedLine,
    code_span: Span,
) -> Option<ParsedMusicLineWithDiagnostics> {
    let line_text = source.slice(line.text_span)?;
    let code_text = source.slice(code_span)?;
    let dialect = fields
        .tune(tune_index)
        .map(|tune| tune.current.dialect.clone())
        .unwrap_or_else(|| DialectState::from_options(Default::default()));
    let mut parser = MusicLineParser::new(code_text, code_span.start, dialect);
    let mut parsed_line = parser.parse(line.index, line.span, code_span);
    let diagnostics = parser.diagnostics;

    if let ScoreLineBreak::Suppressed { marker_span } = line.score_line_break {
        parsed_line.tokens.push(MusicToken {
            kind: MusicTokenKind::ScoreLineBreak,
            span: marker_span,
        });
    }
    if let Some(comment_span) = line.trailing_comment {
        parsed_line.tokens.push(MusicToken {
            kind: MusicTokenKind::Comment,
            span: comment_span,
        });
    } else if code_span.end < line.text_span.end
        && line_text[code_span.end - line.text_span.start..]
            .trim_start()
            .starts_with('%')
    {
        parsed_line.tokens.push(MusicToken {
            kind: MusicTokenKind::Comment,
            span: Span::new(code_span.end, line.text_span.end),
        });
    }

    parsed_line.tokens.sort_by_key(|token| token.span.start);
    Some(ParsedMusicLineWithDiagnostics {
        line: parsed_line,
        diagnostics,
    })
}

fn same_line_voice_music(
    fields: &ParsedAbcFields,
    line: &crate::syntax::tune::ClassifiedLine,
) -> Option<(MusicFieldLine, Span)> {
    let field = fields
        .fields
        .iter()
        .find(|field| field.line_index == line.index)?;
    let ParsedFieldKind::Voice(voice) = &field.kind else {
        return None;
    };
    let music = voice.value.properties.clone();
    if !looks_like_same_line_music(&music.value) {
        return None;
    }

    let voice_value = Spanned::new(voice.value.id.value.clone(), voice.value.id.span);
    let voice = Spanned::new(parse_voice_for_music(voice_value.clone()), voice_value.span);
    Some((
        MusicFieldLine {
            line_index: field.line_index,
            code: field.code,
            line_span: field.line_span,
            marker_span: field.marker_span,
            value: voice_value,
            kind: MusicFieldLineKind::Voice(voice),
        },
        music.span,
    ))
}

fn looks_like_same_line_music(value: &str) -> bool {
    let trimmed = value.trim_start();
    if trimmed.is_empty() {
        return false;
    }
    let first_token = trimmed
        .split(char::is_whitespace)
        .next()
        .unwrap_or_default()
        .trim();
    let first_token_lower = first_token.to_ascii_lowercase();
    if first_token.contains('=')
        || matches!(
            first_token_lower.as_str(),
            "name"
                | "nm"
                | "subname"
                | "snm"
                | "clef"
                | "stem"
                | "octave"
                | "transpose"
                | "merge"
                | "up"
                | "down"
        )
    {
        return false;
    }
    if first_token.chars().all(|ch| ch.is_ascii_alphabetic())
        && !first_token
            .chars()
            .all(|ch| matches!(ch, 'A'..='G' | 'a'..='g' | 'x' | 'X' | 'z' | 'Z' | 'y'))
    {
        return false;
    }
    let Some(ch) = trimmed.chars().next() else {
        return false;
    };
    matches!(
        ch,
        'A'..='G'
            | 'a'..='g'
            | 'z'
            | 'Z'
            | 'x'
            | 'X'
            | 'y'
            | '^'
            | '_'
            | '='
            | '['
            | '|'
            | ']'
            | ':'
            | '"'
            | '{'
            | '('
            | '.'
            | '!'
            | '+'
            | '~'
            | 'H'
            | 'L'
            | 'M'
            | 'O'
            | 'P'
            | 'S'
            | 'T'
            | 'u'
            | 'v'
            | '<'
            | '>'
            | '&'
            | '-'
    )
}

fn music_field_for_line(
    fields: &ParsedAbcFields,
    line: &crate::syntax::tune::ClassifiedLine,
) -> Option<MusicFieldLine> {
    let field = fields
        .fields
        .iter()
        .find(|field| field.line_index == line.index)?;
    let value = match &field.kind {
        ParsedFieldKind::Meter(value) => Spanned::new(value.value.raw.clone(), value.span),
        ParsedFieldKind::UnitNoteLength(value) => Spanned::new(
            format!(
                "{}/{}",
                value.value.fraction.numerator, value.value.fraction.denominator
            ),
            value.span,
        ),
        ParsedFieldKind::Key(value) => Spanned::new(value.value.raw.clone(), value.span),
        ParsedFieldKind::Voice(voice) => {
            let mut raw = voice.value.id.value.clone();
            if !voice.value.properties.value.is_empty() {
                if !raw.is_empty() {
                    raw.push(' ');
                }
                raw.push_str(&voice.value.properties.value);
            }
            Spanned::new(raw, voice.span)
        }
        ParsedFieldKind::LyricLine(value)
        | ParsedFieldKind::SymbolLine(value)
        | ParsedFieldKind::TextMetadata(crate::parse::field::TextMetadataField { value, .. }) => {
            value.clone()
        }
        ParsedFieldKind::Interpretation(InterpretationField::Score { directive }) => {
            directive.value.clone()
        }
        ParsedFieldKind::Interpretation(InterpretationField::Unknown { directive, value }) => {
            Spanned::new(
                if value.value.is_empty() {
                    directive.value.clone()
                } else {
                    format!("{} {}", directive.value, value.value)
                },
                Span::new(directive.span.start, value.span.end),
            )
        }
        ParsedFieldKind::Unknown(unknown) => unknown.value.clone(),
        _ => Spanned::new(String::new(), field.parsed_value_span),
    };
    let kind = match &field.kind {
        ParsedFieldKind::Meter(value) => MusicFieldLineKind::Meter(value.clone()),
        ParsedFieldKind::UnitNoteLength(value) => MusicFieldLineKind::UnitNoteLength(value.clone()),
        ParsedFieldKind::Key(value) => MusicFieldLineKind::Key(value.clone()),
        ParsedFieldKind::Voice(voice) => MusicFieldLineKind::Voice(voice.clone()),
        ParsedFieldKind::LyricLine(value) => {
            MusicFieldLineKind::Lyric(parse_lyric_line(line.index, field.line_span, value.clone()))
        }
        ParsedFieldKind::SymbolLine(value) => MusicFieldLineKind::Symbol(parse_symbol_line(
            line.index,
            field.line_span,
            value.clone(),
        )),
        ParsedFieldKind::TextMetadata(metadata) if metadata.code == 'W' => {
            MusicFieldLineKind::PostTuneText(metadata.value.clone())
        }
        ParsedFieldKind::Interpretation(InterpretationField::Score { directive }) => {
            MusicFieldLineKind::Score(directive.clone())
        }
        ParsedFieldKind::Interpretation(InterpretationField::Unknown { directive, value }) => {
            let span = Span::new(directive.span.start, value.span.end);
            let text = if value.value.is_empty() {
                directive.value.clone()
            } else {
                format!("{} {}", directive.value, value.value)
            };
            MusicFieldLineKind::Unknown(Spanned::new(text, span))
        }
        ParsedFieldKind::Unknown(unknown) => MusicFieldLineKind::Unknown(unknown.value.clone()),
        _ => MusicFieldLineKind::Other,
    };

    Some(MusicFieldLine {
        line_index: field.line_index,
        code: field.code,
        line_span: field.line_span,
        marker_span: field.marker_span,
        value,
        kind,
    })
}

fn score_directive_syntax_from_field(
    field_line: &MusicFieldLine,
    score: &ScoreDirective,
) -> ScoreDirectiveSyntax {
    ScoreDirectiveSyntax {
        line_index: field_line.line_index,
        span: field_line.line_span,
        marker_span: field_line.marker_span,
        name_span: Span::new(
            field_line.value.span.start,
            field_line
                .value
                .span
                .start
                .saturating_add("score".len())
                .min(field_line.value.span.end),
        ),
        value: score.value.clone(),
        directive: score.clone(),
    }
}

fn tune_index_for_line_context(context: LineContext) -> Option<usize> {
    match context {
        LineContext::TuneHeader { tune_index } | LineContext::TuneBody { tune_index } => {
            Some(tune_index)
        }
        LineContext::Preamble
        | LineContext::FileHeader
        | LineContext::BetweenBlocks
        | LineContext::FreeText
        | LineContext::TypesetText
        | LineContext::TuneTerminator { .. } => None,
    }
}

pub(super) fn trim_spanned_string(value: &str, offset: usize) -> Spanned<String> {
    let leading = value.len() - value.trim_start().len();
    let trailing = value.trim_end().len();
    if leading >= trailing {
        let end = offset + value.len();
        return Spanned::new(String::new(), Span::new(end, end));
    }
    Spanned::new(
        value[leading..trailing].to_owned(),
        Span::new(offset + leading, offset + trailing),
    )
}

pub(crate) struct MusicLineParser<'line> {
    pub(super) text: &'line str,
    pub(super) line_offset: usize,
    pub(super) index: usize,
    pub(super) dialect: DialectState,
    pub(super) pending_attachments: AttachmentBundle,
    pub(super) tokens: Vec<MusicToken>,
    pub(super) items: Vec<MusicItem>,
    pub(super) diagnostics: Vec<Diagnostic>,
}

impl<'line> MusicLineParser<'line> {
    pub(super) fn new(text: &'line str, line_offset: usize, dialect: DialectState) -> Self {
        Self {
            text,
            line_offset,
            index: 0,
            dialect,
            pending_attachments: AttachmentBundle::default(),
            tokens: Vec::new(),
            items: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub(super) fn parse(&mut self, line_index: usize, span: Span, code_span: Span) -> MusicLine {
        while self.index < self.text.len() {
            let Some(ch) = self.peek_char() else {
                break;
            };

            match ch {
                ch if ch.is_whitespace() => self.parse_whitespace(),
                '^' | '_' | '=' => self.parse_accidental_or_malformed(),
                'A'..='G' | 'a'..='g' => self.parse_note(None),
                'z' => self.parse_rest(RestVisibility::Visible),
                'x' => self.parse_rest(RestVisibility::Invisible),
                'Z' => self.parse_multi_measure_rest(RestVisibility::Visible),
                'X' => self.parse_multi_measure_rest(RestVisibility::Invisible),
                'y' => self.parse_spacer(),
                '.' => self.parse_dot(),
                '[' => self.parse_left_bracket(),
                '|' | ']' => self.parse_barline(false),
                ':' => self.parse_colon(),
                '"' => self.parse_quoted_text(),
                '{' => self.parse_grace_group(),
                '(' => self.parse_open_paren(),
                ')' => self.parse_slur(SlurDirection::End, false),
                '<' | '>' => self.parse_broken_rhythm(),
                '&' => self.parse_overlay(),
                '-' => self.parse_tie(false),
                '!' | '+' => self.parse_decoration(ch),
                '~' | 'H' | 'L' | 'M' | 'O' | 'P' | 'S' | 'T' | 'u' | 'v' => {
                    self.parse_shorthand_decoration()
                }
                ch if self.is_user_symbol(ch) => self.parse_shorthand_decoration(),
                '\'' | ',' => self.parse_malformed_single(
                    MalformedSyntaxKind::StandaloneOctave,
                    "abc.music.malformed_octave",
                    "Octave marks must follow a note",
                ),
                '/' => self.parse_malformed_single(
                    MalformedSyntaxKind::StandaloneLength,
                    "abc.music.malformed_length",
                    "Length suffixes must follow a note or rest",
                ),
                ch if ch.is_ascii_digit() => self.parse_malformed_digits(),
                '#' | '*' | ';' | '?' | '@' => self.parse_unsupported_single(
                    UnsupportedSyntaxKind::Reserved,
                    "abc.music.reserved",
                    "Reserved music character was preserved and skipped",
                ),
                _ => self.parse_malformed_single(
                    MalformedSyntaxKind::UnknownToken,
                    "abc.music.unknown_token",
                    "Unknown music token was preserved and skipped",
                ),
            }
        }

        self.flush_pending_attachments();

        MusicLine {
            line_index,
            span,
            code_span,
            tokens: std::mem::take(&mut self.tokens),
            items: std::mem::take(&mut self.items),
        }
    }

    pub(super) fn parse_whitespace(&mut self) {
        let start = self.index;
        while self.peek_char().is_some_and(char::is_whitespace) {
            self.bump_char();
        }
        self.push_token(MusicTokenKind::Whitespace, self.span(start, self.index));
    }

    pub(super) fn parse_accidental_or_malformed(&mut self) {
        let Some(accidental) = self.parse_accidental_token() else {
            return;
        };
        if self.peek_char().is_some_and(is_note_letter) {
            self.parse_note(Some(accidental));
            return;
        }

        self.flush_pending_attachments();
        self.push_malformed(
            accidental.span,
            MalformedSyntaxKind::DanglingAccidental,
            "abc.music.malformed_accidental",
            "Accidentals must appear immediately before a note",
        );
    }

    pub(super) fn parse_spacer(&mut self) {
        self.flush_pending_attachments();
        let start = self.index;
        self.bump_char();
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Spacer, span);
        self.items.push(MusicItem::Spacer(SpacerSyntax { span }));
    }

    pub(super) fn parse_dot(&mut self) {
        if self.peek_next_char() == Some('-') {
            self.parse_tie(true);
            return;
        }
        if self.peek_next_char() == Some('(') {
            self.parse_slur(SlurDirection::Start, true);
            return;
        }
        if self.peek_next_char() == Some(')') {
            self.parse_slur(SlurDirection::End, true);
            return;
        }
        // A dotted barline is `.|`, `.:|`, etc. `[` and `]` are barline
        // characters too, but `.[...]` is a staccato chord (and `.]` is
        // meaningless), so only `|`/`:` start a dotted barline here.
        if matches!(self.peek_next_char(), Some('|' | ':')) {
            self.flush_pending_attachments();
            self.parse_barline(true);
            return;
        }
        self.parse_shorthand_decoration();
    }

    pub(super) fn parse_left_bracket(&mut self) {
        if self.is_inline_field_start() {
            self.flush_pending_attachments();
            self.parse_inline_field();
            return;
        }

        if self.starts_with("[|]") || self.peek_next_char().is_some_and(is_barline_char) {
            self.flush_pending_attachments();
            self.parse_barline(false);
            return;
        }

        if self.peek_next_char().is_some_and(|ch| ch.is_ascii_digit()) {
            self.flush_pending_attachments();
            self.parse_variant_ending(false);
            return;
        }

        self.parse_chord();
    }

    pub(super) fn parse_inline_field(&mut self) {
        let start = self.index;
        self.bump_char();
        let marker_start = self.index;
        let code = self.bump_char().unwrap_or(' ');
        self.bump_char();
        let marker_span = self.span(marker_start, self.index);
        let value_start = self.index;
        let mut closed = false;
        while let Some(ch) = self.bump_char() {
            if ch == ']' {
                closed = true;
                break;
            }
        }
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::InlineField, span);
        if closed {
            let value_end = self.index.saturating_sub(1);
            let value = trim_spanned_string(
                &self.text[value_start..value_end],
                self.line_offset + value_start,
            );
            let voice = (code == 'V').then(|| {
                Spanned::new(
                    crate::parse::field::parse_voice_for_music(value.clone()),
                    value.span,
                )
            });
            self.items.push(MusicItem::InlineField(InlineFieldSyntax {
                span,
                marker_span,
                code,
                value,
                voice,
            }));
        } else {
            self.push_malformed(
                span,
                MalformedSyntaxKind::UnclosedInlineField,
                "abc.music.unclosed_inline_field",
                "Inline field was preserved and skipped",
            );
        }
    }

    pub(super) fn parse_open_paren(&mut self) {
        if self.peek_next_char().is_some_and(|ch| ch.is_ascii_digit()) {
            self.parse_tuplet();
        } else {
            self.parse_slur(SlurDirection::Start, false);
        }
    }

    pub(super) fn take_pending_attachments(&mut self) -> AttachmentBundle {
        std::mem::take(&mut self.pending_attachments)
    }

    pub(super) fn flush_pending_attachments(&mut self) {
        let attachments = self.take_pending_attachments();
        for grace in attachments.grace_groups {
            self.items.push(MusicItem::GraceGroup(grace));
        }
        for chord_symbol in attachments.chord_symbols {
            self.items.push(MusicItem::ChordSymbol(chord_symbol));
        }
        for annotation in attachments.annotations {
            self.items.push(MusicItem::Annotation(annotation));
        }
        for decoration in attachments.decorations {
            self.items.push(MusicItem::Decoration(decoration));
        }
    }

    pub(super) fn is_user_symbol(&self, symbol: char) -> bool {
        self.dialect
            .user_symbols
            .iter()
            .any(|definition| definition.symbol.value == symbol)
    }

    /// The `U:`-defined replacement text for `symbol`, if one is in scope. The
    /// last definition wins, matching ABC redefinition semantics.
    pub(super) fn user_symbol_replacement(&self, symbol: char) -> Option<String> {
        self.dialect
            .user_symbols
            .iter()
            .rev()
            .find(|definition| definition.symbol.value == symbol)
            .map(|definition| definition.replacement.value.clone())
    }

    pub(super) fn parse_malformed_single(
        &mut self,
        kind: MalformedSyntaxKind,
        code: &'static str,
        message: &'static str,
    ) {
        self.flush_pending_attachments();
        let start = self.index;
        self.bump_char();
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Malformed, span);
        self.push_malformed(span, kind, code, message);
    }

    pub(super) fn parse_malformed_digits(&mut self) {
        self.flush_pending_attachments();
        let start = self.index;
        while self.peek_char().is_some_and(|ch| ch.is_ascii_digit()) {
            self.bump_char();
        }
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Malformed, span);
        let previous = self.previous_non_whitespace_char(start);
        if previous.is_some_and(|ch| matches!(ch, '|' | ':')) {
            self.push_malformed(
                span,
                MalformedSyntaxKind::InvalidRepeatEnding,
                "abc.music.invalid_repeat_ending",
                "Repeat-ending shorthand must be adjacent to the barline",
            );
        } else {
            self.push_malformed(
                span,
                MalformedSyntaxKind::StandaloneLength,
                "abc.music.malformed_length",
                "Length suffixes must follow a note or rest",
            );
        }
    }

    pub(super) fn previous_non_whitespace_char(&self, before: usize) -> Option<char> {
        self.text[..before]
            .chars()
            .rev()
            .find(|ch| !ch.is_whitespace())
    }

    pub(super) fn parse_unsupported_single(
        &mut self,
        kind: UnsupportedSyntaxKind,
        code: &'static str,
        message: &'static str,
    ) {
        self.flush_pending_attachments();
        let start = self.index;
        self.bump_char();
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Unsupported, span);
        self.items
            .push(MusicItem::Unsupported(UnsupportedSyntax { span, kind }));
        self.push_unsupported_diagnostic(span, code, message);
    }

    pub(super) fn is_inline_field_start(&self) -> bool {
        let mut chars = self.text[self.index..].chars();
        matches!(chars.next(), Some('['))
            && chars.next().is_some_and(|ch| ch.is_ascii_alphabetic())
            && matches!(chars.next(), Some(':'))
    }

    pub(super) fn push_token(&mut self, kind: MusicTokenKind, span: Span) {
        self.tokens.push(MusicToken { kind, span });
    }

    pub(super) fn push_malformed(
        &mut self,
        span: Span,
        kind: MalformedSyntaxKind,
        code: &'static str,
        message: &'static str,
    ) {
        self.items
            .push(MusicItem::Malformed(MalformedSyntax { span, kind }));
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, code, message, span)
                .with_spec_reference(abc_music_reference())
                .with_recovery_note(RecoveryNote::new(
                    "The malformed token was preserved and skipped.",
                )),
        );
    }

    pub(super) fn push_unsupported_diagnostic(
        &mut self,
        span: Span,
        code: &'static str,
        message: &'static str,
    ) {
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, code, message, span)
                .with_spec_reference(abc_music_reference())
                .with_recovery_note(RecoveryNote::new(
                    "The construct remains in the syntax tree but does not produce notes yet.",
                )),
        );
    }

    pub(super) fn peek_char(&self) -> Option<char> {
        self.text[self.index..].chars().next()
    }

    pub(super) fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.text[self.index..].chars();
        chars.next()?;
        chars.next()
    }

    pub(super) fn bump_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.index += ch.len_utf8();
        Some(ch)
    }

    pub(super) fn starts_with(&self, pattern: &str) -> bool {
        self.text[self.index..].starts_with(pattern)
    }

    pub(super) fn span(&self, start: usize, end: usize) -> Span {
        Span::new(self.line_offset + start, self.line_offset + end)
    }
}

pub(super) fn is_note_letter(ch: char) -> bool {
    matches!(ch, 'A'..='G' | 'a'..='g')
}

pub(super) fn is_barline_char(ch: char) -> bool {
    matches!(ch, '|' | '[' | ']' | ':')
}

/// Canonical decoration name for an ABC 2.1 §4.14 single-char shorthand, so the
/// shorthand maps through the same export path as the long-form `!...!` name.
///
/// `.` (staccato) is intentionally left untouched: it is already handled as the
/// canonical `"."` by the exporter and shares this code path via `parse_dot`.
pub(super) fn shorthand_canonical_name(symbol: char) -> Option<String> {
    let canonical = match symbol {
        '~' => "roll",
        'H' => "fermata",
        'L' => "accent",
        'M' => "lowermordent",
        'O' => "coda",
        'P' => "uppermordent",
        'S' => "segno",
        'T' => "trill",
        'u' => "upbow",
        'v' => "downbow",
        _ => return None,
    };
    Some(canonical.to_string())
}

/// Canonical decoration name for a `U:`-defined replacement. Replacements are
/// stored verbatim (e.g. `!trill!`); strip the `!...!` delimiters and, when the
/// inner text is itself a single-char shorthand, normalize it too.
pub(super) fn user_symbol_canonical_name(replacement: &str) -> Option<String> {
    let trimmed = replacement.trim();
    let inner = trimmed
        .strip_prefix('!')
        .and_then(|rest| rest.strip_suffix('!'))
        .or_else(|| {
            trimmed
                .strip_prefix('+')
                .and_then(|rest| rest.strip_suffix('+'))
        })?;
    if inner.is_empty() {
        return None;
    }
    if let Some(symbol) = inner.chars().next().filter(|_| inner.chars().count() == 1)
        && let Some(canonical) = shorthand_canonical_name(symbol)
    {
        return Some(canonical);
    }
    Some(inner.to_string())
}

pub(super) fn is_escaped(text: &str, offset: usize) -> bool {
    let mut slash_count = 0;
    for byte in text[..offset].bytes().rev() {
        if byte == b'\\' {
            slash_count += 1;
        } else {
            break;
        }
    }
    slash_count % 2 == 1
}

pub(super) fn classify_quoted_text(text: &str) -> QuotedTextKind {
    match text.chars().next() {
        Some('^') => QuotedTextKind::Annotation(AnnotationPlacement::Above),
        Some('_') => QuotedTextKind::Annotation(AnnotationPlacement::Below),
        Some('<') => QuotedTextKind::Annotation(AnnotationPlacement::Left),
        Some('>') => QuotedTextKind::Annotation(AnnotationPlacement::Right),
        Some('@') => QuotedTextKind::Annotation(AnnotationPlacement::Free),
        _ => QuotedTextKind::ChordSymbol,
    }
}

pub(super) fn invalid_length_warning(span: Span, message: &'static str) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.malformed_length",
        message,
        span,
    )
    .with_spec_reference(abc_music_reference())
    .with_recovery_note(RecoveryNote::new(
        "The length suffix was preserved and a safe duration was used.",
    ))
}

fn unsupported_directive_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.directive.unsupported",
        "Unsupported stylesheet directive was preserved as metadata",
        span,
    )
    .with_spec_reference(abc_field_reference())
    .with_recovery_note(RecoveryNote::new(
        "The directive did not produce music events.",
    ))
}

fn abc_music_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 tune body")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}
