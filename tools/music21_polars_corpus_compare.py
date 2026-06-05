#!/usr/bin/env python3
"""Build corpus-level Polars comparison tables from music21 facts."""

from __future__ import annotations

import argparse
import json
import sys
import time
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from music21_compare import (
    Music21ParseFailure,
    Music21Unavailable,
    decode_fact_value,
    extract_facts,
    import_polars,
    music21_fact_rows,
)


REPORT_SCHEMA = "croma-music21-polars-corpus-compare-v1"


def main() -> int:
    args = parse_args()
    pl = import_polars()

    facts_jsonl = args.facts_jsonl or sibling_jsonl(args.facts_parquet)
    comparison_jsonl = args.comparison_jsonl or sibling_jsonl(args.comparison_parquet)
    mismatches_jsonl = args.mismatches_jsonl or sibling_jsonl(args.mismatches_parquet)

    for path in [facts_jsonl, comparison_jsonl, mismatches_jsonl]:
        if path is not None:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("", encoding="utf-8")

    results = load_results(args.results_jsonl)
    if args.limit is not None:
        results = results[: args.limit]

    started_at = now_utc()
    started = time.monotonic()
    counters: Counter[str] = Counter()
    category_counts: Counter[str] = Counter()
    status_counts: Counter[str] = Counter()
    examples: dict[str, list[dict[str, Any]]] = defaultdict(list)
    import_failures: list[dict[str, Any]] = []
    croma_failures: list[dict[str, Any]] = []

    facts_rows_written = 0
    comparison_rows_written = 0
    mismatch_rows_written = 0

    with (
        optional_jsonl_handle(facts_jsonl) as facts_handle,
        optional_jsonl_handle(comparison_jsonl) as comparison_handle,
        optional_jsonl_handle(mismatches_jsonl) as mismatches_handle,
    ):
        for index, item in enumerate(results, start=1):
            counters["files_attempted"] += 1
            relative_path = relative_path_for(item)
            if relative_path is None:
                status_counts["missing_relative_path"] += 1
                continue

            if item.get("status") != "success":
                counters["croma_export_failures"] += 1
                croma_failures.append(croma_failure_summary(item, relative_path))
                continue

            counters["croma_export_successes"] += 1
            croma_xml = resolve_croma_xml(args.croma_xml_root, item, relative_path)
            reference_xml = resolve_reference_xml(args.reference_root, item, relative_path)

            croma_facts = None
            reference_facts = None
            if croma_xml is None:
                status_counts["croma_xml_missing"] += 1
            else:
                counters["croma_musicxml_import_attempts"] += 1
                croma_facts = parse_musicxml(croma_xml, "croma", counters, import_failures)

            if reference_xml is None:
                status_counts["reference_xml_missing"] += 1
            else:
                counters["reference_musicxml_import_attempts"] += 1
                reference_facts = parse_musicxml(reference_xml, "reference", counters, import_failures)

            if croma_facts is None or reference_facts is None:
                continue

            fact_rows = corpus_fact_rows(relative_path, croma_facts, reference_facts)
            facts_rows_written += len(fact_rows)
            write_jsonl_rows(facts_handle, fact_rows)

            comparison = comparison_frame(pl, fact_rows)
            mismatches = comparison.filter(~pl.col("matches"))
            comparison_rows_written += comparison.height
            mismatch_rows_written += mismatches.height
            write_frame_jsonl(comparison_handle, comparison)
            write_frame_jsonl(mismatches_handle, mismatches)

            if mismatches.height:
                counters["structural_mismatches"] += 1
                update_mismatch_summary(
                    mismatches,
                    category_counts,
                    examples,
                    args.max_examples_per_category,
                )
            else:
                counters["structural_matches"] += 1

            if args.progress_every and index % args.progress_every == 0:
                print(f"processed {index}/{len(results)}", file=sys.stderr)

    write_parquet(pl, facts_jsonl, args.facts_parquet, fact_schema(pl))
    write_parquet(pl, comparison_jsonl, args.comparison_parquet, comparison_schema(pl))
    write_parquet(pl, mismatches_jsonl, args.mismatches_parquet, comparison_schema(pl))

    elapsed = time.monotonic() - started
    report = {
        "schema": REPORT_SCHEMA,
        "started_at": started_at,
        "finished_at": now_utc(),
        "elapsed_seconds": round(elapsed, 3),
        "results_jsonl": str(args.results_jsonl),
        "croma_xml_root": str(args.croma_xml_root) if args.croma_xml_root else None,
        "reference_root": str(args.reference_root),
        "files_discovered": len(load_results(args.results_jsonl)),
        "files_selected": len(results),
        "files_attempted": counters["files_attempted"],
        "croma_export_successes": counters["croma_export_successes"],
        "croma_export_failures": counters["croma_export_failures"],
        "croma_musicxml_import_attempts": counters["croma_musicxml_import_attempts"],
        "croma_musicxml_import_successes": counters["croma_musicxml_import_successes"],
        "croma_musicxml_import_failures": counters["croma_musicxml_import_failures"],
        "reference_musicxml_import_attempts": counters["reference_musicxml_import_attempts"],
        "reference_musicxml_import_successes": counters["reference_musicxml_import_successes"],
        "reference_musicxml_import_failures": counters["reference_musicxml_import_failures"],
        "structural_matches": counters["structural_matches"],
        "structural_mismatches": counters["structural_mismatches"],
        "status_counts": dict(sorted(status_counts.items())),
        "mismatch_category_counts": dict(sorted(category_counts.items())),
        "fact_rows": facts_rows_written,
        "comparison_rows": comparison_rows_written,
        "mismatch_rows": mismatch_rows_written,
        "tables": {
            "facts_jsonl": str(facts_jsonl) if facts_jsonl else None,
            "facts_parquet": str(args.facts_parquet) if args.facts_parquet else None,
            "comparison_jsonl": str(comparison_jsonl) if comparison_jsonl else None,
            "comparison_parquet": str(args.comparison_parquet) if args.comparison_parquet else None,
            "mismatches_jsonl": str(mismatches_jsonl) if mismatches_jsonl else None,
            "mismatches_parquet": str(args.mismatches_parquet) if args.mismatches_parquet else None,
        },
        "table_keys": ["relative_path", "file_name", "category", "path"],
        "fact_table_columns": ["relative_path", "file_name", "side", "category", "path", "value"],
        "comparison_table_columns": [
            "relative_path",
            "file_name",
            "category",
            "path",
            "croma_present",
            "reference_present",
            "croma",
            "reference",
            "matches",
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
    print_summary(args.report, report)
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
    parser.add_argument("--limit", type=int)
    parser.add_argument("--progress-every", type=int, default=500)
    parser.add_argument("--max-examples-per-category", type=int, default=5)
    parser.add_argument("--max-failures", type=int, default=20)
    return parser.parse_args()


def sibling_jsonl(path: Path | None) -> Path | None:
    return path.with_suffix(".jsonl") if path is not None else None


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


def relative_path_for(item: dict[str, Any]) -> str | None:
    relative_path = item.get("relative_path")
    if isinstance(relative_path, str) and relative_path:
        return relative_path
    path = item.get("path")
    if isinstance(path, str) and path:
        return Path(path).name
    return None


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


def parse_musicxml(
    path: Path,
    label: str,
    counters: Counter[str],
    import_failures: list[dict[str, Any]],
) -> dict[str, Any] | None:
    try:
        facts = extract_facts(path, label)
    except Music21Unavailable as error:
        counters[f"{label}_musicxml_import_failures"] += 1
        import_failures.append({"label": label, "path": str(path), "error": str(error)})
        return None
    except Music21ParseFailure as error:
        counters[f"{label}_musicxml_import_failures"] += 1
        import_failures.append({"label": label, "path": str(path), "error": str(error)})
        return None
    except Exception as error:  # noqa: BLE001 - tool failures are report data here.
        counters[f"{label}_musicxml_import_failures"] += 1
        import_failures.append({"label": label, "path": str(path), "error": str(error)})
        return None

    counters[f"{label}_musicxml_import_successes"] += 1
    return facts


def corpus_fact_rows(
    relative_path: str,
    croma_facts: dict[str, Any],
    reference_facts: dict[str, Any],
) -> list[dict[str, str | None]]:
    file_name = Path(relative_path).name
    rows = []
    for side, facts in [("croma", croma_facts), ("reference", reference_facts)]:
        for row in music21_fact_rows(facts, side):
            rows.append(
                {
                    "relative_path": relative_path,
                    "file_name": file_name,
                    "side": row["side"],
                    "category": row["category"],
                    "path": row["path"],
                    "value": row["value"],
                }
            )
    return rows


def comparison_frame(pl: Any, fact_rows: list[dict[str, str | None]]) -> Any:
    keys = ["relative_path", "file_name", "category", "path"]
    facts = pl.DataFrame(fact_rows, schema=fact_schema(pl))
    croma = (
        facts.filter(pl.col("side") == "croma")
        .select(
            *keys,
            pl.col("value").alias("croma"),
            pl.lit(True).alias("croma_present"),
        )
    )
    reference = (
        facts.filter(pl.col("side") == "reference")
        .select(
            *keys,
            pl.col("value").alias("reference"),
            pl.lit(True).alias("reference_present"),
        )
    )
    return (
        croma.join(reference, on=keys, how="full", coalesce=True)
        .with_columns(
            pl.col("croma_present").fill_null(False),
            pl.col("reference_present").fill_null(False),
        )
        .with_columns(
            (
                pl.col("croma_present")
                & pl.col("reference_present")
                & (
                    (pl.col("croma") == pl.col("reference"))
                    | (pl.col("croma").is_null() & pl.col("reference").is_null())
                )
            )
            .fill_null(False)
            .alias("matches")
        )
        .select(*keys, "croma_present", "reference_present", "croma", "reference", "matches")
        .sort(keys)
    )


def update_mismatch_summary(
    mismatches: Any,
    category_counts: Counter[str],
    examples: dict[str, list[dict[str, Any]]],
    max_examples_per_category: int,
) -> None:
    for row in mismatches.iter_rows(named=True):
        category = str(row["category"])
        category_counts[category] += 1
        if len(examples[category]) >= max_examples_per_category:
            continue
        examples[category].append(
            {
                "relative_path": row["relative_path"],
                "category": category,
                "path": row["path"],
                "croma_present": row["croma_present"],
                "reference_present": row["reference_present"],
                "croma": decode_fact_value(row["croma"]),
                "reference": decode_fact_value(row["reference"]),
            }
        )


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


def write_jsonl_rows(handle: Any, rows: list[dict[str, Any]]) -> None:
    for row in rows:
        handle.write(json.dumps(row, sort_keys=True, ensure_ascii=False, default=str))
        handle.write("\n")


def write_frame_jsonl(handle: Any, frame: Any) -> None:
    for row in frame.iter_rows(named=True):
        handle.write(json.dumps(row, sort_keys=True, ensure_ascii=False, default=str))
        handle.write("\n")


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
        "file_name": pl.String,
        "side": pl.String,
        "category": pl.String,
        "path": pl.String,
        "value": pl.String,
    }


def comparison_schema(pl: Any) -> dict[str, Any]:
    return {
        "relative_path": pl.String,
        "file_name": pl.String,
        "category": pl.String,
        "path": pl.String,
        "croma_present": pl.Boolean,
        "reference_present": pl.Boolean,
        "croma": pl.String,
        "reference": pl.String,
        "matches": pl.Boolean,
    }


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


def now_utc() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def print_summary(report_path: Path, report: dict[str, Any]) -> None:
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
