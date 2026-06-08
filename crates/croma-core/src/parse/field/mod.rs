pub(crate) mod key;
pub(crate) mod meter;
pub(crate) mod misc;
pub(crate) mod tempo;
pub(crate) mod voice;

use crate::diagnostic::{Diagnostic, Span};
use crate::model::Fraction;
use crate::options::{AbcSpecVersion, ParseMode, ParseOptions};
use crate::source::SourceText;
use crate::syntax::tune::{ContinuationKind, LineContext, LineKind, SurfaceMap};

pub(crate) use key::parse_key;
use meter::ensure_default_unit_note_length;
pub(crate) use meter::{parse_meter, parse_unit_note_length};
use misc::{
    invalid_field_warning, parse_version_line, parse_version_value, split_first_word,
    trimmed_uncommented_span, unknown_field_warning, unknown_instruction_warning,
};
use tempo::{parse_decoration_delimiter, parse_line_break_mode, parse_macro, parse_user_symbol};
use voice::parse_voice;
pub(crate) use voice::{parse_score_directive, parse_voice_for_music};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAbcFields {
    pub fields: Vec<ParsedField>,
    pub file_header: FieldState,
    pub tunes: Vec<ParsedTuneFields>,
    pub state_transitions: Vec<StateTransition>,
}

impl ParsedAbcFields {
    pub fn tune(&self, tune_index: usize) -> Option<&ParsedTuneFields> {
        self.tunes.iter().find(|tune| tune.index == tune_index)
    }

