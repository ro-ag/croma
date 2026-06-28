//! Shared field-parsing helpers: version-line parsing, span/token trimming,
//! text unescaping, field diagnostics, and the `FieldParser` state-application
//! pass.

use super::meter::{default_unit_note_length_for_meter, ensure_default_unit_note_length};
use super::voice::upsert_voice_definition;
use super::*;
use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
use crate::options::{AbcSpecVersion, ParseMode};
use crate::source::SourceText;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderPhase {
    FileHeader,
    TuneHeader,
    TuneBody,
}

pub(super) fn parse_version_line(
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

pub(super) fn parse_version_value(value: &str) -> AbcVersion {
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

pub(super) fn spec_for_version(version: &AbcVersion) -> Option<AbcSpecVersion> {
    match (version.major, version.minor) {
        (Some(major), Some(minor)) if major > 2 || (major == 2 && minor >= 2) => {
            Some(AbcSpecVersion::V22Draft)
        }
        (Some(_), Some(_)) => Some(AbcSpecVersion::V21),
        _ => None,
    }
}

pub(super) fn mode_for_version(version: &AbcVersion) -> Option<ParseMode> {
    match (version.major, version.minor) {
        (Some(major), Some(minor)) if major > 2 || (major == 2 && minor >= 1) => {
            Some(ParseMode::Strict)
        }
        (Some(_), Some(_)) => Some(ParseMode::Loose),
        _ => None,
    }
}

pub(super) fn trim_quoted_value_span(value: &str, start_offset: usize) -> Spanned<String> {
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

pub(super) fn unescape_text(value: &str) -> String {
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

pub(super) fn split_assignment(value: Spanned<String>) -> (Spanned<String>, Spanned<String>) {
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

pub(super) fn split_first_word(value: Spanned<String>) -> (Spanned<String>, Spanned<String>) {
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

pub(super) fn trim_value_span(value: &str, start_offset: usize) -> Spanned<String> {
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

pub(super) fn trimmed_uncommented_span(
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

pub(super) fn trim_span(source: &SourceText, start: usize, end: usize) -> Span {
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

pub(super) fn tokens_with_spans(value: &str, offset: usize) -> Vec<Spanned<String>> {
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

pub(super) fn unknown_field_warning(code: char, span: Span) -> Diagnostic {
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

pub(super) fn unknown_instruction_warning(directive: &str, span: Span) -> Diagnostic {
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

pub(super) fn invalid_field_warning(code: &'static str, field: &str, span: Span) -> Diagnostic {
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

pub(super) fn abc_field_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 information fields")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

impl<'source> FieldParser<'source> {
    pub(super) fn apply_field_state(&mut self, field_index: usize) {
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

    pub(super) fn apply_version_directive(
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
            InterpretationField::CromaTimeSymbol { .. } => {}
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
}
