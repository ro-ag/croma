use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use anstream::stderr as color_stderr;
use clap::Parser;
use croma_core::{
    AbcDocument, Diagnostic, LowerOptions, ParseOptions, ParseReport, Score, Severity, SourceText,
    lower_score, parse_document, write_musicxml,
};
use croma_fmt::{
    Change, FixKind, FixResult, FormatOptions, auto_fix, format as fmt_format, is_formatted,
};
use owo_colors::OwoColorize;
use serde_json::json;

mod cli;

use cli::{
    CheckArgs, Cli, Command, CommonArgs, DiagnosticsFormat, DumpArgs, DumpKind, FmtArgs, XmlArgs,
};

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            // clap renders help/version to stdout (exit 0) and errors to stderr
            // (exit 2). Preserve that convention; the integration tests only
            // require a non-zero exit and that usage is printed somewhere
            // sensible, which clap handles.
            error.print().ok();
            return if error.use_stderr() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            };
        }
    };

    match run(cli) {
        Ok(code) => code,
        Err(error) => {
            let _ = writeln!(color_stderr(), "{}", error.message);
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, CliError> {
    match cli.command {
        Command::Xml(args) => run_xml(args),
        Command::Check(args) => run_check(args),
        Command::Dump(args) => run_dump(args),
        Command::Fmt(args) => run_fmt(args),
    }
}

fn run_xml(args: XmlArgs) -> Result<ExitCode, CliError> {
    let XmlArgs {
        file,
        output,
        common,
    } = args;
    let source = read_source(&file)?;
    let source_text = SourceText::with_file_name(source.clone(), file.display().to_string());
    let PipelineResult {
        document: _document,
        score: _score,
        diagnostics,
        musicxml,
    } = run_export_pipeline(&source, common.parse_options());

    emit_diagnostics(&common, &source_text, &file, &diagnostics)?;

    if diagnostics_should_fail(&diagnostics, common.warnings_as_errors()) {
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

fn run_check(args: CheckArgs) -> Result<ExitCode, CliError> {
    let CheckArgs { file, common } = args;
    let source = read_source(&file)?;
    let source_text = SourceText::with_file_name(source.clone(), file.display().to_string());
    let CheckResult { diagnostics, .. } = run_check_pipeline(&source, common.parse_options());

    emit_diagnostics(&common, &source_text, &file, &diagnostics)?;

    if diagnostics_should_fail(&diagnostics, common.warnings_as_errors()) {
        Ok(ExitCode::FAILURE)
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn run_dump(args: DumpArgs) -> Result<ExitCode, CliError> {
    let DumpArgs { kind, file, common } = args;
    let source = read_source(&file)?;
    let source_text = SourceText::with_file_name(source.clone(), file.display().to_string());
    let report = parse_document(&source, common.parse_options());
    let document = report.value;
    let mut diagnostics = report.diagnostics;

    match kind {
        DumpKind::Tokens => {
            emit_diagnostics(&common, &source_text, &file, &diagnostics)?;
            if diagnostics_should_fail(&diagnostics, common.warnings_as_errors()) {
                return Ok(ExitCode::FAILURE);
            }
            println!("{:#?}", document.surface.tokens);
        }
        DumpKind::Tree => {
            emit_diagnostics(&common, &source_text, &file, &diagnostics)?;
            if diagnostics_should_fail(&diagnostics, common.warnings_as_errors()) {
                return Ok(ExitCode::FAILURE);
            }
            println!("LineMap:\n{:#?}", document.surface.line_map);
            println!("Fields:\n{:#?}", document.fields);
            println!("Music:\n{:#?}", document.music);
        }
        DumpKind::Score => {
            let lower_report = lower_score(&document, LowerOptions);
            diagnostics.extend(lower_report.diagnostics);
            emit_diagnostics(&common, &source_text, &file, &diagnostics)?;
            if diagnostics_should_fail(&diagnostics, common.warnings_as_errors()) {
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

fn run_fmt(args: FmtArgs) -> Result<ExitCode, CliError> {
    let FmtArgs {
        file,
        check,
        write,
        auto_fix: use_auto_fix,
        common,
    } = args;

    if check && write {
        return Err(CliError::message(
            "`--check` and `--write` cannot be used together",
        ));
    }

    let source = read_source(&file)?;
    let options = FormatOptions {
        parse: common.parse_options(),
    };

    if use_auto_fix {
        let FixResult {
            output,
            changes,
            skipped,
        } = auto_fix(&source, options);
        report_fixes(&changes, &skipped)?;

        if check {
            return check_result(output == source, &file);
        }
        if write {
            write_output(&file, &output)?;
        } else {
            print!("{output}");
            flush_stdout()?;
        }
        return Ok(ExitCode::SUCCESS);
    }

    if check {
        return check_result(is_formatted(&source, options), &file);
    }

    let formatted = fmt_format(&source, options);
    if write {
        write_output(&file, &formatted)?;
    } else {
        print!("{formatted}");
        flush_stdout()?;
    }

    Ok(ExitCode::SUCCESS)
}

/// Resolve a `--check` outcome: exit 0 (clean) or exit 1 with a `would reformat`
/// note on stderr. `clean` is true when the file already matches the canonical
/// (or auto-fixed) output.
fn check_result(clean: bool, file: &Path) -> Result<ExitCode, CliError> {
    if clean {
        Ok(ExitCode::SUCCESS)
    } else {
        let mut stderr = color_stderr();
        writeln!(stderr, "would reformat: {}", file.display()).map_err(stderr_error)?;
        Ok(ExitCode::FAILURE)
    }
}

/// Emit one stderr line per applied or skipped auto-fix change.
fn report_fixes(changes: &[Change], skipped: &[Change]) -> Result<(), CliError> {
    let mut stderr = color_stderr();
    for change in changes {
        writeln!(
            stderr,
            "fixed [{}] {}",
            change.kind.label().green(),
            fix_detail(change)
        )
        .map_err(stderr_error)?;
    }
    for change in skipped {
        writeln!(
            stderr,
            "skipped (would change notes) [{}] {}",
            change.kind.label().yellow(),
            fix_detail(change)
        )
        .map_err(stderr_error)?;
    }
    Ok(())
}

/// A human-readable description of a curation. When the edit's before/after text
/// is meaningful (chord unwrap, tempo collapse) show `` `before` -> `after` ``;
/// otherwise (e.g. removing whitespace for a detached length) describe the kind.
fn fix_detail(change: &Change) -> String {
    if change.before.trim().is_empty() {
        match change.kind {
            FixKind::DetachedLength => "joined a length to its note".to_string(),
            FixKind::ChordSymbolInBrackets => "moved chord symbol out of brackets".to_string(),
            FixKind::DoubledTempo => "collapsed a doubled tempo".to_string(),
        }
    } else {
        format!("`{}` -> `{}`", change.before, change.after)
    }
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

fn emit_diagnostics(
    common: &CommonArgs,
    source: &SourceText,
    path: &Path,
    diagnostics: &[Diagnostic],
) -> Result<(), CliError> {
    if diagnostics.is_empty() {
        return Ok(());
    }

    match common.diagnostics_format() {
        DiagnosticsFormat::Text => {
            // Human-facing diagnostics route through anstream, which strips
            // color when stderr is not a TTY or NO_COLOR is set.
            let mut stderr = color_stderr();
            for diagnostic in diagnostics {
                write_text_diagnostic(&mut stderr, source, path, diagnostic)?;
            }
        }
        DiagnosticsFormat::Json => {
            // JSON must stay byte-identical and parseable: never colorize it.
            // Write to the raw stderr handle so no ANSI processing applies.
            let payload = diagnostics_json(source, path, diagnostics)?;
            let mut stderr = io::stderr();
            writeln!(stderr, "{payload}").map_err(stderr_error)?;
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
    let severity = colored_severity(diagnostic.severity);
    if let Some(line_span) = line_span {
        writeln!(
            output,
            "{}:{}:{}-{}:{}: {}[{}]: {}",
            path.display(),
            line_span.start.line,
            line_span.start.column,
            line_span.end.line,
            line_span.end.column,
            severity,
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
            severity,
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

/// Render a severity label colored by class (error=red, warning=yellow,
/// info=blue). anstream strips the color when stderr is not a TTY, so the
/// underlying text stays `error`/`warning`/`info`.
fn colored_severity(severity: Severity) -> String {
    let name = severity_name(severity);
    match severity {
        Severity::Error => name.red().to_string(),
        Severity::Warning => name.yellow().to_string(),
        Severity::Info => name.blue().to_string(),
    }
}

#[derive(Debug)]
struct PipelineResult {
    #[allow(dead_code)]
    document: AbcDocument,
    #[allow(dead_code)]
    score: Option<Score>,
    diagnostics: Vec<Diagnostic>,
    musicxml: Option<String>,
}

#[derive(Debug)]
struct CheckResult {
    #[allow(dead_code)]
    document: AbcDocument,
    #[allow(dead_code)]
    score: Option<Score>,
    diagnostics: Vec<Diagnostic>,
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
}

fn stderr_error(error: io::Error) -> CliError {
    CliError::message(format!("failed to write stderr: {error}"))
}
