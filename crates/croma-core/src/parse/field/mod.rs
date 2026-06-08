pub(crate) mod meter;

use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
use crate::model::Fraction;
use crate::options::{AbcSpecVersion, ParseMode, ParseOptions};
use crate::source::SourceText;
use crate::syntax::tune::{ContinuationKind, LineContext, LineKind, SurfaceMap};

use meter::{default_unit_note_length_for_meter, ensure_default_unit_note_length};
pub(crate) use meter::{parse_meter, parse_unit_note_length};

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

    fn apply_field_state(&mut self, field_index: usize) {
        let scope = FieldScope::from(self.fields[field_index].context);
        match self.fields[field_index].context {
            LineContext::Preamble | LineContext::FileHeader => {
                let mut state = std::mem::replace(
                    &mut self.file_header,
                    FieldState::from_options(self.options),
                );
                self.apply_to_state(field_index, &mut state, scope, HeaderPhase::FileHeader);
                self.file_header = state;
            }
            LineContext::TuneHeader { tune_index } => {
                self.ensure_tune(tune_index);
                if let Some(mut tune) = self.tunes.get_mut(tune_index).and_then(Option::take) {
                    self.apply_to_state(
                        field_index,
                        &mut tune.header,
                        scope,
                        HeaderPhase::TuneHeader,
                    );
                    tune.current = tune.header.clone();
                    self.tunes[tune_index] = Some(tune);
                }
            }
            LineContext::TuneBody { tune_index } => {
                self.ensure_tune(tune_index);
                if let Some(mut tune) = self.tunes.get_mut(tune_index).and_then(Option::take) {
                    self.apply_to_state(
                        field_index,
                        &mut tune.current,
                        scope,
                        HeaderPhase::TuneBody,
                    );
                    self.tunes[tune_index] = Some(tune);
                }
            }
            LineContext::BetweenBlocks
            | LineContext::FreeText
            | LineContext::TypesetText
            | LineContext::TuneTerminator { .. } => {}
        }
    }

    fn apply_version_directive(
        &mut self,
        field_index: usize,
        context: LineContext,
        version: Option<&Spanned<AbcVersion>>,
        span: Span,
    ) {
        let scope = FieldScope::from(context);
        match context {
            LineContext::Preamble | LineContext::FileHeader => {
                let mut state = std::mem::replace(
                    &mut self.file_header,
                    FieldState::from_options(self.options),
                );
                self.apply_version_to_dialect(
                    field_index,
                    scope,
                    &mut state.dialect,
                    version,
                    span,
                );
                self.file_header = state;
            }
            LineContext::TuneHeader { tune_index } | LineContext::TuneBody { tune_index } => {
                self.ensure_tune(tune_index);
                if let Some(mut tune) = self.tunes.get_mut(tune_index).and_then(Option::take) {
                    match context {
                        LineContext::TuneHeader { .. } => {
                            self.apply_version_to_dialect(
                                field_index,
                                scope,
                                &mut tune.header.dialect,
                                version,
                                span,
                            );
                            tune.current = tune.header.clone();
                        }
                        LineContext::TuneBody { .. } => {
                            self.apply_version_to_dialect(
                                field_index,
                                scope,
                                &mut tune.current.dialect,
                                version,
                                span,
                            );
                        }
                        _ => {}
                    }
                    self.tunes[tune_index] = Some(tune);
                }
            }
            LineContext::BetweenBlocks
            | LineContext::FreeText
            | LineContext::TypesetText
            | LineContext::TuneTerminator { .. } => {}
        }
    }

    fn apply_to_state(
        &mut self,
        field_index: usize,
        state: &mut FieldState,
        scope: FieldScope,
        phase: HeaderPhase,
    ) {
        let span = self.fields[field_index].parsed_value_span;
        match self.fields[field_index].kind.clone() {
            ParsedFieldKind::Version(version) => {
                self.apply_version_to_dialect(
                    field_index,
                    scope,
                    &mut state.dialect,
                    version.version.as_ref(),
                    span,
                );
            }
            ParsedFieldKind::Meter(meter) => {
                let from = state.meter.as_ref().map(|meter| meter.value.clone());
                state.meter = Some(meter.clone());
                self.push_transition(
                    scope,
                    Some(field_index),
                    meter.span,
                    StateTransitionKind::Meter {
                        from,
                        to: meter.value.clone(),
                    },
                );
                if phase != HeaderPhase::TuneBody
                    && !matches!(
                        state
                            .unit_note_length
                            .as_ref()
                            .map(|unit| unit.value.origin),
                        Some(UnitNoteLengthOrigin::Explicit)
                    )
                {
                    let unit = UnitNoteLength {
                        fraction: default_unit_note_length_for_meter(&meter.value),
                        origin: UnitNoteLengthOrigin::DefaultFromMeter,
                    };
                    let from = state.unit_note_length.as_ref().map(|unit| unit.value);
                    state.unit_note_length = Some(Spanned::new(unit, meter.span));
                    self.push_transition(
                        scope,
                        Some(field_index),
                        meter.span,
                        StateTransitionKind::UnitNoteLength { from, to: unit },
                    );
                }
            }
            ParsedFieldKind::UnitNoteLength(unit) => {
                let from = state.unit_note_length.as_ref().map(|unit| unit.value);
                state.unit_note_length = Some(unit.clone());
                self.push_transition(
                    scope,
                    Some(field_index),
                    unit.span,
                    StateTransitionKind::UnitNoteLength {
                        from,
                        to: unit.value,
                    },
                );
            }
            ParsedFieldKind::Key(key) => {
                ensure_default_unit_note_length(state, key.span, self.options);
                let from = state.key.as_ref().map(|key| key.value.clone());
                state.key = Some(key.clone());
                self.push_transition(
                    scope,
                    Some(field_index),
                    key.span,
                    StateTransitionKind::Key {
                        from,
                        to: key.value,
                    },
                );
            }
            ParsedFieldKind::Voice(voice) => {
                let from = state.voice.as_ref().map(|voice| voice.value.clone());
                state.voice = Some(voice.clone());
                upsert_voice_definition(&mut state.voices, voice.clone());
                self.push_transition(
                    scope,
                    Some(field_index),
                    voice.span,
                    StateTransitionKind::Voice {
                        from,
                        to: voice.value,
                    },
                );
            }
            ParsedFieldKind::Interpretation(interpretation) => {
                self.apply_interpretation(field_index, scope, &mut state.dialect, interpretation);
            }
            ParsedFieldKind::UserSymbol(symbol) => {
                if let Some(symbol_value) = symbol.symbol.clone() {
                    state.dialect.user_symbols.push(UserSymbolDefinition {
                        span: Span::new(symbol_value.span.start, symbol.replacement.span.end),
                        symbol: symbol_value,
                        replacement: symbol.replacement.clone(),
                    });
                }
                self.push_transition(
                    scope,
                    Some(field_index),
                    span,
                    StateTransitionKind::UserSymbol {
                        symbol: symbol.symbol.as_ref().map(|symbol| symbol.value),
                    },
                );
            }
            ParsedFieldKind::Macro(macro_definition) => {
                self.push_transition(
                    scope,
                    Some(field_index),
                    span,
                    StateTransitionKind::Macro {
                        name: macro_definition.name.value,
                    },
                );
            }
            ParsedFieldKind::Reference(_)
            | ParsedFieldKind::Title(_)
            | ParsedFieldKind::Tempo(_)
            | ParsedFieldKind::Part(_)
            | ParsedFieldKind::TextMetadata(_)
            | ParsedFieldKind::LyricLine(_)
            | ParsedFieldKind::SymbolLine(_)
            | ParsedFieldKind::Continuation(_)
            | ParsedFieldKind::Unknown(_) => {}
        }
    }

    fn apply_interpretation(
        &mut self,
        field_index: usize,
        scope: FieldScope,
        dialect: &mut DialectState,
        interpretation: InterpretationField,
    ) {
        match interpretation {
            InterpretationField::AbcVersion { version } => {
                self.apply_version_to_dialect(
                    field_index,
                    scope,
                    dialect,
                    Some(&version),
                    version.span,
                );
            }
            InterpretationField::AbcCharset { charset } => {
                let from = dialect
                    .charset
                    .as_ref()
                    .map(|charset| charset.value.clone());
                dialect.charset = Some(charset.clone());
                self.push_transition(
                    scope,
                    Some(field_index),
                    charset.span,
                    StateTransitionKind::Charset {
                        from,
                        to: charset.value,
                    },
                );
            }
            InterpretationField::LineBreak { mode } => {
                let from = dialect.line_break;
                dialect.line_break = mode.value;
                self.push_transition(
                    scope,
                    Some(field_index),
                    mode.span,
                    StateTransitionKind::LineBreak {
                        from,
                        to: mode.value,
                    },
                );
                if mode.value.bang {
                    self.set_decoration_delimiter(
                        scope,
                        Some(field_index),
                        mode.span,
                        dialect,
                        DecorationDelimiter::Plus,
                    );
                }
            }
            InterpretationField::Decoration { delimiter } => {
                self.set_decoration_delimiter(
                    scope,
                    Some(field_index),
                    delimiter.span,
                    dialect,
                    delimiter.value,
                );
            }
            InterpretationField::Score { .. } => {}
            InterpretationField::Unknown { .. } => {}
        }
    }

    fn apply_version_to_dialect(
        &mut self,
        field_index: usize,
        scope: FieldScope,
        dialect: &mut DialectState,
        version: Option<&Spanned<AbcVersion>>,
        span: Span,
    ) {
        let Some(version) = version else {
            if self.options.mode != ParseMode::Recover && dialect.mode != ParseMode::Loose {
                let from = dialect.mode;
                dialect.mode = ParseMode::Loose;
                self.push_transition(
                    scope,
                    Some(field_index),
                    span,
                    StateTransitionKind::InterpretationMode {
                        from,
                        to: ParseMode::Loose,
                    },
                );
            }
            return;
        };

        dialect.declared_version = Some(version.clone());
        if let Some(spec) = spec_for_version(&version.value)
            && spec != dialect.spec
        {
            let from = dialect.spec;
            dialect.spec = spec;
            self.push_transition(
                scope,
                Some(field_index),
                version.span,
                StateTransitionKind::SpecVersion { from, to: spec },
            );
        }

        if self.options.mode == ParseMode::Recover {
            return;
        }

        if let Some(mode) = mode_for_version(&version.value)
            && mode != dialect.mode
        {
            let from = dialect.mode;
            dialect.mode = mode;
            self.push_transition(
                scope,
                Some(field_index),
                version.span,
                StateTransitionKind::InterpretationMode { from, to: mode },
            );
        }
    }

    fn set_decoration_delimiter(
        &mut self,
        scope: FieldScope,
        field_index: Option<usize>,
        span: Span,
        dialect: &mut DialectState,
        delimiter: DecorationDelimiter,
    ) {
        if dialect.decoration_delimiter == delimiter {
            return;
        }
        let from = dialect.decoration_delimiter;
        dialect.decoration_delimiter = delimiter;
        self.push_transition(
            scope,
            field_index,
            span,
            StateTransitionKind::DecorationDelimiter {
                from,
                to: delimiter,
            },
        );
    }

    fn push_transition(
        &mut self,
        scope: FieldScope,
        field_index: Option<usize>,
        span: Span,
        kind: StateTransitionKind,
    ) {
        self.state_transitions.push(StateTransition {
            scope,
            field_index,
            span,
            kind,
        });
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderPhase {
    FileHeader,
    TuneHeader,
    TuneBody,
}

fn parse_version_line(
    line_text: &str,
    line_offset: usize,
) -> Option<(Span, Option<Spanned<AbcVersion>>)> {
    let trimmed_offset = line_text.len() - line_text.trim_start().len();
    let trimmed = &line_text[trimmed_offset..];
    let rest = trimmed.strip_prefix("%abc")?;
    let marker_span = Span::new(
        line_offset + trimmed_offset,
        line_offset + trimmed_offset + 4,
    );
    let rest_offset = trimmed_offset + 4;
    let Some(version_text) = rest.strip_prefix('-') else {
        return Some((marker_span, None));
    };
    let version_start = rest_offset + 1;
    let version_end = version_text
        .find(char::is_whitespace)
        .unwrap_or(version_text.len());
    let version_span = Span::new(
        line_offset + version_start,
        line_offset + version_start + version_end,
    );
    Some((
        marker_span,
        Some(Spanned::new(
            parse_version_value(&version_text[..version_end]),
            version_span,
        )),
    ))
}

fn parse_version_value(value: &str) -> AbcVersion {
    let trimmed = value.trim();
    let mut parts = trimmed.split('.');
    let major = parts.next().and_then(|value| value.parse().ok());
    let minor = parts.next().and_then(|value| value.parse().ok());
    AbcVersion {
        raw: trimmed.to_owned(),
        major,
        minor,
    }
}

fn spec_for_version(version: &AbcVersion) -> Option<AbcSpecVersion> {
    match (version.major, version.minor) {
        (Some(major), Some(minor)) if major > 2 || (major == 2 && minor >= 2) => {
            Some(AbcSpecVersion::V22Draft)
        }
        (Some(_), Some(_)) => Some(AbcSpecVersion::V21),
        _ => None,
    }
}

fn mode_for_version(version: &AbcVersion) -> Option<ParseMode> {
    match (version.major, version.minor) {
        (Some(major), Some(minor)) if major > 2 || (major == 2 && minor >= 1) => {
            Some(ParseMode::Strict)
        }
        (Some(_), Some(_)) => Some(ParseMode::Loose),
        _ => None,
    }
}

pub(crate) fn parse_key(value: &str, value_span: Span) -> KeySignature {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return KeySignature {
            raw: trimmed.to_owned(),
            tonic: None,
            mode: KeyMode::None,
            accidentals: Vec::new(),
            explicit: false,
        };
    }

    if trimmed == "HP" || trimmed == "Hp" {
        return KeySignature {
            raw: trimmed.to_owned(),
            tonic: None,
            mode: if trimmed == "HP" {
                KeyMode::HighlandPipes
            } else {
                KeyMode::HighlandPipesMarked
            },
            accidentals: Vec::new(),
            explicit: false,
        };
    }

    let tokens = tokens_with_spans(trimmed, value_span.start);
    let mut tonic = None;
    let mut mode = KeyMode::Major;
    let mut explicit = false;
    let mut token_start = 0;

    if let Some(first) = tokens.first()
        && let Some((parsed_tonic, inline_mode)) = parse_tonic_token(&first.value)
    {
        tonic = Some(parsed_tonic);
        if let Some(inline_mode) = inline_mode {
            mode = inline_mode;
            explicit = mode == KeyMode::Explicit;
        }
        token_start = 1;
    }

    if let Some(token) = tokens.get(token_start)
        && let Some(parsed_mode) = parse_key_mode(&token.value)
    {
        explicit = parsed_mode == KeyMode::Explicit;
        mode = parsed_mode;
        token_start += 1;
    }

    let accidentals = tokens[token_start..]
        .iter()
        .filter_map(parse_key_accidental)
        .collect();

    KeySignature {
        raw: trimmed.to_owned(),
        tonic,
        mode,
        accidentals,
        explicit,
    }
}

