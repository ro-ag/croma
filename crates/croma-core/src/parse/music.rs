//! Music-line parser: text -> surface music AST.
//!
//! The lowering half (text-AST -> model) remains in `crate::music`.

use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
use crate::parse::field::{
    AccidentalSign, DecorationDelimiter, DialectState, FieldState, InterpretationField, KeyMode,
    KeySignature, KeyTonicAccidental, Meter, MeterKind, ParsedAbcFields, ParsedFieldKind,
    ScoreDirective, Spanned, StemDirection, UnitNoteLength, VoiceDefinition, parse_voice_for_music,
};
use crate::model::{
    Accidental, AccidentalMark, AccidentalPolicy, AccidentalScope, AlignedLyric, AlignedSymbol,
    AlignedSymbolKind, AnnotationPlacementModel, BarlineKind, ChordEvent, ChordMemberEvent,
    DecorationAttachment, DecorationSourceKind, Event, EventAttachments, Fraction, GraceEvent,
    GraceEventKind, GraceGroupAttachment, GraceNoteEvent, KeyAccidentalModel, KeySignatureModel,
    LoweredEventAtom, LoweredEventAtomKind, LyricControl, Measure, MeasureBarline, MeasureId,
    MeterModel, NoteEvent, OverlaySegment, Part, PartId, Pitch, PreservedDirective,
    RepeatEndingModel, RepeatEndingPartModel, RestEvent, RestVisibility, Score,
    ScoreDirectiveModel, ScoreDirectiveTokenKindModel, ScoreDirectiveTokenModel, ScoreMetadata,
    SlurAttachment, SlurRole, Staff, StaffId, StemDirectionModel, TempoBeat, TempoModel,
    TextAttachment, TextLine, TieAttachment, TieRole, TimedEvent, TimedEventKind,
    TimelineEventKind, TupletAttachment, TupletRole, Voice, VoiceId, VoiceMeasureTimeline,
    VoicePropertiesModel, VoiceTimedEvent, VoiceTimeline, lcm,
};
use crate::options::ParseMode;
use crate::parse::ParseReport;
use crate::source::SourceText;
use crate::syntax::tune::{LineContext, LineKind, ScoreLineBreak, SurfaceMap};
use crate::syntax::{
    AccidentalSyntax, AnnotationPlacement, AttachmentBundle, BarlineSyntax, BrokenRhythmDirection,
    BrokenRhythmSyntax, ChordMemberSyntax, ChordSyntax, DecorationKind, DecorationSyntax,
    GraceElementSyntax, GraceGroupSyntax, InlineFieldSyntax, LengthSyntax, LyricLineSyntax,
    LyricTokenKind, LyricTokenSyntax, MalformedSyntax, MalformedSyntaxKind, MultiMeasureRestSyntax,
    MusicFieldLine, MusicFieldLineKind, MusicItem, MusicLine, MusicToken, MusicTokenKind,
    NoteSyntax, OctaveMark, OctaveMarkSyntax, OverlaySyntax, ParsedMusicDocument, ParsedTuneMusic,
    PitchSyntax, PreservedDirectiveSyntax, QuotedTextKind, QuotedTextSyntax, RestSyntax,
    ScoreDirectiveSyntax, SlurDirection, SlurSyntax, SpacerSyntax, SpannedNumber, SymbolLineSyntax,
    SymbolTokenKind, SymbolTokenSyntax, TieSyntax, TupletSyntax, UnsupportedSyntax,
    UnsupportedSyntaxKind, VariantEndingPart, VariantEndingSyntax,
};
use crate::music::{
    abc_barline_reference, abc_field_reference, invalid_tuplet_warning, music_code_span,
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

struct MusicLineParser<'line> {
    text: &'line str,
    line_offset: usize,
    index: usize,
    dialect: DialectState,
    pending_attachments: AttachmentBundle,
    tokens: Vec<MusicToken>,
    items: Vec<MusicItem>,
    diagnostics: Vec<Diagnostic>,
}

