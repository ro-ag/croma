use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

static NEXT_TEST_DIR: AtomicUsize = AtomicUsize::new(0);

#[test]
fn xml_emits_musicxml_to_stdout() {
    let input = basic_abc();
    let output = run_croma([os("xml"), input.as_os_str()]);

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(stdout.contains("<score-partwise version=\"4.0\">"));
    assert!(stdout.contains("<work-title>Scale</work-title>"));
    assert!(stderr(&output).is_empty());
}

#[test]
fn xml_writes_to_output_file_and_keeps_stdout_quiet() {
    let dir = TestDir::new("xml-output");
    let input = basic_abc();
    let output_path = dir.path().join("basic.musicxml");
    let output = run_croma([
        os("xml"),
        input.as_os_str(),
        os("-o"),
        output_path.as_os_str(),
    ]);

    assert_success(&output);
    assert!(stdout(&output).is_empty());
    let written = read_file(&output_path);
    assert!(written.contains("<score-partwise version=\"4.0\">"));
    assert!(written.contains("<part-name>Scale</part-name>"));
}

#[test]
fn check_accepts_basic_abc() {
    let input = basic_abc();
    let output = run_croma([os("check"), input.as_os_str()]);

    assert_success(&output);
    assert!(stdout(&output).is_empty());
    assert!(stderr(&output).is_empty());
}

#[test]
fn dump_commands_emit_debug_output() {
    let cases = [
        ("tokens", "SurfaceToken"),
        ("tree", "LineMap"),
        ("score", "Score"),
    ];

    for (kind, expected) in cases {
        let input = basic_abc();
        let output = run_croma([os("dump"), OsStr::new(kind), input.as_os_str()]);

        assert_success(&output);
        assert!(stdout(&output).contains(expected));
    }
}

#[test]
fn json_diagnostics_are_parseable_when_diagnostics_exist() {
    let dir = TestDir::new("json-diagnostics");
    let file = dir.write("missing-key.abc", "X:1\nT:No Key\n");
    let output = run_croma([
        os("check"),
        os("--diagnostics"),
        os("json"),
        file.as_os_str(),
    ]);

    assert_failure(&output);
    let diagnostics = json_diagnostics(&output);
    assert_eq!(diagnostics[0]["code"], json!("abc.file.missing_k"));
    assert_eq!(diagnostics[0]["severity"], json!("error"));
    assert_eq!(diagnostics[0]["span"], json!({ "start": 13, "end": 13 }));
    assert_eq!(diagnostics[0]["snippet"]["marker"], json!("^"));
}

#[test]
fn missing_file_exits_nonzero_with_clear_error() {
    let path = workspace_root().join("does-not-exist.abc");
    let output = run_croma([os("check"), path.as_os_str()]);

    assert_failure(&output);
    let stderr = stderr(&output);
    assert!(stderr.contains("failed to read"));
    assert!(stderr.contains("does-not-exist.abc"));
}

#[test]
fn invalid_command_exits_nonzero_and_prints_usage() {
    let output = run_croma([os("nope")]);

    assert_failure(&output);
    let stderr = stderr(&output);
    // Wording updated for clap: the hand-rolled parser said "unknown command
    // `nope`" / "usage: croma"; clap emits "unrecognized subcommand 'nope'" and
    // a "Usage:" line. Behavior preserved: non-zero exit plus usage on stderr.
    assert!(stderr.contains("unrecognized subcommand 'nope'"));
    assert!(stderr.contains("Usage: croma"));
}

#[test]
fn invalid_option_combination_exits_nonzero() {
    let input = basic_abc();
    let output = run_croma([
        os("check"),
        os("--strict"),
        os("--loose"),
        input.as_os_str(),
    ]);

    assert_failure(&output);
    // Wording updated for clap: conflicting parse-mode flags are now enforced by
    // a clap ArgGroup, which reports "cannot be used with". Behavior preserved:
    // non-zero exit when more than one of --strict/--loose/--recover is given.
    assert!(stderr(&output).contains("cannot be used with"));
}

