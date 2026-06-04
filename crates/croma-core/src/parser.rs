use crate::diagnostic::{Diagnostic, Severity, Span, SpecReference};
use crate::error::{CromaError, Result};
use crate::model::{Event, Fraction, Tune};
use crate::options::ParseOptions;
use crate::source::SourceText;
use crate::surface::SurfaceMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseReport<T> {
    pub value: T,
    pub diagnostics: Vec<Diagnostic>,
}

impl<T> ParseReport<T> {
    pub fn new(value: T, diagnostics: Vec<Diagnostic>) -> Self {
        Self { value, diagnostics }
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbcDocument {
    pub source: SourceText,
    pub options: ParseOptions,
}

pub fn parse_document(source: &str, options: ParseOptions) -> ParseReport<AbcDocument> {
    parse_document_source(SourceText::new(source), options)
}

pub fn parse_document_source(
    source: SourceText,
    options: ParseOptions,
) -> ParseReport<AbcDocument> {
    let diagnostics = source_level_diagnostics(&source);

    ParseReport::new(AbcDocument { source, options }, diagnostics)
}

pub fn parse_tune(
    source: &SourceText,
    _surface: &SurfaceMap,
    _options: ParseOptions,
) -> Result<Tune> {
    let report = parse_tune_report(source, _surface, _options);
    report
        .value
        .ok_or_else(|| CromaError::from_diagnostics(report.diagnostics))
}

pub fn parse_tune_report(
    source: &SourceText,
    _surface: &SurfaceMap,
    _options: ParseOptions,
) -> ParseReport<Option<Tune>> {
    if source.content().trim().is_empty() {
        return ParseReport::new(None, source_level_diagnostics(source));
    }

    let mut reference = String::new();
    let mut title = String::new();
    let mut meter = String::from("4/4");
    let mut unit = Fraction::new(1, 8);
    let mut key = String::new();
    let mut in_body = false;
    let mut events = Vec::new();
    let mut body_start = None;

    for index in 0..source.line_count() {
        let Some(source_line) = source.line(index) else {
            continue;
        };
        let Some(line) = source.line_text(index) else {
            continue;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('%') {
            continue;
        }
        if !in_body {
            if let Some(value) = trimmed.strip_prefix("X:") {
                reference = value.trim().to_owned();
                continue;
            }
            if let Some(value) = trimmed.strip_prefix("T:") {
                title = value.trim().to_owned();
                continue;
            }
            if let Some(value) = trimmed.strip_prefix("M:") {
                meter = value.trim().to_owned();
                continue;
            }
            if let Some(value) = trimmed.strip_prefix("L:") {
                if let Some(parsed) = Fraction::parse(value.trim()) {
                    unit = parsed;
                }
                continue;
            }
            if let Some(value) = trimmed.strip_prefix("K:") {
                key = value.trim().to_owned();
                in_body = true;
                body_start = Some(source_line.end());
                continue;
            }
        }

        if in_body {
            parse_music_line(trimmed, unit, &mut events);
        }
    }

    if key.is_empty() {
        return ParseReport::new(None, vec![missing_key_diagnostic(source)]);
    }
    if events.iter().all(|event| matches!(event, Event::Bar)) {
        return ParseReport::new(None, vec![no_music_diagnostic(source, body_start)]);
    }

    ParseReport::new(
        Some(Tune {
            reference,
            title,
            meter,
            key,
            divisions: 8,
            events,
        }),
        Vec::new(),
    )
}

fn source_level_diagnostics(source: &SourceText) -> Vec<Diagnostic> {
    if source.content().trim().is_empty() {
        return vec![
            Diagnostic::new(
                Severity::Error,
                "abc.file.empty",
                "ABC source is empty",
                Span::new(source.content_start(), source.content_start()),
            )
            .with_spec_reference(abc_file_structure_reference()),
        ];
    }

    let has_non_comment_content = (0..source.line_count()).any(|index| {
        source
            .line_text(index)
            .map(|line| {
                let trimmed = line.trim_start();
                !trimmed.trim_end().is_empty() && !trimmed.starts_with('%')
            })
            .unwrap_or(false)
    });

    if has_non_comment_content {
        Vec::new()
    } else {
        vec![
            Diagnostic::new(
                Severity::Error,
                "abc.file.no_tune",
                "ABC source contains comments only and no tune content",
                source.content_span(),
            )
            .with_spec_reference(abc_file_structure_reference()),
        ]
    }
}

fn abc_file_structure_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 file structure")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn missing_key_diagnostic(source: &SourceText) -> Diagnostic {
    Diagnostic::new(
        Severity::Error,
        "abc.file.missing_k",
        "ABC source is missing a K: field",
        Span::new(source.len(), source.len()),
    )
    .with_spec_reference(abc_file_structure_reference())
}

fn no_music_diagnostic(source: &SourceText, body_start: Option<usize>) -> Diagnostic {
    Diagnostic::new(
        Severity::Error,
        "abc.file.no_music",
        "ABC source does not contain body music",
        Span::new(body_start.unwrap_or_else(|| source.len()), source.len()),
    )
    .with_spec_reference(abc_file_structure_reference())
}

fn parse_music_line(line: &str, unit: Fraction, events: &mut Vec<Event>) {
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        match ch {
            '%' => break,
            '|' => {
                events.push(Event::Bar);
                index += 1;
            }
            'A'..='G' | 'a'..='g' => {
                let step = ch.to_ascii_uppercase();
                let octave = if ch.is_ascii_lowercase() { 5 } else { 4 };
                index += 1;
                let (length, next) = parse_length(&chars, index);
                events.push(Event::Note {
                    step,
                    octave,
                    duration: duration_divisions(unit, length),
                });
                index = next;
            }
            'z' | 'x' => {
                index += 1;
                let (length, next) = parse_length(&chars, index);
                events.push(Event::Rest {
                    duration: duration_divisions(unit, length),
                });
                index = next;
            }
            _ => {
                index += 1;
            }
        }
    }
}

fn parse_length(chars: &[char], start: usize) -> (Fraction, usize) {
    let mut index = start;
    let mut numerator = String::new();
    while index < chars.len() && chars[index].is_ascii_digit() {
        numerator.push(chars[index]);
        index += 1;
    }

    if index < chars.len() && chars[index] == '/' {
        index += 1;
        let mut denominator = String::new();
        while index < chars.len() && chars[index].is_ascii_digit() {
            denominator.push(chars[index]);
            index += 1;
        }
        let numerator = parse_u32_or_one(&numerator);
        let denominator = parse_u32_or_default(&denominator, 2);
        return (Fraction::new(numerator, denominator), index);
    }

    (Fraction::new(parse_u32_or_one(&numerator), 1), index)
}

fn parse_u32_or_one(value: &str) -> u32 {
    parse_u32_or_default(value, 1)
}

fn parse_u32_or_default(value: &str, default: u32) -> u32 {
    value.parse::<u32>().unwrap_or(default)
}

fn duration_divisions(unit: Fraction, length: Fraction) -> u32 {
    let numerator = 32 * unit.numerator * length.numerator;
    let denominator = unit.denominator * length.denominator;
    numerator
        .checked_div(denominator)
        .filter(|v| *v > 0)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_empty_input_with_exact_span() {
        let report = parse_document("", ParseOptions::default());

        assert!(report.value.source.is_empty());
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, "abc.file.empty");
        assert_eq!(report.diagnostics[0].span, Span::new(0, 0));
        assert!(report.diagnostics[0].spec_reference.is_some());
    }