impl<'line> MusicLineParser<'line> {
    fn new(text: &'line str, line_offset: usize, dialect: DialectState) -> Self {
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

    fn parse(&mut self, line_index: usize, span: Span, code_span: Span) -> MusicLine {
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

    fn parse_whitespace(&mut self) {
        let start = self.index;
        while self.peek_char().is_some_and(char::is_whitespace) {
            self.bump_char();
        }
        self.push_token(MusicTokenKind::Whitespace, self.span(start, self.index));
    }

    fn parse_accidental_or_malformed(&mut self) {
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

    fn parse_note(&mut self, accidental: Option<AccidentalSyntax>) {
        let attachments = self.take_pending_attachments();
        let core_start = accidental
            .as_ref()
            .map(|accidental| accidental.span.start - self.line_offset)
            .unwrap_or(self.index);
        let start = attachments
            .span_start()
            .map(|start| start - self.line_offset)
            .unwrap_or(core_start)
            .min(core_start);
        let pitch_start = self.index;
        let Some(step) = self.bump_char() else {
            return;
        };
        let pitch_span = self.span(pitch_start, self.index);
        self.push_token(MusicTokenKind::Pitch, pitch_span);

        let octave_marks = self.parse_octave_marks();
        let length = self.parse_length_suffix();
        let end = length
            .as_ref()
            .map(|length| length.span.end - self.line_offset)
            .or_else(|| {
                octave_marks
                    .last()
                    .map(|mark| mark.span.end - self.line_offset)
            })
            .unwrap_or(self.index);
        let note = NoteSyntax {
            span: self.span(start, end),
            attachments,
            accidental,
            pitch: PitchSyntax {
                step,
                span: pitch_span,
            },
            octave_marks,
            length,
        };
        self.items.push(MusicItem::Note(note));
    }

    fn parse_accidental_token(&mut self) -> Option<AccidentalSyntax> {
        let start = self.index;
        let sign = if self.starts_with("__") {
            self.index += 2;
            Accidental::DoubleFlat
        } else if self.starts_with("^^") {
            self.index += 2;
            Accidental::DoubleSharp
        } else {
            match self.bump_char()? {
                '_' => Accidental::Flat,
                '^' => Accidental::Sharp,
                '=' => Accidental::Natural,
                _ => return None,
            }
        };
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Accidental, span);
        Some(AccidentalSyntax { sign, span })
    }

    fn parse_octave_marks(&mut self) -> Vec<OctaveMarkSyntax> {
        let mut marks = Vec::new();
        while let Some(ch @ ('\'' | ',')) = self.peek_char() {
            let start = self.index;
            self.bump_char();
            let span = self.span(start, self.index);
            self.push_token(MusicTokenKind::OctaveMark, span);
            marks.push(OctaveMarkSyntax {
                mark: if ch == '\'' {
                    OctaveMark::Raise
                } else {
                    OctaveMark::Lower
                },
                span,
            });
        }
        marks
    }

    fn parse_rest(&mut self, visibility: RestVisibility) {
        let attachments = self.take_pending_attachments();
        let start = self.index;
        self.bump_char();
        let marker_span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Rest, marker_span);
        let length = self.parse_length_suffix();
        let span = length
            .as_ref()
            .map(|length| {
                Span::new(
                    attachments.span_start().unwrap_or(marker_span.start),
                    length.span.end,
                )
            })
            .unwrap_or_else(|| {
                Span::new(
                    attachments.span_start().unwrap_or(marker_span.start),
                    marker_span.end,
                )
            });
        self.items.push(MusicItem::Rest(RestSyntax {
            span,
            attachments,
            visibility,
            marker_span,
            length,
        }));
    }

    fn parse_multi_measure_rest(&mut self, visibility: RestVisibility) {
        self.flush_pending_attachments();
        let start = self.index;
        self.bump_char();
        let marker_span = self.span(start, self.index);
        self.push_token(MusicTokenKind::MultiMeasureRest, marker_span);
        let count = self.parse_number_token();
        if let Some(count) = count {
            self.push_token(MusicTokenKind::Length, count.span);
        }
        let span = count
            .map(|count| Span::new(marker_span.start, count.span.end))
            .unwrap_or(marker_span);
        self.items
            .push(MusicItem::MultiMeasureRest(MultiMeasureRestSyntax {
                span,
                visibility,
                marker_span,
                count,
            }));
    }

    fn parse_spacer(&mut self) {
        self.flush_pending_attachments();
        let start = self.index;
        self.bump_char();
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Spacer, span);
        self.items.push(MusicItem::Spacer(SpacerSyntax { span }));
    }

