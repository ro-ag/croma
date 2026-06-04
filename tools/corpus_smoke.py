#!/usr/bin/env python3
"""Run a real-corpus smoke pass through the croma CLI.

This script intentionally shells out to the `croma` binary. It owns corpus
orchestration and reporting only; parsing, lowering, diagnostics, and MusicXML
export stay in croma-core behind the CLI.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
import time
from collections import Counter
from pathlib import Path
from typing import Any


def main() -> int:
    args = parse_args()
    repo_root = Path(__file__).resolve().parents[1]
    corpus = args.corpus.resolve()
    report_path = args.report or repo_root / "docs" / "untracked" / "corpus-smoke-report.json"
    compare_script = args.music21_script or repo_root / "tools" / "music21_compare.py"

    discovered = discover_abc_files(corpus)
    attempted_files = discovered[: args.limit] if args.limit is not None else discovered
    started = time.monotonic()

    top_codes: Counter[str] = Counter()
    first_failures: list[dict[str, Any]] = []
    results: list[dict[str, Any]] = []
    successes = 0
    failures = 0
    panics = 0
    hard_errors = 0
    timeouts = 0
    music21_compared = 0
    music21_differences = 0
    music21_failures = 0
    reference_missing = 0

    with tempfile.TemporaryDirectory(prefix="croma-corpus-") as temp_dir_name:
        temp_dir = Path(temp_dir_name)
        for index, abc_path in enumerate(attempted_files, start=1):
            item = run_one(args, abc_path)
            diagnostics = item.get("diagnostics", [])
            top_codes.update(
                diagnostic.get("code", "unknown") for diagnostic in diagnostics if diagnostic
            )

            if item["status"] == "success":
                successes += 1
            else:
                failures += 1
                if item.get("panic"):
                    panics += 1
                if item.get("hard_error"):
                    hard_errors += 1
                if item.get("timeout"):
                    timeouts += 1
                if len(first_failures) < args.first_failures:
                    first_failures.append(failure_summary(item))

            if (
                args.music21_compare
                and args.mode == "xml"
                and item["status"] == "success"
                and args.reference_root is not None
            ):
                comparison = run_music21_compare(
                    compare_script=compare_script,
                    reference_root=args.reference_root.resolve(),
                    corpus_root=corpus,
                    abc_path=abc_path,
                    croma_xml=item.get("stdout", ""),
                    temp_dir=temp_dir,
                )
                item["music21"] = comparison
                status = comparison.get("status")
                if status == "reference_missing":
                    reference_missing += 1
                elif status == "match":
                    music21_compared += 1
                elif status == "difference":
                    music21_compared += 1
                    music21_differences += 1
                else:
                    music21_failures += 1

            item["stdout"] = stdout_summary(item.get("stdout", ""))
            results.append(item)

            if args.fail_fast and item["status"] != "success":
                break

            if args.progress_every and index % args.progress_every == 0:
                print(f"processed {index}/{len(attempted_files)}", file=sys.stderr)

    elapsed = time.monotonic() - started
    report = {
        "schema": "croma-corpus-smoke-v1",
        "mode": args.mode,
        "corpus_path": str(corpus),
        "files_discovered": len(discovered),
        "discovered_files": [str(path) for path in discovered],
        "total_files_attempted": len(results),
        "attempted_files": [item["path"] for item in results],
        "successes": successes,
        "failures": failures,
        "panics": panics,
        "hard_errors": hard_errors,
        "timeouts": timeouts,
        "top_diagnostic_codes": top_codes.most_common(args.top_codes),
        "first_failures": first_failures,
        "music21": {
            "enabled": args.music21_compare,
            "compared": music21_compared,
            "differences": music21_differences,
            "tool_failures": music21_failures,
            "reference_missing": reference_missing,
        },
        "elapsed_seconds": round(elapsed, 3),
        "results": results,
    }

    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print_summary(report_path, report)

    if panics or hard_errors or timeouts:
        return 2
    if args.fail_on_failure and failures:
        return 1
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run croma over an ABC corpus")
    parser.add_argument("--croma", type=Path, default=Path("target/debug/croma"))
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--mode", choices=["check", "xml"], default="check")
    parser.add_argument("--report", type=Path)
    parser.add_argument("--limit", type=int)
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--first-failures", type=int, default=20)
    parser.add_argument("--top-codes", type=int, default=20)
    parser.add_argument("--progress-every", type=int, default=0)
    parser.add_argument("--fail-fast", action="store_true")
    parser.add_argument("--fail-on-failure", action="store_true")
    parser.add_argument("--music21-compare", action="store_true")
    parser.add_argument("--reference-root", type=Path)
    parser.add_argument("--music21-script", type=Path)
    return parser.parse_args()


def discover_abc_files(corpus: Path) -> list[Path]:
    return sorted(
        path
        for path in corpus.rglob("*")
        if path.is_file() and path.suffix.lower() == ".abc"
    )


def run_one(args: argparse.Namespace, abc_path: Path) -> dict[str, Any]:
    command = [str(args.croma), args.mode, "--diagnostics=json", str(abc_path)]
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
            "path": str(abc_path),
            "status": "failure",
            "returncode": None,
            "timeout": True,
            "panic": False,
            "hard_error": True,
            "diagnostics": [],
            "stderr_excerpt": (error.stderr or "")[:1000],
            "stdout": (error.stdout or "")[:1000],
        }

    diagnostics, diagnostics_error = parse_diagnostics(completed.stderr)
    stderr_excerpt = completed.stderr[:1000]
    panic = "panicked at" in completed.stderr or "thread '" in completed.stderr
    hard_error = completed.returncode != 0 and not diagnostics
    status = "success" if completed.returncode == 0 else "failure"

    item = {
        "path": str(abc_path),
        "status": status,
        "returncode": completed.returncode,
        "timeout": False,
        "panic": panic,
        "hard_error": hard_error,
        "diagnostics": diagnostics,
        "diagnostics_parse_error": diagnostics_error,
        "stderr_excerpt": stderr_excerpt,
        "stdout": completed.stdout,
    }
    return item


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


def failure_summary(item: dict[str, Any]) -> dict[str, Any]:
    diagnostics = item.get("diagnostics", [])
    return {
        "path": item["path"],
        "returncode": item.get("returncode"),
        "panic": item.get("panic", False),
        "hard_error": item.get("hard_error", False),
        "timeout": item.get("timeout", False),
        "diagnostics": [
            {
                "code": diagnostic.get("code"),
                "severity": diagnostic.get("severity"),
                "message": diagnostic.get("message"),
                "span": diagnostic.get("span"),
            }
            for diagnostic in diagnostics[:5]
        ],
        "stderr_excerpt": item.get("stderr_excerpt", "")[:500],
    }


def stdout_summary(stdout: str) -> dict[str, Any]:
    return {
        "bytes": len(stdout.encode("utf-8")),
        "excerpt": stdout[:200],
    }


def run_music21_compare(
    *,
    compare_script: Path,
    reference_root: Path,
    corpus_root: Path,
    abc_path: Path,
    croma_xml: str,
    temp_dir: Path,
) -> dict[str, Any]:
    relative = abc_path.relative_to(corpus_root)
    reference = reference_root / relative.with_suffix(".musicxml")
    if not reference.exists():
        reference = reference_root / relative.with_suffix(".xml")
    if not reference.exists():
        return {"status": "reference_missing", "reference": str(reference)}

    croma_xml_path = temp_dir / f"{abc_path.stem}.musicxml"
    croma_xml_path.write_text(croma_xml, encoding="utf-8")
    command = [
        sys.executable,
        str(compare_script),
        "--croma-xml",
        str(croma_xml_path),
        "--reference-xml",
        str(reference),
        "--json",
    ]
    completed = subprocess.run(command, check=False, capture_output=True, text=True)
    parsed_stdout = parse_json_object(completed.stdout)
    if parsed_stdout is not None:
        parsed_stdout["returncode"] = completed.returncode
        if completed.stderr:
            parsed_stdout["stderr_excerpt"] = completed.stderr[:500]
        return parsed_stdout
    if completed.returncode != 0:
        return {
            "status": "music21_tool_failure",
            "returncode": completed.returncode,
            "stderr_excerpt": completed.stderr[:500],
        }
    return {
        "status": "music21_tool_failure",
        "error": "music21 comparer did not return a JSON object",
        "stdout_excerpt": completed.stdout[:500],
    }


def parse_json_object(text: str) -> dict[str, Any] | None:
    try:
        value = json.loads(text)
    except json.JSONDecodeError:
        return None
    return value if isinstance(value, dict) else None


def print_summary(report_path: Path, report: dict[str, Any]) -> None:
    print(f"report: {report_path}")
    print(f"corpus: {report['corpus_path']}")
    print(f"files discovered: {report['files_discovered']}")
    print(f"attempted: {report['total_files_attempted']}")
    print(f"successes: {report['successes']}")
    print(f"failures: {report['failures']}")
    print(f"panics: {report['panics']}")
    print(f"hard errors: {report['hard_errors']}")
    print(f"timeouts: {report['timeouts']}")
    print(f"elapsed seconds: {report['elapsed_seconds']}")
    if report["music21"]["enabled"]:
        music21 = report["music21"]
        print(f"music21 compared: {music21['compared']}")
        print(f"music21 differences: {music21['differences']}")
        print(f"music21 tool failures: {music21['tool_failures']}")
        print(f"music21 reference missing: {music21['reference_missing']}")
    if report["top_diagnostic_codes"]:
        print("top diagnostic codes:")
        for code, count in report["top_diagnostic_codes"]:
            print(f"  {code}: {count}")


if __name__ == "__main__":
    raise SystemExit(main())
