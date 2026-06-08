use crate::diagnostic::{Diagnostic, Severity, Span, SpecReference};
use crate::error::{CromaError, Result};
use crate::fields::{ParsedAbcFields, ParsedFieldKind, parse_fields};
use crate::model::{Score, TextLine, Tune};
use crate::music::{
    ScoreModelInput, build_score_model, lower_tune_music, parse_music_document,
};
use crate::syntax::ParsedMusicDocument;
use crate::options::ParseOptions;
use crate::source::SourceText;
use crate::syntax::tune::{SurfaceMap, analyze_source};

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
    pub surface: SurfaceMap,
    pub fields: ParsedAbcFields,
    pub music: ParsedMusicDocument,
}

pub fn parse_document(source: &str, options: ParseOptions) -> ParseReport<AbcDocument> {
    parse_document_source(SourceText::new(source), options)
}

pub fn parse_document_source(
    source: SourceText,
    options: ParseOptions,
) -> ParseReport<AbcDocument> {
    let mut diagnostics = source_level_diagnostics(&source);
    let surface = analyze_source(&source);
    let (fields, field_diagnostics) = parse_fields(&source, &surface, options);
    diagnostics.extend(field_diagnostics);
    let ParseReport {
        value: music,
        diagnostics: music_diagnostics,
    } = parse_music_document(&source, &surface, &fields);
    diagnostics.extend(music_diagnostics);

    ParseReport::new(
        AbcDocument {
            source,
            options,
            surface,
            fields,
            music,
        },
        diagnostics,
    )
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
    surface: &SurfaceMap,
    options: ParseOptions,
) -> ParseReport<Option<Tune>> {
    if source.content().trim().is_empty() {
        return ParseReport::new(None, source_level_diagnostics(source));
    }

    let fallback_surface;
    let surface = if surface.line_map.lines.is_empty() && source.line_count() > 0 {
        fallback_surface = analyze_source(source);
        &fallback_surface
    } else {
        surface
    };

    let (fields, mut diagnostics) = parse_fields(source, surface, options);
    let music_report = parse_music_document(source, surface, &fields);
    diagnostics.extend(music_report.diagnostics);
    parse_tune_report_with_fields(
        source,
        surface,
        &fields,
        Some(&music_report.value),
        diagnostics,
    )
}

pub fn parse_tune_report_from_document(document: &AbcDocument) -> ParseReport<Option<Tune>> {
    parse_tune_report_with_fields(
        &document.source,
        &document.surface,
        &document.fields,
        Some(&document.music),
        Vec::new(),
    )
}