fn parse_tonic_token(token: &str) -> Option<(KeyTonic, Option<KeyMode>)> {
    let mut chars = token.char_indices();
    let (_, first) = chars.next()?;
    if !matches!(first.to_ascii_uppercase(), 'A'..='G') {
        return None;
    }

    let mut accidental = None;
    let mut mode_start = first.len_utf8();
    if let Some((offset, ch)) = chars.next() {
        match ch {
            '#' => {
                accidental = Some(KeyTonicAccidental::Sharp);
                mode_start = offset + ch.len_utf8();
            }
            'b' => {
                accidental = Some(KeyTonicAccidental::Flat);
                mode_start = offset + ch.len_utf8();
            }
            _ => {
                mode_start = offset;
            }
        }
    }

    let mode = if mode_start < token.len() {
        // Any text after the tonic letter (and optional accidental) must be a
        // recognised mode suffix, e.g. `Cmaj`, `Ador`. Otherwise the token is
        // not a key tonic at all — for instance a clef shorthand (`bass`,
        // `alto`) or a property token (`clef=bass`) that merely happens to start
        // with a note letter must not be misread as a key change.
        match parse_key_mode(&token[mode_start..]) {
            Some(mode) => Some(mode),
            None => return None,
        }
    } else {
        None
    };

    Some((
        KeyTonic {
            step: first.to_ascii_uppercase(),
            accidental,
        },
        mode,
    ))
}

