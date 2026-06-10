#!/usr/bin/env python3
"""Build corpus-level Polars comparison tables from music21 facts."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
import time
import traceback
from collections import Counter, defaultdict
from concurrent.futures import ProcessPoolExecutor
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import orjson

from compare_cache import (
    CACHE_DB_ENV_VAR,
    DEFAULT_CACHE_DB,
    CompareCache,
    facts_cache_version,
    file_sha256,
    pair_result_key,
    result_cache_version,
)
from music21_compare import (
    Music21ParseFailure,
    Music21Unavailable,
    decode_fact_value,
    encode_fact_value,
    extract_facts,
    import_polars,
)


REPORT_SCHEMA = "croma-music21-polars-corpus-compare-v2"

FACT_COLUMNS = [
    "relative_path",
    "filename",
    "source_side",
    "component",
    "field_name",
    "part_id",
    "part_index",
    "measure_number",
    "measure_index",
    "voice",
    "staff",
    "event_index",
    "alignment_index",
    "onset",
    "duration",
    "pitch_step",
    "pitch_alter",
    "pitch_octave",
    "value_text",
    "raw_value",
    "source_path",
    "xml_path",
    "extraction_status",
    "diagnostic",
    "comparison_key",
]

COMPARISON_COLUMNS = [
    "relative_path",
    "filename",
    "source_side",
    "component",
    "field_name",
    "part_id",
    "part_index",
    "measure_number",
    "measure_index",
    "voice",
    "staff",
    "event_index",
    "alignment_index",
    "onset",
    "duration",
    "pitch_step",
    "pitch_alter",
    "pitch_octave",
    "source_path",
    "croma_xml_path",
    "reference_xml_path",
    "croma_present",
    "reference_present",
    "croma_value",
    "reference_value",
    "croma_raw_value",
    "reference_raw_value",
    "extraction_status",
    "diagnostic",
    "matches",
    "mismatch_category",
    "comparison_key",
]

PER_FILE_SUMMARY_COLUMNS = [
    "relative_path",
    "filename",
    "source_path",
    "croma_xml_path",
    "reference_xml_path",
    "croma_import_status",
    "reference_import_status",
    "comparison_status",
    "fact_rows",
    "comparison_rows",
    "mismatch_rows",
    "mismatch_categories",
    "diagnostics",
]

PER_COMPONENT_SUMMARY_COLUMNS = [
    "component",
    "mismatch_category",
    "mismatch_rows",
    "affected_files",
]

JOIN_KEY_COLUMNS = [
    "relative_path",
    "filename",
    "comparison_key",
]

# Sorted field order so the columnar struct encode below reproduces the
# python `json.dumps(..., sort_keys=True)` key format byte for byte.
COMPARISON_KEY_FIELDS = [
    "alignment_index",
    "component",
    "event_index",
    "field_name",
    "measure_index",
    "part_index",
    "staff",
    "voice",
]

SORT_COLUMNS = [
    "relative_path",
    "filename",
    "component",
    "field_name",
    "part_index",
    "measure_index",
    "voice",
    "staff",
    "event_index",
    "alignment_index",
    "comparison_key",
]

ALL_COMPONENTS = {
    "barline",
    "direction",
    "duration",
    "harmony",
    "lyric",
    "measure",
    "metadata",
    "note",
    "pitch",
    "rest",
    "slur",
    "tie",
    "tuplet",
}

COMPONENT_ALIASES = {
    "accidental": "pitch",
    "accidentals": "pitch",
    "barlines": "barline",
    "directions": "direction",
    "durations": "duration",
    "harmony_chord_symbols": "harmony",
    "lyrics": "lyric",
    "measures": "measure",
    "notes": "note",
    "octave": "pitch",
    "octaves": "pitch",
    "pitches": "pitch",
    "rests": "rest",
    "ties": "tie",
    "ties_slurs": "tie",
    "tuplets": "tuplet",
}


def main() -> int:
    args = parse_args()
    jobs = resolve_jobs(args.jobs)
    worker_chunk_size = resolve_worker_chunk_size(args.worker_chunk_size)
    polars_threads_per_worker = resolve_polars_threads_per_worker(
        args.polars_threads_per_worker,
        jobs,
    )
    component_filter = resolve_component_filter(args.component)

    cache_db_path = resolve_cache_db_path(args)
    facts_version = facts_cache_version() if cache_db_path is not None else None
    result_version = (
        result_cache_version(facts_version) if cache_db_path is not None else None
    )
    parent_cache: CompareCache | None = None
    if cache_db_path is not None:
        # Open (and recover, if corrupt) before workers race on the same file.
        parent_cache = CompareCache.open(cache_db_path)

    facts_jsonl = args.facts_jsonl or sibling_jsonl(args.facts_parquet)
    comparison_jsonl = args.comparison_jsonl or sibling_jsonl(args.comparison_parquet)
    mismatches_jsonl = args.mismatches_jsonl or sibling_jsonl(args.mismatches_parquet)
    per_file_summary_jsonl = args.per_file_summary_jsonl or sibling_jsonl(
        args.per_file_summary_parquet
    )
    per_component_summary_jsonl = args.per_component_summary_jsonl or sibling_jsonl(
        args.per_component_summary_parquet
    )

    clear_jsonl_outputs(
        [
            facts_jsonl,
            comparison_jsonl,
            mismatches_jsonl,
            per_file_summary_jsonl,
            per_component_summary_jsonl,
        ]
    )

    all_results = load_results(args.results_jsonl)
    selected_results = select_results(all_results, args)

    print_start(args, len(selected_results), jobs, parent_cache)

    started_at = now_utc()
    started = time.monotonic()
    counters: Counter[str] = Counter()
    category_counts: Counter[str] = Counter()
    component_category_counts: Counter[tuple[str, str]] = Counter()
    affected_file_counts: Counter[str] = Counter()
    component_affected_file_counts: Counter[tuple[str, str]] = Counter()
    status_counts: Counter[str] = Counter()
    file_mismatch_counts: Counter[str] = Counter()
    file_categories: dict[str, set[str]] = defaultdict(set)
    examples: dict[str, list[dict[str, Any]]] = defaultdict(list)
    import_failures: list[dict[str, Any]] = []
    croma_failures: list[dict[str, Any]] = []
    source_paths: dict[str, str] = {}

    facts_rows_written = 0
    comparison_rows_written = 0
    mismatch_rows_written = 0
    per_file_summary_rows_written = 0
    completed_inputs = 0
    tasks = []

    with (
        optional_jsonl_handle(facts_jsonl) as facts_handle,
        optional_jsonl_handle(comparison_jsonl) as comparison_handle,
        optional_jsonl_handle(mismatches_jsonl) as mismatches_handle,
        optional_jsonl_handle(per_file_summary_jsonl) as per_file_summary_handle,
    ):
        for item in selected_results:
            counters["files_attempted"] += 1
            relative_path = relative_path_for(item)
            if relative_path is None:
                status_counts["missing_relative_path"] += 1
                completed_inputs += 1
                print_progress(args, completed_inputs, len(selected_results))
                continue

            filename = Path(relative_path).name
            source_path = source_path_for(item)
            if source_path is not None:
                source_paths[relative_path] = source_path

            if item.get("status") != "success":
                counters["croma_export_failures"] += 1
                croma_failure = croma_failure_summary(item, relative_path)
                croma_failures.append(croma_failure)
                summary_row = croma_export_failure_summary_row(
                    item=item,
                    relative_path=relative_path,
                    filename=filename,
                    source_path=source_path,
                )
                per_file_summary_rows_written += 1
                write_optional_text(
                    per_file_summary_handle,
                    json.dumps(summary_row, sort_keys=True, ensure_ascii=False) + "\n",
                )
                completed_inputs += 1
                print_progress(args, completed_inputs, len(selected_results))
                continue

            counters["croma_export_successes"] += 1
            croma_xml = resolve_croma_xml(args.croma_xml_root, item, relative_path)
            reference_xml = resolve_reference_xml(args.reference_root, item, relative_path)
            tasks.append(
                {
                    "relative_path": relative_path,
                    "filename": filename,
                    "source_path": source_path,
                    "croma_xml": str(croma_xml) if croma_xml is not None else None,
                    "reference_xml": str(reference_xml) if reference_xml is not None else None,
                    "component_filter": sorted(component_filter) if component_filter else None,
                    "write_facts_jsonl": facts_jsonl is not None,
                    "write_comparison_jsonl": comparison_jsonl is not None,
                    "write_mismatches_jsonl": mismatches_jsonl is not None,
                    "write_per_file_summary_jsonl": per_file_summary_jsonl is not None,
                    "sample_per_category": args.sample_per_category,
                    "strict": args.strict,
                    "cache_db": str(cache_db_path) if cache_db_path is not None else None,
                    "facts_cache_version": facts_version,
                    "result_cache_version": result_version,
                }
            )

        for task_result in comparison_task_results(
            tasks,
            jobs,
            worker_chunk_size,
            polars_threads_per_worker,
        ):
            merge_counter(counters, task_result["counters"])
            merge_counter(status_counts, task_result["status_counts"])
            merge_counter(category_counts, task_result["mismatch_category_counts"])
            merge_component_category_counts(
                component_category_counts,
                task_result["component_category_counts"],
            )
            merge_examples(
                examples,
                task_result["examples"],
                args.sample_per_category,
            )
            import_failures.extend(task_result["import_failures"])

            filename = str(task_result["filename"])
            mismatch_rows = int(task_result["mismatch_rows"])
            if mismatch_rows:
                file_mismatch_counts[filename] += mismatch_rows
            for category in task_result["file_mismatch_categories"]:
                affected_file_counts[str(category)] += 1
                file_categories[filename].add(str(category))
            for component, category in task_result["file_component_categories"]:
                component_affected_file_counts[(str(component), str(category))] += 1

            facts_rows_written += int(task_result["fact_rows"])
            comparison_rows_written += int(task_result["comparison_rows"])
            mismatch_rows_written += mismatch_rows
            per_file_summary_rows_written += int(task_result["per_file_summary_rows"])
            write_optional_text(facts_handle, task_result.get("facts_jsonl"))
            write_optional_text(comparison_handle, task_result.get("comparison_jsonl"))
            write_optional_text(mismatches_handle, task_result.get("mismatches_jsonl"))
            write_optional_text(
                per_file_summary_handle,
                task_result.get("per_file_summary_jsonl"),
            )

            completed_inputs += 1
            print_progress(args, completed_inputs, len(selected_results))

    per_component_summary_rows = build_per_component_summary_rows(
        component_category_counts,
        component_affected_file_counts,
    )
    if per_component_summary_jsonl is not None:
        write_jsonl_rows(per_component_summary_jsonl, per_component_summary_rows)

    pl = import_polars()
    write_parquet(pl, facts_jsonl, args.facts_parquet, fact_schema(pl))
    write_parquet(pl, comparison_jsonl, args.comparison_parquet, comparison_schema(pl))
    write_parquet(pl, mismatches_jsonl, args.mismatches_parquet, comparison_schema(pl))
    write_parquet(
        pl,
        per_file_summary_jsonl,
        args.per_file_summary_parquet,
        per_file_summary_schema(pl),
    )
    write_parquet(
        pl,
        per_component_summary_jsonl,
        args.per_component_summary_parquet,
        per_component_summary_schema(pl),
    )

    baseline_delta = baseline_delta_report(args, file_mismatch_counts)
    single_category_files = files_with_only_one_category(file_categories)
    candidate_files = candidate_file_list(file_mismatch_counts)
    write_candidate_outputs(args, candidate_files, source_paths)

    pruned_rows = 0
    if parent_cache is not None:
        pruned_rows = parent_cache.prune_stale()
        parent_cache.close()

    elapsed = time.monotonic() - started
    report = {
        "schema": REPORT_SCHEMA,
        "started_at": started_at,
        "finished_at": now_utc(),
        "elapsed_seconds": round(elapsed, 3),
        "results_jsonl": str(args.results_jsonl),
        "croma_xml_root": str(args.croma_xml_root) if args.croma_xml_root else None,
        "reference_root": str(args.reference_root),
        "component_filter": sorted(component_filter) if component_filter else None,
        "jobs": jobs,
        "worker_chunk_size": worker_chunk_size,
        "polars_threads_per_worker": polars_threads_per_worker,
        "files_discovered": len(all_results),
        "files_selected": len(selected_results),
        "files_attempted": counters["files_attempted"],
        "croma_export_successes": counters["croma_export_successes"],
        "croma_export_failures": counters["croma_export_failures"],
        "croma_musicxml_import_attempts": counters["croma_musicxml_import_attempts"],
        "croma_musicxml_import_successes": counters["croma_musicxml_import_successes"],
        "croma_musicxml_import_failures": counters["croma_musicxml_import_failures"],
        "reference_musicxml_import_attempts": counters["reference_musicxml_import_attempts"],
        "reference_musicxml_import_successes": counters["reference_musicxml_import_successes"],
        "reference_musicxml_import_failures": counters["reference_musicxml_import_failures"],
        "worker_failures": counters["worker_failures"],
        "comparison_harness_issues": counters["comparison_harness_issues"],
        "cache": {
            "enabled": cache_db_path is not None,
            "path": str(cache_db_path) if cache_db_path is not None else None,
            "facts_version": facts_version,
            "result_version": result_version,
            "facts_hits": counters["facts_cache_hits"],
            "facts_misses": counters["facts_cache_misses"],
            "result_hits": counters["result_cache_hits"],
            "result_misses": counters["result_cache_misses"],
            "errors": counters["cache_errors"],
            "pruned_rows": pruned_rows,
        },
        "structural_matches": counters["structural_matches"],
        "structural_mismatches": counters["structural_mismatches"],
        "status_counts": dict(sorted(status_counts.items())),
        "mismatch_category_counts": dict(sorted(category_counts.items())),
        "top_mismatch_categories": top_counter_rows(category_counts),
        "affected_file_counts_by_category": dict(sorted(affected_file_counts.items())),
        "worst_files_by_mismatch_rows": top_counter_rows(file_mismatch_counts),
        "single_category_files": single_category_files,
        "candidate_files": candidate_files[: args.report_candidate_files],
        "baseline": baseline_delta,
        "fact_rows": facts_rows_written,
        "comparison_rows": comparison_rows_written,
        "mismatch_rows": mismatch_rows_written,
        "per_file_summary_rows": per_file_summary_rows_written,
        "per_component_summary_rows": len(per_component_summary_rows),
        "tables": {
            "facts_jsonl": str(facts_jsonl) if facts_jsonl else None,
            "facts_parquet": str(args.facts_parquet) if args.facts_parquet else None,
            "comparison_jsonl": str(comparison_jsonl) if comparison_jsonl else None,
            "comparison_parquet": str(args.comparison_parquet) if args.comparison_parquet else None,
            "mismatches_jsonl": str(mismatches_jsonl) if mismatches_jsonl else None,
            "mismatches_parquet": str(args.mismatches_parquet) if args.mismatches_parquet else None,
            "per_file_summary_jsonl": (
                str(per_file_summary_jsonl) if per_file_summary_jsonl else None
            ),
            "per_file_summary_parquet": (
                str(args.per_file_summary_parquet)
                if args.per_file_summary_parquet
                else None
            ),
            "per_component_summary_jsonl": (
                str(per_component_summary_jsonl)
                if per_component_summary_jsonl
                else None
            ),
            "per_component_summary_parquet": (
                str(args.per_component_summary_parquet)
                if args.per_component_summary_parquet
                else None
            ),
            "write_file_list": str(args.write_file_list) if args.write_file_list else None,
            "write_target_corpus_dir": (
                str(args.write_target_corpus_dir)
                if args.write_target_corpus_dir
                else None
            ),
        },
        "fact_table_columns": FACT_COLUMNS,
        "comparison_table_columns": COMPARISON_COLUMNS,
        "per_file_summary_columns": PER_FILE_SUMMARY_COLUMNS,
        "per_component_summary_columns": PER_COMPONENT_SUMMARY_COLUMNS,
        "comparison_keys": [
            "filename",
            "component",
            "part_index",
            "measure_index",
            "voice",
            "staff",
            "event_index",
            "alignment_index",
            "source_side",
            "mismatch_category",
        ],
        "examples": {
            category: rows
            for category, rows in sorted(examples.items())
        },
        "croma_failures": croma_failures[: args.max_failures],
        "import_failures": import_failures[: args.max_failures],
    }

    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    print_summary(args, args.report, report)
    if args.strict and strict_failures(report):
        return 2
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare a corpus of Croma and reference MusicXML files through Polars tables"
    )
    parser.add_argument("--results-jsonl", type=Path, required=True)
    parser.add_argument("--croma-xml-root", type=Path)
    parser.add_argument("--reference-root", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--facts-jsonl", type=Path)
    parser.add_argument("--facts-parquet", type=Path)
    parser.add_argument("--comparison-jsonl", type=Path)
    parser.add_argument("--comparison-parquet", type=Path)
    parser.add_argument("--mismatches-jsonl", type=Path)
    parser.add_argument("--mismatches-parquet", type=Path)
    parser.add_argument("--per-file-summary-jsonl", type=Path)
    parser.add_argument("--per-file-summary-parquet", type=Path)
    parser.add_argument("--per-component-summary-jsonl", type=Path)
    parser.add_argument("--per-component-summary-parquet", type=Path)
    parser.add_argument("--limit", type=int)
    parser.add_argument("--limit-files", type=int)
    parser.add_argument(
        "--only-files",
        action="append",
        default=[],
        help=(
            "Restrict to file names/relative paths. Values that point to existing files "
            "are read as newline-separated file lists. May be repeated."
        ),
    )
    parser.add_argument(
        "--component",
        action="append",
        default=[],
        help="Comma-separated component filter. May be repeated.",
    )
    parser.add_argument("--progress-every", type=int, default=500)
    parser.add_argument("--sample-per-category", type=int, default=5)
    parser.add_argument(
        "--max-examples-per-category",
        dest="sample_per_category",
        type=int,
        help="Backward-compatible alias for --sample-per-category.",
    )
    parser.add_argument("--max-failures", type=int, default=20)
    parser.add_argument("--report-candidate-files", type=int, default=50)
    parser.add_argument("--write-target-corpus-dir", type=Path)
    parser.add_argument("--write-file-list", type=Path)
    parser.add_argument("--baseline-report", type=Path)
    parser.add_argument("--baseline-mismatches", type=Path)
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Return a non-zero status when any import/extraction/harness failure is reported.",
    )
    parser.add_argument(
        "--jobs",
        type=int,
        default=0,
        help="Number of music21 worker processes. 0 (default) uses host CPU count minus 1.",
    )
    parser.add_argument(
        "--cache-db",
        type=Path,
        help=(
            "SQLite comparison cache path. Defaults to $"
            + CACHE_DB_ENV_VAR
            + f" or {DEFAULT_CACHE_DB}."
        ),
    )
    parser.add_argument(
        "--no-cache",
        action="store_true",
        help="Disable the content-addressed comparison cache.",
    )
    parser.add_argument(
        "--log-format",
        choices=["jsonl", "text"],
        default="jsonl",
        help=(
            "Log/progress format. `jsonl` (default) emits one machine-readable"
            " JSON object per event for agent consumption; `text` keeps the"
            " legacy human-oriented lines."
        ),
    )
    parser.add_argument(
        "--worker-chunk-size",
        type=int,
        default=8,
        help="Number of files to hand to each worker process at a time.",
    )
    parser.add_argument(
        "--polars-threads-per-worker",
        type=int,
        help=(
            "Set POLARS_MAX_THREADS inside worker processes. "
            "Defaults to 1 when --jobs is greater than 1."
        ),
    )
    args = parser.parse_args()
    if args.limit_files is not None and args.limit is not None:
        raise SystemExit("use only one of --limit or --limit-files")
    if args.limit_files is None:
        args.limit_files = args.limit
    if args.sample_per_category is None:
        args.sample_per_category = 5
    return args


def resolve_jobs(requested_jobs: int) -> int:
    if requested_jobs < 0:
        raise SystemExit("--jobs must be >= 0")
    if requested_jobs == 0:
        return max(1, (os.cpu_count() or 1) - 1)
    return requested_jobs


def resolve_worker_chunk_size(requested_chunk_size: int) -> int:
    if requested_chunk_size < 1:
        raise SystemExit("--worker-chunk-size must be >= 1")
    return requested_chunk_size


def resolve_polars_threads_per_worker(
    requested_threads: int | None,
    jobs: int,
) -> int | None:
    if requested_threads is not None:
        if requested_threads < 1:
            raise SystemExit("--polars-threads-per-worker must be >= 1")
        return requested_threads
    if jobs > 1:
        return 1
    return None


def sibling_jsonl(path: Path | None) -> Path | None:
    return path.with_suffix(".jsonl") if path is not None else None


def clear_jsonl_outputs(paths: list[Path | None]) -> None:
    for path in paths:
        if path is not None:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("", encoding="utf-8")


def load_results(path: Path) -> list[dict[str, Any]]:
    results = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip():
            continue
        value = json.loads(line)
        if not isinstance(value, dict):
            raise SystemExit(f"{path}: line {line_number} is not a JSON object")
        results.append(value)
    return results


def select_results(results: list[dict[str, Any]], args: argparse.Namespace) -> list[dict[str, Any]]:
    selected = results
    only_files = load_only_files(args.only_files)
    if only_files:
        selected = [
            item
            for item in selected
            if result_matches_only_files(item, only_files)
        ]
    if args.limit_files is not None:
        if args.limit_files < 0:
            raise SystemExit("--limit-files must be non-negative")
        selected = selected[: args.limit_files]
    return selected


def load_only_files(values: list[str]) -> set[str]:
    selected: set[str] = set()
    for value in values:
        path = Path(value)
        if path.exists() and path.is_file():
            for line in path.read_text(encoding="utf-8").splitlines():
                stripped = line.strip()
                if stripped and not stripped.startswith("#"):
                    selected.add(stripped)
                    selected.add(Path(stripped).name)
            continue
        selected.add(value)
        selected.add(Path(value).name)
    return selected


def result_matches_only_files(item: dict[str, Any], only_files: set[str]) -> bool:
    relative_path = relative_path_for(item)
    path = item.get("path")
    candidates = [
        relative_path,
        Path(relative_path).name if relative_path else None,
        path if isinstance(path, str) else None,
        Path(path).name if isinstance(path, str) else None,
    ]
    return any(candidate in only_files for candidate in candidates if candidate)


def resolve_component_filter(component_args: list[str]) -> set[str] | None:
    components: set[str] = set()
    for value in component_args:
        for raw_component in value.split(","):
            component = raw_component.strip().lower()
            if not component:
                continue
            component = COMPONENT_ALIASES.get(component, component)
            if component not in ALL_COMPONENTS:
                choices = ", ".join(sorted(ALL_COMPONENTS))
                raise SystemExit(f"unknown component `{raw_component}`; expected one of: {choices}")
            components.add(component)
    return components or None


def relative_path_for(item: dict[str, Any]) -> str | None:
    relative_path = item.get("relative_path")
    if isinstance(relative_path, str) and relative_path:
        return relative_path
    path = item.get("path")
    if isinstance(path, str) and path:
        return Path(path).name
    return None


def source_path_for(item: dict[str, Any]) -> str | None:
    path = item.get("path")
    return path if isinstance(path, str) and path else None


def resolve_croma_xml(
    croma_xml_root: Path | None,
    item: dict[str, Any],
    relative_path: str,
) -> Path | None:
    music21 = item.get("music21", {})
    for candidate in [music21.get("croma_xml") if isinstance(music21, dict) else None]:
        if isinstance(candidate, str):
            path = Path(candidate)
            if path.exists():
                return path

    if croma_xml_root is not None:
        path = croma_xml_root / Path(relative_path).with_suffix(".croma.musicxml")
        if path.exists():
            return path
    return None


def resolve_reference_xml(
    reference_root: Path,
    item: dict[str, Any],
    relative_path: str,
) -> Path | None:
    music21 = item.get("music21", {})
    if isinstance(music21, dict):
        for key in ["reference_xml", "reference"]:
            candidate = music21.get(key)
            if isinstance(candidate, str):
                path = Path(candidate)
                if path.exists():
                    return path

    relative = Path(relative_path)
    for suffix in [".musicxml", ".xml"]:
        path = reference_root / relative.with_suffix(suffix)
        if path.exists():
            return path
    return None


def comparison_task_results(
    tasks: list[dict[str, Any]],
    jobs: int,
    worker_chunk_size: int,
    polars_threads_per_worker: int | None,
) -> Any:
    if jobs == 1:
        configure_worker_environment(polars_threads_per_worker)
        for task in tasks:
            yield run_comparison_task(task)
        return

    with ProcessPoolExecutor(
        max_workers=jobs,
        initializer=configure_worker_environment,
        initargs=(polars_threads_per_worker,),
    ) as executor:
        yield from executor.map(
            run_comparison_task,
            tasks,
            chunksize=worker_chunk_size,
        )


def configure_worker_environment(polars_threads_per_worker: int | None) -> None:
    if polars_threads_per_worker is not None:
        os.environ["POLARS_MAX_THREADS"] = str(polars_threads_per_worker)


def resolve_cache_db_path(args: argparse.Namespace) -> Path | None:
    if args.no_cache:
        return None
    if args.cache_db is not None:
        return args.cache_db
    env_value = os.environ.get(CACHE_DB_ENV_VAR)
    if env_value:
        return Path(env_value)
    return DEFAULT_CACHE_DB


_WORKER_CACHE: CompareCache | None = None
_WORKER_CACHE_PATH: str | None = None
_WORKER_CACHE_FAILED_PATH: str | None = None


def task_cache(task: dict[str, Any], counters: Counter[str]) -> CompareCache | None:
    """Open (once per process) the cache configured for this task."""
    global _WORKER_CACHE, _WORKER_CACHE_PATH, _WORKER_CACHE_FAILED_PATH
    path = task.get("cache_db")
    if path is None:
        return None
    if _WORKER_CACHE is not None and _WORKER_CACHE_PATH == path:
        return _WORKER_CACHE
    if _WORKER_CACHE_FAILED_PATH == path:
        return None
    try:
        cache = CompareCache.open(Path(path))
    except Exception:  # noqa: BLE001 - a broken cache must never fail the run.
        counters["cache_errors"] += 1
        _WORKER_CACHE_FAILED_PATH = path
        return None
    if _WORKER_CACHE is not None:
        _WORKER_CACHE.close()
    _WORKER_CACHE = cache
    _WORKER_CACHE_PATH = path
    return cache


CACHE_COUNTER_KEYS = {
    "facts_cache_hits",
    "facts_cache_misses",
    "result_cache_hits",
    "result_cache_misses",
    "cache_errors",
}


def cacheable_result_payload(result: dict[str, Any], counters: Counter[str]) -> dict[str, Any]:
    payload = dict(result)
    payload["counters"] = {
        key: int(value) for key, value in counters.items() if key not in CACHE_COUNTER_KEYS
    }
    payload["facts_jsonl"] = None
    payload["comparison_jsonl"] = None
    payload["mismatches_jsonl"] = None
    return payload


def replay_cached_result(
    task: dict[str, Any],
    payload: dict[str, Any],
    counters: Counter[str],
) -> dict[str, Any]:
    result = dict(payload)
    merged: Counter[str] = Counter()
    merge_counter(merged, result.get("counters", {}))
    merge_counter(merged, dict(counters))
    result["counters"] = dict(merged)
    result["filename"] = task["filename"]
    result["per_file_summary_jsonl"] = patch_per_file_summary_paths(
        result.get("per_file_summary_jsonl"), task
    )
    return result


def patch_per_file_summary_paths(text: str | None, task: dict[str, Any]) -> str | None:
    if not text:
        return text
    row = json.loads(text)
    row["source_path"] = task.get("source_path")
    row["croma_xml_path"] = task.get("croma_xml")
    row["reference_xml_path"] = task.get("reference_xml")
    return json.dumps(row, sort_keys=True, ensure_ascii=False) + "\n"


def run_comparison_task(task: dict[str, Any]) -> dict[str, Any]:
    try:
        return run_comparison_task_inner(task)
    except Exception as error:  # noqa: BLE001 - worker failures are report data.
        if task.get("strict"):
            raise
        return worker_failure_result(task, error)


def run_comparison_task_inner(task: dict[str, Any]) -> dict[str, Any]:
    counters: Counter[str] = Counter()
    status_counts: Counter[str] = Counter()
    import_failures: list[dict[str, Any]] = []
    component_filter = set(task["component_filter"]) if task.get("component_filter") else None

    croma_xml = Path(task["croma_xml"]) if task.get("croma_xml") is not None else None
    reference_xml = (
        Path(task["reference_xml"]) if task.get("reference_xml") is not None else None
    )

    cache = task_cache(task, counters)
    side_hashes = {
        "croma": file_sha256(croma_xml) if cache is not None and croma_xml is not None else None,
        "reference": (
            file_sha256(reference_xml)
            if cache is not None and reference_xml is not None
            else None
        ),
    }
    writes_row_tables = bool(
        task.get("write_facts_jsonl")
        or task.get("write_comparison_jsonl")
        or task.get("write_mismatches_jsonl")
    )
    pair_key = None
    if (
        cache is not None
        and not writes_row_tables
        and side_hashes["croma"] is not None
        and side_hashes["reference"] is not None
    ):
        pair_key = pair_result_key(
            side_hashes["croma"],
            side_hashes["reference"],
            task["result_cache_version"],
            task["relative_path"],
            {
                "component_filter": task.get("component_filter"),
                "sample_per_category": int(task["sample_per_category"]),
            },
        )
        try:
            cached_payload = cache.get_result(pair_key)
        except Exception:  # noqa: BLE001 - cache failures fall back to recompute.
            counters["cache_errors"] += 1
            cached_payload = None
        if cached_payload is not None:
            counters["result_cache_hits"] += 1
            return replay_cached_result(task, cached_payload, counters)
        counters["result_cache_misses"] += 1

    side_results: dict[str, dict[str, Any]] = {}
    for side, xml_path in [("croma", croma_xml), ("reference", reference_xml)]:
        side_results[side] = extract_side(
            task=task,
            side=side,
            xml_path=xml_path,
            component_filter=component_filter,
            counters=counters,
            status_counts=status_counts,
            import_failures=import_failures,
            cache=cache,
            content_hash=side_hashes[side],
        )

    fact_rows = side_results["croma"]["fact_rows"] + side_results["reference"]["fact_rows"]
    result = empty_task_result(task, counters, status_counts, import_failures)

    if side_results["croma"]["status"] != "success" or side_results["reference"]["status"] != "success":
        failure_rows = [
            failure_comparison_row(task, side, side_result)
            for side, side_result in side_results.items()
            if side_result["status"] != "success"
        ]
        result.update(serialize_task_rows(task, fact_rows, failure_rows, failure_rows))
        result["counters"] = merge_task_counter_dicts(
            result["counters"],
            {"structural_mismatches": 1},
        )
        result["mismatch_category_counts"] = dict(
            Counter(row["mismatch_category"] for row in failure_rows)
        )
        result["component_category_counts"] = component_category_count_rows(failure_rows)
        result["file_mismatch_categories"] = sorted(
            {row["mismatch_category"] for row in failure_rows}
        )
        result["file_component_categories"] = sorted(
            {(row["component"], row["mismatch_category"]) for row in failure_rows}
        )
        result["examples"] = failure_examples(failure_rows, int(task["sample_per_category"]))
        result["per_file_summary_jsonl"] = per_file_summary_jsonl_text(
            task,
            side_results,
            len(fact_rows),
            len(failure_rows),
            len(failure_rows),
            result["mismatch_category_counts"],
        )
        result["per_file_summary_rows"] = 1
        return result

    pl = import_polars()
    facts = facts_frame(pl, fact_rows)
    comparison = comparison_frame(pl, facts, sort_rows=True, task=task)
    mismatches = comparison.filter(~pl.col("matches"))

    result.update(
        serialize_task_frames(
            task=task,
            facts=facts,
            comparison=comparison,
            mismatches=mismatches,
        )
    )

    if mismatches.height:
        counters["structural_mismatches"] += 1
        summary = mismatch_summary(mismatches, int(task["sample_per_category"]))
        result["mismatch_category_counts"] = dict(summary["category_counts"])
        result["component_category_counts"] = summary["component_category_counts"]
        result["file_mismatch_categories"] = summary["file_mismatch_categories"]
        result["file_component_categories"] = summary["file_component_categories"]
        result["examples"] = summary["examples"]
    else:
        counters["structural_matches"] += 1

    result["per_file_summary_jsonl"] = per_file_summary_jsonl_text(
        task,
        side_results,
        facts.height,
        comparison.height,
        mismatches.height,
        result["mismatch_category_counts"],
    )
    result["per_file_summary_rows"] = 1
    if pair_key is not None and cache is not None:
        try:
            cache.put_result(
                pair_key,
                task["relative_path"],
                cacheable_result_payload(result, counters),
            )
        except Exception:  # noqa: BLE001 - cache failures must never fail the run.
            counters["cache_errors"] += 1
    result["counters"] = dict(counters)
    return result


def extract_side(
    *,
    task: dict[str, Any],
    side: str,
    xml_path: Path | None,
    component_filter: set[str] | None,
    counters: Counter[str],
    status_counts: Counter[str],
    import_failures: list[dict[str, Any]],
    cache: CompareCache | None = None,
    content_hash: str | None = None,
) -> dict[str, Any]:
    if xml_path is None:
        status_counts[f"{side}_xml_missing"] += 1
        counters["comparison_harness_issues"] += 1
        diagnostic = failure_diagnostic(
            task,
            side,
            None,
            "FileNotFoundError",
            f"{side} MusicXML file was not found",
            None,
            "comparison_harness_issue",
        )
        return {
            "status": "missing",
            "fact_rows": [diagnostic_fact_row(task, side, None, diagnostic)],
            "diagnostic": diagnostic,
        }

    counters[f"{side}_musicxml_import_attempts"] += 1
    facts_version = task.get("facts_cache_version")
    cached_facts = None
    if cache is not None and content_hash is not None and facts_version:
        try:
            cached_facts = cache.get_facts(content_hash, facts_version)
        except Exception:  # noqa: BLE001 - cache failures fall back to extraction.
            counters["cache_errors"] += 1
            cached_facts = None
        if cached_facts is not None:
            counters["facts_cache_hits"] += 1
        else:
            counters["facts_cache_misses"] += 1
    if cached_facts is not None:
        counters[f"{side}_musicxml_import_successes"] += 1
        return {
            "status": "success",
            "fact_rows": normalized_fact_rows(
                task=task,
                side=side,
                xml_path=xml_path,
                facts=cached_facts,
                component_filter=component_filter,
            ),
            "diagnostic": None,
        }
    try:
        facts = extract_facts(xml_path, side)
    except Music21Unavailable as error:
        counters[f"{side}_musicxml_import_failures"] += 1
        diagnostic = failure_diagnostic(
            task,
            side,
            xml_path,
            error.__class__.__name__,
            str(error),
            traceback.format_exc(),
            "import_failure",
        )
        import_failures.append(diagnostic)
        return {
            "status": "import_failure",
            "fact_rows": [diagnostic_fact_row(task, side, xml_path, diagnostic)],
            "diagnostic": diagnostic,
        }
    except Music21ParseFailure as error:
        counters[f"{side}_musicxml_import_failures"] += 1
        diagnostic = failure_diagnostic(
            task,
            side,
            xml_path,
            error.__class__.__name__,
            str(error),
            traceback.format_exc(),
            "import_failure",
        )
        import_failures.append(diagnostic)
        return {
            "status": "import_failure",
            "fact_rows": [diagnostic_fact_row(task, side, xml_path, diagnostic)],
            "diagnostic": diagnostic,
        }
    except Exception as error:  # noqa: BLE001 - extraction failures are report data.
        counters[f"{side}_musicxml_import_failures"] += 1
        diagnostic = failure_diagnostic(
            task,
            side,
            xml_path,
            error.__class__.__name__,
            str(error),
            traceback.format_exc(),
            "extraction_failure",
        )
        import_failures.append(diagnostic)
        return {
            "status": "extraction_failure",
            "fact_rows": [diagnostic_fact_row(task, side, xml_path, diagnostic)],
            "diagnostic": diagnostic,
        }

    if cache is not None and content_hash is not None and facts_version:
        try:
            cache.put_facts(content_hash, facts_version, task.get("relative_path"), side, facts)
        except Exception:  # noqa: BLE001 - cache failures must never fail the run.
            counters["cache_errors"] += 1

    counters[f"{side}_musicxml_import_successes"] += 1
    return {
        "status": "success",
        "fact_rows": normalized_fact_rows(
            task=task,
            side=side,
            xml_path=xml_path,
            facts=facts,
            component_filter=component_filter,
        ),
        "diagnostic": None,
    }


def failure_diagnostic(
    task: dict[str, Any],
    side: str,
    xml_path: Path | None,
    exception_type: str,
    message: str,
    traceback_text: str | None,
    category: str,
) -> dict[str, Any]:
    return {
        "filename": task["filename"],
        "relative_path": task["relative_path"],
        "source_side": side,
        "source_path": task.get("source_path"),
        "xml_path": str(xml_path) if xml_path is not None else None,
        "exception_type": exception_type,
        "message": message,
        "traceback": traceback_text,
        "mismatch_category": category,
    }


def normalized_fact_rows(
    *,
    task: dict[str, Any],
    side: str,
    xml_path: Path,
    facts: dict[str, Any],
    component_filter: set[str] | None,
) -> list[dict[str, Any]]:
    builder = FactBuilder(task, side, xml_path, component_filter)
    builder.add_global("metadata", "part_count", facts.get("part_count"))

    for part_index, part in enumerate(facts.get("parts", [])):
        part_id = stable_part_id(part.get("id"))
        builder.add_global(
            "measure",
            "measure_count",
            part.get("measure_count"),
            part_id=part_id,
            part_index=part_index,
            alignment_index=part_index,
        )
        for measure_index, measure in enumerate(part.get("measures", [])):
            measure_number = optional_string(measure.get("number"))
            measure_base = {
                "part_id": part_id,
                "part_index": part_index,
                "measure_number": measure_number,
                "measure_index": measure_index,
            }
            builder.add("measure", "number", measure.get("number"), **measure_base)
            builder.add("measure", "offset", measure.get("offset"), **measure_base)
            builder.add("measure", "duration", measure.get("duration"), **measure_base)
            builder.add("measure", "bar_duration", measure.get("bar_duration"), **measure_base)
            builder.add(
                "measure",
                "event_count",
                len(measure.get("events", [])),
                **measure_base,
            )
            for voice_index, voice in enumerate(measure.get("voices", [])):
                builder.add(
                    "metadata",
                    "voice",
                    voice,
                    **measure_base,
                    voice=optional_string(voice.get("id")),
                    alignment_index=voice_index,
                )
            for side_name, barline in (measure.get("barlines") or {}).items():
                builder.add(
                    "barline",
                    side_name,
                    barline,
                    **measure_base,
                    alignment_index=0 if side_name == "left" else 1,
                )
            for event_index, event in enumerate(measure.get("events", [])):
                add_event_rows(builder, event, measure_base, event_index)

    for index, slur in enumerate(facts.get("slurs", [])):
        builder.add_global("slur", "item", slur, alignment_index=index)
    for index, repeat_ending in enumerate(facts.get("repeat_endings", [])):
        builder.add_global("barline", "repeat_ending", repeat_ending, alignment_index=index)
    for index, harmony in enumerate(facts.get("harmony", [])):
        builder.add_global("harmony", "item", harmony, alignment_index=index)
    for index, direction in enumerate(facts.get("directions", [])):
        builder.add_global("direction", "item", direction, alignment_index=index)

    return builder.rows


def add_event_rows(
    builder: "FactBuilder",
    event: dict[str, Any],
    measure_base: dict[str, Any],
    event_index: int,
) -> None:
    kind = optional_string(event.get("kind"))
    component = "rest" if kind == "rest" else "note"
    voice = optional_string(event.get("voice"))
    onset = optional_string(event.get("offset"))
    duration = event.get("duration", {})
    duration_value = optional_string(duration.get("quarter_length"))
    event_base = {
        **measure_base,
        "voice": voice,
        "event_index": event_index,
        "alignment_index": 0,
        "onset": onset,
        "duration": duration_value,
    }
    builder.add(component, "kind", kind, raw_value=event, **event_base)
    builder.add(component, "voice", voice, **event_base)
    builder.add("duration", "quarter_length", duration.get("quarter_length"), **event_base)
    builder.add("duration", "type", duration.get("type"), **event_base)
    builder.add("duration", "dots", duration.get("dots"), **event_base)
    builder.add("tuplet", "tuplets", duration.get("tuplets", []), **event_base)
    builder.add("tie", "type", event.get("tie"), **event_base)

    for lyric_index, lyric_text in enumerate(event.get("lyrics", [])):
        lyric_base = {**event_base, "alignment_index": lyric_index}
        builder.add(
            "lyric",
            "text",
            lyric_text,
            **lyric_base,
        )

    if "pitch" in event:
        add_pitch_rows(builder, event.get("pitch") or {}, event_base, 0)
    if "pitches" in event:
        pitches = event.get("pitches", [])
        builder.add("pitch", "count", len(pitches), **event_base)
        for pitch_index, pitch in enumerate(pitches):
            add_pitch_rows(builder, pitch, event_base, pitch_index)


def add_pitch_rows(
    builder: "FactBuilder",
    pitch: dict[str, Any],
    event_base: dict[str, Any],
    pitch_index: int,
) -> None:
    accidental = pitch.get("accidental")
    pitch_kwargs = {
        **event_base,
        "alignment_index": pitch_index,
        "pitch_step": optional_string(pitch.get("step")),
        "pitch_alter": accidental_to_alter(accidental),
        "pitch_octave": optional_int(pitch.get("octave")),
    }
    builder.add("pitch", "step", pitch.get("step"), **pitch_kwargs)
    builder.add("pitch", "alter", accidental_to_alter(accidental), raw_value=accidental, **pitch_kwargs)
    builder.add("pitch", "octave", pitch.get("octave"), **pitch_kwargs)


class FactBuilder:
    def __init__(
        self,
        task: dict[str, Any],
        side: str,
        xml_path: Path,
        component_filter: set[str] | None,
    ) -> None:
        self.task = task
        self.side = side
        self.xml_path = xml_path
        self.component_filter = component_filter
        self.rows: list[dict[str, Any]] = []

    def add_global(
        self,
        component: str,
        field_name: str,
        value: Any,
        **kwargs: Any,
    ) -> None:
        self.add(component, field_name, value, **kwargs)

    def add(
        self,
        component: str,
        field_name: str,
        value: Any,
        *,
        raw_value: Any | None = None,
        part_id: str | None = None,
        part_index: int | None = None,
        measure_number: str | None = None,
        measure_index: int | None = None,
        voice: str | None = None,
        staff: str | None = None,
        event_index: int | None = None,
        alignment_index: int | None = None,
        onset: str | None = None,
        duration: str | None = None,
        pitch_step: str | None = None,
        pitch_alter: float | None = None,
        pitch_octave: int | None = None,
    ) -> None:
        if self.component_filter is not None and component not in self.component_filter:
            return
        value_text = fast_encode_fact_value(value)
        raw_text = value_text if raw_value is None else fast_encode_fact_value(raw_value)
        row = {
            "relative_path": self.task["relative_path"],
            "filename": self.task["filename"],
            "source_side": self.side,
            "component": component,
            "field_name": field_name,
            "part_id": part_id,
            "part_index": part_index,
            "measure_number": measure_number,
            "measure_index": measure_index,
            "voice": voice,
            "staff": staff,
            "event_index": event_index,
            "alignment_index": alignment_index,
            "onset": onset,
            "duration": duration,
            "pitch_step": pitch_step,
            "pitch_alter": pitch_alter,
            "pitch_octave": pitch_octave,
            "value_text": value_text,
            "raw_value": raw_text,
            "source_path": self.task.get("source_path"),
            "xml_path": str(self.xml_path),
            "extraction_status": "success",
            "diagnostic": None,
        }
        self.rows.append(row)


def diagnostic_fact_row(
    task: dict[str, Any],
    side: str,
    xml_path: Path | None,
    diagnostic: dict[str, Any],
) -> dict[str, Any]:
    row = {
        "relative_path": task["relative_path"],
        "filename": task["filename"],
        "source_side": side,
        "component": "metadata",
        "field_name": "extraction_status",
        "part_id": None,
        "part_index": None,
        "measure_number": None,
        "measure_index": None,
        "voice": None,
        "staff": None,
        "event_index": None,
        "alignment_index": None,
        "onset": None,
        "duration": None,
        "pitch_step": None,
        "pitch_alter": None,
        "pitch_octave": None,
        "value_text": diagnostic["mismatch_category"],
        "raw_value": encode_fact_value(diagnostic),
        "source_path": task.get("source_path"),
        "xml_path": str(xml_path) if xml_path is not None else None,
        "extraction_status": diagnostic["mismatch_category"],
        "diagnostic": encode_fact_value(diagnostic),
    }
    return row


def comparison_key_expr(pl: Any) -> Any:
    """Columnar equivalent of the former per-row json.dumps comparison key."""
    return (
        pl.struct([pl.col(field) for field in COMPARISON_KEY_FIELDS])
        .struct.json_encode()
        .alias("comparison_key")
    )


def fast_encode_fact_value(value: Any) -> str | None:
    """orjson-backed encoder for the ~25M fact values per corpus run.

    Scalar output is byte-identical to music21_compare.encode_fact_value;
    containers are compact (`{"a":1}` rather than `{"a": 1}`). Both sides of
    a comparison run through this same encoder, so match verdicts are
    unaffected by the format difference. Values orjson rejects (ints beyond
    64 bits, exotic types) fall back to the stdlib encoder.
    """
    if value is None:
        return None
    try:
        return orjson.dumps(value, option=orjson.OPT_SORT_KEYS, default=str).decode("utf-8")
    except TypeError:
        return encode_fact_value(value)


def accidental_to_alter(accidental: Any) -> float | None:
    if accidental is None:
        return None
    text = str(accidental)
    mapping = {
        "natural": 0.0,
        "sharp": 1.0,
        "flat": -1.0,
        "double-sharp": 2.0,
        "double-flat": -2.0,
        "half-sharp": 0.5,
        "half-flat": -0.5,
        "one-and-a-half-sharp": 1.5,
        "one-and-a-half-flat": -1.5,
    }
    return mapping.get(text)


def failure_comparison_row(
    task: dict[str, Any],
    side: str,
    side_result: dict[str, Any],
) -> dict[str, Any]:
    diagnostic = side_result["diagnostic"]
    category = diagnostic["mismatch_category"]
    source_side = side
    row = {
        "relative_path": task["relative_path"],
        "filename": task["filename"],
        "source_side": source_side,
        "component": "metadata",
        "field_name": "extraction_status",
        "part_id": None,
        "part_index": None,
        "measure_number": None,
        "measure_index": None,
        "voice": None,
        "staff": None,
        "event_index": None,
        "alignment_index": None,
        "onset": None,
        "duration": None,
        "pitch_step": None,
        "pitch_alter": None,
        "pitch_octave": None,
        "source_path": task.get("source_path"),
        "croma_xml_path": task.get("croma_xml"),
        "reference_xml_path": task.get("reference_xml"),
        "croma_present": side == "croma",
        "reference_present": side == "reference",
        "croma_value": encode_fact_value(diagnostic) if side == "croma" else None,
        "reference_value": encode_fact_value(diagnostic) if side == "reference" else None,
        "croma_raw_value": encode_fact_value(diagnostic) if side == "croma" else None,
        "reference_raw_value": encode_fact_value(diagnostic) if side == "reference" else None,
        "extraction_status": category,
        "diagnostic": encode_fact_value(diagnostic),
        "matches": False,
        "mismatch_category": category,
    }
    return row


def facts_frame(pl: Any, fact_rows: list[dict[str, Any]]) -> Any:
    if not fact_rows:
        return pl.DataFrame(schema=fact_schema(pl))
    return (
        pl.DataFrame(fact_rows, schema=fact_schema(pl))
        .with_columns(comparison_key_expr(pl))
        .select(FACT_COLUMNS)
    )


def comparison_frame(
    pl: Any,
    facts: Any,
    *,
    sort_rows: bool,
    task: dict[str, Any],
) -> Any:
    # relative_path and filename are constant within a task, so the join key
    # is comparison_key alone; the constants are re-attached as literals.
    value_columns = [column for column in FACT_COLUMNS if column not in JOIN_KEY_COLUMNS]
    croma = (
        facts.filter(pl.col("source_side") == "croma")
        .select(
            "comparison_key",
            *[pl.col(column).alias(f"croma_{column}") for column in value_columns],
            pl.lit(True).alias("croma_present"),
        )
    )
    reference = (
        facts.filter(pl.col("source_side") == "reference")
        .select(
            "comparison_key",
            *[pl.col(column).alias(f"reference_{column}") for column in value_columns],
            pl.lit(True).alias("reference_present"),
        )
    )
    joined = (
        croma.join(reference, on="comparison_key", how="full", coalesce=True)
        .with_columns(
            pl.col("croma_present").fill_null(False),
            pl.col("reference_present").fill_null(False),
        )
        .with_columns(
            (
                pl.col("croma_present")
                & pl.col("reference_present")
                & (
                    (pl.col("croma_value_text") == pl.col("reference_value_text"))
                    | (
                        pl.col("croma_value_text").is_null()
                        & pl.col("reference_value_text").is_null()
                    )
                )
            )
            .fill_null(False)
            .alias("matches")
        )
    )
    comparison = joined.with_columns(
        pl.lit(task["relative_path"], dtype=pl.String).alias("relative_path"),
        pl.lit(task["filename"], dtype=pl.String).alias("filename"),
        coalesce_string(pl, "component"),
        coalesce_string(pl, "field_name"),
        coalesce_string(pl, "part_id"),
        coalesce_int(pl, "part_index"),
        coalesce_string(pl, "measure_number"),
        coalesce_int(pl, "measure_index"),
        coalesce_string(pl, "voice"),
        coalesce_string(pl, "staff"),
        coalesce_int(pl, "event_index"),
        coalesce_int(pl, "alignment_index"),
        coalesce_string(pl, "onset"),
        coalesce_string(pl, "duration"),
        coalesce_string(pl, "pitch_step"),
        coalesce_float(pl, "pitch_alter"),
        coalesce_int(pl, "pitch_octave"),
        coalesce_string(pl, "source_path"),
        pl.col("croma_xml_path").alias("croma_xml_path"),
        pl.col("reference_xml_path").alias("reference_xml_path"),
        source_side_expr(pl).alias("source_side"),
        extraction_status_expr(pl).alias("extraction_status"),
        diagnostic_expr(pl).alias("diagnostic"),
        mismatch_category_expr(pl).alias("mismatch_category"),
    ).select(
        "relative_path",
        "filename",
        "source_side",
        "component",
        "field_name",
        "part_id",
        "part_index",
        "measure_number",
        "measure_index",
        "voice",
        "staff",
        "event_index",
        "alignment_index",
        "onset",
        "duration",
        "pitch_step",
        "pitch_alter",
        "pitch_octave",
        "source_path",
        "croma_xml_path",
        "reference_xml_path",
        "croma_present",
        "reference_present",
        pl.col("croma_value_text").alias("croma_value"),
        pl.col("reference_value_text").alias("reference_value"),
        pl.col("croma_raw_value").alias("croma_raw_value"),
        pl.col("reference_raw_value").alias("reference_raw_value"),
        "extraction_status",
        "diagnostic",
        "matches",
        "mismatch_category",
        "comparison_key",
    )
    comparison = comparison.with_columns(
        pl.col("source_path").fill_null(pl.lit(task.get("source_path"))),
        pl.col("croma_xml_path").fill_null(pl.lit(task.get("croma_xml"))),
        pl.col("reference_xml_path").fill_null(pl.lit(task.get("reference_xml"))),
    )
    if sort_rows:
        return comparison.sort(SORT_COLUMNS)
    return comparison


def coalesce_string(pl: Any, column: str) -> Any:
    return pl.coalesce([pl.col(f"croma_{column}"), pl.col(f"reference_{column}")]).alias(column)


def coalesce_int(pl: Any, column: str) -> Any:
    return pl.coalesce([pl.col(f"croma_{column}"), pl.col(f"reference_{column}")]).alias(column)


def coalesce_float(pl: Any, column: str) -> Any:
    return pl.coalesce([pl.col(f"croma_{column}"), pl.col(f"reference_{column}")]).alias(column)


def source_side_expr(pl: Any) -> Any:
    return (
        pl.when(pl.col("croma_present") & pl.col("reference_present"))
        .then(pl.lit("both"))
        .when(pl.col("croma_present"))
        .then(pl.lit("croma"))
        .when(pl.col("reference_present"))
        .then(pl.lit("reference"))
        .otherwise(pl.lit("unknown"))
    )


def extraction_status_expr(pl: Any) -> Any:
    return (
        pl.when(pl.col("croma_extraction_status") != "success")
        .then(pl.col("croma_extraction_status"))
        .when(pl.col("reference_extraction_status") != "success")
        .then(pl.col("reference_extraction_status"))
        .otherwise(pl.lit("success"))
    )


def diagnostic_expr(pl: Any) -> Any:
    return pl.coalesce([pl.col("croma_diagnostic"), pl.col("reference_diagnostic")])


def mismatch_category_expr(pl: Any) -> Any:
    component = pl.coalesce([pl.col("croma_component"), pl.col("reference_component")])
    field_name = pl.coalesce([pl.col("croma_field_name"), pl.col("reference_field_name")])
    return (
        pl.when(pl.col("matches"))
        .then(pl.lit(None, dtype=pl.String))
        .when(~pl.col("croma_present"))
        .then(pl.lit("missing_in_croma"))
        .when(~pl.col("reference_present"))
        .then(pl.lit("extra_in_croma"))
        .when((component == "pitch") & (field_name == "step"))
        .then(pl.lit("pitch"))
        .when((component == "pitch") & (field_name == "octave"))
        .then(pl.lit("octave"))
        .when((component == "pitch") & (field_name == "alter"))
        .then(pl.lit("accidental"))
        .when(component == "duration")
        .then(pl.lit("duration"))
        .when(component == "lyric")
        .then(pl.lit("lyric"))
        .when(component == "tie")
        .then(pl.lit("tie"))
        .when(component == "slur")
        .then(pl.lit("slur"))
        .when(component == "tuplet")
        .then(pl.lit("tuplet"))
        .when(component == "barline")
        .then(pl.lit("barline"))
        .when(component == "harmony")
        .then(pl.lit("harmony"))
        .when(component == "direction")
        .then(pl.lit("direction"))
        .when(field_name == "voice")
        .then(pl.lit("voice"))
        .when(field_name == "staff")
        .then(pl.lit("staff"))
        .when(component == "metadata")
        .then(pl.lit("metadata"))
        .otherwise(pl.lit("measure_alignment"))
    )


def serialize_task_rows(
    task: dict[str, Any],
    fact_rows: list[dict[str, Any]],
    comparison_rows: list[dict[str, Any]],
    mismatch_rows: list[dict[str, Any]],
) -> dict[str, Any]:
    pl = import_polars()
    facts = facts_frame(pl, fact_rows)
    comparison = comparison_rows_frame(pl, comparison_rows)
    mismatches = comparison_rows_frame(pl, mismatch_rows)
    return serialize_task_frames(
        task=task,
        facts=facts,
        comparison=comparison,
        mismatches=mismatches,
    )


def comparison_rows_frame(pl: Any, rows: list[dict[str, Any]]) -> Any:
    if not rows:
        return pl.DataFrame(schema=comparison_schema(pl))
    return (
        pl.DataFrame(rows, schema=comparison_schema(pl))
        .with_columns(comparison_key_expr(pl))
        .select(COMPARISON_COLUMNS)
    )


def serialize_task_frames(
    *,
    task: dict[str, Any],
    facts: Any,
    comparison: Any,
    mismatches: Any,
) -> dict[str, Any]:
    output = {
        "fact_rows": facts.height,
        "comparison_rows": comparison.height,
        "mismatch_rows": mismatches.height,
        "facts_jsonl": None,
        "comparison_jsonl": None,
        "mismatches_jsonl": None,
    }
    if task.get("write_facts_jsonl"):
        output["facts_jsonl"] = frame_jsonl_text(facts)
    if task.get("write_comparison_jsonl"):
        output["comparison_jsonl"] = frame_jsonl_text(comparison)
    if task.get("write_mismatches_jsonl"):
        output["mismatches_jsonl"] = frame_jsonl_text(mismatches)
    return output


def empty_task_result(
    task: dict[str, Any],
    counters: Counter[str],
    status_counts: Counter[str],
    import_failures: list[dict[str, Any]],
) -> dict[str, Any]:
    return {
        "filename": task["filename"],
        "counters": dict(counters),
        "status_counts": dict(status_counts),
        "mismatch_category_counts": {},
        "component_category_counts": [],
        "file_mismatch_categories": [],
        "file_component_categories": [],
        "examples": {},
        "import_failures": import_failures,
        "fact_rows": 0,
        "comparison_rows": 0,
        "mismatch_rows": 0,
        "per_file_summary_rows": 0,
        "facts_jsonl": None,
        "comparison_jsonl": None,
        "mismatches_jsonl": None,
        "per_file_summary_jsonl": None,
    }


def worker_failure_result(task: dict[str, Any], error: Exception) -> dict[str, Any]:
    counters: Counter[str] = Counter({"worker_failures": 1, "comparison_harness_issues": 1})
    status_counts: Counter[str] = Counter({"worker_failure": 1})
    diagnostic = failure_diagnostic(
        task,
        "comparison",
        None,
        error.__class__.__name__,
        str(error),
        traceback.format_exc(),
        "comparison_harness_issue",
    )
    side_result = {
        "status": "worker_failure",
        "fact_rows": [diagnostic_fact_row(task, "comparison", None, diagnostic)],
        "diagnostic": diagnostic,
    }
    result = empty_task_result(task, counters, status_counts, [diagnostic])
    failure_rows = [failure_comparison_row(task, "comparison", side_result)]
    result.update(serialize_task_rows(task, side_result["fact_rows"], failure_rows, failure_rows))
    result["counters"] = dict(counters + Counter({"structural_mismatches": 1}))
    result["mismatch_category_counts"] = {"comparison_harness_issue": 1}
    result["component_category_counts"] = [["metadata", "comparison_harness_issue", 1]]
    result["file_mismatch_categories"] = ["comparison_harness_issue"]
    result["file_component_categories"] = [["metadata", "comparison_harness_issue"]]
    result["examples"] = failure_examples(failure_rows, int(task["sample_per_category"]))
    result["per_file_summary_jsonl"] = per_file_summary_jsonl_text(
        task,
        {"croma": side_result, "reference": side_result},
        1,
        1,
        1,
        result["mismatch_category_counts"],
    )
    result["per_file_summary_rows"] = 1
    return result


def mismatch_summary(
    mismatches: Any,
    sample_per_category: int,
) -> dict[str, Any]:
    category_counts: Counter[str] = Counter(
        {
            str(category): int(count)
            for category, count in mismatches.group_by("mismatch_category").len().iter_rows()
        }
    )
    component_counts = component_category_count_rows_from_frame(mismatches)
    file_mismatch_categories = sorted(category_counts)
    file_component_categories = sorted(
        {
            (str(component), str(category))
            for component, category in mismatches.select(
                "component",
                "mismatch_category",
            )
            .unique()
            .iter_rows()
        }
    )
    examples: dict[str, list[dict[str, Any]]] = defaultdict(list)
    if sample_per_category > 0:
        example_rows = mismatches.group_by("mismatch_category", maintain_order=True).head(
            sample_per_category
        )
        for row in example_rows.iter_rows(named=True):
            category = str(row["mismatch_category"])
            examples[category].append(example_from_comparison_row(row))
    return {
        "category_counts": category_counts,
        "component_category_counts": component_counts,
        "file_mismatch_categories": file_mismatch_categories,
        "file_component_categories": file_component_categories,
        "examples": dict(examples),
    }


def component_category_count_rows_from_frame(mismatches: Any) -> list[list[Any]]:
    return [
        [str(component), str(category), int(count)]
        for component, category, count in mismatches.group_by(
            "component",
            "mismatch_category",
        )
        .len()
        .iter_rows()
    ]


def component_category_count_rows(rows: list[dict[str, Any]]) -> list[list[Any]]:
    counts: Counter[tuple[str, str]] = Counter(
        (row["component"], row["mismatch_category"]) for row in rows
    )
    return [[component, category, count] for (component, category), count in sorted(counts.items())]


def failure_examples(
    rows: list[dict[str, Any]],
    sample_per_category: int,
) -> dict[str, list[dict[str, Any]]]:
    examples: dict[str, list[dict[str, Any]]] = defaultdict(list)
    if sample_per_category <= 0:
        return {}
    for row in rows:
        category = str(row["mismatch_category"])
        if len(examples[category]) >= sample_per_category:
            continue
        examples[category].append(example_from_comparison_row(row))
    return dict(examples)


def example_from_comparison_row(row: dict[str, Any]) -> dict[str, Any]:
    return {
        "relative_path": row["relative_path"],
        "filename": row["filename"],
        "source_side": row["source_side"],
        "component": row["component"],
        "field_name": row["field_name"],
        "part_index": row["part_index"],
        "measure_index": row["measure_index"],
        "voice": row["voice"],
        "staff": row["staff"],
        "event_index": row["event_index"],
        "alignment_index": row["alignment_index"],
        "mismatch_category": row["mismatch_category"],
        "croma_present": row["croma_present"],
        "reference_present": row["reference_present"],
        "croma": decode_fact_value(row["croma_value"]),
        "reference": decode_fact_value(row["reference_value"]),
        "diagnostic": decode_fact_value(row["diagnostic"]),
    }


def per_file_summary_jsonl_text(
    task: dict[str, Any],
    side_results: dict[str, dict[str, Any]],
    fact_rows: int,
    comparison_rows: int,
    mismatch_rows: int,
    mismatch_categories: dict[str, int],
) -> str:
    row = {
        "relative_path": task["relative_path"],
        "filename": task["filename"],
        "source_path": task.get("source_path"),
        "croma_xml_path": task.get("croma_xml"),
        "reference_xml_path": task.get("reference_xml"),
        "croma_import_status": side_results["croma"]["status"],
        "reference_import_status": side_results["reference"]["status"],
        "comparison_status": "match" if mismatch_rows == 0 else "mismatch",
        "fact_rows": fact_rows,
        "comparison_rows": comparison_rows,
        "mismatch_rows": mismatch_rows,
        "mismatch_categories": encode_fact_value(mismatch_categories),
        "diagnostics": encode_fact_value(
            [
                side_result["diagnostic"]
                for side_result in side_results.values()
                if side_result.get("diagnostic") is not None
            ]
        ),
    }
    return json.dumps(row, sort_keys=True, ensure_ascii=False) + "\n"


def croma_export_failure_summary_row(
    *,
    item: dict[str, Any],
    relative_path: str,
    filename: str,
    source_path: str | None,
) -> dict[str, Any]:
    diagnostics = croma_failure_summary(item, relative_path)
    return {
        "relative_path": relative_path,
        "filename": filename,
        "source_path": source_path,
        "croma_xml_path": None,
        "reference_xml_path": None,
        "croma_import_status": "croma_export_failure",
        "reference_import_status": "not_attempted",
        "comparison_status": "not_attempted",
        "fact_rows": 0,
        "comparison_rows": 0,
        "mismatch_rows": 0,
        "mismatch_categories": encode_fact_value({}),
        "diagnostics": encode_fact_value([diagnostics]),
    }
    return row


def merge_task_counter_dicts(*sources: dict[str, int]) -> dict[str, int]:
    counter: Counter[str] = Counter()
    for source in sources:
        merge_counter(counter, source)
    return dict(counter)


def merge_counter(target: Counter[str], source: dict[str, int]) -> None:
    for key, count in source.items():
        target[str(key)] += int(count)


def merge_component_category_counts(
    target: Counter[tuple[str, str]],
    rows: list[list[Any]],
) -> None:
    for component, category, count in rows:
        target[(str(component), str(category))] += int(count)


def merge_examples(
    target: dict[str, list[dict[str, Any]]],
    source: dict[str, list[dict[str, Any]]],
    sample_per_category: int,
) -> None:
    for category, rows in source.items():
        for row in rows:
            if len(target[category]) >= sample_per_category:
                break
            target[category].append(row)


def build_per_component_summary_rows(
    component_category_counts: Counter[tuple[str, str]],
    component_affected_file_counts: Counter[tuple[str, str]],
) -> list[dict[str, Any]]:
    rows = []
    for (component, category), count in sorted(component_category_counts.items()):
        rows.append(
            {
                "component": component,
                "mismatch_category": category,
                "mismatch_rows": int(count),
                "affected_files": int(component_affected_file_counts[(component, category)]),
            }
        )
    return rows


def files_with_only_one_category(file_categories: dict[str, set[str]]) -> list[dict[str, Any]]:
    rows = []
    for filename, categories in sorted(file_categories.items()):
        if len(categories) == 1:
            rows.append({"filename": filename, "mismatch_category": next(iter(categories))})
    return rows


def candidate_file_list(file_mismatch_counts: Counter[str]) -> list[dict[str, Any]]:
    return [
        {"filename": filename, "mismatch_rows": int(count)}
        for filename, count in sorted(
            file_mismatch_counts.items(),
            key=lambda item: (-item[1], item[0]),
        )
    ]


def top_counter_rows(counter: Counter[str], limit: int = 20) -> list[dict[str, Any]]:
    return [
        {"key": key, "count": int(count)}
        for key, count in sorted(counter.items(), key=lambda item: (-item[1], item[0]))[:limit]
    ]


def baseline_delta_report(
    args: argparse.Namespace,
    current_file_counts: Counter[str],
) -> dict[str, Any] | None:
    if args.baseline_report is None and args.baseline_mismatches is None:
        return None
    baseline_report = None
    if args.baseline_report is not None and args.baseline_report.exists():
        baseline_report = json.loads(args.baseline_report.read_text(encoding="utf-8"))

    baseline_counts = (
        load_mismatch_file_counts(args.baseline_mismatches)
        if args.baseline_mismatches is not None
        else Counter()
    )
    current_counts = Counter(current_file_counts)
    all_files = sorted(set(baseline_counts) | set(current_counts))
    improved = []
    regressed = []
    unchanged = []
    for filename in all_files:
        before = int(baseline_counts[filename])
        after = int(current_counts[filename])
        delta = after - before
        row = {
            "filename": filename,
            "baseline_mismatch_rows": before,
            "current_mismatch_rows": after,
            "delta": delta,
            "classification": delta_classification(before, after),
        }
        if delta < 0:
            improved.append(row)
        elif delta > 0:
            regressed.append(row)
        else:
            unchanged.append(row)
    return {
        "baseline_report": str(args.baseline_report) if args.baseline_report else None,
        "baseline_mismatches": str(args.baseline_mismatches) if args.baseline_mismatches else None,
        "baseline_schema": baseline_report.get("schema") if isinstance(baseline_report, dict) else None,
        "improved": sorted(improved, key=lambda row: (row["delta"], row["filename"]))[:50],
        "regressed": sorted(regressed, key=lambda row: (-row["delta"], row["filename"]))[:50],
        "unchanged_count": len(unchanged),
    }


def delta_classification(before: int, after: int) -> str:
    if before == 0 and after > 0:
        return "new_regression"
    if before > 0 and after == 0:
        return "resolved"
    if after < before:
        return "improved"
    if after > before:
        return "regressed"
    return "unchanged"


def load_mismatch_file_counts(path: Path | None) -> Counter[str]:
    if path is None or not path.exists():
        return Counter()
    if path.stat().st_size == 0:
        return Counter()
    pl = import_polars()
    if path.suffix == ".parquet":
        frame = pl.read_parquet(path)
    else:
        frame = pl.read_ndjson(path)
    columns = set(frame.columns)
    if "filename" in columns:
        filename_column = "filename"
    elif "file_name" in columns:
        filename_column = "file_name"
    elif "relative_path" in columns:
        filename_column = "relative_path"
    else:
        return Counter()
    return Counter(
        {
            str(filename): int(count)
            for filename, count in frame.group_by(filename_column).len().iter_rows()
        }
    )


def write_candidate_outputs(
    args: argparse.Namespace,
    candidate_files: list[dict[str, Any]],
    source_paths: dict[str, str],
) -> None:
    if args.write_file_list is not None:
        args.write_file_list.parent.mkdir(parents=True, exist_ok=True)
        args.write_file_list.write_text(
            "".join(f"{row['filename']}\n" for row in candidate_files),
            encoding="utf-8",
        )
    if args.write_target_corpus_dir is not None:
        args.write_target_corpus_dir.mkdir(parents=True, exist_ok=True)
        source_by_name = {Path(path).name: path for path in source_paths.values()}
        for row in candidate_files:
            source_path = source_by_name.get(row["filename"])
            if source_path is None:
                continue
            source = Path(source_path)
            if source.exists() and source.is_file():
                shutil.copy2(source, args.write_target_corpus_dir / source.name)


def write_parquet(pl: Any, jsonl: Path | None, parquet: Path | None, schema: dict[str, Any]) -> None:
    if parquet is None:
        return
    parquet.parent.mkdir(parents=True, exist_ok=True)
    if jsonl is None or not jsonl.exists() or jsonl.stat().st_size == 0:
        pl.DataFrame(schema=schema).write_parquet(parquet)
        return
    pl.scan_ndjson(jsonl, schema=schema).sink_parquet(parquet)


def fact_schema(pl: Any) -> dict[str, Any]:
    return {
        "relative_path": pl.String,
        "filename": pl.String,
        "source_side": pl.String,
        "component": pl.String,
        "field_name": pl.String,
        "part_id": pl.String,
        "part_index": pl.Int64,
        "measure_number": pl.String,
        "measure_index": pl.Int64,
        "voice": pl.String,
        "staff": pl.String,
        "event_index": pl.Int64,
        "alignment_index": pl.Int64,
        "onset": pl.String,
        "duration": pl.String,
        "pitch_step": pl.String,
        "pitch_alter": pl.Float64,
        "pitch_octave": pl.Int64,
        "value_text": pl.String,
        "raw_value": pl.String,
        "source_path": pl.String,
        "xml_path": pl.String,
        "extraction_status": pl.String,
        "diagnostic": pl.String,
        "comparison_key": pl.String,
    }


def comparison_schema(pl: Any) -> dict[str, Any]:
    return {
        "relative_path": pl.String,
        "filename": pl.String,
        "source_side": pl.String,
        "component": pl.String,
        "field_name": pl.String,
        "part_id": pl.String,
        "part_index": pl.Int64,
        "measure_number": pl.String,
        "measure_index": pl.Int64,
        "voice": pl.String,
        "staff": pl.String,
        "event_index": pl.Int64,
        "alignment_index": pl.Int64,
        "onset": pl.String,
        "duration": pl.String,
        "pitch_step": pl.String,
        "pitch_alter": pl.Float64,
        "pitch_octave": pl.Int64,
        "source_path": pl.String,
        "croma_xml_path": pl.String,
        "reference_xml_path": pl.String,
        "croma_present": pl.Boolean,
        "reference_present": pl.Boolean,
        "croma_value": pl.String,
        "reference_value": pl.String,
        "croma_raw_value": pl.String,
        "reference_raw_value": pl.String,
        "extraction_status": pl.String,
        "diagnostic": pl.String,
        "matches": pl.Boolean,
        "mismatch_category": pl.String,
        "comparison_key": pl.String,
    }


def per_file_summary_schema(pl: Any) -> dict[str, Any]:
    return {
        "relative_path": pl.String,
        "filename": pl.String,
        "source_path": pl.String,
        "croma_xml_path": pl.String,
        "reference_xml_path": pl.String,
        "croma_import_status": pl.String,
        "reference_import_status": pl.String,
        "comparison_status": pl.String,
        "fact_rows": pl.Int64,
        "comparison_rows": pl.Int64,
        "mismatch_rows": pl.Int64,
        "mismatch_categories": pl.String,
        "diagnostics": pl.String,
    }


def per_component_summary_schema(pl: Any) -> dict[str, Any]:
    return {
        "component": pl.String,
        "mismatch_category": pl.String,
        "mismatch_rows": pl.Int64,
        "affected_files": pl.Int64,
    }


def optional_jsonl_handle(path: Path | None) -> Any:
    if path is None:
        return NullJsonlHandle()
    return path.open("a", encoding="utf-8")


class NullJsonlHandle:
    def __enter__(self) -> "NullJsonlHandle":
        return self

    def __exit__(self, *_args: Any) -> None:
        return None

    def write(self, _text: str) -> None:
        return None


def frame_jsonl_text(frame: Any) -> str:
    return frame.write_ndjson()


def write_optional_text(handle: Any, text: str | None) -> None:
    if text:
        handle.write(text)


def write_jsonl_rows(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, sort_keys=True, ensure_ascii=False, default=str))
            handle.write("\n")


def croma_failure_summary(item: dict[str, Any], relative_path: str) -> dict[str, Any]:
    return {
        "relative_path": relative_path,
        "path": item.get("path"),
        "returncode": item.get("returncode"),
        "panic": bool(item.get("panic")),
        "hard_error": bool(item.get("hard_error")),
        "timeout": bool(item.get("timeout")),
        "classification": item.get("classification"),
        "diagnostics": item.get("diagnostics", [])[:5],
    }


def optional_string(value: Any) -> str | None:
    if value is None:
        return None
    return str(value)


def stable_part_id(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    return value


def optional_int(value: Any) -> int | None:
    if value is None:
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def now_utc() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def emit_event(
    args: argparse.Namespace,
    stream: Any,
    event: str,
    **fields: Any,
) -> bool:
    """Emit one JSONL log event; returns False when text logging is active."""
    if args.log_format != "jsonl":
        return False
    print(
        json.dumps({"event": event, **fields}, sort_keys=True, ensure_ascii=False),
        file=stream,
        flush=True,
    )
    return True


def print_start(args: argparse.Namespace, files_selected: int, jobs: int, cache: Any) -> None:
    emit_event(
        args,
        sys.stderr,
        "start",
        tool="music21_polars_corpus_compare",
        files_selected=files_selected,
        jobs=jobs,
        cache_enabled=cache is not None,
        report=str(args.report),
    )


def print_progress(args: argparse.Namespace, completed: int, total: int) -> None:
    if args.progress_every and completed % args.progress_every == 0:
        if not emit_event(args, sys.stderr, "progress", completed=completed, total=total):
            print(f"processed {completed}/{total}", file=sys.stderr)


def strict_failures(report: dict[str, Any]) -> bool:
    return bool(
        report["croma_export_failures"]
        or report["croma_musicxml_import_failures"]
        or report["reference_musicxml_import_failures"]
        or report["worker_failures"]
        or report["comparison_harness_issues"]
    )


def print_summary(args: argparse.Namespace, report_path: Path, report: dict[str, Any]) -> None:
    summary_fields = {
        "report": str(report_path),
        "files_attempted": report["files_attempted"],
        "croma_export_successes": report["croma_export_successes"],
        "croma_export_failures": report["croma_export_failures"],
        "croma_musicxml_import_failures": report["croma_musicxml_import_failures"],
        "reference_musicxml_import_failures": report["reference_musicxml_import_failures"],
        "worker_failures": report["worker_failures"],
        "comparison_harness_issues": report["comparison_harness_issues"],
        "structural_matches": report["structural_matches"],
        "structural_mismatches": report["structural_mismatches"],
        "fact_rows": report["fact_rows"],
        "comparison_rows": report["comparison_rows"],
        "mismatch_rows": report["mismatch_rows"],
        "mismatch_category_counts": report["mismatch_category_counts"],
        "cache": report["cache"],
        "elapsed_seconds": report["elapsed_seconds"],
    }
    if emit_event(args, sys.stdout, "summary", **summary_fields):
        return
    print(f"report: {report_path}")
    print(f"files attempted: {report['files_attempted']}")
    print(f"croma export successes: {report['croma_export_successes']}")
    print(f"croma export failures: {report['croma_export_failures']}")
    print(f"croma musicxml import failures: {report['croma_musicxml_import_failures']}")
    print(f"reference musicxml import failures: {report['reference_musicxml_import_failures']}")
    print(f"structural matches: {report['structural_matches']}")
    print(f"structural mismatches: {report['structural_mismatches']}")
    print(f"fact rows: {report['fact_rows']}")
    print(f"comparison rows: {report['comparison_rows']}")
    print(f"mismatch rows: {report['mismatch_rows']}")
    print(f"elapsed seconds: {report['elapsed_seconds']}")


if __name__ == "__main__":
    raise SystemExit(main())