fn parse_tune_report_with_fields(
    source: &SourceText,
    surface: &SurfaceMap,
    fields: &ParsedAbcFields,
    music: Option<&ParsedMusicDocument>,
    mut diagnostics: Vec<Diagnostic>,
) -> ParseReport<Option<Tune>> {
    if source.content().trim().is_empty() {
        return ParseReport::new(None, source_level_diagnostics(source));
    }

    let mut reference = String::new();
    let mut reference_line = TextLine {
        text: String::new(),
        span: Span::new(0, 0),
    };
    let mut title = String::new();
    let mut title_line = None;
    let mut composers = Vec::new();
    let mut tempo_line = None;
    let mut meter = String::from("4/4");
    let mut key = String::new();
    let mut has_key = false;
    let mut events = Vec::new();
    let mut divisions = 8;
    let mut voices = Vec::new();
    let mut score_directives = Vec::new();
    let mut preserved_directives = Vec::new();
    let mut post_tune_lyrics = Vec::new();
    let mut score = None;
    let mut body_start = None;
    let mut tune_field_state = None;

    let Some(tune) = surface.line_map.tunes.first() else {
        diagnostics.push(missing_key_diagnostic(source));
        return ParseReport::new(None, diagnostics);
    };

    if let Some(tune_fields) = fields.tune(tune.index) {
        if let Some(parsed_meter) = tune_fields.header.meter.as_ref() {
            meter = parsed_meter.value.raw.clone();
        }
        tune_field_state = Some(&tune_fields.header);

        for field_index in &tune_fields.header_field_indices {
            let Some(field) = fields.field(*field_index) else {
                continue;
            };
            match &field.kind {
                ParsedFieldKind::Reference(value) => {
                    reference = value.value.clone();
                    reference_line = TextLine {
                        text: value.value.clone(),
                        span: value.span,
                    };
                }
                ParsedFieldKind::Title(value) if title.is_empty() => {
                    title = value.value.clone();
                    title_line = Some(TextLine {
                        text: value.value.clone(),
                        span: value.span,
                    });
                }
                ParsedFieldKind::TextMetadata(metadata) if metadata.code == 'C' => {
                    composers.push(TextLine {
                        text: metadata.value.value.clone(),
                        span: metadata.value.span,
                    });
                }
                ParsedFieldKind::Tempo(value) if tempo_line.is_none() => {
                    tempo_line = Some(TextLine {
                        text: value.value.clone(),
                        span: value.span,
                    });
                }
                ParsedFieldKind::Key(value) => {
                    has_key = true;
                    key = if value.value.raw.is_empty() {
                        String::from("none")
                    } else {
                        value.value.raw.clone()
                    };
                    body_start = Some(field.line_span.end);
                }
                _ => {}
            }
        }
    }

    if let (Some(tune_music), Some(field_state)) = (
        music.and_then(|music| music.tune(tune.index)),
        tune_field_state,
    ) {
        let lower_report = lower_tune_music(tune_music, field_state);
        diagnostics.extend(lower_report.diagnostics);
        events = lower_report.value.events;
        divisions = lower_report.value.divisions;
        voices = lower_report.value.voices;
        score_directives = lower_report.value.score_directives;
        preserved_directives = lower_report.value.preserved_directives;
        post_tune_lyrics = lower_report.value.post_tune_lyrics;
        score = Some(build_score_model(ScoreModelInput {
            reference: reference_line.clone(),
            title: title_line.clone(),
            composers: composers.clone(),
            tempo: tempo_line.clone(),
            source_span: tune.span,
            field_state,
            voices: &voices,
            score_directives: &score_directives,
            preserved_directives: &preserved_directives,
            post_tune_lyrics: &post_tune_lyrics,
            diagnostics: &diagnostics,
            divisions,
        }));
    }

    if !has_key {
        diagnostics.push(missing_key_diagnostic(source));
        return ParseReport::new(None, diagnostics);
    }
    if events.iter().all(|event| !event.is_time_bearing()) {
        diagnostics.push(no_music_diagnostic(source, body_start));
        return ParseReport::new(None, diagnostics);
    }

    ParseReport::new(
        Some(Tune {
            reference,
            title,
            meter,
            key,
            divisions,
            events,
            voices,
            score_directives,
            preserved_directives,
            post_tune_lyrics,
            score: score.unwrap_or_else(|| empty_score(tune.span, diagnostics.clone())),
        }),
        diagnostics,
    )
}

fn empty_score(span: Span, diagnostics: Vec<Diagnostic>) -> Score {
    Score {
        metadata: crate::model::ScoreMetadata {
            reference: TextLine {
                text: String::new(),
                span,
            },
            title: None,
            composers: Vec::new(),
            tempo: None,
            tempo_model: None,
            meter: None,
            key: None,
            directives: Vec::new(),
            preserved_directives: Vec::new(),
            post_tune_lyrics: Vec::new(),
            source_span: span,
        },
        parts: Vec::new(),
        diagnostics,
        divisions: 1,
        source_span: span,
        accidental_policy: crate::model::AccidentalPolicy {
            preserve_explicit_accidentals: true,
            reset_at_barlines: true,
            scope: crate::model::AccidentalScope::PitchAndOctave,
            source_span: span,
        },
    }
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
        let diagnostic = report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "abc.file.no_music")
            .expect("expected no-music diagnostic");
        assert_eq!(source.slice(diagnostic.span), Some("|||\n"));
    }
}
