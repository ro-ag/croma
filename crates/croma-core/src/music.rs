use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
use crate::fields::{
    DecorationDelimiter, DialectState, FieldState, InterpretationField, MeterKind, ParsedAbcFields,
    ParsedFieldKind, ScoreDirective, Spanned, StemDirection, VoiceDefinition,
};
use crate::model::{
    Accidental, AlignedLyric, AlignedSymbol, AlignedSymbolKind, BarlineKind, Event, Fraction,
    LyricControl, OverlaySegment, RestVisibility, ScoreDirectiveModel,
    ScoreDirectiveTokenKindModel, ScoreDirectiveTokenModel, StemDirectionModel, TextLine,
    TimedEvent, TimedEventKind, TimelineEventKind, VoiceId, VoiceMeasureTimeline,
    VoicePropertiesModel, VoiceTimedEvent, VoiceTimeline, lcm,
};
use crate::options::ParseMode;
use crate::parser::ParseReport;
use crate::source::SourceText;
use crate::surface::{LineContext, LineKind, ScoreLineBreak, SurfaceMap};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedMusicDocument {
    pub tunes: Vec<ParsedTuneMusic>,
}

impl ParsedMusicDocument {
    pub fn tune(&self, tune_index: usize) -> Option<&ParsedTuneMusic> {
        self.tunes.iter().find(|tune| tune.tune_index == tune_index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTuneMusic {
    pub tune_index: usize,
    pub span: Span,
    pub lines: Vec<MusicLine>,
    pub body_fields: Vec<MusicFieldLine>,
    pub lyric_lines: Vec<LyricLineSyntax>,
    pub symbol_lines: Vec<SymbolLineSyntax>,
    pub score_directives: Vec<ScoreDirectiveSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicLine {
    pub line_index: usize,
    pub span: Span,
    pub code_span: Span,
    pub tokens: Vec<MusicToken>,
    pub items: Vec<MusicItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MusicToken {
    pub kind: MusicTokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicTokenKind {
    Whitespace,
    Accidental,
    Pitch,
    OctaveMark,
    Length,
    Rest,
    MultiMeasureRest,
    Spacer,
    Chord,
    GraceGroup,
    ChordSymbol,
    Annotation,
    Decoration,
    Tuplet,
    Slur,
    Tie,
    BrokenRhythm,
    Overlay,
    RepeatEnding,
    Barline,
    InlineField,
    Unsupported,
    Malformed,
    Comment,
    ScoreLineBreak,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MusicItem {
    Note(NoteSyntax),
    Rest(RestSyntax),
    MultiMeasureRest(MultiMeasureRestSyntax),
    Spacer(SpacerSyntax),
    Chord(ChordSyntax),
    GraceGroup(GraceGroupSyntax),
    ChordSymbol(QuotedTextSyntax),
    Annotation(QuotedTextSyntax),
    Decoration(DecorationSyntax),
    Tuplet(TupletSyntax),
    Slur(SlurSyntax),
    Tie(TieSyntax),
    BrokenRhythm(BrokenRhythmSyntax),
    Overlay(OverlaySyntax),
    VariantEnding(VariantEndingSyntax),
    Barline(BarlineSyntax),
    InlineField(InlineFieldSyntax),
    Unsupported(UnsupportedSyntax),
    Malformed(MalformedSyntax),
}

impl MusicItem {
    pub fn span(&self) -> Span {
        match self {
            Self::Note(item) => item.span,
            Self::Rest(item) => item.span,
            Self::MultiMeasureRest(item) => item.span,
            Self::Spacer(item) => item.span,
            Self::Chord(item) => item.span,
            Self::GraceGroup(item) => item.span,
            Self::ChordSymbol(item) | Self::Annotation(item) => item.span,
            Self::Decoration(item) => item.span,
            Self::Tuplet(item) => item.span,
            Self::Slur(item) => item.span,
            Self::Tie(item) => item.span,
            Self::BrokenRhythm(item) => item.span,
            Self::Overlay(item) => item.span,
            Self::VariantEnding(item) => item.span,
            Self::Barline(item) => item.span,
            Self::InlineField(item) => item.span,
            Self::Unsupported(item) => item.span,
            Self::Malformed(item) => item.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSyntax {
    pub span: Span,
    pub attachments: AttachmentBundle,
    pub accidental: Option<AccidentalSyntax>,
    pub pitch: PitchSyntax,
    pub octave_marks: Vec<OctaveMarkSyntax>,
    pub length: Option<LengthSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AttachmentBundle {
    pub grace_groups: Vec<GraceGroupSyntax>,
    pub chord_symbols: Vec<QuotedTextSyntax>,
    pub annotations: Vec<QuotedTextSyntax>,
    pub decorations: Vec<DecorationSyntax>,
}

impl AttachmentBundle {
    pub fn is_empty(&self) -> bool {
        self.grace_groups.is_empty()
            && self.chord_symbols.is_empty()
            && self.annotations.is_empty()
            && self.decorations.is_empty()
    }

    fn span_start(&self) -> Option<usize> {
        self.grace_groups
            .iter()
            .map(|item| item.span.start)
            .chain(self.chord_symbols.iter().map(|item| item.span.start))
            .chain(self.annotations.iter().map(|item| item.span.start))
            .chain(self.decorations.iter().map(|item| item.span.start))
            .min()
    }

    fn push_quoted_text(&mut self, text: QuotedTextSyntax) {
        match text.kind {
            QuotedTextKind::ChordSymbol => self.chord_symbols.push(text),
            QuotedTextKind::Annotation(_) => self.annotations.push(text),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitchSyntax {
    pub step: char,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccidentalSyntax {
    pub sign: Accidental,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OctaveMarkSyntax {
    pub mark: OctaveMark,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OctaveMark {
    Lower,
    Raise,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LengthSyntax {
    pub span: Span,
    pub raw: String,
    pub numerator: Option<SpannedNumber>,
    pub slash_count: u8,
    pub denominator: Option<SpannedNumber>,
    pub multiplier: Fraction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpannedNumber {
    pub value: u32,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestSyntax {
    pub span: Span,
    pub attachments: AttachmentBundle,
    pub visibility: RestVisibility,
    pub marker_span: Span,
    pub length: Option<LengthSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiMeasureRestSyntax {
    pub span: Span,
    pub visibility: RestVisibility,
    pub marker_span: Span,
    pub count: Option<SpannedNumber>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpacerSyntax {
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChordSyntax {
    pub span: Span,
    pub attachments: AttachmentBundle,
    pub open_span: Span,
    pub close_span: Option<Span>,
    pub members: Vec<ChordMemberSyntax>,
    pub length: Option<LengthSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChordMemberSyntax {
    pub span: Span,
    pub note: NoteSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraceGroupSyntax {
    pub span: Span,
    pub slash_span: Option<Span>,
    pub elements: Vec<GraceElementSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraceElementSyntax {
    Note(NoteSyntax),
    Chord(ChordSyntax),
    Rest(RestSyntax),
    Malformed(MalformedSyntax),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotedTextSyntax {
    pub span: Span,
    pub content_span: Span,
    pub text: String,
    pub kind: QuotedTextKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotedTextKind {
    ChordSymbol,
    Annotation(AnnotationPlacement),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationPlacement {
    Above,
    Below,
    Left,
    Right,
    Free,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecorationSyntax {
    pub span: Span,
    pub name_span: Span,
    pub name: String,
    pub kind: DecorationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationKind {
    Named,
    LegacyNamed,
    Shorthand,
    UserDefined,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TupletSyntax {
    pub span: Span,
    pub p: SpannedNumber,
    pub q: Option<SpannedNumber>,
    pub r: Option<SpannedNumber>,
}

impl TupletSyntax {
    fn q_value(&self) -> u32 {
        self.q
            .map(|q| q.value)
            .unwrap_or_else(|| default_tuplet_q(self.p.value))
    }

    fn r_value(&self) -> u32 {
        self.r.map(|r| r.value).unwrap_or(self.p.value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlurSyntax {
    pub span: Span,
    pub dotted: bool,
    pub direction: SlurDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlurDirection {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TieSyntax {
    pub span: Span,
    pub dotted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokenRhythmSyntax {
    pub span: Span,
    pub direction: BrokenRhythmDirection,
    pub count: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlaySyntax {
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokenRhythmDirection {
    LeftShorter,
    RightShorter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantEndingSyntax {
    pub span: Span,
    pub shorthand: bool,
    pub endings: Vec<VariantEndingPart>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantEndingPart {
    Single(SpannedNumber),
    Range {
        start: SpannedNumber,
        end: SpannedNumber,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarlineSyntax {
    pub span: Span,
    pub kind: BarlineKind,
    pub dotted: bool,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineFieldSyntax {
    pub span: Span,
    pub marker_span: Span,
    pub code: char,
    pub value: Spanned<String>,
    pub voice: Option<Spanned<VoiceDefinition>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicFieldLine {
    pub line_index: usize,
    pub code: char,
    pub line_span: Span,
    pub marker_span: Span,
    pub value: Spanned<String>,
    pub kind: MusicFieldLineKind,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MusicFieldLineKind {
    Voice(Spanned<VoiceDefinition>),
    Lyric(LyricLineSyntax),
    Symbol(SymbolLineSyntax),
    PostTuneText(Spanned<String>),
    Score(ScoreDirective),
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricLineSyntax {
    pub line_index: usize,
    pub span: Span,
    pub value: Spanned<String>,
    pub tokens: Vec<LyricTokenSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricTokenSyntax {
    pub span: Span,
    pub text: String,
    pub kind: LyricTokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LyricTokenKind {
    Syllable,
    Hyphen,
    Extender,
    Skip,
    Bar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolLineSyntax {
    pub line_index: usize,
    pub span: Span,
    pub value: Spanned<String>,
    pub tokens: Vec<SymbolTokenSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolTokenSyntax {
    pub span: Span,
    pub text: String,
    pub kind: SymbolTokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolTokenKind {
    Decoration,
    ChordSymbol,
    Annotation,
    Raw,
    Skip,
    Bar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreDirectiveSyntax {
    pub line_index: usize,
    pub span: Span,
    pub marker_span: Span,
    pub name_span: Span,
    pub value: Spanned<String>,
    pub directive: ScoreDirective,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedSyntax {
    pub span: Span,
    pub kind: UnsupportedSyntaxKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedSyntaxKind {
    Chord,
    GraceGroup,
    Tuplet,
    Slur,
    Tie,
    Decoration,
    BrokenRhythm,
    RepeatEnding,
    QuotedText,
    Reserved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MalformedSyntax {
    pub span: Span,
    pub kind: MalformedSyntaxKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MalformedSyntaxKind {
    DanglingAccidental,
    StandaloneOctave,
    StandaloneLength,
    InvalidLength,
    UnclosedInlineField,
    UnclosedChord,
    UnclosedGraceGroup,
    UnclosedQuotedText,
    UnclosedSlur,
    UnclosedDecoration,
    InvalidBarline,
    InvalidTuplet,
    InvalidRepeatEnding,
    InvalidDecoration,
    UnknownToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredMusic {
    pub events: Vec<Event>,
    pub divisions: u32,
    pub voices: Vec<VoiceTimeline>,
    pub score_directives: Vec<ScoreDirectiveModel>,
    pub post_tune_lyrics: Vec<TextLine>,
}

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
                    MusicFieldLineKind::PostTuneText(_) => tune.body_fields.push(field_line),
                    MusicFieldLineKind::Voice(_)
                    | MusicFieldLineKind::Lyric(_)
                    | MusicFieldLineKind::Symbol(_)
                    | MusicFieldLineKind::Other => {}
                }
            }
            if matches!(
                line.context,
                LineContext::TuneHeader { .. } | LineContext::TuneBody { .. }
            ) && line.kind == LineKind::StylesheetDirective
                && let Some((tune_index, directive)) =
                    parse_score_stylesheet_directive(source, line)
                && let Some(tune) = tunes.iter_mut().find(|tune| tune.tune_index == tune_index)
            {
                tune.score_directives.push(directive);
            }
            continue;
        };

        let Some(tune) = tunes.iter_mut().find(|tune| tune.tune_index == tune_index) else {
            continue;
        };

        if line.kind == LineKind::InformationField {
            if let Some(field_line) = music_field_for_line(fields, line) {
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
                    MusicFieldLineKind::Voice(_)
                    | MusicFieldLineKind::PostTuneText(_)
                    | MusicFieldLineKind::Other => {}
                }
                tune.body_fields.push(field_line);
            }
            continue;
        }

        if line.kind == LineKind::StylesheetDirective {
            if let Some((_, directive)) = parse_score_stylesheet_directive(source, line) {
                tune.score_directives.push(directive);
            }
            continue;
        }

        if line.kind != LineKind::MusicCode {
            continue;
        }
        let Some(line_text) = source.slice(line.text_span) else {
            continue;
        };

        let code_span = music_code_span(line);
        let Some(code_text) = source.slice(code_span) else {
            continue;
        };

        let dialect = fields
            .tune(tune_index)
            .map(|tune| tune.current.dialect.clone())
            .unwrap_or_else(|| DialectState::from_options(Default::default()));
        let mut parser = MusicLineParser::new(code_text, code_span.start, dialect);
        let mut parsed_line = parser.parse(line.index, line.span, code_span);
        diagnostics.extend(parser.diagnostics);

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
        tune.lines.push(parsed_line);
    }

    ParseReport::new(ParsedMusicDocument { tunes }, diagnostics)
}

fn music_field_for_line(
    fields: &ParsedAbcFields,
    line: &crate::surface::ClassifiedLine,
) -> Option<MusicFieldLine> {
    let field = fields
        .fields
        .iter()
        .find(|field| field.line_index == line.index)?;
    let value = match &field.kind {
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
        | ParsedFieldKind::TextMetadata(crate::fields::TextMetadataField { value, .. }) => {
            value.clone()
        }
        ParsedFieldKind::Interpretation(InterpretationField::Score { directive }) => {
            directive.value.clone()
        }
        _ => Spanned::new(String::new(), field.parsed_value_span),
    };
    let kind = match &field.kind {
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

fn parse_score_stylesheet_directive(
    source: &SourceText,
    line: &crate::surface::ClassifiedLine,
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
    if !name.eq_ignore_ascii_case("score") {
        return None;
    }
    let name_span = Span::new(name_start, name_start + name.len());
    let value_start = name_span.end;
    let value_text = &text[value_start.saturating_sub(line.content_span.start)..];
    let value = trim_spanned_string(value_text, value_start);
    let directive = crate::fields::parse_score_directive(value.clone());
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

fn parse_lyric_line(line_index: usize, span: Span, value: Spanned<String>) -> LyricLineSyntax {
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
        if ch.is_whitespace() {
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
                    tokens.push(LyricTokenSyntax {
                        span: Span::new(offset + index, offset + index + 1),
                        text: "-".to_owned(),
                        kind: LyricTokenKind::Syllable,
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

fn parse_symbol_line(line_index: usize, span: Span, value: Spanned<String>) -> SymbolLineSyntax {
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

fn trim_spanned_string(value: &str, offset: usize) -> Spanned<String> {
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

pub(crate) fn lower_tune_music(
    tune_music: &ParsedTuneMusic,
    field_state: &FieldState,
) -> ParseReport<LoweredMusic> {
    let unit = field_state.unit_note_length_fraction();
    let meter_duration = field_state
        .meter
        .as_ref()
        .and_then(|meter| meter_duration(&meter.value.kind));
    let mut lowering = MultiVoiceLowering::new(unit, field_state, tune_music.span);
    let mut entries = Vec::new();
    for (index, line) in tune_music.lines.iter().enumerate() {
        entries.push((line.line_index, 1usize, index));
    }
    for (index, field) in tune_music.body_fields.iter().enumerate() {
        entries.push((field.line_index, 0usize, index));
    }
    entries.sort_unstable();

    for (_line_index, kind, index) in entries {
        if kind == 0 {
            lowering.apply_field(&tune_music.body_fields[index]);
        } else {
            lowering.apply_music_line(&tune_music.lines[index], meter_duration);
        }
    }

    let lyric_lines = lowering.lyric_lines.clone();
    let symbol_lines = lowering.symbol_lines.clone();
    let mut diagnostics = lowering.finish_open_constructs();
    let all_lowered = lowering
        .voices
        .iter()
        .flat_map(|voice| voice.lowered.iter())
        .collect::<Vec<_>>();
    let divisions = all_lowered.iter().fold(8, |divisions, event| match event {
        LoweredEvent::Timed(timed) => lcm(divisions, timed.event.duration.divisions_requirement()),
        LoweredEvent::Untimed(_) | LoweredEvent::Overlay(_) => divisions,
    });
    let events = all_lowered
        .into_iter()
        .filter_map(|event| match event {
            LoweredEvent::Timed(timed) => Some(timed.event.into_event(divisions)),
            LoweredEvent::Untimed(event) => Some(event.clone()),
            LoweredEvent::Overlay(_) => None,
        })
        .collect();
    let mut voices = lowering.into_voice_timelines(meter_duration, &mut diagnostics);
    align_lyrics(&mut voices, &lyric_lines, &mut diagnostics);
    align_symbols(&mut voices, &symbol_lines, &mut diagnostics);
    let score_directives = tune_music
        .score_directives
        .iter()
        .map(score_directive_model)
        .collect();
    let post_tune_lyrics = tune_music
        .body_fields
        .iter()
        .filter_map(|field| match &field.kind {
            MusicFieldLineKind::PostTuneText(text) => Some(TextLine {
                text: text.value.clone(),
                span: text.span,
            }),
            _ => None,
        })
        .collect();

    ParseReport::new(
        LoweredMusic {
            events,
            divisions,
            voices,
            score_directives,
            post_tune_lyrics,
        },
        diagnostics,
    )
}

struct MultiVoiceLowering {
    unit: Fraction,
    voices: Vec<LoweringState>,
    current_voice: String,
    source_order: u32,
    lyric_lines: Vec<VoicedLyricLine>,
    symbol_lines: Vec<VoicedSymbolLine>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VoicedLyricLine {
    voice_id: String,
    line: LyricLineSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VoicedSymbolLine {
    voice_id: String,
    line: SymbolLineSyntax,
}

impl MultiVoiceLowering {
    fn new(unit: Fraction, field_state: &FieldState, fallback_span: Span) -> Self {
        let mut lowering = Self {
            unit,
            voices: Vec::new(),
            current_voice: String::new(),
            source_order: 0,
            lyric_lines: Vec::new(),
            symbol_lines: Vec::new(),
            diagnostics: Vec::new(),
        };

        for voice in &field_state.voices {
            lowering.ensure_voice(voice.clone());
        }

        let initial = field_state
            .voices
            .first()
            .cloned()
            .or_else(|| field_state.voice.clone())
            .unwrap_or_else(|| default_voice_definition(fallback_span));
        lowering.current_voice = initial.value.id.value.clone();
        lowering.ensure_voice(initial);
        lowering
    }

    fn apply_field(&mut self, field: &MusicFieldLine) {
        match &field.kind {
            MusicFieldLineKind::Voice(voice) => self.switch_voice(voice.clone()),
            MusicFieldLineKind::Lyric(line) => self.lyric_lines.push(VoicedLyricLine {
                voice_id: self.current_voice.clone(),
                line: line.clone(),
            }),
            MusicFieldLineKind::Symbol(line) => self.symbol_lines.push(VoicedSymbolLine {
                voice_id: self.current_voice.clone(),
                line: line.clone(),
            }),
            MusicFieldLineKind::PostTuneText(_)
            | MusicFieldLineKind::Score(_)
            | MusicFieldLineKind::Other => {}
        }
    }

    fn apply_music_line(&mut self, line: &MusicLine, meter_duration: Option<Fraction>) {
        for item in &line.items {
            match item {
                MusicItem::Note(note) => {
                    let source_order = self.next_source_order();
                    self.current_state()
                        .push_note_group(note, line.line_index, source_order);
                }
                MusicItem::Rest(rest) => {
                    let source_order = self.next_source_order();
                    self.current_state()
                        .push_rest_group(rest, line.line_index, source_order);
                }
                MusicItem::MultiMeasureRest(rest) => {
                    let count = rest.count.map(|count| count.value).unwrap_or(1);
                    let duration = if let Some(meter_duration) = meter_duration {
                        meter_duration.checked_mul_u32(count)
                    } else {
                        self.current_state()
                            .diagnostics
                            .push(free_meter_multirest_warning(rest.span));
                        self.unit.checked_mul_u32(count)
                    };
                    let source_order = self.next_source_order();
                    self.current_state().push_time_group(
                        vec![(
                            TimedEvent {
                                kind: TimedEventKind::Rest {
                                    visibility: rest.visibility,
                                    span: rest.span,
                                },
                                duration,
                            },
                            false,
                        )],
                        line.line_index,
                        source_order,
                    );
                }
                MusicItem::Spacer(spacer) => self
                    .current_state()
                    .lowered
                    .push(LoweredEvent::Untimed(Event::Spacer { span: spacer.span })),
                MusicItem::Chord(chord) => {
                    let source_order = self.next_source_order();
                    self.current_state()
                        .push_chord_group(chord, line.line_index, source_order);
                }
                MusicItem::BrokenRhythm(marker) => {
                    self.current_state().apply_broken_rhythm(*marker)
                }
                MusicItem::Tuplet(tuplet) => self.current_state().start_tuplet(tuplet),
                MusicItem::Slur(slur) => self.current_state().apply_slur(*slur),
                MusicItem::Overlay(overlay) => self
                    .current_state()
                    .lowered
                    .push(LoweredEvent::Overlay(*overlay)),
                MusicItem::Barline(barline) => {
                    if matches!(barline.kind, BarlineKind::Dotted | BarlineKind::Invisible) {
                        self.current_state()
                            .diagnostics
                            .push(barline_export_policy_info(barline.span, barline.kind));
                    }
                    self.current_state()
                        .lowered
                        .push(LoweredEvent::Untimed(Event::Barline {
                            kind: barline.kind,
                            span: barline.span,
                        }));
                }
                MusicItem::InlineField(inline) if inline.code == 'V' => {
                    if let Some(voice) = &inline.voice {
                        self.switch_voice(voice.clone());
                    }
                }
                MusicItem::GraceGroup(_)
                | MusicItem::ChordSymbol(_)
                | MusicItem::Annotation(_)
                | MusicItem::Decoration(_)
                | MusicItem::Tie(_)
                | MusicItem::VariantEnding(_)
                | MusicItem::InlineField(_)
                | MusicItem::Unsupported(_)
                | MusicItem::Malformed(_) => {}
            }
        }
    }

    fn switch_voice(&mut self, voice: Spanned<VoiceDefinition>) {
        self.current_voice = voice.value.id.value.clone();
        self.ensure_voice(voice);
    }

    fn next_source_order(&mut self) -> u32 {
        let order = self.source_order;
        self.source_order = self.source_order.saturating_add(1);
        order
    }

    fn ensure_voice(&mut self, voice: Spanned<VoiceDefinition>) -> usize {
        if let Some(index) = self
            .voices
            .iter()
            .position(|state| state.id.value == voice.value.id.value)
        {
            self.voices[index].properties = voice_properties_model(&voice.value);
            self.voices[index].source_span = voice.span;
            return index;
        }
        let id = VoiceId {
            value: voice.value.id.value.clone(),
            span: voice.value.id.span,
        };
        let properties = voice_properties_model(&voice.value);
        self.voices
            .push(LoweringState::new(id, properties, self.unit));
        self.voices.len() - 1
    }

    fn current_state(&mut self) -> &mut LoweringState {
        let index = self
            .voices
            .iter()
            .position(|state| state.id.value == self.current_voice)
            .unwrap_or_else(|| {
                let voice = default_voice_definition(Span::new(0, 0));
                self.current_voice = voice.value.id.value.clone();
                self.ensure_voice(voice)
            });
        &mut self.voices[index]
    }

    fn finish_open_constructs(&mut self) -> Vec<Diagnostic> {
        let mut diagnostics = std::mem::take(&mut self.diagnostics);
        for voice in &mut self.voices {
            voice.finish_open_constructs();
            diagnostics.extend(std::mem::take(&mut voice.diagnostics));
        }
        diagnostics
    }

    fn into_voice_timelines(
        self,
        meter_duration: Option<Fraction>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<VoiceTimeline> {
        let mut voices = self
            .voices
            .into_iter()
            .map(|voice| build_voice_timeline(voice, meter_duration, diagnostics))
            .collect::<Vec<_>>();
        if voices.len() > 1 {
            voices.retain(|voice| {
                voice_has_timeline_content(voice)
                    || voice.properties != VoicePropertiesModel::default()
                    || voice.id.value != "1"
            });
        }
        voices
    }
}

fn voice_has_timeline_content(voice: &VoiceTimeline) -> bool {
    voice.measures.iter().any(|measure| {
        !measure.overlays.is_empty()
            || measure.events.iter().any(|event| {
                matches!(
                    event.kind,
                    TimelineEventKind::Note { .. } | TimelineEventKind::Rest { .. }
                )
            })
    })
}

fn default_voice_definition(span: Span) -> Spanned<VoiceDefinition> {
    let id = Spanned::new("1".to_owned(), span);
    Spanned::new(
        VoiceDefinition {
            id: id.clone(),
            properties: Spanned::new(String::new(), Span::new(span.end, span.end)),
            parsed_properties: Default::default(),
        },
        span,
    )
}

fn voice_properties_model(voice: &VoiceDefinition) -> VoicePropertiesModel {
    VoicePropertiesModel {
        name: voice
            .parsed_properties
            .name
            .as_ref()
            .map(text_line_from_spanned),
        nm: voice
            .parsed_properties
            .nm
            .as_ref()
            .map(text_line_from_spanned),
        subname: voice
            .parsed_properties
            .subname
            .as_ref()
            .map(text_line_from_spanned),
        snm: voice
            .parsed_properties
            .snm
            .as_ref()
            .map(text_line_from_spanned),
        clef: voice
            .parsed_properties
            .clef
            .as_ref()
            .map(text_line_from_spanned),
        stem: voice
            .parsed_properties
            .stem
            .as_ref()
            .map(|stem| match stem.value {
                StemDirection::Up => StemDirectionModel::Up,
                StemDirection::Down => StemDirectionModel::Down,
            }),
        octave: voice
            .parsed_properties
            .octave
            .as_ref()
            .map(text_line_from_spanned),
        transpose: voice
            .parsed_properties
            .transpose
            .as_ref()
            .map(text_line_from_spanned),
    }
}

fn text_line_from_spanned(value: &Spanned<String>) -> TextLine {
    TextLine {
        text: value.value.clone(),
        span: value.span,
    }
}

fn build_voice_timeline(
    voice: LoweringState,
    meter_duration: Option<Fraction>,
    diagnostics: &mut Vec<Diagnostic>,
) -> VoiceTimeline {
    let mut builder = VoiceTimelineBuilder::new(voice.id.clone(), meter_duration);
    for event in voice.lowered {
        builder.push(event, diagnostics);
    }
    let measures = builder.finish(diagnostics);
    VoiceTimeline {
        id: voice.id,
        properties: voice.properties,
        measures,
        source_span: voice.source_span,
    }
}

struct VoiceTimelineBuilder {
    voice_id: VoiceId,
    meter_duration: Option<Fraction>,
    measures: Vec<VoiceMeasureTimeline>,
    measure_index: u32,
    onset: Fraction,
    active_overlay: Option<OverlayBuilder>,
    overlay_count: u32,
}

impl VoiceTimelineBuilder {
    fn new(voice_id: VoiceId, meter_duration: Option<Fraction>) -> Self {
        Self {
            voice_id,
            meter_duration,
            measures: vec![VoiceMeasureTimeline {
                index: 0,
                span: Span::new(0, 0),
                events: Vec::new(),
                overlays: Vec::new(),
            }],
            measure_index: 0,
            onset: Fraction::zero(),
            active_overlay: None,
            overlay_count: 0,
        }
    }

    fn push(&mut self, event: LoweredEvent, diagnostics: &mut Vec<Diagnostic>) {
        match event {
            LoweredEvent::Timed(timed) => self.push_timed(timed),
            LoweredEvent::Untimed(Event::Barline { kind, span }) => {
                self.finish_overlay(diagnostics);
                let onset = self.onset;
                let measure = self.current_measure_mut();
                measure.events.push(VoiceTimedEvent {
                    onset,
                    duration: Fraction::zero(),
                    span,
                    line_index: 0,
                    source_order: 0,
                    alignable: false,
                    kind: TimelineEventKind::Barline { kind },
                    lyrics: Vec::new(),
                    symbols: Vec::new(),
                });
                measure.span = extend_span(measure.span, span);
                self.measure_index = self.measure_index.saturating_add(1);
                self.onset = Fraction::zero();
                self.measures.push(VoiceMeasureTimeline {
                    index: self.measure_index,
                    span: Span::new(span.end, span.end),
                    events: Vec::new(),
                    overlays: Vec::new(),
                });
            }
            LoweredEvent::Untimed(Event::Spacer { span }) => {
                let onset = self.onset;
                self.current_measure_mut().events.push(VoiceTimedEvent {
                    onset,
                    duration: Fraction::zero(),
                    span,
                    line_index: 0,
                    source_order: 0,
                    alignable: false,
                    kind: TimelineEventKind::Spacer,
                    lyrics: Vec::new(),
                    symbols: Vec::new(),
                });
            }
            LoweredEvent::Untimed(Event::Note { .. } | Event::Rest { .. }) => {}
            LoweredEvent::Overlay(overlay) => {
                self.finish_overlay(diagnostics);
                let expected_duration = if self.onset == Fraction::zero() {
                    self.meter_duration.unwrap_or_else(Fraction::zero)
                } else {
                    self.onset
                };
                let overlay_id = VoiceId {
                    value: format!("{}.overlay{}", self.voice_id.value, self.overlay_count + 1),
                    span: overlay.span,
                };
                self.overlay_count = self.overlay_count.saturating_add(1);
                self.active_overlay = Some(OverlayBuilder {
                    id: overlay_id,
                    start_span: overlay.span,
                    span: overlay.span,
                    measure_index: self.measure_index,
                    expected_duration,
                    actual_duration: Fraction::zero(),
                    events: Vec::new(),
                });
            }
        }
    }

    fn push_timed(&mut self, timed: LoweredTimedEvent) {
        let span = timed_span(timed.event);
        let event = VoiceTimedEvent {
            onset: self
                .active_overlay
                .as_ref()
                .map(|overlay| overlay.actual_duration)
                .unwrap_or(self.onset),
            duration: timed.event.duration,
            span,
            line_index: timed.line_index,
            source_order: timed.source_order,
            alignable: timed.alignable && matches!(timed.event.kind, TimedEventKind::Note { .. }),
            kind: timeline_event_kind(timed.event.kind),
            lyrics: Vec::new(),
            symbols: Vec::new(),
        };
        if let Some(overlay) = &mut self.active_overlay {
            overlay.actual_duration = overlay.actual_duration.checked_add(timed.event.duration);
            overlay.span = extend_span(overlay.span, span);
            overlay.events.push(event);
        } else {
            self.onset = self.onset.checked_add(timed.event.duration);
            self.current_measure_mut().span = extend_span(self.current_measure_mut().span, span);
            self.current_measure_mut().events.push(event);
        }
    }

    fn finish(mut self, diagnostics: &mut Vec<Diagnostic>) -> Vec<VoiceMeasureTimeline> {
        self.finish_overlay(diagnostics);
        while self
            .measures
            .last()
            .is_some_and(|measure| measure.events.is_empty() && measure.overlays.is_empty())
            && self.measures.len() > 1
        {
            self.measures.pop();
        }
        self.measures
    }

    fn finish_overlay(&mut self, diagnostics: &mut Vec<Diagnostic>) {
        let Some(overlay) = self.active_overlay.take() else {
            return;
        };
        if overlay.actual_duration.less_than(overlay.expected_duration) {
            diagnostics.push(overlay_incomplete_measure_warning(
                overlay.start_span,
                overlay.actual_duration,
                overlay.expected_duration,
            ));
        } else if overlay.expected_duration.less_than(overlay.actual_duration) {
            diagnostics.push(overlay_overfull_measure_warning(
                overlay.start_span,
                overlay.actual_duration,
                overlay.expected_duration,
            ));
        }
        self.current_measure_mut().overlays.push(OverlaySegment {
            id: overlay.id,
            span: overlay.span,
            measure_index: overlay.measure_index,
            expected_duration: overlay.expected_duration,
            actual_duration: overlay.actual_duration,
            events: overlay.events,
        });
    }

    fn current_measure_mut(&mut self) -> &mut VoiceMeasureTimeline {
        self.measures
            .last_mut()
            .expect("timeline builder always has a current measure")
    }
}

struct OverlayBuilder {
    id: VoiceId,
    start_span: Span,
    span: Span,
    measure_index: u32,
    expected_duration: Fraction,
    actual_duration: Fraction,
    events: Vec<VoiceTimedEvent>,
}

fn timed_span(event: TimedEvent) -> Span {
    match event.kind {
        TimedEventKind::Note { span, .. } | TimedEventKind::Rest { span, .. } => span,
    }
}

fn timeline_event_kind(kind: TimedEventKind) -> TimelineEventKind {
    match kind {
        TimedEventKind::Note {
            step,
            octave,
            accidental,
            chord,
            ..
        } => TimelineEventKind::Note {
            step,
            octave,
            accidental,
            chord,
        },
        TimedEventKind::Rest { visibility, .. } => TimelineEventKind::Rest { visibility },
    }
}

fn extend_span(current: Span, next: Span) -> Span {
    if current.is_empty() {
        return next;
    }
    Span::new(current.start.min(next.start), current.end.max(next.end))
}

fn score_directive_model(syntax: &ScoreDirectiveSyntax) -> ScoreDirectiveModel {
    ScoreDirectiveModel {
        span: syntax.span,
        value: TextLine {
            text: syntax.value.value.clone(),
            span: syntax.value.span,
        },
        tokens: syntax
            .directive
            .tokens
            .iter()
            .map(|token| ScoreDirectiveTokenModel {
                span: token.span,
                kind: match &token.kind {
                    crate::fields::ScoreDirectiveTokenKind::Voice(id) => {
                        ScoreDirectiveTokenKindModel::Voice(id.clone())
                    }
                    crate::fields::ScoreDirectiveTokenKind::GroupStart(ch) => {
                        ScoreDirectiveTokenKindModel::GroupStart(*ch)
                    }
                    crate::fields::ScoreDirectiveTokenKind::GroupEnd(ch) => {
                        ScoreDirectiveTokenKindModel::GroupEnd(*ch)
                    }
                    crate::fields::ScoreDirectiveTokenKind::StaffSeparator => {
                        ScoreDirectiveTokenKindModel::StaffSeparator
                    }
                    crate::fields::ScoreDirectiveTokenKind::MeasureSeparator => {
                        ScoreDirectiveTokenKindModel::MeasureSeparator
                    }
                    crate::fields::ScoreDirectiveTokenKind::FloatingVoiceMarker => {
                        ScoreDirectiveTokenKindModel::FloatingVoiceMarker
                    }
                },
            })
            .collect(),
    }
}

#[derive(Debug, Clone, Copy)]
struct AlignableRef {
    measure_index: usize,
    event_index: usize,
    line_index: usize,
    source_order: u32,
    measure_number: u32,
}

fn align_lyrics(
    voices: &mut [VoiceTimeline],
    lyric_lines: &[VoicedLyricLine],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for voice in voices {
        let voice_lines = lyric_lines
            .iter()
            .filter(|line| line.voice_id == voice.id.value)
            .map(|line| &line.line)
            .collect::<Vec<_>>();
        align_lyrics_for_voice(voice, &voice_lines, diagnostics);
    }
}

fn align_lyrics_for_voice(
    voice: &mut VoiceTimeline,
    lyric_lines: &[&LyricLineSyntax],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let refs = alignable_refs(voice);
    let mut cursor = 0usize;
    let mut block_start = 0usize;
    let mut block_available_end = 0usize;
    let mut previous_line = None;
    let mut verse = 1u32;

    for line in lyric_lines {
        let available_end = refs
            .iter()
            .take_while(|reference| reference.line_index < line.line_index)
            .count();
        let adjacent = previous_line.is_some_and(|previous| previous + 1 == line.line_index);
        let (start, end, line_verse) = if adjacent {
            verse = verse.saturating_add(1);
            (block_start, block_available_end, verse)
        } else {
            verse = 1;
            block_start = cursor;
            block_available_end = available_end;
            cursor = available_end;
            (block_start, block_available_end, verse)
        };

        align_lyric_line(voice, &refs, start, end, line_verse, line, diagnostics);
        previous_line = Some(line.line_index);
    }
}

fn align_lyric_line(
    voice: &mut VoiceTimeline,
    refs: &[AlignableRef],
    start: usize,
    end: usize,
    verse: u32,
    line: &LyricLineSyntax,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut position = start;
    for token in &line.tokens {
        match token.kind {
            LyricTokenKind::Syllable => {
                if let Some(reference) = refs.get(position).copied().filter(|_| position < end) {
                    attach_lyric(
                        voice,
                        reference,
                        AlignedLyric {
                            verse,
                            text: token.text.clone(),
                            span: token.span,
                            control: LyricControl::Syllable,
                        },
                    );
                    position += 1;
                } else {
                    diagnostics.push(lyric_syllable_count_warning(token.span));
                }
            }
            LyricTokenKind::Hyphen => {
                if position > start
                    && let Some(reference) = refs.get(position - 1).copied()
                {
                    attach_lyric(
                        voice,
                        reference,
                        AlignedLyric {
                            verse,
                            text: token.text.clone(),
                            span: token.span,
                            control: LyricControl::Hyphen,
                        },
                    );
                }
            }
            LyricTokenKind::Extender => {
                if let Some(reference) = refs.get(position).copied().filter(|_| position < end) {
                    attach_lyric(
                        voice,
                        reference,
                        AlignedLyric {
                            verse,
                            text: String::new(),
                            span: token.span,
                            control: LyricControl::Extender,
                        },
                    );
                    position += 1;
                }
            }
            LyricTokenKind::Skip => {
                position = position.saturating_add(1).min(end);
            }
            LyricTokenKind::Bar => {
                if bar_marker_is_at_boundary(refs, position) {
                    continue;
                }
                if let Some(current) = refs.get(position) {
                    let measure = current.measure_number;
                    while position < end
                        && refs
                            .get(position)
                            .is_some_and(|reference| reference.measure_number == measure)
                    {
                        position += 1;
                    }
                }
            }
        }
    }
}

fn align_symbols(
    voices: &mut [VoiceTimeline],
    symbol_lines: &[VoicedSymbolLine],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for voice in voices {
        let voice_lines = symbol_lines
            .iter()
            .filter(|line| line.voice_id == voice.id.value)
            .map(|line| &line.line)
            .collect::<Vec<_>>();
        align_symbols_for_voice(voice, &voice_lines, diagnostics);
    }
}

fn align_symbols_for_voice(
    voice: &mut VoiceTimeline,
    symbol_lines: &[&SymbolLineSyntax],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let refs = alignable_refs(voice);
    let mut cursor = 0usize;
    let mut block_start = 0usize;
    let mut block_available_end = 0usize;
    let mut previous_line = None;
    let mut layer = 1u32;

    for line in symbol_lines {
        let available_end = refs
            .iter()
            .take_while(|reference| reference.line_index < line.line_index)
            .count();
        let adjacent = previous_line.is_some_and(|previous| previous + 1 == line.line_index);
        let (start, end, line_layer) = if adjacent {
            layer = layer.saturating_add(1);
            (block_start, block_available_end, layer)
        } else {
            layer = 1;
            block_start = cursor;
            block_available_end = available_end;
            cursor = available_end;
            (block_start, block_available_end, layer)
        };
        align_symbol_line(voice, &refs, start, end, line_layer, line, diagnostics);
        previous_line = Some(line.line_index);
    }
}

fn align_symbol_line(
    voice: &mut VoiceTimeline,
    refs: &[AlignableRef],
    start: usize,
    end: usize,
    layer: u32,
    line: &SymbolLineSyntax,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut position = start;
    for token in &line.tokens {
        match token.kind {
            SymbolTokenKind::Skip => {
                position = position.saturating_add(1).min(end);
            }
            SymbolTokenKind::Bar => {
                if bar_marker_is_at_boundary(refs, position) {
                    continue;
                }
                if let Some(current) = refs.get(position) {
                    let measure = current.measure_number;
                    while position < end
                        && refs
                            .get(position)
                            .is_some_and(|reference| reference.measure_number == measure)
                    {
                        position += 1;
                    }
                }
            }
            SymbolTokenKind::Decoration
            | SymbolTokenKind::ChordSymbol
            | SymbolTokenKind::Annotation
            | SymbolTokenKind::Raw => {
                if let Some(reference) = refs.get(position).copied().filter(|_| position < end) {
                    attach_symbol(
                        voice,
                        reference,
                        AlignedSymbol {
                            layer,
                            text: token.text.clone(),
                            span: token.span,
                            kind: match token.kind {
                                SymbolTokenKind::Decoration => AlignedSymbolKind::Decoration,
                                SymbolTokenKind::ChordSymbol => AlignedSymbolKind::ChordSymbol,
                                SymbolTokenKind::Annotation => AlignedSymbolKind::Annotation,
                                SymbolTokenKind::Raw => AlignedSymbolKind::Raw,
                                SymbolTokenKind::Skip | SymbolTokenKind::Bar => {
                                    AlignedSymbolKind::Raw
                                }
                            },
                        },
                    );
                    position += 1;
                } else {
                    diagnostics.push(symbol_count_warning(token.span));
                }
            }
        }
    }
}

fn alignable_refs(voice: &VoiceTimeline) -> Vec<AlignableRef> {
    let mut refs = Vec::new();
    for (measure_index, measure) in voice.measures.iter().enumerate() {
        for (event_index, event) in measure.events.iter().enumerate() {
            if event.alignable {
                refs.push(AlignableRef {
                    measure_index,
                    event_index,
                    line_index: event.line_index,
                    source_order: event.source_order,
                    measure_number: measure.index,
                });
            }
        }
    }
    refs.sort_by_key(|reference| reference.source_order);
    refs
}

fn bar_marker_is_at_boundary(refs: &[AlignableRef], position: usize) -> bool {
    position > 0
        && position < refs.len()
        && refs[position - 1].measure_number != refs[position].measure_number
}

fn attach_lyric(voice: &mut VoiceTimeline, reference: AlignableRef, lyric: AlignedLyric) {
    if let Some(event) = voice
        .measures
        .get_mut(reference.measure_index)
        .and_then(|measure| measure.events.get_mut(reference.event_index))
    {
        event.lyrics.push(lyric);
    }
}

fn attach_symbol(voice: &mut VoiceTimeline, reference: AlignableRef, symbol: AlignedSymbol) {
    if let Some(event) = voice
        .measures
        .get_mut(reference.measure_index)
        .and_then(|measure| measure.events.get_mut(reference.event_index))
    {
        event.symbols.push(symbol);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LoweredEvent {
    Timed(LoweredTimedEvent),
    Untimed(Event),
    Overlay(OverlaySyntax),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LoweredTimedEvent {
    event: TimedEvent,
    line_index: usize,
    source_order: u32,
    alignable: bool,
}

#[derive(Debug, Clone, Copy)]
struct ActiveTuplet {
    remaining: u32,
    multiplier: Fraction,
}

#[derive(Debug)]
struct LoweringState {
    id: VoiceId,
    properties: VoicePropertiesModel,
    source_span: Span,
    unit: Fraction,
    lowered: Vec<LoweredEvent>,
    time_groups: Vec<Vec<usize>>,
    diagnostics: Vec<Diagnostic>,
    active_tuplets: Vec<ActiveTuplet>,
    pending_broken: Option<(Fraction, Span)>,
    open_slurs: Vec<SlurSyntax>,
}

impl LoweringState {
    fn new(id: VoiceId, properties: VoicePropertiesModel, unit: Fraction) -> Self {
        let source_span = id.span;
        Self {
            id,
            properties,
            source_span,
            unit,
            lowered: Vec::new(),
            time_groups: Vec::new(),
            diagnostics: Vec::new(),
            active_tuplets: Vec::new(),
            pending_broken: None,
            open_slurs: Vec::new(),
        }
    }

    fn push_note_group(&mut self, note: &NoteSyntax, line_index: usize, source_order: u32) {
        self.push_time_group(
            vec![(
                TimedEvent {
                    kind: TimedEventKind::Note {
                        step: note.pitch.step.to_ascii_uppercase(),
                        octave: lowered_octave(note),
                        accidental: note.accidental.map(|accidental| accidental.sign),
                        chord: false,
                        span: note.span,
                    },
                    duration: self
                        .unit
                        .checked_mul(length_multiplier(note.length.as_ref())),
                },
                true,
            )],
            line_index,
            source_order,
        );
    }

    fn push_rest_group(&mut self, rest: &RestSyntax, line_index: usize, source_order: u32) {
        self.push_time_group(
            vec![(
                TimedEvent {
                    kind: TimedEventKind::Rest {
                        visibility: rest.visibility,
                        span: rest.span,
                    },
                    duration: self
                        .unit
                        .checked_mul(length_multiplier(rest.length.as_ref())),
                },
                false,
            )],
            line_index,
            source_order,
        );
    }

    fn push_chord_group(&mut self, chord: &ChordSyntax, line_index: usize, source_order: u32) {
        if chord.members.is_empty() {
            return;
        }

        let outer_multiplier = length_multiplier(chord.length.as_ref());
        let first_duration = chord.members.first().map(|member| {
            length_multiplier(member.note.length.as_ref()).checked_mul(outer_multiplier)
        });
        if let Some(first_duration) = first_duration
            && chord.members.iter().any(|member| {
                length_multiplier(member.note.length.as_ref()).checked_mul(outer_multiplier)
                    != first_duration
            })
        {
            self.diagnostics
                .push(variable_chord_duration_warning(chord.span));
        }

        let events = chord
            .members
            .iter()
            .enumerate()
            .map(|(index, member)| {
                let member_multiplier =
                    length_multiplier(member.note.length.as_ref()).checked_mul(outer_multiplier);
                (
                    TimedEvent {
                        kind: TimedEventKind::Note {
                            step: member.note.pitch.step.to_ascii_uppercase(),
                            octave: lowered_octave(&member.note),
                            accidental: member.note.accidental.map(|accidental| accidental.sign),
                            chord: index > 0,
                            span: member.note.span,
                        },
                        duration: self.unit.checked_mul(member_multiplier),
                    },
                    index == 0,
                )
            })
            .collect();
        self.push_time_group(events, line_index, source_order);
    }

    fn push_time_group(
        &mut self,
        events: Vec<(TimedEvent, bool)>,
        line_index: usize,
        source_order: u32,
    ) {
        if events.is_empty() {
            return;
        }

        let group_multiplier = self.consume_group_multiplier();
        let start_index = self.lowered.len();
        for (mut event, alignable) in events {
            event.duration = event.duration.checked_mul(group_multiplier);
            self.lowered.push(LoweredEvent::Timed(LoweredTimedEvent {
                event,
                line_index,
                source_order,
                alignable,
            }));
        }
        let group = (start_index..self.lowered.len()).collect::<Vec<_>>();
        self.time_groups.push(group);
    }

    fn consume_group_multiplier(&mut self) -> Fraction {
        let mut multiplier = Fraction::one();
        if let Some((broken_multiplier, _span)) = self.pending_broken.take() {
            multiplier = multiplier.checked_mul(broken_multiplier);
        }

        for tuplet in &mut self.active_tuplets {
            if tuplet.remaining > 0 {
                multiplier = multiplier.checked_mul(tuplet.multiplier);
                tuplet.remaining -= 1;
            }
        }
        self.active_tuplets.retain(|tuplet| tuplet.remaining > 0);
        multiplier
    }

    fn apply_broken_rhythm(&mut self, marker: BrokenRhythmSyntax) {
        let (left_multiplier, right_multiplier) = broken_rhythm_multipliers(marker);
        let Some(group) = self.time_groups.last() else {
            self.diagnostics
                .push(broken_rhythm_without_left_warning(marker.span));
            self.pending_broken = Some((right_multiplier, marker.span));
            return;
        };

        for index in group {
            if let Some(LoweredEvent::Timed(timed)) = self.lowered.get_mut(*index) {
                timed.event.duration = timed.event.duration.checked_mul(left_multiplier);
            }
        }
        if self
            .pending_broken
            .replace((right_multiplier, marker.span))
            .is_some()
        {
            self.diagnostics
                .push(overlapping_broken_rhythm_warning(marker.span));
        }
    }

    fn start_tuplet(&mut self, tuplet: &TupletSyntax) {
        let p = tuplet.p.value;
        let q = tuplet.q_value();
        let r = tuplet.r_value();
        if !(2..=9).contains(&p) || q == 0 || r == 0 {
            self.diagnostics.push(invalid_tuplet_warning(tuplet.span));
            return;
        }
        self.active_tuplets.push(ActiveTuplet {
            remaining: r,
            multiplier: Fraction::new(q, p),
        });
    }

    fn apply_slur(&mut self, slur: SlurSyntax) {
        match slur.direction {
            SlurDirection::Start => self.open_slurs.push(slur),
            SlurDirection::End => {
                if self.open_slurs.pop().is_none() {
                    self.diagnostics.push(unmatched_slur_warning(slur.span));
                }
            }
        }
    }

    fn finish_open_constructs(&mut self) {
        if let Some((_multiplier, span)) = self.pending_broken.take() {
            self.diagnostics
                .push(broken_rhythm_without_right_warning(span));
        }
        for slur in std::mem::take(&mut self.open_slurs) {
            self.diagnostics.push(unclosed_slur_warning(slur.span));
        }
    }
}

fn music_code_span(line: &crate::surface::ClassifiedLine) -> Span {
    let mut end = line.text_span.end;
    if let Some(comment_span) = line.trailing_comment {
        end = end.min(comment_span.start);
    }
    if let ScoreLineBreak::Suppressed { marker_span } = line.score_line_break {
        end = end.min(marker_span.start);
    }
    Span::new(line.text_span.start, end)
}

fn lowered_octave(note: &NoteSyntax) -> i8 {
    let base_octave = if note.pitch.step.is_ascii_lowercase() {
        5
    } else {
        4
    };
    let adjustment = note
        .octave_marks
        .iter()
        .map(|mark| match mark.mark {
            OctaveMark::Lower => -1,
            OctaveMark::Raise => 1,
        })
        .sum::<i8>();
    base_octave + adjustment
}

fn length_multiplier(length: Option<&LengthSyntax>) -> Fraction {
    length
        .map(|length| length.multiplier)
        .unwrap_or_else(Fraction::one)
}

fn meter_duration(kind: &MeterKind) -> Option<Fraction> {
    match kind {
        MeterKind::CommonTime => Some(Fraction::new(4, 4)),
        MeterKind::CutTime => Some(Fraction::new(2, 2)),
        MeterKind::Fraction {
            numerator,
            denominator,
        } => Some(Fraction::new(*numerator, *denominator)),
        MeterKind::None | MeterKind::Complex => None,
    }
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
        if self.peek_next_char().is_some_and(is_barline_char) {
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
        while self.peek_char().is_some_and(is_barline_char) {
            self.bump_char();
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
                    crate::fields::parse_voice_for_music(value.clone()),
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
            self.push_token(MusicTokenKind::Malformed, span);
            self.push_malformed(
                span,
                MalformedSyntaxKind::UnclosedDecoration,
                "abc.music.unclosed_decoration",
                "Decoration was preserved and skipped",
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
        let kind = if self.is_user_symbol(symbol) {
            DecorationKind::UserDefined
        } else {
            DecorationKind::Shorthand
        };
        self.pending_attachments.decorations.push(DecorationSyntax {
            span,
            name_span: span,
            name: symbol.to_string(),
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
        "|:" | "|::" => BarlineKind::RepeatStart,
        ":|" | "::|" => BarlineKind::RepeatEnd,
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

fn is_escaped(text: &str, offset: usize) -> bool {
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

fn classify_quoted_text(text: &str) -> QuotedTextKind {
    match text.chars().next() {
        Some('^') => QuotedTextKind::Annotation(AnnotationPlacement::Above),
        Some('_') => QuotedTextKind::Annotation(AnnotationPlacement::Below),
        Some('<') => QuotedTextKind::Annotation(AnnotationPlacement::Left),
        Some('>') => QuotedTextKind::Annotation(AnnotationPlacement::Right),
        Some('@') => QuotedTextKind::Annotation(AnnotationPlacement::Free),
        _ => QuotedTextKind::ChordSymbol,
    }
}

fn default_tuplet_q(p: u32) -> u32 {
    match p {
        2 | 4 | 8 => 3,
        _ => 2,
    }
}

fn broken_rhythm_multipliers(marker: BrokenRhythmSyntax) -> (Fraction, Fraction) {
    let shift = u32::from(marker.count).min(30);
    let denominator = 1u32.checked_shl(shift).unwrap_or(u32::MAX).max(1);
    let long = denominator
        .checked_mul(2)
        .and_then(|value| value.checked_sub(1))
        .unwrap_or(u32::MAX);
    match marker.direction {
        BrokenRhythmDirection::LeftShorter => (
            Fraction::new(1, denominator),
            Fraction::new(long, denominator),
        ),
        BrokenRhythmDirection::RightShorter => (
            Fraction::new(long, denominator),
            Fraction::new(1, denominator),
        ),
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

fn variable_chord_duration_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.chord.variable_duration",
        "Chord members have different durations; members were preserved with their own durations",
        span,
    )
    .with_spec_reference(abc_chord_reference())
    .with_recovery_note(RecoveryNote::new(
        "ABC chord members should use a consistent duration within one chord group.",
    ))
}

fn invalid_tuplet_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.invalid_tuplet",
        "Tuplet specifier is outside the supported ABC range",
        span,
    )
    .with_spec_reference(abc_tuplet_reference())
    .with_recovery_note(RecoveryNote::new(
        "The tuplet syntax was preserved and ignored during lowering.",
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

fn broken_rhythm_without_left_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.broken_rhythm.missing_left",
        "Broken rhythm marker has no preceding time-bearing note group",
        span,
    )
    .with_spec_reference(abc_broken_rhythm_reference())
    .with_recovery_note(RecoveryNote::new(
        "The marker was preserved and applied only to the following note group when possible.",
    ))
}

fn broken_rhythm_without_right_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.broken_rhythm.missing_right",
        "Broken rhythm marker has no following time-bearing note group",
        span,
    )
    .with_spec_reference(abc_broken_rhythm_reference())
    .with_recovery_note(RecoveryNote::new(
        "The marker was preserved after applying the preceding-side duration change.",
    ))
}

fn overlapping_broken_rhythm_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.broken_rhythm.overlap",
        "Broken rhythm markers overlap before the next note group",
        span,
    )
    .with_spec_reference(abc_broken_rhythm_reference())
    .with_recovery_note(RecoveryNote::new(
        "The later marker determines the following-side duration change.",
    ))
}

fn unmatched_slur_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.unmatched_slur",
        "Slur end has no matching open slur",
        span,
    )
    .with_spec_reference(abc_slur_reference())
    .with_recovery_note(RecoveryNote::new(
        "The unmatched slur marker was preserved and skipped during lowering.",
    ))
}

fn unclosed_slur_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.unclosed_slur",
        "Slur start has no matching close slur",
        span,
    )
    .with_spec_reference(abc_slur_reference())
    .with_recovery_note(RecoveryNote::new(
        "The open slur marker was preserved and skipped during lowering.",
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

fn barline_export_policy_info(span: Span, kind: BarlineKind) -> Diagnostic {
    Diagnostic::new(
        Severity::Info,
        "abc.musicxml.barline_policy",
        match kind {
            BarlineKind::Dotted => "Dotted barline is exported as a MusicXML dotted bar-style",
            BarlineKind::Invisible => "Invisible barline is exported as a MusicXML none bar-style",
            _ => "Barline export policy applied",
        },
        span,
    )
    .with_spec_reference(abc_barline_reference())
}

fn free_meter_multirest_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.multirest.free_meter",
        "Multi-measure rest in free meter has no measure duration; recovered using unit note length",
        span,
    )
    .with_spec_reference(abc_rest_reference())
    .with_recovery_note(RecoveryNote::new(
        "The rest count was preserved and each measure was lowered as one unit note length.",
    ))
}

fn overlay_incomplete_measure_warning(
    span: Span,
    actual: Fraction,
    expected: Fraction,
) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.voice.overlay_incomplete_measure",
        format!(
            "Overlay voice duration {}/{} is shorter than the measure-local duration {}/{}",
            actual.numerator, actual.denominator, expected.numerator, expected.denominator
        ),
        span,
    )
    .with_spec_reference(abc_overlay_reference())
    .with_recovery_note(RecoveryNote::new(
        "The overlay segment was preserved as a temporary measure-local voice.",
    ))
}

fn overlay_overfull_measure_warning(
    span: Span,
    actual: Fraction,
    expected: Fraction,
) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.voice.overlay_overfull_measure",
        format!(
            "Overlay voice duration {}/{} is longer than the measure-local duration {}/{}",
            actual.numerator, actual.denominator, expected.numerator, expected.denominator
        ),
        span,
    )
    .with_spec_reference(abc_overlay_reference())
    .with_recovery_note(RecoveryNote::new(
        "The overlay segment was preserved as a temporary measure-local voice.",
    ))
}

fn lyric_syllable_count_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.lyric.syllable_count",
        "Lyric line has more syllables than available notes",
        span,
    )
    .with_spec_reference(abc_lyric_reference())
    .with_recovery_note(RecoveryNote::new(
        "The excess lyric token was preserved but not aligned to a note.",
    ))
}

fn symbol_count_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.symbol.count",
        "Symbol line has more symbols than available notes",
        span,
    )
    .with_spec_reference(abc_symbol_reference())
    .with_recovery_note(RecoveryNote::new(
        "The excess symbol was preserved but not aligned to a note.",
    ))
}

fn abc_music_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 tune body")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_barline_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.8 repeat/bar symbols")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_rest_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.5 rests")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_chord_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.11 chords")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_tuplet_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.13 tuplets")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_broken_rhythm_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.7 broken rhythm")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_slur_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.10 ties and slurs")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_overlay_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 7.4 voice overlay")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_lyric_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 5.1 lyrics alignment")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_symbol_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.15 symbol lines")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::ParseOptions;
    use crate::parser::{parse_document, parse_tune_report_from_document};

    fn events_for(source: &str) -> (Vec<Event>, Vec<Diagnostic>) {
        let document = parse_document(source, ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);
        (
            report.value.expect("expected tune").events,
            report.diagnostics,
        )
    }

    fn count_diagnostics(diagnostics: &[Diagnostic], code: &'static str) -> usize {
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == code)
            .count()
    }

    #[test]
    fn normalizes_pitch_case_and_mixed_octave_marks() {
        let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\nC C' c C,',\n");

        assert!(diagnostics.is_empty());
        let octaves = events
            .iter()
            .filter_map(|event| match event {
                Event::Note { octave, .. } => Some(*octave),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(octaves, vec![4, 5, 5, 3]);
    }

    #[test]
    fn recovers_standalone_octave_marks_without_attaching_to_neighbor_notes() {
        let document_report = parse_document("X:1\nL:1/8\nK:C\n' , C\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(&document_report.diagnostics, "abc.music.malformed_octave"),
            2
        );

        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let malformed = tune_music.lines[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MusicItem::Malformed(item) => Some(item),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(malformed.len(), 2);
        assert!(malformed.iter().all(|item| !item.span.is_empty()));

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let events = tune_report.value.expect("expected tune").events;
        assert!(matches!(
            events.as_slice(),
            [Event::Note {
                step: 'C',
                octave: 4,
                accidental: None,
                ..
            }]
        ));
    }

    #[test]
    fn preserves_explicit_accidentals_in_semantic_events() {
        let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\n^C _D =E ^^F __G\n");

        assert!(diagnostics.is_empty());
        let accidentals = events
            .iter()
            .filter_map(|event| match event {
                Event::Note { accidental, .. } => Some(*accidental),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            accidentals,
            vec![
                Some(Accidental::Sharp),
                Some(Accidental::Flat),
                Some(Accidental::Natural),
                Some(Accidental::DoubleSharp),
                Some(Accidental::DoubleFlat),
            ]
        );
    }

    #[test]
    fn recovers_dangling_accidentals_without_leaking_into_later_notes() {
        let document_report = parse_document("X:1\nL:1/8\nK:C\n^ _ = C\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(
                &document_report.diagnostics,
                "abc.music.malformed_accidental"
            ),
            3
        );

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let events = tune_report.value.expect("expected tune").events;
        assert!(matches!(
            events.as_slice(),
            [Event::Note {
                step: 'C',
                accidental: None,
                ..
            }]
        ));
    }

    #[test]
    fn lowers_fractional_lengths_and_slash_shorthand() {
        let document = parse_document(
            "X:1\nL:1/8\nK:C\nA2 A/ A// A3/2 A/4\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert!(report.diagnostics.is_empty());
        assert_eq!(tune.divisions, 8);
        let durations = tune
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Note { duration, .. } => Some(*duration),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(durations, vec![8, 2, 1, 6, 1]);
    }

    #[test]
    fn recovers_malformed_lengths_and_preserves_valid_neighbors() {
        let document_report =
            parse_document("X:1\nL:1/8\nK:C\nA0 B/0 C 3 / D\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(&document_report.diagnostics, "abc.music.malformed_length"),
            4
        );

        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let malformed_spans = tune_music.lines[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MusicItem::Malformed(item) => Some(item.span),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(malformed_spans.len(), 2);
        assert!(
            malformed_spans
                .iter()
                .all(|span| document_report.value.source.slice(*span).is_some())
        );

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let tune = tune_report.value.expect("expected tune");
        let durations = tune
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Note { duration, .. } => Some(*duration),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(durations, vec![4, 4, 4, 4]);
    }

    #[test]
    fn lowers_multi_measure_rests_in_known_and_free_meter() {
        let (known_events, known_diagnostics) = events_for("X:1\nM:2/4\nL:1/8\nK:C\nZ2 X\n");
        assert!(known_diagnostics.is_empty());
        let known_durations = known_events
            .iter()
            .filter_map(|event| match event {
                Event::Rest {
                    duration,
                    visibility,
                    ..
                } => Some((*duration, *visibility)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            known_durations,
            vec![
                (32, RestVisibility::Visible),
                (16, RestVisibility::Invisible),
            ]
        );

        let document =
            parse_document("X:1\nM:none\nL:1/8\nK:C\nZ3\n", ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.multirest.free_meter")
        );
        assert_eq!(tune.events.len(), 1);
        assert!(matches!(tune.events[0], Event::Rest { duration: 12, .. }));
    }

    #[test]
    fn lowers_visible_invisible_rests_and_spacers() {
        let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\nz x y C\n");

        assert!(diagnostics.is_empty());
        let rests = events
            .iter()
            .filter_map(|event| match event {
                Event::Rest {
                    visibility,
                    duration,
                    ..
                } => Some((*visibility, *duration)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rests,
            vec![(RestVisibility::Visible, 4), (RestVisibility::Invisible, 4),]
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, Event::Spacer { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn malformed_rest_lengths_recover_to_safe_durations() {
        let document_report = parse_document("X:1\nL:1/8\nK:C\nz0 x/0\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(&document_report.diagnostics, "abc.music.malformed_length"),
            2
        );

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let rests = tune_report
            .value
            .expect("expected tune")
            .events
            .into_iter()
            .filter_map(|event| match event {
                Event::Rest {
                    visibility,
                    duration,
                    ..
                } => Some((visibility, duration)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rests,
            vec![(RestVisibility::Visible, 4), (RestVisibility::Invisible, 4),]
        );
    }

    #[test]
    fn lowers_basic_double_and_repeat_barlines() {
        let (events, diagnostics) = events_for("X:1\nK:C\nC|D||E|:F:|G::A[|B|]c\n");

        assert!(diagnostics.is_empty());
        let barlines = events
            .iter()
            .filter_map(|event| match event {
                Event::Barline { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            barlines,
            vec![
                BarlineKind::Regular,
                BarlineKind::Double,
                BarlineKind::RepeatStart,
                BarlineKind::RepeatEnd,
                BarlineKind::RepeatBoth,
                BarlineKind::Initial,
                BarlineKind::Final,
            ]
        );
    }

    #[test]
    fn recovers_invalid_barline_fragments_as_skipped_malformed_items() {
        let document_report = parse_document("X:1\nK:C\nC : D\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(&document_report.diagnostics, "abc.music.invalid_barline"),
            1
        );

        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        assert!(tune_music.lines[0].items.iter().any(|item| matches!(
            item,
            MusicItem::Malformed(MalformedSyntax {
                kind: MalformedSyntaxKind::InvalidBarline,
                ..
            })
        )));

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let notes = tune_report
            .value
            .expect("expected tune")
            .events
            .into_iter()
            .filter(|event| matches!(event, Event::Note { .. }))
            .count();
        assert_eq!(notes, 2);
    }

    #[test]
    fn parses_liberal_dotted_and_invisible_barlines_with_diagnostics() {
        let report = parse_document("X:1\nK:C\nC |[| D .| E [|] F\n", ParseOptions::default());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.barline.liberal")
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.barline.policy")
        );

        let tune_report = parse_tune_report_from_document(&report.value);
        assert!(
            tune_report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.musicxml.barline_policy")
        );
    }

    #[test]
    fn unclosed_inline_fields_groups_and_strings_are_recoverable_syntax() {
        let document_report = parse_document(
            "X:1\nK:C\nC [M:3/4\nD {ef\nE \"Am\nF [CE\nG\n",
            ParseOptions::default(),
        );

        for code in [
            "abc.music.unclosed_inline_field",
            "abc.music.unclosed_grace",
            "abc.music.unclosed_quoted_text",
            "abc.music.unclosed_chord",
        ] {
            assert!(
                document_report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == code),
                "expected diagnostic {code}"
            );
        }

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let notes = tune_report
            .value
            .expect("expected tune")
            .events
            .into_iter()
            .filter_map(|event| match event {
                Event::Note { step, .. } => Some(step),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(notes, vec!['C', 'D', 'E', 'F', 'G']);
    }

    #[test]
    fn parses_spec_attachment_order_around_note_group() {
        let document_report =
            parse_document("X:1\nL:1/8\nK:C\n\"Gm7\"v.=G,2\n", ParseOptions::default());
        assert!(document_report.diagnostics.is_empty());
        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let note = tune_music.lines[0]
            .items
            .iter()
            .find_map(|item| match item {
                MusicItem::Note(note) => Some(note),
                _ => None,
            })
            .expect("expected note");

        assert_eq!(note.attachments.chord_symbols[0].text, "Gm7");
        assert_eq!(
            note.attachments
                .decorations
                .iter()
                .map(|decoration| decoration.name.as_str())
                .collect::<Vec<_>>(),
            vec!["v", "."]
        );
        assert_eq!(
            note.accidental.map(|accidental| accidental.sign),
            Some(Accidental::Natural)
        );
        assert_eq!(note.octave_marks[0].mark, OctaveMark::Lower);
        assert_eq!(
            note.length.as_ref().map(|length| length.raw.as_str()),
            Some("2")
        );
    }

    #[test]
    fn classifies_quoted_chord_symbols_and_annotations() {
        let document_report = parse_document(
            "X:1\nL:1/8\nK:C\n\"Am7\"C \"^above\"D \"_below\"E \"<left\"F \">right\"G \"@free\"A\n",
            ParseOptions::default(),
        );
        assert!(document_report.diagnostics.is_empty());
        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let notes = tune_music.lines[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MusicItem::Note(note) => Some(note),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(notes[0].attachments.chord_symbols[0].text, "Am7");
        let placements = notes[1..]
            .iter()
            .map(|note| note.attachments.annotations[0].kind)
            .collect::<Vec<_>>();
        assert_eq!(
            placements,
            vec![
                QuotedTextKind::Annotation(AnnotationPlacement::Above),
                QuotedTextKind::Annotation(AnnotationPlacement::Below),
                QuotedTextKind::Annotation(AnnotationPlacement::Left),
                QuotedTextKind::Annotation(AnnotationPlacement::Right),
                QuotedTextKind::Annotation(AnnotationPlacement::Free),
            ]
        );
    }

    #[test]
    fn parses_user_defined_and_legacy_decoration_symbols_from_dialect_state() {
        let user_symbol = parse_document("X:1\nU:W=!trill!\nK:C\nWC\n", ParseOptions::default());
        assert!(user_symbol.diagnostics.is_empty());
        let tune_music = user_symbol
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let note = tune_music.lines[0]
            .items
            .iter()
            .find_map(|item| match item {
                MusicItem::Note(note) => Some(note),
                _ => None,
            })
            .expect("expected note");
        assert_eq!(
            note.attachments.decorations[0].kind,
            DecorationKind::UserDefined
        );
        assert_eq!(note.attachments.decorations[0].name, "W");

        let legacy_allowed = parse_document(
            "X:1\nI:decoration +\nK:C\n+trill+C\n",
            ParseOptions::default(),
        );
        assert!(legacy_allowed.diagnostics.is_empty());
        let tune_music = legacy_allowed
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let note = tune_music.lines[0]
            .items
            .iter()
            .find_map(|item| match item {
                MusicItem::Note(note) => Some(note),
                _ => None,
            })
            .expect("expected note");
        assert_eq!(
            note.attachments.decorations[0].kind,
            DecorationKind::LegacyNamed
        );

        let legacy_rejected = parse_document("X:1\nK:C\n+trill+C\n", ParseOptions::default());
        assert!(
            legacy_rejected
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.invalid_decoration")
        );
    }

    #[test]
    fn parses_chord_with_inside_and_outside_decorations() {
        let document_report =
            parse_document("X:1\nL:1/8\nK:C\n!trill![.CEG]\n", ParseOptions::default());
        assert!(document_report.diagnostics.is_empty());
        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let chord = tune_music.lines[0]
            .items
            .iter()
            .find_map(|item| match item {
                MusicItem::Chord(chord) => Some(chord),
                _ => None,
            })
            .expect("expected chord");

        assert_eq!(chord.attachments.decorations[0].name, "trill");
        assert_eq!(chord.members.len(), 3);
        assert_eq!(chord.members[0].note.attachments.decorations[0].name, ".");
    }

    #[test]
    fn lowers_chord_member_and_outer_duration_multipliers() {
        let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\n[C2E2G2]3\n");
        assert!(diagnostics.is_empty());
        let notes = events
            .iter()
            .filter_map(|event| match event {
                Event::Note {
                    step,
                    duration,
                    chord,
                    ..
                } => Some((*step, *duration, *chord)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            notes,
            vec![('C', 24, false), ('E', 24, true), ('G', 24, true)]
        );
    }

    #[test]
    fn variable_duration_chord_members_emit_diagnostic() {
        let document = parse_document("X:1\nL:1/8\nK:C\n[E2G,6]\n", ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.chord.variable_duration")
        );
    }

    #[test]
    fn broken_rhythm_is_transparent_across_grace_groups() {
        let (left_events, left_diagnostics) = events_for("X:1\nL:1/8\nK:C\nA<{g}A\n");
        let (right_events, right_diagnostics) = events_for("X:1\nL:1/8\nK:C\nA{g}<A\n");

        assert!(left_diagnostics.is_empty());
        assert!(right_diagnostics.is_empty());
        let durations = |events: Vec<Event>| {
            events
                .into_iter()
                .filter_map(|event| match event {
                    Event::Note { duration, .. } => Some(duration),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(durations(left_events), durations(right_events));
    }

    #[test]
    fn parses_staccato_triplet_without_spaces() {
        let document_report =
            parse_document("X:1\nL:1/8\nK:C\n(3.a.b.c\n", ParseOptions::default());
        assert!(document_report.diagnostics.is_empty());
        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");

        assert!(matches!(tune_music.lines[0].items[0], MusicItem::Tuplet(_)));
        let staccato_count = tune_music.lines[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MusicItem::Note(note) => Some(&note.attachments.decorations),
                _ => None,
            })
            .filter(|decorations| decorations.iter().any(|decoration| decoration.name == "."))
            .count();
        assert_eq!(staccato_count, 3);
    }

    #[test]
    fn parses_adjacent_repeat_endings_after_barlines() {
        let document_report = parse_document("X:1\nK:C\n:|2 C|1D A:|2B\n", ParseOptions::default());
        assert!(document_report.diagnostics.is_empty());
        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");

        let endings = tune_music.lines[0]
            .items
            .iter()
            .filter(|item| matches!(item, MusicItem::VariantEnding(_)))
            .count();
        let repeat_ends = tune_music.lines[0]
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    MusicItem::Barline(BarlineSyntax {
                        kind: BarlineKind::RepeatEnd,
                        ..
                    })
                )
            })
            .count();
        assert_eq!(endings, 3);
        assert_eq!(repeat_ends, 2);
    }

    #[test]
    fn parses_bracketed_variant_ending_lists_and_ranges() {
        let document_report = parse_document(
            "X:1\nK:C\n[1 C | [2 D | [1,3] E | [1-3] F | [1,3,5-7] G\n",
            ParseOptions::default(),
        );
        assert!(document_report.diagnostics.is_empty());
        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let endings = tune_music.lines[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MusicItem::VariantEnding(ending) => Some(ending),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(endings.len(), 5);
        assert_eq!(endings[0].endings.len(), 1);
        assert_eq!(endings[2].endings.len(), 2);
        assert!(matches!(
            endings[3].endings[0],
            VariantEndingPart::Range { .. }
        ));
        assert_eq!(endings[4].endings.len(), 3);
    }

    #[test]
    fn repeat_ending_shorthand_must_be_adjacent() {
        let legal = parse_document("X:1\nK:C\nC| [1D\n", ParseOptions::default());
        assert!(
            !legal
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.invalid_repeat_ending")
        );

        let spaced = parse_document("X:1\nK:C\nC| 1D\n", ParseOptions::default());
        assert!(
            spaced
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.invalid_repeat_ending")
        );
    }

    #[test]
    fn unclosed_slurs_are_recoverable_in_lowering() {
        let document = parse_document("X:1\nK:C\n(C D\n", ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.unclosed_slur")
        );
        assert_eq!(
            report
                .value
                .expect("expected tune")
                .events
                .iter()
                .filter(|event| matches!(event, Event::Note { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn non_music_lines_and_chords_do_not_leak_comments_or_directives() {
        let document_report = parse_document(
            "X:1\nT:ABC\n+:DEF\nK:C\n%%text GAB\n[CDE] C % FED\n",
            ParseOptions::default(),
        );
        let report = parse_tune_report_from_document(&document_report.value);
        let events = report.value.expect("expected tune").events;

        let notes = events
            .iter()
            .filter(|event| matches!(event, Event::Note { .. }))
            .count();
        assert_eq!(notes, 4);
    }

    #[test]
    fn lowers_sequential_body_voice_blocks_to_explicit_timelines() {
        let document =
            parse_document("X:1\nK:C\nV:1\nC D|\nV:2\nE F|\n", ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert_eq!(
            tune.voices
                .iter()
                .map(|voice| voice.id.value.as_str())
                .collect::<Vec<_>>(),
            vec!["1", "2"]
        );
        let note_counts = tune
            .voices
            .iter()
            .map(|voice| {
                voice
                    .measures
                    .iter()
                    .flat_map(|measure| &measure.events)
                    .filter(|event| matches!(event.kind, TimelineEventKind::Note { .. }))
                    .count()
            })
            .collect::<Vec<_>>();
        assert_eq!(note_counts, vec![2, 2]);
    }

    #[test]
    fn lowers_inline_voice_switches_to_interleaved_timelines() {
        let document = parse_document(
            "X:1\nK:C\n[V:T1] C D| [V:T2] E F|\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert_eq!(tune.voices.len(), 2);
        assert!(tune.voices.iter().any(|voice| voice.id.value == "T1"));
        assert!(tune.voices.iter().any(|voice| voice.id.value == "T2"));
        let inline_voice = document
            .music
            .tune(0)
            .expect("expected parsed tune music")
            .lines
            .first()
            .expect("expected music line")
            .items
            .iter()
            .find_map(|item| match item {
                MusicItem::InlineField(field) if field.code == 'V' => Some(field),
                _ => None,
            })
            .expect("expected inline V field");
        assert_eq!(inline_voice.value.value, "T1");
    }

    #[test]
    fn aligns_postponed_and_adjacent_lyrics_under_abc21_cursor_rules() {
        let document = parse_document(
            "X:1\nK:C\nC D E F|\nG A B c|\nw: doh re mi fa sol la ti doh\nw: alt verse words here more text ok done\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let lyrics = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .flat_map(|event| &event.lyrics)
            .filter(|lyric| lyric.control == LyricControl::Syllable)
            .map(|lyric| (lyric.verse, lyric.text.as_str()))
            .collect::<Vec<_>>();

        assert!(lyrics.contains(&(1, "doh")));
        assert!(lyrics.contains(&(1, "doh")));
        assert!(lyrics.contains(&(2, "alt")));
    }

    #[test]
    fn empty_lyric_line_consumes_notes_and_later_lyrics_start_after_them() {
        let document = parse_document(
            "X:1\nK:C\nC D E F|\nw:\nG A B c|\nw: sol la ti doh\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let lyrics = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .filter_map(|event| {
                event
                    .lyrics
                    .iter()
                    .find(|lyric| lyric.control == LyricControl::Syllable)
                    .map(|lyric| lyric.text.as_str())
            })
            .collect::<Vec<_>>();

        assert_eq!(lyrics, vec!["sol", "la", "ti", "doh"]);
    }

    #[test]
    fn lyrics_skip_rests_spacers_grace_notes_and_bar_marker_advances() {
        let document = parse_document(
            "X:1\nK:C\nC z y {g}D|E F|\nw: one | two three\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let aligned = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .filter_map(|event| {
                event
                    .lyrics
                    .iter()
                    .find(|lyric| lyric.control == LyricControl::Syllable)
                    .map(|lyric| lyric.text.as_str())
            })
            .collect::<Vec<_>>();

        assert_eq!(aligned, vec!["one", "two", "three"]);
    }

    #[test]
    fn overlay_rewinds_to_previous_barline_and_warns_when_incomplete() {
        let document = parse_document("X:1\nL:1/8\nK:C\nC D & E|\n", ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.voice.overlay_incomplete_measure")
        );
        let overlay = &tune.voices[0].measures[0].overlays[0];
        assert_eq!(overlay.events.len(), 1);
        assert_eq!(overlay.events[0].onset, Fraction::zero());
    }

    #[test]
    fn symbol_lines_align_to_notes_and_preserve_symbol_kinds() {
        let document = parse_document(
            "X:1\nK:C\nC z D E F|\ns: \"C\" * !>! \"^slow\"\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let symbols = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .flat_map(|event| &event.symbols)
            .map(|symbol| (symbol.text.as_str(), symbol.kind))
            .collect::<Vec<_>>();

        assert_eq!(
            symbols,
            vec![
                ("C", AlignedSymbolKind::ChordSymbol),
                (">", AlignedSymbolKind::Decoration),
                ("^slow", AlignedSymbolKind::Annotation),
            ]
        );
    }

    #[test]
    fn preserves_score_directives_and_post_tune_words_in_lowered_tune() {
        let document = parse_document(
            "X:1\nK:C\n%%score (T1 T2)\nC\nW:after words\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert_eq!(tune.score_directives.len(), 1);
        assert_eq!(tune.score_directives[0].value.text, "(T1 T2)");
        assert_eq!(tune.post_tune_lyrics[0].text, "after words");
    }

    #[test]
    fn preserves_header_score_directive_and_header_words() {
        let document = parse_document(
            "X:1\nI:score (A B)\nW:header words\nK:C\nC\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert_eq!(tune.score_directives.len(), 1);
        assert_eq!(tune.score_directives[0].value.text, "(A B)");
        assert_eq!(tune.post_tune_lyrics[0].text, "header words");
    }

    #[test]
    fn body_voice_properties_override_header_definition_in_timeline() {
        let document = parse_document(
            "X:1\nV:1 name=Header\nK:C\nV:1 name=Body stem=down\nC\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert_eq!(
            tune.voices[0]
                .properties
                .name
                .as_ref()
                .map(|name| name.text.as_str()),
            Some("Body")
        );
        assert_eq!(
            tune.voices[0].properties.stem,
            Some(StemDirectionModel::Down)
        );
    }

    #[test]
    fn lyrics_controls_preserve_text_and_diagnose_excess_tokens() {
        let source = "X:1\nK:C\nC D E F G A|\nw: time__ of~the \\-dash * extra too\n";
        let document = parse_document(source, ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let note_lyrics = tune.voices[0].measures[0]
            .events
            .iter()
            .filter(|event| event.alignable)
            .map(|event| {
                event
                    .lyrics
                    .iter()
                    .map(|lyric| (lyric.control, lyric.text.as_str()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        assert_eq!(note_lyrics[0], vec![(LyricControl::Syllable, "time")]);
        assert_eq!(note_lyrics[1], vec![(LyricControl::Extender, "")]);
        assert_eq!(note_lyrics[2], vec![(LyricControl::Extender, "")]);
        assert_eq!(note_lyrics[3], vec![(LyricControl::Syllable, "of the")]);
        assert_eq!(note_lyrics[4], vec![(LyricControl::Syllable, "-dash")]);
        assert!(note_lyrics[5].is_empty());
        assert_eq!(
            count_diagnostics(&report.diagnostics, "abc.lyric.syllable_count"),
            2
        );
    }

    #[test]
    fn lyric_bar_marker_is_ignored_when_cursor_is_already_at_measure_boundary() {
        let document = parse_document(
            "X:1\nK:C\nC D|E F|\nw: one two | three four\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let aligned = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .filter_map(|event| {
                event
                    .lyrics
                    .iter()
                    .find(|lyric| lyric.control == LyricControl::Syllable)
                    .map(|lyric| lyric.text.as_str())
            })
            .collect::<Vec<_>>();

        assert_eq!(aligned, vec!["one", "two", "three", "four"]);
    }

    #[test]
    fn symbol_bar_boundary_and_excess_symbol_are_diagnosed_without_realigning() {
        let document = parse_document(
            "X:1\nK:C\nC D|E F|\ns: \"C\" !>! | \"^slow\" !fermata! !extra!\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let aligned = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .flat_map(|event| &event.symbols)
            .map(|symbol| (symbol.text.as_str(), symbol.kind))
            .collect::<Vec<_>>();

        assert_eq!(
            aligned,
            vec![
                ("C", AlignedSymbolKind::ChordSymbol),
                (">", AlignedSymbolKind::Decoration),
                ("^slow", AlignedSymbolKind::Annotation),
                ("fermata", AlignedSymbolKind::Decoration),
            ]
        );
        assert_eq!(
            count_diagnostics(&report.diagnostics, "abc.symbol.count"),
            1
        );
    }

    #[test]
    fn lyrics_use_the_current_voice_cursor_in_interleaved_body_fields() {
        let document = parse_document(
            "X:1\nK:C\nV:1\nC D|\nw: one two\nV:2\nE F|\nw: three four\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let lyrics_for_voice = |voice_id: &str| {
            tune.voices
                .iter()
                .find(|voice| voice.id.value == voice_id)
                .expect("expected voice")
                .measures
                .iter()
                .flat_map(|measure| &measure.events)
                .flat_map(|event| &event.lyrics)
                .filter(|lyric| lyric.control == LyricControl::Syllable)
                .map(|lyric| lyric.text.as_str())
                .collect::<Vec<_>>()
        };

        assert_eq!(lyrics_for_voice("1"), vec!["one", "two"]);
        assert_eq!(lyrics_for_voice("2"), vec!["three", "four"]);
    }

    #[test]
    fn overfull_overlay_measure_duration_emits_diagnostic() {
        let document = parse_document("X:1\nL:1/8\nK:C\nC & D E|\n", ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);

        assert_eq!(
            count_diagnostics(&report.diagnostics, "abc.voice.overlay_overfull_measure"),
            1
        );
    }

    #[test]
    fn ampersand_in_lyric_and_symbol_lines_is_not_music_overlay_syntax() {
        let document = parse_document(
            "X:1\nK:C\nC D E|\nw: Tom & Jerry\ns: & * !>!\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        let lyrics = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .flat_map(|event| &event.lyrics)
            .filter(|lyric| lyric.control == LyricControl::Syllable)
            .map(|lyric| lyric.text.as_str())
            .collect::<Vec<_>>();
        let symbols = tune.voices[0]
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .flat_map(|event| &event.symbols)
            .map(|symbol| (symbol.text.as_str(), symbol.kind))
            .collect::<Vec<_>>();

        assert_eq!(lyrics, vec!["Tom", "&", "Jerry"]);
        assert_eq!(
            symbols,
            vec![
                ("&", AlignedSymbolKind::Raw),
                (">", AlignedSymbolKind::Decoration)
            ]
        );
        assert!(tune.voices[0].measures[0].overlays.is_empty());
    }
}
