use std::collections::VecDeque;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use croma_core::{
    AbcDocument, AbcSpecVersion, Diagnostic, LowerOptions, ParseMode, ParseOptions, ParseReport,
    Score, Severity, SourceText, lower_score, parse_document, write_musicxml,
};
use serde_json::json;

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{}", error.message);
            ExitCode::FAILURE
        }
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<ExitCode, CliError> {
    let mut args: VecDeque<String> = args.into_iter().collect();
    let mut options = CliOptions::default();

    if args.is_empty() {
        return Err(CliError::usage());
    }

    loop {
        match consume_common_option(&mut args, &mut options)? {
            CommonOption::Consumed => {}
            CommonOption::Help => {
                println!("{}", usage());
                return Ok(ExitCode::SUCCESS);
            }
            CommonOption::NotCommon => break,
        }
    }

    let Some(command) = args.pop_front() else {
        return Err(CliError::usage());
    };

    match command.as_str() {
        "xml" => run_xml(args, options),
        "check" => run_check(args, options),
        "dump" => run_dump(args, options),
        "--help" | "-h" | "help" => {
            println!("{}", usage());
            Ok(ExitCode::SUCCESS)
        }
        _ => Err(CliError::usage_with(format!("unknown command `{command}`"))),
    }
}

fn run_xml(mut args: VecDeque<String>, mut options: CliOptions) -> Result<ExitCode, CliError> {
    let mut input = None;
    let mut output = None;

    while !args.is_empty() {
        match consume_common_option(&mut args, &mut options)? {
            CommonOption::Consumed => continue,
            CommonOption::Help => {
                println!("{}", usage());
                return Ok(ExitCode::SUCCESS);
            }
            CommonOption::NotCommon => {}
        }

        let Some(arg) = args.pop_front() else {
            break;
        };

        match arg.as_str() {
            "-o" | "--output" => {
                if output.is_some() {
                    return Err(CliError::usage_with(
                        "output path was provided more than once",
                    ));
                }
                let Some(path) = args.pop_front() else {
                    return Err(CliError::usage_with("missing value for `-o`"));
                };
                output = Some(PathBuf::from(path));
            }
            _ if arg.starts_with("--output=") => {
                if output.is_some() {
                    return Err(CliError::usage_with(
                        "output path was provided more than once",
                    ));
                }
                let Some((_, path)) = arg.split_once('=') else {
                    return Err(CliError::usage());
                };
                output = Some(PathBuf::from(path));
            }
            _ if arg.starts_with('-') => {
                return Err(CliError::usage_with(format!("unknown option `{arg}`")));
            }
            _ => {
                if input.is_some() {
                    return Err(CliError::usage_with(
                        "input path was provided more than once",
                    ));
                }
                input = Some(PathBuf::from(arg));
            }
        }
    }

    let input = input.ok_or_else(|| CliError::usage_with("missing input ABC file"))?;
    let source = read_source(&input)?;
    let source_text = SourceText::with_file_name(source.clone(), input.display().to_string());
    let PipelineResult {
        document: _document,
        score: _score,
        diagnostics,
        musicxml,
    } = run_export_pipeline(&source, options.parse_options());

    emit_diagnostics(&options, &source_text, &input, &diagnostics)?;

    if diagnostics_should_fail(&diagnostics, options.warnings_as_errors) {
        return Ok(ExitCode::FAILURE);
    }

    let Some(musicxml) = musicxml else {
        return Ok(ExitCode::FAILURE);
    };

    if let Some(output) = output {
        write_output(&output, &musicxml)?;
    } else {
        print!("{musicxml}");
        flush_stdout()?;
    }

    Ok(ExitCode::SUCCESS)
}