#[test]
fn invalid_abc_check_reports_code_span_and_snippet() {
    let dir = TestDir::new("invalid-abc");
    let file = dir.write("missing-key.abc", "X:1\nT:No Key\n");
    let output = run_croma([os("check"), file.as_os_str()]);

    assert_failure(&output);
    let stderr = stderr(&output);
    assert!(stderr.contains("error[abc.file.missing_k]"));
    assert!(stderr.contains("byte span 13..13"));
    assert!(stderr.contains("  | ^"));
}

#[test]
fn warnings_as_errors_turns_warning_only_input_into_failure() {
    let dir = TestDir::new("warnings-as-errors");
    let file = dir.write("warning.abc", "X:1\nT:Warn\nH:ignored\nK:C\nC\n");

    let warning_only = run_croma([os("check"), file.as_os_str()]);
    assert_success(&warning_only);
    assert!(stderr(&warning_only).contains("warning[abc.field.unknown]"));

    let warnings_as_errors = run_croma([os("check"), os("--warnings-as-errors"), file.as_os_str()]);
    assert_failure(&warnings_as_errors);
    assert!(stderr(&warnings_as_errors).contains("warning[abc.field.unknown]"));
}

#[test]
fn fmt_writes_canonical_formatting_to_stdout() {
    let dir = TestDir::new("fmt-stdout");
    let file = dir.write("unformatted.abc", "X:1\nK:C\nC   D  |\n");
    let output = run_croma([os("fmt"), file.as_os_str()]);

    assert_success(&output);
    assert!(stdout(&output).contains("C D |"));
    assert!(stderr(&output).is_empty());
}

#[test]
fn fmt_check_exits_zero_when_already_formatted() {
    let dir = TestDir::new("fmt-check-clean");
    let file = dir.write("formatted.abc", "X:1\nK:C\nC D |\n");
    let output = run_croma([os("fmt"), os("--check"), file.as_os_str()]);

    assert_success(&output);
    assert!(stdout(&output).is_empty());
    assert!(stderr(&output).is_empty());
}

#[test]
fn fmt_check_exits_one_and_reports_when_unformatted() {
    let dir = TestDir::new("fmt-check-dirty");
    let file = dir.write("unformatted.abc", "X:1\nK:C\nC   D  |\n");
    let output = run_croma([os("fmt"), os("--check"), file.as_os_str()]);

    assert_failure(&output);
    assert!(stdout(&output).is_empty());
    assert!(stderr(&output).contains("would reformat:"));
    assert!(stderr(&output).contains("unformatted.abc"));
}

#[test]
fn fmt_write_updates_file_in_place() {
    let dir = TestDir::new("fmt-write");
    let file = dir.write("unformatted.abc", "X:1\nK:C\nC   D  |\n");
    let output = run_croma([os("fmt"), os("-w"), file.as_os_str()]);

    assert_success(&output);
    assert!(stdout(&output).is_empty());
    assert!(read_file(&file).contains("C D |"));
}

#[test]
fn fmt_auto_fix_applies_detached_length_and_reports_it() {
    let dir = TestDir::new("fmt-auto-fix");
    let file = dir.write("detached.abc", "X:1\nK:C\ng 2\n");
    let output = run_croma([os("fmt"), os("--auto-fix"), file.as_os_str()]);

    assert_success(&output);
    assert!(stdout(&output).contains("g2"));
    assert!(stderr(&output).contains("fixed"));
    assert!(stderr(&output).contains("detached-length"));
}

#[test]
fn fmt_check_and_write_together_is_a_usage_error() {
    let dir = TestDir::new("fmt-check-write");
    let file = dir.write("unformatted.abc", "X:1\nK:C\nC   D  |\n");
    let output = run_croma([os("fmt"), os("--check"), os("-w"), file.as_os_str()]);

    assert_failure(&output);
}

#[test]
fn xml_output_write_error_does_not_dump_partial_xml_to_stdout() {
    let input = basic_abc();
    let output = run_croma([
        os("xml"),
        input.as_os_str(),
        os("-o"),
        Path::new("/dev/null/out.musicxml").as_os_str(),
    ]);

    assert_failure(&output);
    assert!(stdout(&output).is_empty());
    assert!(stderr(&output).contains("failed to write"));
}