    fn parse_dot(&mut self) {
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

    fn parse_left_bracket(&mut self) {
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

    fn parse_barline(&mut self, dotted: bool) {
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

    fn parse_colon(&mut self) {
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

    fn parse_inline_field(&mut self) {
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

    fn parse_quoted_text(&mut self) {
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

    fn parse_open_paren(&mut self) {
        if self.peek_next_char().is_some_and(|ch| ch.is_ascii_digit()) {
            self.parse_tuplet();
        } else {
            self.parse_slur(SlurDirection::Start, false);
        }
    }

    fn parse_decoration(&mut self, delimiter: char) {
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

    fn parse_invalid_decoration(&mut self, delimiter: char) {
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

    fn parse_shorthand_decoration(&mut self) {
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

    fn parse_broken_rhythm(&mut self) {
        let start = self.index;
        let Some(marker) = self.bump_char() else {
            return;
        };
        while self.peek_char() == Some(marker) {
            self.bump_char();
        }
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::BrokenRhythm, span);
        self.items.push(MusicItem::BrokenRhythm(BrokenRhythmSyntax {
            span,
            direction: if marker == '<' {
                BrokenRhythmDirection::LeftShorter
            } else {
                BrokenRhythmDirection::RightShorter
            },
            count: u8::try_from(self.index - start).unwrap_or(u8::MAX),
        }));
    }

    fn parse_overlay(&mut self) {
        self.flush_pending_attachments();
        let start = self.index;
        self.bump_char();
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Overlay, span);
        self.items.push(MusicItem::Overlay(OverlaySyntax { span }));
    }

    fn parse_tie(&mut self, dotted: bool) {
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

    fn parse_slur(&mut self, direction: SlurDirection, dotted: bool) {
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

    fn parse_tuplet(&mut self) {
        self.flush_pending_attachments();
        let start = self.index;
        self.bump_char();
        let Some(p) = self.parse_number_token() else {
            let span = self.span(start, self.index);
            self.push_token(MusicTokenKind::Malformed, span);
            self.push_malformed(
                span,
                MalformedSyntaxKind::InvalidTuplet,
                "abc.music.invalid_tuplet",
                "Tuplet specifier must start with a number",
            );
            return;
        };

        let mut q = None;
        let mut r = None;
        if self.peek_char() == Some(':') {
            self.bump_char();
            q = self.parse_number_token();
            if self.peek_char() == Some(':') {
                self.bump_char();
                r = self.parse_number_token();
            }
        }

        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Tuplet, span);
        if !(2..=9).contains(&p.value) {
            self.diagnostics.push(invalid_tuplet_warning(span));
        }
        self.items
            .push(MusicItem::Tuplet(TupletSyntax { span, p, q, r }));
    }

    fn parse_variant_ending(&mut self, shorthand: bool) {
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

    fn parse_chord(&mut self) {
        let attachments = self.take_pending_attachments();
        let start = attachments
            .span_start()
            .map(|start| start - self.line_offset)
            .unwrap_or(self.index);
        let open_start = self.index;
        self.bump_char();
        let open_span = self.span(open_start, self.index);
        let mut members = Vec::new();
        let mut closed = false;

        while let Some(ch) = self.peek_char() {
            match ch {
                ']' => {
                    self.bump_char();
                    closed = true;
                    break;
                }
                ch if ch.is_whitespace() => self.parse_whitespace(),
                '^' | '_' | '=' => {
                    let Some(accidental) = self.parse_accidental_token() else {
                        continue;
                    };
                    if self.peek_char().is_some_and(is_note_letter) {
                        if let Some(note) = self.parse_note_syntax(Some(accidental)) {
                            members.push(ChordMemberSyntax {
                                span: note.span,
                                note,
                            });
                        }
                    } else {
                        self.push_malformed(
                            accidental.span,
                            MalformedSyntaxKind::DanglingAccidental,
                            "abc.music.malformed_accidental",
                            "Accidentals must appear immediately before a chord member note",
                        );
                    }
                }
                'A'..='G' | 'a'..='g' => {
                    if let Some(note) = self.parse_note_syntax(None) {
                        members.push(ChordMemberSyntax {
                            span: note.span,
                            note,
                        });
                    }
                }
                '"' => self.parse_quoted_text(),
                '{' => self.parse_grace_group(),
                '!' | '+' => self.parse_decoration(ch),
                '.' => self.parse_dot(),
                '~' | 'H' | 'L' | 'M' | 'O' | 'P' | 'S' | 'T' | 'u' | 'v' => {
                    self.parse_shorthand_decoration()
                }
                ch if self.is_user_symbol(ch) => self.parse_shorthand_decoration(),
                _ => {
                    self.parse_malformed_single(
                        MalformedSyntaxKind::UnknownToken,
                        "abc.music.unknown_chord_token",
                        "Unknown chord-member token was preserved and skipped",
                    );
                }
            }
        }

        let close_span = closed.then(|| self.span(self.index - 1, self.index));
        if !closed {
            let span = self.span(start, self.index);
            self.push_token(MusicTokenKind::Malformed, span);
            self.push_malformed(
                span,
                MalformedSyntaxKind::UnclosedChord,
                "abc.music.unclosed_chord",
                "Chord group was preserved and skipped",
            );
            return;
        }

        let length = self.parse_length_suffix();
        let end = length
            .as_ref()
            .map(|length| length.span.end - self.line_offset)
            .unwrap_or(self.index);
        let span = self.span(start, end);
        self.push_token(MusicTokenKind::Chord, span);
        self.items.push(MusicItem::Chord(ChordSyntax {
            span,
            attachments,
            open_span,
            close_span,
            members,
            length,
        }));
    }

    fn parse_grace_group(&mut self) {
        let start = self.index;
        self.bump_char();
        let slash_span = if self.peek_char() == Some('/') {
            let slash_start = self.index;
            self.bump_char();
            Some(self.span(slash_start, self.index))
        } else {
            None
        };
        let mut elements = Vec::new();
        let mut closed = false;

        while let Some(ch) = self.peek_char() {
            match ch {
                '}' => {
                    self.bump_char();
                    closed = true;
                    break;
                }
                ch if ch.is_whitespace() => self.parse_whitespace(),
                '^' | '_' | '=' => {
                    let Some(accidental) = self.parse_accidental_token() else {
                        continue;
                    };
                    if self.peek_char().is_some_and(is_note_letter) {
                        if let Some(note) = self.parse_note_syntax(Some(accidental)) {
                            elements.push(GraceElementSyntax::Note(note));
                        }
                    } else {
                        let malformed = MalformedSyntax {
                            span: accidental.span,
                            kind: MalformedSyntaxKind::DanglingAccidental,
                        };
                        elements.push(GraceElementSyntax::Malformed(malformed.clone()));
                        self.push_malformed(
                            malformed.span,
                            malformed.kind,
                            "abc.music.malformed_accidental",
                            "Accidentals must appear immediately before a grace note",
                        );
                    }
                }
                'A'..='G' | 'a'..='g' => {
                    if let Some(note) = self.parse_note_syntax(None) {
                        elements.push(GraceElementSyntax::Note(note));
                    }
                }
                'z' => {
                    let rest = self.parse_rest_syntax(RestVisibility::Visible);
                    elements.push(GraceElementSyntax::Rest(rest));
                }
                'x' => {
                    let rest = self.parse_rest_syntax(RestVisibility::Invisible);
                    elements.push(GraceElementSyntax::Rest(rest));
                }
                '[' => {
                    self.parse_chord();
                    if let Some(MusicItem::Chord(chord)) = self.items.pop() {
                        elements.push(GraceElementSyntax::Chord(chord));
                    }
                }
                '"' => self.parse_quoted_text(),
                '!' | '+' => self.parse_decoration(ch),
                '.' => self.parse_dot(),
                '~' | 'H' | 'L' | 'M' | 'O' | 'P' | 'S' | 'T' | 'u' | 'v' => {
                    self.parse_shorthand_decoration()
                }
                ch if self.is_user_symbol(ch) => self.parse_shorthand_decoration(),
                _ => {
                    let start = self.index;
                    self.bump_char();
                    let malformed = MalformedSyntax {
                        span: self.span(start, self.index),
                        kind: MalformedSyntaxKind::UnknownToken,
                    };
                    elements.push(GraceElementSyntax::Malformed(malformed.clone()));
                    self.push_malformed(
                        malformed.span,
                        malformed.kind,
                        "abc.music.unknown_grace_token",
                        "Unknown grace-group token was preserved and skipped",
                    );
                }
            }
        }

        let span = self.span(start, self.index);
        if closed {
            self.push_token(MusicTokenKind::GraceGroup, span);
            self.pending_attachments
                .grace_groups
                .push(GraceGroupSyntax {
                    span,
                    slash_span,
                    elements,
                });
        } else {
            self.push_token(MusicTokenKind::Malformed, span);
            self.push_malformed(
                span,
                MalformedSyntaxKind::UnclosedGraceGroup,
                "abc.music.unclosed_grace",
                "Grace group was preserved and skipped",
            );
        }
    }

    fn parse_note_syntax(&mut self, accidental: Option<AccidentalSyntax>) -> Option<NoteSyntax> {
        let attachments = self.take_pending_attachments();
        let core_start = accidental
            .as_ref()
            .map(|accidental| accidental.span.start - self.line_offset)
            .unwrap_or(self.index);
        let start = attachments
            .span_start()
            .map(|start| start - self.line_offset)
            .unwrap_or(core_start)
            .min(core_start);
        let pitch_start = self.index;
        let step = self.bump_char()?;
        let pitch_span = self.span(pitch_start, self.index);
        self.push_token(MusicTokenKind::Pitch, pitch_span);

        let octave_marks = self.parse_octave_marks();
        let length = self.parse_length_suffix();
        let end = length
            .as_ref()
            .map(|length| length.span.end - self.line_offset)
            .or_else(|| {
                octave_marks
                    .last()
                    .map(|mark| mark.span.end - self.line_offset)
            })
            .unwrap_or(self.index);
        Some(NoteSyntax {
            span: self.span(start, end),
            attachments,
            accidental,
            pitch: PitchSyntax {
                step,
                span: pitch_span,
            },
            octave_marks,
            length,
        })
    }

    fn parse_rest_syntax(&mut self, visibility: RestVisibility) -> RestSyntax {
        let attachments = self.take_pending_attachments();
        let start = self.index;
        self.bump_char();
        let marker_span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Rest, marker_span);
        let length = self.parse_length_suffix();
        let span = length
            .as_ref()
            .map(|length| {
                Span::new(
                    attachments.span_start().unwrap_or(marker_span.start),
                    length.span.end,
                )
            })
            .unwrap_or_else(|| {
                Span::new(
                    attachments.span_start().unwrap_or(marker_span.start),
                    marker_span.end,
                )
            });
        RestSyntax {
            span,
            attachments,
            visibility,
            marker_span,
            length,
        }
    }

    fn take_pending_attachments(&mut self) -> AttachmentBundle {
        std::mem::take(&mut self.pending_attachments)
    }

    fn flush_pending_attachments(&mut self) {
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

    fn is_user_symbol(&self, symbol: char) -> bool {
        self.dialect
            .user_symbols
            .iter()
            .any(|definition| definition.symbol.value == symbol)
    }

    /// The `U:`-defined replacement text for `symbol`, if one is in scope. The
    /// last definition wins, matching ABC redefinition semantics.
    fn user_symbol_replacement(&self, symbol: char) -> Option<String> {
        self.dialect
            .user_symbols
            .iter()
            .rev()
            .find(|definition| definition.symbol.value == symbol)
            .map(|definition| definition.replacement.value.clone())
    }

    fn parse_malformed_single(
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

    fn parse_malformed_digits(&mut self) {
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

    fn previous_non_whitespace_char(&self, before: usize) -> Option<char> {
        self.text[..before]
            .chars()
            .rev()
            .find(|ch| !ch.is_whitespace())
    }

    fn parse_unsupported_single(
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

    fn parse_length_suffix(&mut self) -> Option<LengthSyntax> {
        let start = self.index;
        let numerator = self.parse_number_token();
        let mut slash_count = 0u8;
        let mut denominator = None;

        if self.peek_char() == Some('/') {
            while self.peek_char() == Some('/') {
                slash_count = slash_count.saturating_add(1);
                self.bump_char();
            }
            denominator = self.parse_number_token();
        }

        if numerator.is_none() && slash_count == 0 {
            return None;
        }

        let end = self.index;
        let span = self.span(start, end);
        self.push_token(MusicTokenKind::Length, span);
        let numerator_value = numerator.map(|number| number.value).unwrap_or(1);
        let denominator_value = match (slash_count, denominator) {
            (0, _) => 1,
            (_, Some(number)) if number.value == 0 => {
                self.diagnostics.push(invalid_length_warning(
                    number.span,
                    "Length denominator cannot be zero; recovered as denominator 1",
                ));
                1
            }
            (_, Some(number)) => number.value,
            (slashes, None) => slash_denominator(slashes).unwrap_or_else(|| {
                self.diagnostics.push(invalid_length_warning(
                    span,
                    "Length slash shorthand is too long; recovered as denominator 1",
                ));
                1
            }),
        };
        let numerator_value = if let Some(number) = numerator
            && number.value == 0
        {
            self.diagnostics.push(invalid_length_warning(
                number.span,
                "Length numerator cannot be zero; recovered as numerator 1",
            ));
            1
        } else {
            numerator_value
        };

        Some(LengthSyntax {
            span,
            raw: self.text[start..end].to_owned(),
            numerator,
            slash_count,
            denominator,
            multiplier: Fraction::new(numerator_value, denominator_value),
        })
    }

    fn parse_number_token(&mut self) -> Option<SpannedNumber> {
        let start = self.index;
        while self.peek_char().is_some_and(|ch| ch.is_ascii_digit()) {
            self.bump_char();
        }
        if start == self.index {
            return None;
        }
        let span = self.span(start, self.index);
        let raw = &self.text[start..self.index];
        let value = raw.parse().unwrap_or_else(|_| {
            self.diagnostics.push(invalid_length_warning(
                span,
                "Number is too large; recovered as 1",
            ));
            1
        });
        Some(SpannedNumber { value, span })
    }

    fn is_inline_field_start(&self) -> bool {
        let mut chars = self.text[self.index..].chars();
        matches!(chars.next(), Some('['))
            && chars.next().is_some_and(|ch| ch.is_ascii_alphabetic())
            && matches!(chars.next(), Some(':'))
    }

    fn push_token(&mut self, kind: MusicTokenKind, span: Span) {
        self.tokens.push(MusicToken { kind, span });
    }

    fn push_malformed(
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

    fn push_unsupported_diagnostic(
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

    fn peek_char(&self) -> Option<char> {
        self.text[self.index..].chars().next()
    }

    fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.text[self.index..].chars();
        chars.next()?;
        chars.next()
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.index += ch.len_utf8();
        Some(ch)
    }

    fn starts_with(&self, pattern: &str) -> bool {
        self.text[self.index..].starts_with(pattern)
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(self.line_offset + start, self.line_offset + end)
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

fn is_note_letter(ch: char) -> bool {
    matches!(ch, 'A'..='G' | 'a'..='g')
}

fn is_barline_char(ch: char) -> bool {
    matches!(ch, '|' | '[' | ']' | ':')
}

/// Canonical decoration name for an ABC 2.1 §4.14 single-char shorthand, so the
/// shorthand maps through the same export path as the long-form `!...!` name.
///
/// `.` (staccato) is intentionally left untouched: it is already handled as the
/// canonical `"."` by the exporter and shares this code path via `parse_dot`.
fn shorthand_canonical_name(symbol: char) -> Option<String> {
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
fn user_symbol_canonical_name(replacement: &str) -> Option<String> {
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

fn slash_denominator(slash_count: u8) -> Option<u32> {
    1u32.checked_shl(u32::from(slash_count))
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

fn invalid_length_warning(span: Span, message: &'static str) -> Diagnostic {
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