    #[test]
    fn reports_bom_only_input_as_empty_after_file_start_bom() {
        let report = parse_document("\u{feff}", ParseOptions::default());

        assert_eq!(report.value.source.content_start(), 3);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, "abc.file.empty");
        assert_eq!(report.diagnostics[0].span, Span::new(3, 3));
    }

    #[test]
    fn reports_comment_only_input_with_content_span() {
        let report = parse_document("% only\r\n  % comments\n", ParseOptions::default());

        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, "abc.file.no_tune");
        assert_eq!(
            report.diagnostics[0].span,
            report.value.source.content_span()
        );
    }

    #[test]
    fn does_not_ignore_mid_file_bom_as_empty_content() {
        let report = parse_document("% comment\n\u{feff}\n", ParseOptions::default());

        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn reports_missing_key_at_eof() {
        let source = SourceText::new("X:1\nT:No Key\n");
        let surface = SurfaceMap::default();
        let report = parse_tune_report(&source, &surface, ParseOptions::default());

        assert!(report.value.is_none());
        assert!(report.has_errors());
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, "abc.file.missing_k");
        assert_eq!(
            report.diagnostics[0].span,
            Span::new(source.len(), source.len())
        );
        assert_eq!(
            source.line_column_span(report.diagnostics[0].span),
            Some(crate::LineColumnSpan {
                start: crate::LineColumn::new(3, 1),
                end: crate::LineColumn::new(3, 1),
            })
        );
    }

    #[test]
    fn reports_no_music_over_body_span() {
        let source = SourceText::new("X:1\nK:C\n|||\n");
        let surface = SurfaceMap::default();
        let report = parse_tune_report(&source, &surface, ParseOptions::default());

        assert!(report.value.is_none());
        assert!(report.has_errors());
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, "abc.file.no_music");
        assert_eq!(source.slice(report.diagnostics[0].span), Some("|||\n"));
    }
}