#[test]
fn json_diagnostics_escape_quotes_and_xml_like_text_in_messages() {
    let dir = TestDir::new("json-escaping");
    let file = dir.write(
        "quoted-message.abc",
        "X:1\nT:Quote\nI:weird\"<x> value\nK:C\nC\n",
    );
    let output = run_croma([os("check"), os("--diagnostics=json"), file.as_os_str()]);

    assert_success(&output);
    let diagnostics = json_diagnostics(&output);
    assert_eq!(
        diagnostics[0]["code"],
        json!("abc.field.unknown_instruction")
    );
    assert_eq!(
        diagnostics[0]["message"],
        json!("Unknown I: instruction `weird\"<x>` was ignored")
    );
}

#[test]
fn corpus_smoke_script_handles_failing_file_without_stopping() {
    let dir = TestDir::new("corpus-smoke");
    let corpus = dir.path().join("corpus");
    create_dir(&corpus);
    write_path(
        &corpus.join("good.abc"),
        "X:1\nT:Good\nM:4/4\nL:1/8\nK:C\nC D E F|\n",
    );
    write_path(&corpus.join("bad.abc"), "X:1\nT:Missing Key\n");
    let report_path = dir.path().join("report.json");

    let mut command = Command::new("python3");
    command.current_dir(workspace_root());
    command.arg(workspace_root().join("tools/corpus_smoke.py"));
    command.arg("--croma");
    command.arg(env!("CARGO_BIN_EXE_croma"));
    command.arg("--corpus");
    command.arg(&corpus);
    command.arg("--report");
    command.arg(&report_path);
    command.arg("--mode");
    command.arg("check");
    let output = command_output(command);

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("attempted: 2"));
    assert!(stdout.contains("successes: 1"));
    assert!(stdout.contains("failures: 1"));

    let report = json_file(&report_path);
    assert_eq!(report["files_discovered"], json!(2));
    assert_eq!(report["total_files_attempted"], json!(2));
    assert_eq!(report["successes"], json!(1));
    assert_eq!(report["failures"], json!(1));
    assert_eq!(
        report["first_failures"][0]["diagnostics"][0]["code"],
        json!("abc.file.missing_k")
    );
}

#[test]
fn corpus_harness_report_format_is_stable() {
    let dir = TestDir::new("corpus-harness-format");
    let corpus = dir.path().join("corpus");
    create_dir(&corpus);
    write_path(
        &corpus.join("good.abc"),
        "X:1\nT:Good\nM:4/4\nL:1/8\nK:C\nC D E F|\n",
    );
    let report_path = dir.path().join("report.json");
    let results_path = dir.path().join("results.jsonl");

    let output = run_corpus_harness([
        os("--croma"),
        os(env!("CARGO_BIN_EXE_croma")),
        os("--corpus"),
        corpus.as_os_str(),
        os("--report"),
        report_path.as_os_str(),
        os("--results-jsonl"),
        results_path.as_os_str(),
        os("--mode"),
        os("check"),
    ]);

    assert_success(&output);
    let report = json_file(&report_path);
    assert_eq!(report["schema"], json!("croma-corpus-harness-v1"));
    assert_eq!(report["mode"], json!("check"));
    assert_eq!(report["files_discovered"], json!(1));
    assert_eq!(report["files_selected"], json!(1));
    assert_eq!(report["files_attempted"], json!(1));
    assert_eq!(report["successes"], json!(1));
    assert_eq!(report["failures"], json!(0));
    assert_eq!(report["music21"]["enabled"], json!(false));
    assert!(read_file(&report_path).contains("\"label\": \"Croma bug\""));

    let results = jsonl_file(&results_path);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["schema"], json!("croma-corpus-result-v1"));
    assert_eq!(results[0]["status"], json!("success"));
}