fn parse_key_mode(value: &str) -> Option<KeyMode> {
    let lower = value.to_ascii_lowercase();
    if lower == "m" {
        return Some(KeyMode::Minor);
    }

    let prefix = &lower[..lower.len().min(3)];
    match prefix {
        "maj" => Some(KeyMode::Major),
        "ion" => Some(KeyMode::Ionian),
        "min" => Some(KeyMode::Minor),
        "aeo" => Some(KeyMode::Aeolian),
        "mix" => Some(KeyMode::Mixolydian),
        "dor" => Some(KeyMode::Dorian),
        "phr" => Some(KeyMode::Phrygian),
        "lyd" => Some(KeyMode::Lydian),
        "loc" => Some(KeyMode::Locrian),
        "exp" => Some(KeyMode::Explicit),
        _ => None,
    }
}

fn parse_key_accidental(token: &Spanned<String>) -> Option<KeyAccidental> {
    let value = token.value.as_str();
    let (sign, sign_len) = if value.starts_with("__") {
        (AccidentalSign::DoubleFlat, 2)
    } else if value.starts_with("^^") {
        (AccidentalSign::DoubleSharp, 2)
    } else if value.starts_with('_') {
        (AccidentalSign::Flat, 1)
    } else if value.starts_with('^') {
        (AccidentalSign::Sharp, 1)
    } else if value.starts_with('=') {
        (AccidentalSign::Natural, 1)
    } else {
        return None;
    };
    let note = value[sign_len..].chars().next()?;
    if !matches!(note.to_ascii_uppercase(), 'A'..='G') {
        return None;
    }
    let note_span = Span::new(
        token.span.start + sign_len,
        token.span.start + sign_len + note.len_utf8(),
    );
    Some(KeyAccidental {
        sign,
        note: Spanned::new(note, note_span),
        span: Span::new(token.span.start, note_span.end),
    })
}