    pub fn field(&self, index: usize) -> Option<&ParsedField> {
        self.fields.get(index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTuneFields {
    pub index: usize,
    pub span: Span,
    pub inherited_file_header: FieldState,
    pub header: FieldState,
    pub current: FieldState,
    pub field_indices: Vec<usize>,
    pub header_field_indices: Vec<usize>,
    pub body_field_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedField {
    pub code: char,
    pub line_index: usize,
    pub context: LineContext,
    pub line_span: Span,
    pub marker_span: Span,
    pub value_span: Span,
    pub parsed_value_span: Span,
    pub continuation_spans: Vec<Span>,
    pub kind: ParsedFieldKind,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedFieldKind {
    Version(VersionDirective),
    Reference(Spanned<String>),
    Title(Spanned<String>),
    Meter(Spanned<Meter>),
    UnitNoteLength(Spanned<UnitNoteLength>),
    Key(Spanned<KeySignature>),
    Tempo(Spanned<String>),
    Part(Spanned<String>),
    Voice(Spanned<VoiceDefinition>),
    TextMetadata(TextMetadataField),
    LyricLine(Spanned<String>),
    SymbolLine(Spanned<String>),
    Interpretation(InterpretationField),
    UserSymbol(UserSymbol),
    Macro(MacroDefinition),
    Continuation(Spanned<String>),
    Unknown(UnknownField),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownField {
    pub code: char,
    pub value: Spanned<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionDirective {
    pub version: Option<Spanned<AbcVersion>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterpretationField {
    AbcVersion {
        version: Spanned<AbcVersion>,
    },
    AbcCharset {
        charset: Spanned<String>,
    },
    LineBreak {
        mode: Spanned<LineBreakMode>,
    },
    Decoration {
        delimiter: Spanned<DecorationDelimiter>,
    },
    Score {
        directive: ScoreDirective,
    },
    Unknown {
        directive: Spanned<String>,
        value: Spanned<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMetadataField {
    pub code: char,
    pub value: Spanned<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserSymbol {
    pub symbol: Option<Spanned<char>>,
    pub replacement: Spanned<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserSymbolDefinition {
    pub symbol: Spanned<char>,
    pub replacement: Spanned<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroDefinition {
    pub name: Spanned<String>,
    pub replacement: Spanned<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceDefinition {
    pub id: Spanned<String>,
    pub properties: Spanned<String>,
    pub parsed_properties: VoiceProperties,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VoiceProperties {
    pub name: Option<Spanned<String>>,
    pub nm: Option<Spanned<String>>,
    pub subname: Option<Spanned<String>>,
    pub snm: Option<Spanned<String>>,
    pub clef: Option<Spanned<String>>,
    pub stem: Option<Spanned<StemDirection>>,
    pub octave: Option<Spanned<String>>,
    pub transpose: Option<Spanned<String>>,
    pub middle: Option<Spanned<String>>,
    pub other: Vec<VoiceProperty>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceProperty {
    pub key: Spanned<String>,
    pub value: Spanned<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StemDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreDirective {
    pub value: Spanned<String>,
    pub tokens: Vec<ScoreDirectiveToken>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreDirectiveToken {
    pub span: Span,
    pub kind: ScoreDirectiveTokenKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreDirectiveTokenKind {
    Voice(String),
    GroupStart(char),
    GroupEnd(char),
    StaffSeparator,
    MeasureSeparator,
    FloatingVoiceMarker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldState {
    pub dialect: DialectState,
    pub meter: Option<Spanned<Meter>>,
    pub unit_note_length: Option<Spanned<UnitNoteLength>>,
    pub key: Option<Spanned<KeySignature>>,
    pub voice: Option<Spanned<VoiceDefinition>>,
    pub voices: Vec<Spanned<VoiceDefinition>>,
}

impl FieldState {
    pub fn from_options(options: ParseOptions) -> Self {
        Self {
            dialect: DialectState::from_options(options),
            meter: None,
            unit_note_length: None,
            key: None,
            voice: None,
            voices: Vec::new(),
        }
    }

    pub(crate) fn unit_note_length_fraction(&self) -> Fraction {
        self.unit_note_length
            .as_ref()
            .map(|unit| unit.value.fraction.to_model_fraction())
            .unwrap_or_else(|| NoteLengthFraction::new(1, 8).to_model_fraction())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialectState {
    pub spec: AbcSpecVersion,
    pub mode: ParseMode,
    pub declared_version: Option<Spanned<AbcVersion>>,
    pub charset: Option<Spanned<String>>,
    pub line_break: LineBreakMode,
    pub decoration_delimiter: DecorationDelimiter,
    pub user_symbols: Vec<UserSymbolDefinition>,
}

impl DialectState {
    pub fn from_options(options: ParseOptions) -> Self {
        Self {
            spec: options.spec,
            mode: options.mode,
            declared_version: None,
            charset: None,
            line_break: LineBreakMode::default(),
            decoration_delimiter: DecorationDelimiter::Bang,
            user_symbols: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbcVersion {
    pub raw: String,
    pub major: Option<u16>,
    pub minor: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineBreakMode {
    pub end_of_line: bool,
    pub dollar: bool,
    pub bang: bool,
}

impl LineBreakMode {
    pub const fn none() -> Self {
        Self {
            end_of_line: false,
            dollar: false,
            bang: false,
        }
    }

    pub const fn uses_bang(self) -> bool {
        self.bang
    }
}

impl Default for LineBreakMode {
    fn default() -> Self {
        Self {
            end_of_line: true,
            dollar: true,
            bang: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationDelimiter {
    Bang,
    Plus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateTransition {
    pub scope: FieldScope,
    pub field_index: Option<usize>,
    pub span: Span,
    pub kind: StateTransitionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldScope {
    Preamble,
    FileHeader,
    BetweenBlocks,
    FreeText,
    TypesetText,
    TuneHeader { tune_index: usize },
    TuneBody { tune_index: usize },
    TuneTerminator { tune_index: usize },
}

impl From<LineContext> for FieldScope {
    fn from(context: LineContext) -> Self {
        match context {
            LineContext::Preamble => Self::Preamble,
            LineContext::FileHeader => Self::FileHeader,
            LineContext::BetweenBlocks => Self::BetweenBlocks,
            LineContext::FreeText => Self::FreeText,
            LineContext::TypesetText => Self::TypesetText,
            LineContext::TuneHeader { tune_index } => Self::TuneHeader { tune_index },
            LineContext::TuneBody { tune_index } => Self::TuneBody { tune_index },
            LineContext::TuneTerminator { tune_index } => Self::TuneTerminator { tune_index },
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateTransitionKind {
    SpecVersion {
        from: AbcSpecVersion,
        to: AbcSpecVersion,
    },
    InterpretationMode {
        from: ParseMode,
        to: ParseMode,
    },
    Charset {
        from: Option<String>,
        to: String,
    },
    LineBreak {
        from: LineBreakMode,
        to: LineBreakMode,
    },
    DecorationDelimiter {
        from: DecorationDelimiter,
        to: DecorationDelimiter,
    },
    Meter {
        from: Option<Meter>,
        to: Meter,
    },
    UnitNoteLength {
        from: Option<UnitNoteLength>,
        to: UnitNoteLength,
    },
    Key {
        from: Option<KeySignature>,
        to: KeySignature,
    },
    Voice {
        from: Option<VoiceDefinition>,
        to: VoiceDefinition,
    },
    UserSymbol {
        symbol: Option<char>,
    },
    Macro {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meter {
    pub raw: String,
    pub kind: MeterKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeterKind {
    CommonTime,
    CutTime,
    None,
    Fraction { numerator: u32, denominator: u32 },
    Complex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteLengthFraction {
    pub numerator: u32,
    pub denominator: u32,
}

impl NoteLengthFraction {
    pub const fn new(numerator: u32, denominator: u32) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    pub(crate) fn to_model_fraction(self) -> Fraction {
        Fraction::new(self.numerator, self.denominator)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitNoteLength {
    pub fraction: NoteLengthFraction,
    pub origin: UnitNoteLengthOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitNoteLengthOrigin {
    Explicit,
    DefaultFromMeter,
    DefaultFreeMeter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySignature {
    pub raw: String,
    pub tonic: Option<KeyTonic>,
    pub mode: KeyMode,
    pub accidentals: Vec<KeyAccidental>,
    pub explicit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyTonic {
    pub step: char,
    pub accidental: Option<KeyTonicAccidental>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyTonicAccidental {
    Sharp,
    Flat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyMode {
    Major,
    Ionian,
    Minor,
    Aeolian,
    Mixolydian,
    Dorian,
    Phrygian,
    Lydian,
    Locrian,
    Explicit,
    None,
    HighlandPipes,
    HighlandPipesMarked,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyAccidental {
    pub sign: AccidentalSign,
    pub note: Spanned<char>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccidentalSign {
    DoubleFlat,
    Flat,
    Natural,
    Sharp,
    DoubleSharp,
}

pub(crate) fn parse_fields(
    source: &SourceText,
    surface: &SurfaceMap,
    options: ParseOptions,
) -> (ParsedAbcFields, Vec<Diagnostic>) {
    let mut parser = FieldParser::new(source, surface, options);
    parser.parse();
    parser.finish()
}

struct FieldParser<'source> {
    source: &'source SourceText,
    surface: &'source SurfaceMap,
    options: ParseOptions,
    fields: Vec<ParsedField>,
    diagnostics: Vec<Diagnostic>,
    state_transitions: Vec<StateTransition>,
    file_header: FieldState,
    tunes: Vec<Option<ParsedTuneFields>>,
}

impl<'source> FieldParser<'source> {
    fn new(
        source: &'source SourceText,
        surface: &'source SurfaceMap,
        options: ParseOptions,
    ) -> Self {
        let tunes = surface.line_map.tunes.iter().map(|_| None).collect();
        Self {
            source,
            surface,
            options,
            fields: Vec::new(),
            diagnostics: Vec::new(),
            state_transitions: Vec::new(),
            file_header: FieldState::from_options(options),
            tunes,
        }
    }

    fn parse(&mut self) {
        for line in &self.surface.line_map.lines {
            match line.kind {
                LineKind::VersionLine => self.parse_version_line(line),
                LineKind::InformationField => self.parse_information_field(line),
                LineKind::FieldContinuation => self.parse_field_continuation(line),
                LineKind::EmptyLine
                | LineKind::Comment
                | LineKind::StylesheetDirective
                | LineKind::MusicCode
                | LineKind::FreeText
                | LineKind::TypesetTextDirective => {}
            }
        }

        for index in 0..self.tunes.len() {
            if let Some(tune) = self.tunes.get_mut(index).and_then(Option::as_mut) {
                ensure_default_unit_note_length(&mut tune.header, tune.span, self.options);
                ensure_default_unit_note_length(&mut tune.current, tune.span, self.options);
            }
        }
    }

    fn parse_version_line(&mut self, line: &crate::syntax::tune::ClassifiedLine) {
        let Some(line_text) = self.source.slice(line.text_span) else {
            return;
        };
        let Some((marker_span, version)) = parse_version_line(line_text, line.text_span.start)
        else {
            return;
        };
        let value_span = version
            .as_ref()
            .map(|version| version.span)
            .unwrap_or_else(|| Span::new(marker_span.end, marker_span.end));
        let field = ParsedField {
            code: '%',
            line_index: line.index,
            context: line.context,
            line_span: line.span,
            marker_span,
            value_span,
            parsed_value_span: value_span,
            continuation_spans: Vec::new(),
            kind: ParsedFieldKind::Version(VersionDirective { version }),
        };

        let field_index = self.push_field(field);
        let version = match &self.fields[field_index].kind {
            ParsedFieldKind::Version(version) => version.version.clone(),
            _ => None,
        };
        self.apply_version_directive(field_index, line.context, version.as_ref(), value_span);
    }

    fn parse_information_field(&mut self, line: &crate::syntax::tune::ClassifiedLine) {
        let Some(field_header) = line.field else {
            return;
        };
        let value = self.field_value(line, field_header.value_span);
        let kind = self.parse_field_kind(field_header.code, value.clone());
        let field = ParsedField {
            code: field_header.code,
            line_index: line.index,
            context: line.context,
            line_span: line.span,
            marker_span: field_header.marker_span,
            value_span: field_header.value_span,
            parsed_value_span: value.span,
            continuation_spans: self.continuation_spans(line.index),
            kind,
        };
        let field_index = self.push_field(field);
        self.apply_field_state(field_index);
    }

    fn parse_field_continuation(&mut self, line: &crate::syntax::tune::ClassifiedLine) {
        let Some(marker_span) = line.marker_span else {
            return;
        };
        let value_span = Span::new(marker_span.end, line.text_span.end);
        let value = self.field_value(line, value_span);
        let field = ParsedField {
            code: '+',
            line_index: line.index,
            context: line.context,
            line_span: line.span,
            marker_span,
            value_span,
            parsed_value_span: value.span,
            continuation_spans: Vec::new(),
            kind: ParsedFieldKind::Continuation(value),
        };
        self.push_field(field);
    }

    fn field_value(
        &self,
        line: &crate::syntax::tune::ClassifiedLine,
        value_span: Span,
    ) -> Spanned<String> {
        let mut spans = vec![trimmed_uncommented_span(self.source, line, value_span)];
        spans.extend(self.continuation_value_spans(line.index));

        let mut value = String::new();
        let mut parsed_span = spans.first().copied().unwrap_or(value_span);
        for span in spans.into_iter().filter(|span| !span.is_empty()) {
            if !value.is_empty() {
                value.push('\n');
            }
            if let Some(text) = self.source.slice(span) {
                value.push_str(text);
            }
            parsed_span = if parsed_span.is_empty() {
                span
            } else {
                Span::new(
                    parsed_span.start.min(span.start),
                    parsed_span.end.max(span.end),
                )
            };
        }

        Spanned::new(value, parsed_span)
    }

    fn continuation_spans(&self, line_index: usize) -> Vec<Span> {
        self.surface
            .line_map
            .continuation_edges
            .iter()
            .filter(|edge| {
                edge.kind == ContinuationKind::FieldContinuation && edge.from_line == line_index
            })
            .filter_map(|edge| {
                self.surface
                    .line_map
                    .lines
                    .get(edge.to_line)
                    .map(|line| line.content_span)
            })
            .collect()
    }

    fn continuation_value_spans(&self, line_index: usize) -> Vec<Span> {
        self.surface
            .line_map
            .continuation_edges
            .iter()
            .filter(|edge| {
                edge.kind == ContinuationKind::FieldContinuation && edge.from_line == line_index
            })
            .filter_map(|edge| {
                let line = self.surface.line_map.lines.get(edge.to_line)?;
                let marker_span = line.marker_span?;
                Some(trimmed_uncommented_span(
                    self.source,
                    line,
                    Span::new(marker_span.end, line.text_span.end),
                ))
            })
            .collect()
    }

    fn parse_field_kind(&mut self, code: char, value: Spanned<String>) -> ParsedFieldKind {
        match code {
            'X' => ParsedFieldKind::Reference(value),
            'T' => ParsedFieldKind::Title(value),
            'M' => ParsedFieldKind::Meter(Spanned::new(parse_meter(&value.value), value.span)),
            'L' => match parse_unit_note_length(&value.value) {
                Some(unit) => ParsedFieldKind::UnitNoteLength(Spanned::new(unit, value.span)),
                None => {
                    self.diagnostics.push(invalid_field_warning(
                        "abc.field.invalid_l",
                        "L",
                        value.span,
                    ));
                    ParsedFieldKind::Unknown(UnknownField {
                        code,
                        value: value.clone(),
                    })
                }
            },
            'K' => ParsedFieldKind::Key(Spanned::new(
                parse_key(&value.value, value.span),
                value.span,
            )),
            'Q' => ParsedFieldKind::Tempo(value),
            'P' => ParsedFieldKind::Part(value),
            'V' => {
                let span = value.span;
                ParsedFieldKind::Voice(Spanned::new(parse_voice(value), span))
            }
            'C' | 'O' | 'R' | 'N' | 'Z' | 'W' => {
                ParsedFieldKind::TextMetadata(TextMetadataField { code, value })
            }
            'w' => ParsedFieldKind::LyricLine(value),
            's' => ParsedFieldKind::SymbolLine(value),
            'I' => self.parse_interpretation(value),
            'U' => ParsedFieldKind::UserSymbol(parse_user_symbol(value)),
            'm' => ParsedFieldKind::Macro(parse_macro(value)),
            _ => {
                self.diagnostics
                    .push(unknown_field_warning(code, value.span));
                ParsedFieldKind::Unknown(UnknownField { code, value })
            }
        }
    }

    fn parse_interpretation(&mut self, value: Spanned<String>) -> ParsedFieldKind {
        let (directive, rest) = split_first_word(value);
        let directive_lower = directive.value.to_ascii_lowercase();
        match directive_lower.as_str() {
            "abc-version" => {
                let version = parse_version_value(rest.value.trim());
                if version.major.is_none() {
                    self.diagnostics.push(invalid_field_warning(
                        "abc.field.invalid_abc_version",
                        "I:abc-version",
                        rest.span,
                    ));
                }
                ParsedFieldKind::Interpretation(InterpretationField::AbcVersion {
                    version: Spanned::new(version, rest.span),
                })
            }
            "abc-charset" => {
                ParsedFieldKind::Interpretation(InterpretationField::AbcCharset { charset: rest })
            }
            "linebreak" => match parse_line_break_mode(&rest.value) {
                Some(mode) => ParsedFieldKind::Interpretation(InterpretationField::LineBreak {
                    mode: Spanned::new(mode, rest.span),
                }),
                None => {
                    self.diagnostics.push(invalid_field_warning(
                        "abc.field.invalid_linebreak",
                        "I:linebreak",
                        rest.span,
                    ));
                    ParsedFieldKind::Interpretation(InterpretationField::Unknown {
                        directive,
                        value: rest,
                    })
                }
            },
            "decoration" => match parse_decoration_delimiter(&rest.value) {
                Some(delimiter) => {
                    ParsedFieldKind::Interpretation(InterpretationField::Decoration {
                        delimiter: Spanned::new(delimiter, rest.span),
                    })
                }
                None => {
                    self.diagnostics.push(invalid_field_warning(
                        "abc.field.invalid_decoration",
                        "I:decoration",
                        rest.span,
                    ));
                    ParsedFieldKind::Interpretation(InterpretationField::Unknown {
                        directive,
                        value: rest,
                    })
                }
            },
            // `%%score` and `%%staves` share the same voice-grouping syntax.
            "score" | "staves" => ParsedFieldKind::Interpretation(InterpretationField::Score {
                directive: parse_score_directive(rest),
            }),
            _ => {
                self.diagnostics.push(unknown_instruction_warning(
                    &directive.value,
                    directive.span,
                ));
                ParsedFieldKind::Interpretation(InterpretationField::Unknown {
                    directive,
                    value: rest,
                })
            }
        }
    }

    fn push_field(&mut self, field: ParsedField) -> usize {
        let field_index = self.fields.len();
        let context = field.context;
        self.fields.push(field);

        match context {
            LineContext::TuneHeader { tune_index } | LineContext::TuneBody { tune_index } => {
                self.ensure_tune(tune_index);
                if let Some(tune) = self.tunes.get_mut(tune_index).and_then(Option::as_mut) {
                    tune.field_indices.push(field_index);
                    match context {
                        LineContext::TuneHeader { .. } => {
                            tune.header_field_indices.push(field_index);
                        }
                        LineContext::TuneBody { .. } => {
                            tune.body_field_indices.push(field_index);
                        }
                        _ => {}
                    }
                }
            }
            LineContext::Preamble
            | LineContext::FileHeader
            | LineContext::BetweenBlocks
            | LineContext::FreeText
            | LineContext::TypesetText
            | LineContext::TuneTerminator { .. } => {}
        }

        field_index
    }

    fn ensure_tune(&mut self, tune_index: usize) {
        if self
            .tunes
            .get(tune_index)
            .and_then(Option::as_ref)
            .is_some()
        {
            return;
        }

        let Some(tune_block) = self
            .surface
            .line_map
            .tunes
            .iter()
            .find(|tune| tune.index == tune_index)
        else {
            return;
        };
        let inherited = self.file_header.clone();
        if let Some(slot) = self.tunes.get_mut(tune_index) {
            *slot = Some(ParsedTuneFields {
                index: tune_index,
                span: tune_block.span,
                inherited_file_header: inherited.clone(),
                header: inherited.clone(),
                current: inherited,
                field_indices: Vec::new(),
                header_field_indices: Vec::new(),
                body_field_indices: Vec::new(),
            });
        }
    }

    fn finish(self) -> (ParsedAbcFields, Vec<Diagnostic>) {
        (
            ParsedAbcFields {
                fields: self.fields,
                file_header: self.file_header,
                tunes: self.tunes.into_iter().flatten().collect(),
                state_transitions: self.state_transitions,
            },
            self.diagnostics,
        )
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