#[test]
fn corpus_harness_keeps_xml_without_music21_compare() {
    let dir = TestDir::new("corpus-harness-keep-xml");
    let corpus = dir.path().join("corpus");
    let xml_dir = dir.path().join("xml");
    create_dir(&corpus);
    write_path(
        &corpus.join("good.abc"),
        "X:1\nT:Good\nM:4/4\nL:1/8\nK:C\nC D E F|\n",
    );
    let report_path = dir.path().join("report.json");
    let results_path = dir.path().join("results.jsonl");

    let output = run_corpus_harness([
        os("--croma"),
        os(env!("CARGO_BIN_EXE_croma")),
        os("--corpus"),
        corpus.as_os_str(),
        os("--report"),
        report_path.as_os_str(),
        os("--results-jsonl"),
        results_path.as_os_str(),
        os("--mode"),
        os("xml"),
        os("--keep-xml-dir"),
        xml_dir.as_os_str(),
    ]);

    assert_success(&output);
    let kept_xml = read_file(&xml_dir.join("good.croma.musicxml"));
    assert!(kept_xml.contains("<score-partwise version=\"4.0\">"));
    assert!(kept_xml.contains("<work-title>Good</work-title>"));
    assert_eq!(json_file(&report_path)["music21"]["enabled"], json!(false));
}

#[test]
fn corpus_harness_handles_failing_file_without_stopping() {
    let dir = TestDir::new("corpus-harness-failures");
    let corpus = dir.path().join("corpus");
    create_dir(&corpus);
    write_path(
        &corpus.join("good.abc"),
        "X:1\nT:Good\nM:4/4\nL:1/8\nK:C\nC D E F|\n",
    );
    write_path(&corpus.join("bad.abc"), "X:1\nT:Missing Key\n");
    let report_path = dir.path().join("report.json");
    let results_path = dir.path().join("results.jsonl");

    let output = run_corpus_harness([
        os("--croma"),
        os(env!("CARGO_BIN_EXE_croma")),
        os("--corpus"),
        corpus.as_os_str(),
        os("--report"),
        report_path.as_os_str(),
        os("--results-jsonl"),
        results_path.as_os_str(),
        os("--mode"),
        os("check"),
    ]);

    assert_success(&output);
    let stdout = stdout(&output);
    assert!(stdout.contains("attempted: 2"));
    assert!(stdout.contains("successes: 1"));
    assert!(stdout.contains("failures: 1"));

    let report = json_file(&report_path);
    assert_eq!(report["files_attempted"], json!(2));
    assert_eq!(report["successes"], json!(1));
    assert_eq!(report["failures"], json!(1));
    assert_eq!(
        report["failing_files"][0]["diagnostics"][0]["code"],
        json!("abc.file.missing_k")
    );
    assert_eq!(
        report["failing_files"][0]["source_snippet"]["lines"][0]["text"],
        json!("X:1")
    );
}

#[test]
fn corpus_harness_keeps_quoted_diagnostic_json_valid() {
    let dir = TestDir::new("corpus-harness-json");
    let corpus = dir.path().join("corpus");
    create_dir(&corpus);
    write_path(
        &corpus.join("quoted.abc"),
        "X:1\nT:Quote\nI:weird\"<x> value\nK:C\nC\n",
    );
    let report_path = dir.path().join("report.json");
    let results_path = dir.path().join("results.jsonl");

    let output = run_corpus_harness([
        os("--croma"),
        os(env!("CARGO_BIN_EXE_croma")),
        os("--corpus"),
        corpus.as_os_str(),
        os("--report"),
        report_path.as_os_str(),
        os("--results-jsonl"),
        results_path.as_os_str(),
        os("--mode"),
        os("check"),
    ]);

    assert_success(&output);
    let report = json_file(&report_path);
    assert_eq!(report["files_attempted"], json!(1));
    assert_eq!(
        report["results"][0]["diagnostics"][0]["message"],
        json!("Unknown I: instruction `weird\"<x>` was ignored")
    );
    assert_eq!(
        report["top_diagnostic_codes"][0],
        json!({ "code": "abc.field.unknown_instruction", "count": 1 })
    );
}