fn parse_line_break_mode(value: &str) -> Option<LineBreakMode> {
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

fn parse_decoration_delimiter(value: &str) -> Option<DecorationDelimiter> {
    match value.trim() {
        "!" => Some(DecorationDelimiter::Bang),
        "+" => Some(DecorationDelimiter::Plus),
        _ => None,
    }
}

fn parse_user_symbol(value: Spanned<String>) -> UserSymbol {
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

fn parse_macro(value: Spanned<String>) -> MacroDefinition {
    let (left, right) = split_assignment(value);
    MacroDefinition {
        name: left,
        replacement: right,
    }
}

fn parse_voice(value: Spanned<String>) -> VoiceDefinition {
    let (id, properties) = split_first_word(value);
    let parsed_properties = parse_voice_properties(&properties);
    VoiceDefinition {
        id,
        properties,
        parsed_properties,
    }
}

pub(crate) fn parse_voice_for_music(value: Spanned<String>) -> VoiceDefinition {
    parse_voice(value)
}

fn upsert_voice_definition(
    voices: &mut Vec<Spanned<VoiceDefinition>>,
    voice: Spanned<VoiceDefinition>,
) {
    if let Some(existing) = voices
        .iter_mut()
        .find(|existing| existing.value.id.value == voice.value.id.value)
    {
        *existing = voice;
    } else {
        voices.push(voice);
    }
}

fn parse_voice_properties(properties: &Spanned<String>) -> VoiceProperties {
    let mut parsed = VoiceProperties::default();
    for property in voice_property_tokens(&properties.value, properties.span.start) {
        let key_lower = property.key.value.to_ascii_lowercase();
        match key_lower.as_str() {
            "name" => parsed.name = Some(property.value.clone()),
            "nm" => parsed.nm = Some(property.value.clone()),
            "subname" => parsed.subname = Some(property.value.clone()),
            "snm" | "sname" => parsed.snm = Some(property.value.clone()),
            "clef" => parsed.clef = Some(property.value.clone()),
            "stem" => {
                parsed.stem = match property.value.value.to_ascii_lowercase().as_str() {
                    "up" => Some(Spanned::new(StemDirection::Up, property.value.span)),
                    "down" => Some(Spanned::new(StemDirection::Down, property.value.span)),
                    _ => None,
                };
                if parsed.stem.is_none() {
                    parsed.other.push(property);
                }
            }
            "octave" | "oct" => parsed.octave = Some(property.value.clone()),
            "transpose" | "transposition" | "score" | "sound" | "shift" => {
                parsed.transpose = Some(property.value.clone());
            }
            _ => parsed.other.push(property),
        }
    }
    parsed
}

fn voice_property_tokens(value: &str, offset: usize) -> Vec<VoiceProperty> {
    let mut properties = Vec::new();
    let mut index = 0;
    while index < value.len() {
        while value[index..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            let Some(ch) = value[index..].chars().next() else {
                break;
            };
            index += ch.len_utf8();
            if index >= value.len() {
                break;
            }
        }
        if index >= value.len() {
            break;
        }

        let start = index;
        let mut in_quote = false;
        while index < value.len() {
            let Some(ch) = value[index..].chars().next() else {
                break;
            };
            if ch == '"' && !is_escaped(value, index) {
                in_quote = !in_quote;
            } else if ch.is_whitespace() && !in_quote {
                break;
            }
            index += ch.len_utf8();
        }

        let token = &value[start..index];
        let span = Span::new(offset + start, offset + index);
        if let Some(eq_offset) = token.find('=') {
            let key = trim_value_span(&token[..eq_offset], offset + start);
            let value_start = start + eq_offset + 1;
            let raw_value = &value[value_start..index];
            let parsed_value = trim_quoted_value_span(raw_value, offset + value_start);
            properties.push(VoiceProperty {
                key,
                value: parsed_value,
                span,
            });
        } else {
            let key = trim_value_span(token, offset + start);
            properties.push(VoiceProperty {
                key,
                value: Spanned::new(String::new(), Span::new(span.end, span.end)),
                span,
            });
        }
    }
    properties
}

fn trim_quoted_value_span(value: &str, start_offset: usize) -> Spanned<String> {
    let trimmed = trim_value_span(value, start_offset);
    if trimmed.value.len() >= 2 && trimmed.value.starts_with('"') && trimmed.value.ends_with('"') {
        let inner_start = trimmed.span.start + 1;
        let inner_end = trimmed.span.end.saturating_sub(1);
        return Spanned::new(
            unescape_text(&trimmed.value[1..trimmed.value.len() - 1]),
            Span::new(inner_start, inner_end),
        );
    }
    trimmed
}

fn unescape_text(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\'
            && let Some(next) = chars.next()
        {
            output.push(next);
        } else {
            output.push(ch);
        }
    }
    output
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

pub(crate) fn parse_score_directive(value: Spanned<String>) -> ScoreDirective {
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < value.value.len() {
        let Some(ch) = value.value[index..].chars().next() else {
            break;
        };
        if ch.is_whitespace() {
            index += ch.len_utf8();
            continue;
        }

        let start = index;
        let kind = match ch {
            '(' | '[' | '{' => {
                index += ch.len_utf8();
                ScoreDirectiveTokenKind::GroupStart(ch)
            }
            ')' | ']' | '}' => {
                index += ch.len_utf8();
                ScoreDirectiveTokenKind::GroupEnd(ch)
            }
            '|' => {
                index += ch.len_utf8();
                ScoreDirectiveTokenKind::StaffSeparator
            }
            ',' => {
                index += ch.len_utf8();
                ScoreDirectiveTokenKind::MeasureSeparator
            }
            '*' => {
                index += ch.len_utf8();
                ScoreDirectiveTokenKind::FloatingVoiceMarker
            }
            _ => {
                while index < value.value.len() {
                    let Some(ch) = value.value[index..].chars().next() else {
                        break;
                    };
                    if ch.is_whitespace() || matches!(ch, '(' | ')' | '[' | ']' | '{' | '}' | '|') {
                        break;
                    }
                    index += ch.len_utf8();
                }
                ScoreDirectiveTokenKind::Voice(value.value[start..index].to_owned())
            }
        };
        tokens.push(ScoreDirectiveToken {
            span: Span::new(value.span.start + start, value.span.start + index),
            kind,
        });
    }
    ScoreDirective { value, tokens }
}

fn split_assignment(value: Spanned<String>) -> (Spanned<String>, Spanned<String>) {
    if let Some(offset) = value.value.find('=') {
        let left = trim_value_span(&value.value[..offset], value.span.start);
        let right_start = value.span.start + offset + 1;
        let right = trim_value_span(&value.value[offset + 1..], right_start);
        return (left, right);
    }

    let left = trim_value_span(&value.value, value.span.start);
    let right = Spanned::new(String::new(), Span::new(value.span.end, value.span.end));
    (left, right)
}

fn split_first_word(value: Spanned<String>) -> (Spanned<String>, Spanned<String>) {
    let trimmed = trim_value_span(&value.value, value.span.start);
    let split = trimmed
        .value
        .char_indices()
        .find_map(|(offset, ch)| ch.is_whitespace().then_some(offset));
    let Some(split) = split else {
        let end = trimmed.span.end;
        return (trimmed, Spanned::new(String::new(), Span::new(end, end)));
    };

    let directive = Spanned::new(
        trimmed.value[..split].to_owned(),
        Span::new(trimmed.span.start, trimmed.span.start + split),
    );
    let rest_start = trimmed.span.start + split;
    let rest = trim_value_span(&trimmed.value[split..], rest_start);
    (directive, rest)
}

fn trim_value_span(value: &str, start_offset: usize) -> Spanned<String> {
    let leading = value.len() - value.trim_start().len();
    let trailing = value.trim_end().len();
    if leading >= trailing {
        let offset = start_offset + value.len();
        return Spanned::new(String::new(), Span::new(offset, offset));
    }
    let start = start_offset + leading;
    let end = start_offset + trailing;
    Spanned::new(value[leading..trailing].to_owned(), Span::new(start, end))
}

fn trimmed_uncommented_span(
    source: &SourceText,
    line: &crate::syntax::tune::ClassifiedLine,
    value_span: Span,
) -> Span {
    let end = line
        .trailing_comment
        .map(|comment| comment.start)
        .unwrap_or(value_span.end)
        .min(value_span.end);
    trim_span(source, value_span.start, end)
}

fn trim_span(source: &SourceText, start: usize, end: usize) -> Span {
    if start >= end {
        return Span::new(end, end);
    }
    let Some(text) = source.slice(Span::new(start, end)) else {
        return Span::new(start, end);
    };
    let leading = text.len() - text.trim_start().len();
    let trailing = text.trim_end().len();
    if leading >= trailing {
        let offset = end;
        return Span::new(offset, offset);
    }
    Span::new(start + leading, start + trailing)
}

fn tokens_with_spans(value: &str, offset: usize) -> Vec<Spanned<String>> {
    let mut tokens = Vec::new();
    let mut token_start = None;
    for (index, ch) in value.char_indices() {
        if ch.is_whitespace() {
            if let Some(start) = token_start.take() {
                tokens.push(Spanned::new(
                    value[start..index].to_owned(),
                    Span::new(offset + start, offset + index),
                ));
            }
        } else if token_start.is_none() {
            token_start = Some(index);
        }
    }
    if let Some(start) = token_start {
        tokens.push(Spanned::new(
            value[start..].to_owned(),
            Span::new(offset + start, offset + value.len()),
        ));
    }
    tokens
}

fn unknown_field_warning(code: char, span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.field.unknown",
        format!("Unknown ABC field `{code}:` was ignored"),
        span,
    )
    .with_spec_reference(abc_field_reference())
    .with_recovery_note(RecoveryNote::new(
        "The field was preserved as non-note input and was not parsed as music.",
    ))
}

fn unknown_instruction_warning(directive: &str, span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.field.unknown_instruction",
        format!("Unknown I: instruction `{directive}` was ignored"),
        span,
    )
    .with_spec_reference(abc_field_reference())
    .with_recovery_note(RecoveryNote::new(
        "The instruction field was preserved but did not change parser state.",
    ))
}

