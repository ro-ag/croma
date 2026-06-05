#!/usr/bin/env python3
"""Durable corpus harness for croma CLI runs.

The harness owns corpus discovery, process orchestration, resume, reporting,
and optional MusicXML structural comparison. It deliberately shells out to the
`croma` CLI so parser, lowering, diagnostics, and MusicXML export remain behind
`croma-core`.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import subprocess
import sys
import tempfile
import time
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


REPORT_SCHEMA = "croma-corpus-harness-v1"
RESULT_SCHEMA = "croma-corpus-result-v1"
DEFAULT_CORPUS = Path("/Users/rodox/dev/rs/trd/test/real/abc")

CLASSIFICATION_CATEGORIES = [
    {"id": "croma_bug", "label": "Croma bug"},
    {"id": "malformed_abc", "label": "malformed ABC"},
    {"id": "unsupported_notation", "label": "unsupported notation"},
    {"id": "reference_artifact", "label": "reference artifact"},
    {"id": "music21_tooling_issue", "label": "music21/tooling issue"},
    {"id": "corpus_harness_bug", "label": "corpus harness bug"},
    {"id": "recovery_candidate", "label": "recovery candidate"},
    {"id": "policy_decision", "label": "policy decision"},
    {"id": "unclassified", "label": "unclassified"},
]
CLASSIFICATION_IDS = {item["id"] for item in CLASSIFICATION_CATEGORIES}

MUSIC21_NON_DIFFERENCE_STATUSES = {
    "match",
    "reference_missing",
    "music21_unavailable",
    "music21_tool_failure",
    "music21_tool_timeout",
    "croma_musicxml_parse_failure",
    "reference_musicxml_parse_failure",
}


def main() -> int:
    args = parse_args()
    repo_root = Path(__file__).resolve().parents[1]
    corpus = resolve_corpus(args.corpus)
    report_path = args.report or repo_root / "docs" / "untracked" / "corpus-harness-report.json"
    results_jsonl = args.results_jsonl or report_path.with_suffix(".jsonl")
    compare_script = args.music21_script or repo_root / "tools" / "music21_compare.py"
    classifications = ClassificationMap.load(args.classifications)

    if args.music21_compare and args.mode != "xml":
        raise SystemExit("--music21-compare requires --mode xml")
    if args.music21_compare and args.reference_root is None:
        raise SystemExit("--music21-compare requires --reference-root")

    discovered = discover_abc_files(corpus)
    selected = select_files(discovered, args)
    previous = load_previous_results(results_jsonl, args) if args.resume else {}

    results_jsonl.parent.mkdir(parents=True, exist_ok=True)
    if not args.resume and results_jsonl.exists():
        results_jsonl.write_text("", encoding="utf-8")

    started_at = now_utc()
    started = time.monotonic()
    run_id = args.run_id or started_at.replace(":", "").replace("-", "")

    results: list[dict[str, Any]] = []
    skipped_by_resume = 0
    keep_xml_dir = args.keep_xml_dir.resolve() if args.keep_xml_dir else None
    if keep_xml_dir is not None:
        keep_xml_dir.mkdir(parents=True, exist_ok=True)
    music21_facts_dir = args.music21_facts_dir.resolve() if args.music21_facts_dir else None
    if music21_facts_dir is not None:
        music21_facts_dir.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="croma-corpus-") as temp_dir_name:
        temp_dir = Path(temp_dir_name)
        append_handle = results_jsonl.open("a", encoding="utf-8")
        try:
            for index, abc_path in enumerate(selected, start=1):
                abc_key = str(abc_path)
                if abc_key in previous:
                    item = previous[abc_key]
                    skipped_by_resume += 1
                else:
                    item = run_one(args, abc_path)
                    enrich_result(
                        item=item,
                        args=args,
                        corpus=corpus,
                        classifications=classifications,
                        compare_script=compare_script,
                        temp_dir=temp_dir,
                        keep_xml_dir=keep_xml_dir,
                        music21_facts_dir=music21_facts_dir,
                    )
                    append_jsonl(append_handle, item)

                results.append(item)

                if should_stop(args, item):
                    break

                if args.progress_every and index % args.progress_every == 0:
                    print(f"processed {index}/{len(selected)}", file=sys.stderr)
        finally:
            append_handle.close()

    elapsed = time.monotonic() - started
    report = build_report(
        args=args,
        corpus=corpus,
        report_path=report_path,
        results_jsonl=results_jsonl,
        compare_script=compare_script,
        discovered=discovered,
        selected=selected,
        results=results,
        skipped_by_resume=skipped_by_resume,
        started_at=started_at,
        elapsed=elapsed,
        run_id=run_id,
    )

    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print_summary(report_path, report)

    if report["panics"] or report["hard_errors"] or report["timeouts"]:
        return 2
    if args.fail_on_croma_failure and report["failures"]:
        return 1
    if args.fail_on_mismatch and report["structural_mismatches"]["differences"]:
        return 1
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run croma over an ABC corpus")
    parser.add_argument("--croma", type=Path, default=Path("target/debug/croma"))
    parser.add_argument("--corpus", type=Path)
    parser.add_argument("--mode", choices=["check", "xml"], default="check")
    parser.add_argument("--report", type=Path)
    parser.add_argument("--results-jsonl", type=Path)
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--run-id")
    parser.add_argument("--limit", type=int)
    parser.add_argument("--sample", type=int)
    parser.add_argument("--sample-seed", type=int, default=0)
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--first-failures", type=int, default=20)
    parser.add_argument("--first-mismatches", type=int, default=20)
    parser.add_argument("--top-codes", type=int, default=20)
    parser.add_argument("--snippet-lines", type=int, default=2)
    parser.add_argument("--progress-every", type=int, default=0)
    parser.add_argument("--fail-fast", action="store_true")
    parser.add_argument("--fail-on-croma-failure", action="store_true")
    parser.add_argument("--fail-on-mismatch", action="store_true")
    parser.add_argument("--music21-compare", action="store_true")
    parser.add_argument("--reference-root", type=Path)
    parser.add_argument("--music21-script", type=Path)
    parser.add_argument("--music21-timeout", type=float, default=30.0)
    parser.add_argument("--music21-comparison-engine", choices=["polars", "python"], default="polars")
    parser.add_argument("--music21-facts-dir", type=Path)
    parser.add_argument("--keep-xml-dir", type=Path)
    parser.add_argument("--classifications", type=Path)
    return parser.parse_args()


def resolve_corpus(corpus_arg: Path | None) -> Path:
    if corpus_arg is not None:
        corpus = corpus_arg
    elif os.environ.get("CROMA_CORPUS"):
        corpus = Path(os.environ["CROMA_CORPUS"])
    else:
        corpus = DEFAULT_CORPUS

    corpus = corpus.resolve()
    if not corpus.exists():
        raise SystemExit(f"corpus path does not exist: {corpus}")
    if not corpus.is_dir():
        raise SystemExit(f"corpus path is not a directory: {corpus}")
    return corpus


def discover_abc_files(corpus: Path) -> list[Path]:
    return sorted(
        path.resolve()
        for path in corpus.rglob("*")
        if path.is_file() and path.suffix.lower() == ".abc"
    )


def select_files(discovered: list[Path], args: argparse.Namespace) -> list[Path]:
    if args.limit is not None and args.limit < 0:
        raise SystemExit("--limit must be non-negative")
    if args.sample is not None and args.sample < 0:
        raise SystemExit("--sample must be non-negative")

    selected = discovered
    if args.sample is not None:
        sample_size = min(args.sample, len(discovered))
        selected = random.Random(args.sample_seed).sample(discovered, sample_size)
        selected.sort()
    if args.limit is not None:
        selected = selected[: args.limit]
    return selected


def load_previous_results(results_jsonl: Path, args: argparse.Namespace) -> dict[str, dict[str, Any]]:
    if not results_jsonl.exists():
        return {}

    previous: dict[str, dict[str, Any]] = {}
    for line_number, line in enumerate(results_jsonl.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip():
            continue
        try:
            item = json.loads(line)
        except json.JSONDecodeError as error:
            raise SystemExit(
                f"cannot resume from {results_jsonl}: invalid JSON on line {line_number}: {error}"
            ) from error
        if not isinstance(item, dict):
            raise SystemExit(f"cannot resume from {results_jsonl}: line {line_number} is not an object")
        if item.get("schema") != RESULT_SCHEMA:
            continue
        if item.get("mode") != args.mode:
            continue
        expected_music21 = bool(args.music21_compare)
        music21 = item.get("music21", {})
        actual_music21 = bool(music21.get("enabled", False))
        if expected_music21 != actual_music21:
            continue
        if expected_music21 and music21.get("comparison_engine") != args.music21_comparison_engine:
            continue
        if expected_music21 and args.music21_facts_dir is not None and not music21.get("facts_parquet"):
            continue
        path = item.get("path")
        if isinstance(path, str):
            previous[path] = item
    return previous


def run_one(args: argparse.Namespace, abc_path: Path) -> dict[str, Any]:
    command = [str(args.croma), args.mode, "--diagnostics=json", str(abc_path)]
    started = time.monotonic()
    try:
        completed = subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            timeout=args.timeout,
        )
    except subprocess.TimeoutExpired as error:
        return {
            "schema": RESULT_SCHEMA,
            "path": str(abc_path),
            "relative_path": None,
            "mode": args.mode,
            "command": command,
            "status": "failure",
            "returncode": None,
            "timeout": True,
            "panic": False,
            "hard_error": True,
            "diagnostics": [],
            "diagnostics_parse_error": None,
            "stderr_excerpt": text_excerpt(error.stderr),
            "stdout": stdout_summary(error.stdout or ""),
            "duration_seconds": round(time.monotonic() - started, 3),
        }

    diagnostics, diagnostics_error = parse_diagnostics(completed.stderr)
    panic = "panicked at" in completed.stderr or "thread '" in completed.stderr
    hard_error = completed.returncode != 0 and not diagnostics
    status = "success" if completed.returncode == 0 else "failure"

    return {
        "schema": RESULT_SCHEMA,
        "path": str(abc_path),
        "relative_path": None,
        "mode": args.mode,
        "command": command,
        "status": status,
        "returncode": completed.returncode,
        "timeout": False,
        "panic": panic,
        "hard_error": hard_error,
        "diagnostics": diagnostics,
        "diagnostics_parse_error": diagnostics_error,
        "stderr_excerpt": completed.stderr[:1000],
        "stdout": completed.stdout,
        "duration_seconds": round(time.monotonic() - started, 3),
    }


def enrich_result(
    *,
    item: dict[str, Any],
    args: argparse.Namespace,
    corpus: Path,
    classifications: ClassificationMap,
    compare_script: Path,
    temp_dir: Path,
    keep_xml_dir: Path | None,
    music21_facts_dir: Path | None,
) -> None:
    abc_path = Path(item["path"])
    relative_path = str(abc_path.relative_to(corpus))
    item["relative_path"] = relative_path

    if item["status"] != "success":
        item["source_snippet"] = source_snippet(
            abc_path,
            item.get("diagnostics", []),
            args.snippet_lines,
        )
    elif args.mode == "xml" and keep_xml_dir is not None and not args.music21_compare:
        write_kept_croma_xml(
            croma_xml=item.get("stdout", ""),
            keep_xml_dir=keep_xml_dir,
            relative_path=relative_path,
        )

    if (
        args.music21_compare
        and args.mode == "xml"
        and item["status"] == "success"
        and args.reference_root is not None
    ):
        item["music21"] = run_music21_compare(
            args=args,
            compare_script=compare_script,
            reference_root=args.reference_root.resolve(),
            corpus_root=corpus,
            abc_path=abc_path,
            croma_xml=item.get("stdout", ""),
            temp_dir=temp_dir,
            keep_xml_dir=keep_xml_dir,
            music21_facts_dir=music21_facts_dir,
        )
    else:
        item["music21"] = {"enabled": bool(args.music21_compare), "status": "not_run"}

    item["classification"] = classifications.classify(item)
    item["stdout"] = stdout_summary(item.get("stdout", ""))


def write_kept_croma_xml(*, croma_xml: str, keep_xml_dir: Path, relative_path: str) -> Path:
    croma_xml_path = keep_xml_dir / Path(relative_path).with_suffix(".croma.musicxml")
    croma_xml_path.parent.mkdir(parents=True, exist_ok=True)
    croma_xml_path.write_text(croma_xml, encoding="utf-8")
    return croma_xml_path


def parse_diagnostics(stderr: str) -> tuple[list[dict[str, Any]], str | None]:
    text = stderr.strip()
    if not text:
        return [], None
    try:
        decoded = json.loads(text)
    except json.JSONDecodeError as error:
        return [], str(error)
    if not isinstance(decoded, list):
        return [], "diagnostics JSON was not an array"
    return [item for item in decoded if isinstance(item, dict)], None


def source_snippet(abc_path: Path, diagnostics: list[dict[str, Any]], context_lines: int) -> dict[str, Any]:
    try:
        raw = abc_path.read_bytes()
    except OSError as error:
        return {"error": f"failed to read source snippet: {error}"}

    text = raw.decode("utf-8", errors="replace")
    lines = text.splitlines()
    if not lines:
        return {"span": None, "start_line": 1, "end_line": 1, "lines": []}

    span = first_span(diagnostics)
    start_byte = span.get("start", 0) if span else 0
    prefix = raw[: max(0, min(start_byte, len(raw)))].decode("utf-8", errors="replace")
    diagnostic_line = prefix.count("\n") + 1
    start_line = max(1, diagnostic_line - context_lines)
    end_line = min(len(lines), diagnostic_line + context_lines)

    return {
        "span": span,
        "start_line": start_line,
        "end_line": end_line,
        "lines": [
            {"number": number, "text": lines[number - 1]}
            for number in range(start_line, end_line + 1)
        ],
    }


def first_span(diagnostics: list[dict[str, Any]]) -> dict[str, int] | None:
    for diagnostic in diagnostics:
        span = diagnostic.get("span")
        if (
            isinstance(span, dict)
            and isinstance(span.get("start"), int)
            and isinstance(span.get("end"), int)
        ):
            return {"start": span["start"], "end": span["end"]}
    return None


def run_music21_compare(
    *,
    args: argparse.Namespace,
    compare_script: Path,
    reference_root: Path,
    corpus_root: Path,
    abc_path: Path,
    croma_xml: str,
    temp_dir: Path,
    keep_xml_dir: Path | None,
    music21_facts_dir: Path | None,
) -> dict[str, Any]:
    relative = abc_path.relative_to(corpus_root)
    reference = reference_root / relative.with_suffix(".musicxml")
    if not reference.exists():
        reference = reference_root / relative.with_suffix(".xml")
    if not reference.exists():
        return {"enabled": True, "status": "reference_missing", "reference": str(reference)}

    xml_dir = keep_xml_dir or temp_dir
    croma_xml_path = xml_dir / relative.with_suffix(".croma.musicxml")
    croma_xml_path.parent.mkdir(parents=True, exist_ok=True)
    croma_xml_path.write_text(croma_xml, encoding="utf-8")
    command = [
        sys.executable,
        str(compare_script),
        "--croma-xml",
        str(croma_xml_path),
        "--reference-xml",
        str(reference),
        "--json",
        "--comparison-engine",
        args.music21_comparison_engine,
    ]
    if music21_facts_dir is not None:
        facts_path = music21_facts_dir / relative.with_suffix(".facts.parquet")
        mismatches_path = music21_facts_dir / relative.with_suffix(".mismatches.parquet")
        facts_path.parent.mkdir(parents=True, exist_ok=True)
        command.extend(
            [
                "--facts-parquet",
                str(facts_path),
                "--mismatches-parquet",
                str(mismatches_path),
            ]
        )
    try:
        completed = subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            timeout=args.music21_timeout,
        )
    except subprocess.TimeoutExpired as error:
        return {
            "enabled": True,
            "status": "music21_tool_timeout",
            "reference": str(reference),
            "croma_xml": str(croma_xml_path) if keep_xml_dir else None,
            "stderr_excerpt": text_excerpt(error.stderr),
            "stdout_excerpt": text_excerpt(error.stdout),
        }

    parsed_stdout = parse_json_object(completed.stdout)
    if parsed_stdout is not None:
        parsed_stdout["enabled"] = True
        parsed_stdout["returncode"] = completed.returncode
        parsed_stdout["reference"] = str(reference)
        if keep_xml_dir is None:
            parsed_stdout["croma_xml"] = None
        if completed.stderr:
            parsed_stdout["stderr_excerpt"] = completed.stderr[:500]
        return parsed_stdout

    return {
        "enabled": True,
        "status": "music21_tool_failure",
        "returncode": completed.returncode,
        "reference": str(reference),
        "croma_xml": str(croma_xml_path) if keep_xml_dir else None,
        "stderr_excerpt": completed.stderr[:500],
        "stdout_excerpt": completed.stdout[:500],
    }


def parse_json_object(text: str) -> dict[str, Any] | None:
    try:
        value = json.loads(text)
    except json.JSONDecodeError:
        return None
    return value if isinstance(value, dict) else None


def stdout_summary(stdout: str) -> dict[str, Any]:
    return {
        "bytes": len(stdout.encode("utf-8")),
        "excerpt": stdout[:200],
    }


def text_excerpt(value: str | bytes | None, limit: int = 1000) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        value = value.decode("utf-8", errors="replace")
    return value[:limit]


def append_jsonl(handle: Any, item: dict[str, Any]) -> None:
    handle.write(json.dumps(item, sort_keys=True))
    handle.write("\n")
    handle.flush()


def should_stop(args: argparse.Namespace, item: dict[str, Any]) -> bool:
    if not args.fail_fast:
        return False
    if item["status"] != "success":
        return True
    music21 = item.get("music21", {})
    return music21.get("status") not in {None, "not_run", "match"}


def build_report(
    *,
    args: argparse.Namespace,
    corpus: Path,
    report_path: Path,
    results_jsonl: Path,
    compare_script: Path,
    discovered: list[Path],
    selected: list[Path],
    results: list[dict[str, Any]],
    skipped_by_resume: int,
    started_at: str,
    elapsed: float,
    run_id: str,
) -> dict[str, Any]:
    top_codes: Counter[str] = Counter()
    classification_counts: Counter[str] = Counter()
    mismatch_categories: Counter[str] = Counter()
    failures = []
    mismatch_examples = []
    successes = 0
    croma_failures = 0
    panics = 0
    hard_errors = 0
    timeouts = 0
    music21_counts = Counter()

    for item in results:
        for diagnostic in item.get("diagnostics", []):
            code = diagnostic.get("code")
            if isinstance(code, str):
                top_codes[code] += 1

        classification = item.get("classification", {}).get("id", "unclassified")
        classification_counts[classification] += 1

        if item.get("status") == "success":
            successes += 1
        else:
            croma_failures += 1
            panics += int(bool(item.get("panic")))
            hard_errors += int(bool(item.get("hard_error")))
            timeouts += int(bool(item.get("timeout")))
            if len(failures) < args.first_failures:
                failures.append(failure_summary(item))

        music21 = item.get("music21", {})
        music21_status = music21.get("status")
        if music21_status and music21_status != "not_run":
            music21_counts[music21_status] += 1

        if music21_status == "difference":
            raw_category_counts = music21.get("category_counts", {})
            if isinstance(raw_category_counts, dict):
                for category, count in raw_category_counts.items():
                    if isinstance(category, str) and isinstance(count, int):
                        mismatch_categories[category] += count
            else:
                for difference in music21.get("differences", []):
                    category = difference.get("category", "structural")
                    mismatch_categories[category] += 1

            for difference in music21.get("differences", []):
                category = difference.get("category", "structural")
                if len(mismatch_examples) < args.first_mismatches:
                    mismatch_examples.append(
                        {
                            "path": item["path"],
                            "relative_path": item.get("relative_path"),
                            "category": category,
                            "difference": difference,
                            "classification": item.get("classification"),
                        }
                    )

    music21_enabled = bool(args.music21_compare)
    music21_compared = music21_counts["match"] + music21_counts["difference"]
    music21_tool_failures = music21_counts["music21_tool_failure"]

    return {
        "schema": REPORT_SCHEMA,
        "run_id": run_id,
        "started_at": started_at,
        "finished_at": now_utc(),
        "elapsed_seconds": round(elapsed, 3),
        "mode": args.mode,
        "croma": str(args.croma),
        "command_options": {
            "limit": args.limit,
            "sample": args.sample,
            "sample_seed": args.sample_seed,
            "timeout": args.timeout,
            "resume": args.resume,
            "music21_compare": args.music21_compare,
            "music21_timeout": args.music21_timeout,
            "music21_comparison_engine": args.music21_comparison_engine,
            "music21_facts_dir": str(args.music21_facts_dir.resolve()) if args.music21_facts_dir else None,
        },
        "corpus_path": str(corpus),
        "reference_root": str(args.reference_root.resolve()) if args.reference_root else None,
        "report_path": str(report_path),
        "results_jsonl": str(results_jsonl),
        "music21_script": str(compare_script),
        "files_discovered": len(discovered),
        "discovered_files": [str(path) for path in discovered],
        "files_selected": len(selected),
        "selected_files": [str(path) for path in selected],
        "files_attempted": len(results),
        "total_files_attempted": len(results),
        "attempted_files": [item["path"] for item in results],
        "files_skipped_by_resume": skipped_by_resume,
        "successes": successes,
        "failures": croma_failures,
        "panics": panics,
        "hard_errors": hard_errors,
        "timeouts": timeouts,
        "top_diagnostic_codes": [
            {"code": code, "count": count}
            for code, count in top_codes.most_common(args.top_codes)
        ],
        "classification_categories": CLASSIFICATION_CATEGORIES,
        "classification_counts": dict(sorted(classification_counts.items())),
        "failing_files": failures,
        "first_failures": failures,
        "music21": {
            "enabled": music21_enabled,
            "compared": music21_compared,
            "matches": music21_counts["match"],
            "differences": music21_counts["difference"],
            "tool_failures": music21_tool_failures,
            "unavailable": music21_counts["music21_unavailable"],
            "tool_timeouts": music21_counts["music21_tool_timeout"],
            "croma_parse_failures": music21_counts["croma_musicxml_parse_failure"],
            "reference_parse_failures": music21_counts["reference_musicxml_parse_failure"],
            "reference_missing": music21_counts["reference_missing"],
            "status_counts": dict(sorted(music21_counts.items())),
        },
        "structural_mismatches": {
            "enabled": music21_enabled,
            "differences": music21_counts["difference"],
            "category_counts": dict(sorted(mismatch_categories.items())),
            "examples": mismatch_examples,
        },
        "results": results,
    }


def failure_summary(item: dict[str, Any]) -> dict[str, Any]:
    return {
        "path": item["path"],
        "relative_path": item.get("relative_path"),
        "returncode": item.get("returncode"),
        "panic": item.get("panic", False),
        "hard_error": item.get("hard_error", False),
        "timeout": item.get("timeout", False),
        "classification": item.get("classification"),
        "diagnostics": [
            {
                "code": diagnostic.get("code"),
                "severity": diagnostic.get("severity"),
                "message": diagnostic.get("message"),
                "span": diagnostic.get("span"),
            }
            for diagnostic in item.get("diagnostics", [])[:5]
        ],
        "source_snippet": item.get("source_snippet"),
        "stderr_excerpt": item.get("stderr_excerpt", "")[:500],
    }


def now_utc() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def print_summary(report_path: Path, report: dict[str, Any]) -> None:
    print(f"report: {report_path}")
    print(f"results jsonl: {report['results_jsonl']}")
    print(f"corpus: {report['corpus_path']}")
    print(f"files discovered: {report['files_discovered']}")
    print(f"selected: {report['files_selected']}")
    print(f"attempted: {report['files_attempted']}")
    print(f"skipped by resume: {report['files_skipped_by_resume']}")
    print(f"successes: {report['successes']}")
    print(f"failures: {report['failures']}")
    print(f"panics: {report['panics']}")
    print(f"hard errors: {report['hard_errors']}")
    print(f"timeouts: {report['timeouts']}")
    print(f"elapsed seconds: {report['elapsed_seconds']}")
    if report["music21"]["enabled"]:
        music21 = report["music21"]
        print(f"music21 compared: {music21['compared']}")
        print(f"music21 matches: {music21['matches']}")
        print(f"music21 differences: {music21['differences']}")
        print(f"music21 tool failures: {music21['tool_failures']}")
        print(f"music21 reference parse failures: {music21['reference_parse_failures']}")
        print(f"music21 croma parse failures: {music21['croma_parse_failures']}")
        print(f"music21 reference missing: {music21['reference_missing']}")
    if report["top_diagnostic_codes"]:
        print("top diagnostic codes:")
        for item in report["top_diagnostic_codes"]:
            print(f"  {item['code']}: {item['count']}")


class ClassificationMap:
    def __init__(
        self,
        *,
        file_map: dict[str, str] | None = None,
        diagnostic_code_map: dict[str, str] | None = None,
        mismatch_category_map: dict[str, str] | None = None,
    ) -> None:
        self.file_map = file_map or {}
        self.diagnostic_code_map = diagnostic_code_map or {}
        self.mismatch_category_map = mismatch_category_map or {}

    @classmethod
    def load(cls, path: Path | None) -> "ClassificationMap":
        if path is None:
            return cls()
        raw = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(raw, dict):
            raise SystemExit("classification file must contain a JSON object")

        file_map = normalize_classification_map(raw.get("files", {}), "files")
        diagnostic_code_map = normalize_classification_map(
            raw.get("diagnostic_codes", {}),
            "diagnostic_codes",
        )
        mismatch_category_map = normalize_classification_map(
            raw.get("mismatch_categories", {}),
            "mismatch_categories",
        )

        if not any([file_map, diagnostic_code_map, mismatch_category_map]):
            file_map = normalize_classification_map(raw, "classifications")

        return cls(
            file_map=file_map,
            diagnostic_code_map=diagnostic_code_map,
            mismatch_category_map=mismatch_category_map,
        )

    def classify(self, item: dict[str, Any]) -> dict[str, Any]:
        path_keys = [
            item.get("path"),
            item.get("relative_path"),
            Path(item["path"]).name if item.get("path") else None,
        ]
        for key in path_keys:
            if isinstance(key, str) and key in self.file_map:
                return classification_payload(self.file_map[key], f"file:{key}")

        music21 = item.get("music21", {})
        if music21.get("status") == "difference":
            for difference in music21.get("differences", []):
                category = difference.get("category")
                if isinstance(category, str) and category in self.mismatch_category_map:
                    return classification_payload(
                        self.mismatch_category_map[category],
                        f"mismatch:{category}",
                    )

        for diagnostic in item.get("diagnostics", []):
            code = diagnostic.get("code")
            if isinstance(code, str) and code in self.diagnostic_code_map:
                return classification_payload(self.diagnostic_code_map[code], f"diagnostic:{code}")

        if item.get("panic"):
            return classification_payload("croma_bug", "panic")
        if music21.get("status") in {
            "music21_unavailable",
            "music21_tool_failure",
            "music21_tool_timeout",
            "croma_musicxml_parse_failure",
            "reference_musicxml_parse_failure",
        }:
            return classification_payload("music21_tooling_issue", str(music21.get("status")))
        return classification_payload("unclassified", "default")


def normalize_classification_map(value: Any, field: str) -> dict[str, str]:
    if value is None:
        return {}
    if not isinstance(value, dict):
        raise SystemExit(f"classification `{field}` must be a JSON object")

    normalized: dict[str, str] = {}
    for key, raw_classification in value.items():
        if not isinstance(key, str):
            raise SystemExit(f"classification `{field}` keys must be strings")
        if isinstance(raw_classification, dict):
            raw_classification = raw_classification.get("classification")
        if not isinstance(raw_classification, str):
            raise SystemExit(f"classification for `{key}` must be a string")
        classification = normalize_classification_id(raw_classification)
        normalized[key] = classification
    return normalized


def normalize_classification_id(value: str) -> str:
    candidate = value.strip().lower().replace(" ", "_").replace("/", "_")
    if candidate == "music21_tooling_issue":
        candidate = "music21_tooling_issue"
    if candidate not in CLASSIFICATION_IDS:
        valid = ", ".join(sorted(CLASSIFICATION_IDS))
        raise SystemExit(f"unknown classification `{value}`; valid values: {valid}")
    return candidate


def classification_payload(classification: str, source: str) -> dict[str, str]:
    return {
        "id": classification,
        "label": next(
            item["label"]
            for item in CLASSIFICATION_CATEGORIES
            if item["id"] == classification
        ),
        "source": source,
    }


if __name__ == "__main__":
    raise SystemExit(main())