#[test]
fn corpus_harness_classification_round_trips_through_resume_jsonl() {
    let dir = TestDir::new("corpus-harness-classification");
    let corpus = dir.path().join("corpus");
    create_dir(&corpus);
    write_path(&corpus.join("bad.abc"), "X:1\nT:Missing Key\n");
    let classifications_path = dir.path().join("classifications.json");
    write_path(
        &classifications_path,
        "{\n  \"files\": {\n    \"bad.abc\": \"malformed ABC\"\n  }\n}\n",
    );
    let report_path = dir.path().join("report.json");
    let results_path = dir.path().join("results.jsonl");

    let first = run_corpus_harness([
        os("--croma"),
        os(env!("CARGO_BIN_EXE_croma")),
        os("--corpus"),
        corpus.as_os_str(),
        os("--report"),
        report_path.as_os_str(),
        os("--results-jsonl"),
        results_path.as_os_str(),
        os("--classifications"),
        classifications_path.as_os_str(),
    ]);

    assert_success(&first);
    let first_report = json_file(&report_path);
    assert_eq!(
        first_report["failing_files"][0]["classification"]["id"],
        json!("malformed_abc")
    );

    let resumed = run_corpus_harness([
        os("--croma"),
        os(env!("CARGO_BIN_EXE_croma")),
        os("--corpus"),
        corpus.as_os_str(),
        os("--report"),
        report_path.as_os_str(),
        os("--results-jsonl"),
        results_path.as_os_str(),
        os("--classifications"),
        classifications_path.as_os_str(),
        os("--resume"),
    ]);

    assert_success(&resumed);
    let resumed_report = json_file(&report_path);
    assert_eq!(resumed_report["files_skipped_by_resume"], json!(1));
    assert_eq!(
        resumed_report["failing_files"][0]["classification"]["id"],
        json!("malformed_abc")
    );
    assert_eq!(jsonl_file(&results_path).len(), 1);
}

fn run_croma<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(env!("CARGO_BIN_EXE_croma"));
    command.args(args);
    command.current_dir(workspace_root());
    command_output(command)
}

fn run_corpus_harness<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("python3");
    command.arg(workspace_root().join("tools/corpus_harness.py"));
    command.args(args);
    command.current_dir(workspace_root());
    command_output(command)
}

fn command_output(mut command: Command) -> Output {
    match command.output() {
        Ok(output) => output,
        Err(error) => panic!("failed to execute command: {error}"),
    }
}

fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let Some(workspace_root) = manifest_dir.parent().and_then(Path::parent) else {
        panic!("failed to resolve workspace root from CARGO_MANIFEST_DIR");
    };
    workspace_root.to_path_buf()
}

fn basic_abc() -> PathBuf {
    workspace_root().join("examples/basic.abc")
}

fn os(value: &str) -> &OsStr {
    OsStr::new(value)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        stderr(output)
    );
}

fn assert_failure(output: &Output) {
    assert!(
        !output.status.success(),
        "expected failure\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        stderr(output)
    );
}

fn json_diagnostics(output: &Output) -> Value {
    match serde_json::from_slice(&output.stderr) {
        Ok(value) => value,
        Err(error) => panic!("failed to parse stderr as JSON diagnostics: {error}"),
    }
}

fn read_file(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) => panic!("failed to read {}: {error}", path.display()),
    }
}

fn json_file(path: &Path) -> Value {
    match serde_json::from_str(&read_file(path)) {
        Ok(value) => value,
        Err(error) => panic!("failed to parse {} as JSON: {error}", path.display()),
    }
}

fn jsonl_file(path: &Path) -> Vec<Value> {
    let content = read_file(path);
    let mut values = Vec::new();
    for (line_index, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str(line) {
            Ok(value) => values.push(value),
            Err(error) => panic!(
                "failed to parse {} line {} as JSON: {error}",
                path.display(),
                line_index + 1
            ),
        }
    }
    values
}

fn create_dir(path: &Path) {
    if let Err(error) = fs::create_dir_all(path) {
        panic!("failed to create {}: {error}", path.display());
    }
}

fn write_path(path: &Path, content: &str) {
    if let Err(error) = fs::write(path, content) {
        panic!("failed to write {}: {error}", path.display());
    }
}

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let id = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("croma-cli-{name}-{}-{id}", std::process::id()));
        if let Err(error) = fs::create_dir_all(&path) {
            panic!("failed to create {}: {error}", path.display());
        }
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, name: &str, content: &str) -> PathBuf {
        let path = self.path.join(name);
        if let Err(error) = fs::write(&path, content) {
            panic!("failed to write {}: {error}", path.display());
        }
        path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ignored = fs::remove_dir_all(&self.path);
    }
}