fn run_check(mut args: VecDeque<String>, mut options: CliOptions) -> Result<ExitCode, CliError> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        println!("{}", usage());
        return Ok(ExitCode::SUCCESS);
    }

    let input = parse_single_input_command(&mut args, &mut options, "check")?;
    let source = read_source(&input)?;
    let source_text = SourceText::with_file_name(source.clone(), input.display().to_string());
    let CheckResult { diagnostics, .. } = run_check_pipeline(&source, options.parse_options());

    emit_diagnostics(&options, &source_text, &input, &diagnostics)?;

    if diagnostics_should_fail(&diagnostics, options.warnings_as_errors) {
        Ok(ExitCode::FAILURE)
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn run_dump(mut args: VecDeque<String>, mut options: CliOptions) -> Result<ExitCode, CliError> {
    let mut kind = None;
    let mut input = None;

    while !args.is_empty() {
        match consume_common_option(&mut args, &mut options)? {
            CommonOption::Consumed => continue,
            CommonOption::Help => {
                println!("{}", usage());
                return Ok(ExitCode::SUCCESS);
            }
            CommonOption::NotCommon => {}
        }

        let Some(arg) = args.pop_front() else {
            break;
        };

        if arg.starts_with('-') {
            return Err(CliError::usage_with(format!("unknown option `{arg}`")));
        }

        if kind.is_none() {
            kind = Some(DumpKind::parse(&arg)?);
        } else if input.is_none() {
            input = Some(PathBuf::from(arg));
        } else {
            return Err(CliError::usage_with("too many arguments for `dump`"));
        }
    }

    let kind = kind.ok_or_else(|| CliError::usage_with("missing dump kind"))?;
    let input = input.ok_or_else(|| CliError::usage_with("missing input ABC file"))?;
    let source = read_source(&input)?;
    let source_text = SourceText::with_file_name(source.clone(), input.display().to_string());
    let report = parse_document(&source, options.parse_options());
    let document = report.value;
    let mut diagnostics = report.diagnostics;

    match kind {
        DumpKind::Tokens => {
            emit_diagnostics(&options, &source_text, &input, &diagnostics)?;
            if diagnostics_should_fail(&diagnostics, options.warnings_as_errors) {
                return Ok(ExitCode::FAILURE);
            }
            println!("{:#?}", document.surface.tokens);
        }
        DumpKind::Tree => {
            emit_diagnostics(&options, &source_text, &input, &diagnostics)?;
            if diagnostics_should_fail(&diagnostics, options.warnings_as_errors) {
                return Ok(ExitCode::FAILURE);
            }
            println!("LineMap:\n{:#?}", document.surface.line_map);
            println!("Fields:\n{:#?}", document.fields);
            println!("Music:\n{:#?}", document.music);
        }
        DumpKind::Score => {
            let lower_report = lower_score(&document, LowerOptions);
            diagnostics.extend(lower_report.diagnostics);
            emit_diagnostics(&options, &source_text, &input, &diagnostics)?;
            if diagnostics_should_fail(&diagnostics, options.warnings_as_errors) {
                return Ok(ExitCode::FAILURE);
            }
            let Some(score) = lower_report.value else {
                return Ok(ExitCode::FAILURE);
            };
            println!("{score:#?}");
        }
    }

    flush_stdout()?;
    Ok(ExitCode::SUCCESS)
}

fn parse_single_input_command(
    args: &mut VecDeque<String>,
    options: &mut CliOptions,
    command: &'static str,
) -> Result<PathBuf, CliError> {
    let mut input = None;

    while !args.is_empty() {
        match consume_common_option(args, options)? {
            CommonOption::Consumed => continue,
            CommonOption::Help => {
                return Err(CliError::usage());
            }
            CommonOption::NotCommon => {}
        }

        let Some(arg) = args.pop_front() else {
            break;
        };

        if arg.starts_with('-') {
            return Err(CliError::usage_with(format!("unknown option `{arg}`")));
        }

        if input.is_some() {
            return Err(CliError::usage_with(format!(
                "too many arguments for `{command}`"
            )));
        }
        input = Some(PathBuf::from(arg));
    }

    input.ok_or_else(|| CliError::usage_with(format!("missing input ABC file for `{command}`")))
}

fn run_check_pipeline(source: &str, options: ParseOptions) -> CheckResult {
    let parse_report = parse_document(source, options);
    run_check_pipeline_with_options(parse_report)
}

fn run_check_pipeline_with_options(parse_report: ParseReport<AbcDocument>) -> CheckResult {
    let document = parse_report.value;
    let mut diagnostics = parse_report.diagnostics;
    let mut score = None;

    if !has_errors(&diagnostics) {
        let lower_report = lower_score(&document, LowerOptions);
        diagnostics.extend(lower_report.diagnostics);
        score = lower_report.value;
    }

    CheckResult {
        document,
        score,
        diagnostics,
    }
}

fn run_export_pipeline(source: &str, options: ParseOptions) -> PipelineResult {
    let parse_report = parse_document(source, options);
    run_export_pipeline_with_options(parse_report)
}

fn run_export_pipeline_with_options(parse_report: ParseReport<AbcDocument>) -> PipelineResult {
    let CheckResult {
        document,
        score,
        mut diagnostics,
    } = run_check_pipeline_with_options(parse_report);
    let mut musicxml = None;

    if !has_errors(&diagnostics)
        && let Some(score) = score.as_ref()
    {
        let write_report = write_musicxml(score);
        diagnostics.extend(write_report.diagnostics);
        musicxml = Some(write_report.musicxml);
    }

    PipelineResult {
        document,
        score,
        diagnostics,
        musicxml,
    }
}

fn read_source(path: &Path) -> Result<String, CliError> {
    fs::read_to_string(path)
        .map_err(|error| CliError::message(format!("failed to read `{}`: {error}", path.display())))
}

fn write_output(path: &Path, content: &str) -> Result<(), CliError> {
    fs::write(path, content).map_err(|error| {
        CliError::message(format!("failed to write `{}`: {error}", path.display()))
    })
}

fn flush_stdout() -> Result<(), CliError> {
    io::stdout()
        .flush()
        .map_err(|error| CliError::message(format!("failed to write stdout: {error}")))
}

fn consume_common_option(
    args: &mut VecDeque<String>,
    options: &mut CliOptions,
) -> Result<CommonOption, CliError> {
    let Some(arg) = args.front().map(String::as_str) else {
        return Ok(CommonOption::NotCommon);
    };

    match arg {
        "--strict" => {
            args.pop_front();
            options.set_parse_mode(ParseMode::Strict)?;
            Ok(CommonOption::Consumed)
        }
        "--loose" => {
            args.pop_front();
            options.set_parse_mode(ParseMode::Loose)?;
            Ok(CommonOption::Consumed)
        }
        "--recover" => {
            args.pop_front();
            options.set_parse_mode(ParseMode::Recover)?;
            Ok(CommonOption::Consumed)
        }
        "--abc-2.2-draft" => {
            args.pop_front();
            options.spec = AbcSpecVersion::V22Draft;
            Ok(CommonOption::Consumed)
        }
        "--warnings-as-errors" => {
            args.pop_front();
            options.warnings_as_errors = true;
            Ok(CommonOption::Consumed)
        }
        "--diagnostics" => {
            args.pop_front();
            let Some(value) = args.pop_front() else {
                return Err(CliError::usage_with("missing value for `--diagnostics`"));
            };
            options.diagnostics = DiagnosticsFormat::parse(&value)?;
            Ok(CommonOption::Consumed)
        }
        "--help" | "-h" => {
            args.pop_front();
            Ok(CommonOption::Help)
        }
        _ if arg.starts_with("--diagnostics=") => {
            let Some(arg) = args.pop_front() else {
                return Err(CliError::usage());
            };
            let Some((_, value)) = arg.split_once('=') else {
                return Err(CliError::usage());
            };
            options.diagnostics = DiagnosticsFormat::parse(value)?;
            Ok(CommonOption::Consumed)
        }
        _ if arg.starts_with("--") => Err(CliError::usage_with(format!("unknown option `{arg}`"))),
        _ => Ok(CommonOption::NotCommon),
    }
}

fn emit_diagnostics(
    options: &CliOptions,
    source: &SourceText,
    path: &Path,
    diagnostics: &[Diagnostic],
) -> Result<(), CliError> {
    if diagnostics.is_empty() {
        return Ok(());
    }

    let mut stderr = io::stderr();
    match options.diagnostics {
        DiagnosticsFormat::Text => {
            for diagnostic in diagnostics {
                write_text_diagnostic(&mut stderr, source, path, diagnostic)?;
            }
        }
        DiagnosticsFormat::Json => {
            let payload = diagnostics_json(source, path, diagnostics)?;
            writeln!(stderr, "{payload}")
                .map_err(|error| CliError::message(format!("failed to write stderr: {error}")))?;
        }
    }
    Ok(())
}

fn write_text_diagnostic(
    output: &mut impl Write,
    source: &SourceText,
    path: &Path,
    diagnostic: &Diagnostic,
) -> Result<(), CliError> {
    let line_span = source.line_column_span(diagnostic.span);
    if let Some(line_span) = line_span {
        writeln!(
            output,
            "{}:{}:{}-{}:{}: {}[{}]: {}",
            path.display(),
            line_span.start.line,
            line_span.start.column,
            line_span.end.line,
            line_span.end.column,
            severity_name(diagnostic.severity),
            diagnostic.code,
            diagnostic.message
        )
        .map_err(stderr_error)?;
    } else {
        writeln!(
            output,
            "{}: bytes {}..{}: {}[{}]: {}",
            path.display(),
            diagnostic.span.start,
            diagnostic.span.end,
            severity_name(diagnostic.severity),
            diagnostic.code,
            diagnostic.message
        )
        .map_err(stderr_error)?;
    }

    writeln!(
        output,
        "  byte span {}..{}",
        diagnostic.span.start, diagnostic.span.end
    )
    .map_err(stderr_error)?;

    if let Some(snippet) = source_snippet(source, diagnostic) {
        writeln!(output, "  | {}", snippet.line).map_err(stderr_error)?;
        writeln!(output, "  | {}", snippet.marker).map_err(stderr_error)?;
    }

    Ok(())
}

fn diagnostics_json(
    source: &SourceText,
    path: &Path,
    diagnostics: &[Diagnostic],
) -> Result<String, CliError> {
    let values = diagnostics
        .iter()
        .map(|diagnostic| {
            let line_span = source.line_column_span(diagnostic.span).map(|span| {
                json!({
                    "start": {
                        "line": span.start.line,
                        "column": span.start.column,
                    },
                    "end": {
                        "line": span.end.line,
                        "column": span.end.column,
                    },
                })
            });
            let snippet = source_snippet(source, diagnostic).map(|snippet| {
                json!({
                    "line": snippet.line,
                    "marker": snippet.marker,
                })
            });
            json!({
                "path": path.display().to_string(),
                "code": diagnostic.code,
                "severity": severity_name(diagnostic.severity),
                "message": diagnostic.message,
                "span": {
                    "start": diagnostic.span.start,
                    "end": diagnostic.span.end,
                },
                "line_span": line_span,
                "snippet": snippet,
            })
        })
        .collect::<Vec<_>>();

    serde_json::to_string_pretty(&values)
        .map_err(|error| CliError::message(format!("failed to serialize diagnostics: {error}")))
}

fn source_snippet(source: &SourceText, diagnostic: &Diagnostic) -> Option<Snippet> {
    let line_span = source.line_column_span(diagnostic.span)?;
    let line_index = line_span.start.line.checked_sub(1)?;
    let line = source.line_text(line_index)?.to_owned();
    let start_column = line_span.start.column.max(1);
    let marker_width = if line_span.start.line == line_span.end.line {
        line_span.end.column.saturating_sub(start_column).max(1)
    } else {
        1
    };
    let marker = format!(
        "{}{}",
        " ".repeat(start_column.saturating_sub(1)),
        "^".repeat(marker_width)
    );

    Some(Snippet { line, marker })
}

fn diagnostics_should_fail(diagnostics: &[Diagnostic], warnings_as_errors: bool) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            || (warnings_as_errors && diagnostic.severity == Severity::Warning)
    })
}

fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

fn usage() -> &'static str {
    "usage: croma [OPTIONS] <COMMAND>\n\ncommands:\n  xml <file.abc> [-o out.musicxml]\n  check <file.abc>\n  dump tokens <file.abc>\n  dump tree <file.abc>\n  dump score <file.abc>\n\noptions:\n  --strict\n  --loose\n  --recover\n  --abc-2.2-draft\n  --diagnostics text|json\n  --warnings-as-errors"
}

#[derive(Debug)]
struct PipelineResult {
    document: AbcDocument,
    score: Option<Score>,
    diagnostics: Vec<Diagnostic>,
    musicxml: Option<String>,
}

#[derive(Debug)]
struct CheckResult {
    document: AbcDocument,
    score: Option<Score>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Copy)]
enum CommonOption {
    Consumed,
    Help,
    NotCommon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiagnosticsFormat {
    Text,
    Json,
}

impl DiagnosticsFormat {
    fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            _ => Err(CliError::usage_with(format!(
                "invalid diagnostics format `{value}`"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DumpKind {
    Tokens,
    Tree,
    Score,
}

impl DumpKind {
    fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "tokens" => Ok(Self::Tokens),
            "tree" => Ok(Self::Tree),
            "score" => Ok(Self::Score),
            _ => Err(CliError::usage_with(format!("invalid dump kind `{value}`"))),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct CliOptions {
    spec: AbcSpecVersion,
    parse_mode: ParseMode,
    parse_mode_seen: bool,
    diagnostics: DiagnosticsFormat,
    warnings_as_errors: bool,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            spec: AbcSpecVersion::V21,
            parse_mode: ParseMode::Strict,
            parse_mode_seen: false,
            diagnostics: DiagnosticsFormat::Text,
            warnings_as_errors: false,
        }
    }
}

impl CliOptions {
    fn parse_options(self) -> ParseOptions {
        ParseOptions::new(self.spec, self.parse_mode)
    }

    fn set_parse_mode(&mut self, mode: ParseMode) -> Result<(), CliError> {
        if self.parse_mode_seen {
            return Err(CliError::usage_with(
                "choose only one of --strict, --loose, or --recover",
            ));
        }
        self.parse_mode = mode;
        self.parse_mode_seen = true;
        Ok(())
    }
}

#[derive(Debug)]
struct Snippet {
    line: String,
    marker: String,
}

#[derive(Debug)]
struct CliError {
    message: String,
}

impl CliError {
    fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn usage() -> Self {
        Self::message(usage())
    }

    fn usage_with(message: impl Into<String>) -> Self {
        Self::message(format!("{}\n\n{}", message.into(), usage()))
    }
}

fn stderr_error(error: io::Error) -> CliError {
    CliError::message(format!("failed to write stderr: {error}"))
}