fn invalid_field_warning(code: &'static str, field: &str, span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        code,
        format!("Invalid {field} field value was ignored"),
        span,
    )
    .with_spec_reference(abc_field_reference())
    .with_recovery_note(RecoveryNote::new(
        "Parsing continued with the previous parser state.",
    ))
}

fn abc_field_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 information fields")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_document;

    #[test]
    fn missing_l_defaults_from_meter() {
        let report = parse_document("X:1\nM:2/4\nK:C\nC\n", ParseOptions::default());
        let tune = report.value.fields.tune(0).expect("expected tune fields");
        let unit = tune
            .header
            .unit_note_length
            .as_ref()
            .expect("expected default unit note length");

        assert_eq!(unit.value.fraction, NoteLengthFraction::new(1, 16));
        assert_eq!(unit.value.origin, UnitNoteLengthOrigin::DefaultFromMeter);
        assert_eq!(report.value.source.slice(unit.span), Some("2/4"));
    }

    #[test]
    fn parses_common_cut_and_free_meter() {
        for (source, expected) in [
            ("X:1\nM:C\nK:C\nC\n", MeterKind::CommonTime),
            ("X:1\nM:C|\nK:C\nC\n", MeterKind::CutTime),
            ("X:1\nM:none\nK:C\nC\n", MeterKind::None),
        ] {
            let report = parse_document(source, ParseOptions::default());
            let tune = report.value.fields.tune(0).expect("expected tune fields");
            let meter = tune.header.meter.as_ref().expect("expected meter");
            assert_eq!(meter.value.kind, expected);
            assert_eq!(
                tune.header
                    .unit_note_length
                    .as_ref()
                    .expect("expected unit")
                    .value
                    .fraction,
                NoteLengthFraction::new(1, 8)
            );
        }
    }

    #[test]
    fn parses_key_modes_and_explicit_accidentals() {
        let report = parse_document("X:1\nK:D Phr ^f _B\nC\n", ParseOptions::default());
        let tune = report.value.fields.tune(0).expect("expected tune fields");
        let key = tune.header.key.as_ref().expect("expected key");

        assert_eq!(
            key.value.tonic,
            Some(KeyTonic {
                step: 'D',
                accidental: None
            })
        );
        assert_eq!(key.value.mode, KeyMode::Phrygian);
        assert_eq!(key.value.accidentals.len(), 2);
        assert_eq!(key.value.accidentals[0].sign, AccidentalSign::Sharp);
        assert_eq!(key.value.accidentals[0].note.value, 'f');
        assert_eq!(key.value.accidentals[1].sign, AccidentalSign::Flat);
        assert_eq!(key.value.accidentals[1].note.value, 'B');

        let explicit = parse_document("X:1\nK:D exp _b _e ^f\nC\n", ParseOptions::default());
        let key = explicit
            .value
            .fields
            .tune(0)
            .and_then(|tune| tune.header.key.as_ref())
            .expect("expected explicit key");
        assert_eq!(key.value.mode, KeyMode::Explicit);
        assert!(key.value.explicit);
        assert_eq!(key.value.accidentals.len(), 3);
    }

    #[test]
    fn unknown_fields_warn_and_stay_out_of_music() {
        let report = parse_document("X:1\nK:C\nY:ABC\nC\n", ParseOptions::default());

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.field.unknown")
        );
        assert_eq!(
            report
                .value
                .surface
                .tokens_of_kind(crate::syntax::tune::SurfaceKind::Note)
                .count(),
            1
        );
    }

    #[test]
    fn version_lines_and_version_instructions_update_interpretation_mode() {
        let strict = parse_document("%abc-2.1\nX:1\nK:C\nC\n", ParseOptions::default());
        assert_eq!(
            strict.value.fields.file_header.dialect.mode,
            ParseMode::Strict
        );

        let loose = parse_document("%abc\nX:1\nK:C\nC\n", ParseOptions::default());
        assert_eq!(
            loose.value.fields.file_header.dialect.mode,
            ParseMode::Loose
        );

        let tune_loose = parse_document(
            "%abc-2.1\nX:1\nI:abc-version 2.0\nK:C\nC\n",
            ParseOptions::default(),
        );
        let tune = tune_loose.value.fields.tune(0).expect("expected tune");
        assert_eq!(tune.header.dialect.mode, ParseMode::Loose);
    }

    #[test]
    fn interpretation_fields_update_decoration_and_linebreak_state() {
        let decoration = parse_document("I:decoration +\n\nX:1\nK:C\nC\n", ParseOptions::default());
        let tune = decoration.value.fields.tune(0).expect("expected tune");
        assert_eq!(
            tune.inherited_file_header.dialect.decoration_delimiter,
            DecorationDelimiter::Plus
        );

        let linebreak = parse_document("X:1\nI:linebreak !\nK:C\nC\n", ParseOptions::default());
        let tune = linebreak.value.fields.tune(0).expect("expected tune");
        assert!(tune.header.dialect.line_break.uses_bang());
        assert_eq!(
            tune.header.dialect.decoration_delimiter,
            DecorationDelimiter::Plus
        );
    }

    #[test]
    fn parses_voice_properties_with_source_spans() {
        let report = parse_document(
            "X:1\nV:T1 name=\"Tenor 1\" nm=T subname=\"Line A\" snm=TA clef=treble stem=up octave=-1 transpose=_B\nK:C\nC\n",
            ParseOptions::default(),
        );
        let tune = report.value.fields.tune(0).expect("expected tune fields");
        let voice = tune
            .header
            .voices
            .first()
            .expect("expected voice definition");
        let properties = &voice.value.parsed_properties;

        assert_eq!(voice.value.id.value, "T1");
        assert_eq!(
            properties.name.as_ref().map(|value| value.value.as_str()),
            Some("Tenor 1")
        );
        assert_eq!(
            properties.nm.as_ref().map(|value| value.value.as_str()),
            Some("T")
        );
        assert_eq!(
            properties
                .subname
                .as_ref()
                .map(|value| value.value.as_str()),
            Some("Line A")
        );
        assert_eq!(
            properties.snm.as_ref().map(|value| value.value.as_str()),
            Some("TA")
        );
        assert_eq!(
            properties.clef.as_ref().map(|value| value.value.as_str()),
            Some("treble")
        );
        assert_eq!(
            properties.stem.as_ref().map(|value| value.value),
            Some(StemDirection::Up)
        );
        assert_eq!(
            report
                .value
                .source
                .slice(properties.name.as_ref().expect("expected name").span),
            Some("Tenor 1")
        );
        assert_eq!(
            properties.octave.as_ref().map(|value| value.value.as_str()),
            Some("-1")
        );
        assert_eq!(
            properties
                .transpose
                .as_ref()
                .map(|value| value.value.as_str()),
            Some("_B")
        );
    }

    #[test]
    fn parses_i_score_as_structured_directive() {
        let report = parse_document("X:1\nK:C\nI:score (T1 T2)\nC\n", ParseOptions::default());
        let score = report
            .value
            .fields
            .fields
            .iter()
            .find_map(|field| match &field.kind {
                ParsedFieldKind::Interpretation(InterpretationField::Score { directive }) => {
                    Some(directive)
                }
                _ => None,
            })
            .expect("expected I:score directive");

        assert_eq!(score.value.value, "(T1 T2)");
        assert_eq!(score.tokens.len(), 4);
    }

    #[test]
    fn invalid_voice_stem_is_preserved_as_other_property_with_span() {
        let report = parse_document(
            "X:1\nV:bad stem=sideways clef=bass\nK:C\nC\n",
            ParseOptions::default(),
        );
        let voice = report
            .value
            .fields
            .tune(0)
            .and_then(|tune| tune.header.voices.first())
            .expect("expected voice definition");
        let properties = &voice.value.parsed_properties;

        assert!(properties.stem.is_none());
        let stem = properties
            .other
            .iter()
            .find(|property| property.key.value == "stem")
            .expect("expected preserved invalid stem property");
        assert_eq!(stem.value.value, "sideways");
        assert_eq!(report.value.source.slice(stem.span), Some("stem=sideways"));
    }
}
