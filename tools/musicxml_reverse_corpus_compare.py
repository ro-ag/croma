#!/usr/bin/env python3
"""Measure croma's gated MusicXML *reader* against FOREIGN (abc2xml) MusicXML.

This is the REVERSE direction of `tools/prove_abc_roundtrip.py` / the forward
`tools/music21_polars_corpus_compare.py` flow. Where the forward flow proves
`croma -> MusicXML` matches abc2xml, this flow proves how much of the abc2xml /
music21 *dialect* croma's reader can round-trip back out. The pipeline per file:

  abc2xml reference XML
    -> `croma read <ref> --format xml`        (the PURE inverse
                                               `write_musicxml(read_musicxml(xml))`;
                                               NO ABC completion, NO ABC pass)
    -> croma re-export MusicXML
    -> music21 SEMANTIC compare vs the ORIGINAL abc2xml XML.

This is cross-writer SEMANTIC parity, not byte idempotence: croma's reader was
built to invert croma's OWN simple ABC-oriented writer, so a LOW match rate
against the richer abc2xml dialect is expected and is itself the finding.

The music21 semantic diff is NOT reimplemented here: this driver re-exports each
file with the built `croma` binary, writes a results manifest, and hands it to
the existing comparator `tools/music21_polars_corpus_compare.py`, which owns the
music21 fact extraction and the mismatch categorisation.

LOCAL ONLY — never wire this into CI. The corpus is external; provision it per
AGENTS.md. All generated re-exports / manifests / reports live under
docs/untracked/ (git-ignored). Run via `uv run` so music21 / polars / orjson
are on the path.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parent.parent
CROMA = REPO_ROOT / "target" / "debug" / "croma"
COMPARATOR = Path(__file__).resolve().parent / "music21_polars_corpus_compare.py"

DEFAULT_REF_ROOT = (
    REPO_ROOT / "docs" / "untracked" / "corpus" / "zenodo-10k" / "musicxml"
)
DEFAULT_OUT_ROOT = REPO_ROOT / "docs" / "untracked" / "phase-60-r2-reverse"

# A re-export at or below this byte size is the reader's empty/error stub
# (`<score-partwise>` with an empty `<part-list>` and no `<part>`); used only as
# a fast pre-filter — the authoritative empty/trivial check inspects the XML for
# a `<part>` element. abc2xml's own files are always far larger.
_TRIVIAL_BYTES = 256


def _init_worker(croma: str) -> None:
    global CROMA
    CROMA = croma


def reexport_one(job: tuple[str, str]) -> dict[str, Any]:
    """Run `croma read <ref> --format xml -o <out>` for a single file.

    Returns a record carrying the manifest fields plus the reader diagnostics
    captured on stderr and whether the re-export is empty/trivial (the reader
    produced a near-empty Score).
    """
    ref_path_str, out_path_str = job
    ref_path = Path(ref_path_str)
    out_path = Path(out_path_str)
    name = ref_path.name

    proc = subprocess.run(
        [CROMA, "read", ref_path_str, "--format", "xml", "-o", out_path_str],
        capture_output=True,
        text=True,
    )

    stderr = proc.stderr or ""
    # Reader diagnostics are one stderr line each, rendered as
    # `<path>: <severity>[<code>]: <message>`. Count lines that name a
    # `[musicxml.read.*]` code; bucket the first code for the report.
    diag_lines = [ln for ln in stderr.splitlines() if "[musicxml.read." in ln]
    first_code = _first_diag_code(diag_lines)

    trivial = True
    out_bytes = 0
    if proc.returncode == 0 and out_path.exists():
        out_bytes = out_path.stat().st_size
        trivial = _is_trivial_reexport(out_path, out_bytes)

    return {
        "file": name,
        "ref": ref_path_str,
        "out": out_path_str,
        "returncode": proc.returncode,
        "out_bytes": out_bytes,
        "trivial": trivial,
        "has_diagnostic": bool(diag_lines),
        "diag_count": len(diag_lines),
        "first_diag_code": first_code,
        "stderr_head": stderr[:400] if proc.returncode != 0 else "",
    }


def _first_diag_code(diag_lines: list[str]) -> str | None:
    for line in diag_lines:
        start = line.find("[musicxml.read.")
        if start == -1:
            continue
        end = line.find("]", start)
        if end == -1:
            continue
        return line[start + 1 : end]
    return None


def _is_trivial_reexport(out_path: Path, out_bytes: int) -> bool:
    """True when the re-export carries no `<part>` (reader produced ~nothing)."""
    if out_bytes <= _TRIVIAL_BYTES:
        return True
    try:
        text = out_path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return True
    return "<part " not in text and "<part>" not in text


def build_manifest(records: list[dict[str, Any]], manifest_path: Path) -> None:
    """Write one JSON object per line for the comparator.

    The comparator keys success on `status == "success"`; resolves the croma
    re-export via `music21.croma_xml` (checked first) and the reference via
    `music21.reference_xml`.
    """
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    with manifest_path.open("w", encoding="utf-8") as handle:
        for record in records:
            item = {
                "path": record["file"],
                "relative_path": record["file"],
                "status": "success" if record["returncode"] == 0 else "error",
                "music21": {
                    "croma_xml": record["out"],
                    "reference_xml": record["ref"],
                },
            }
            handle.write(json.dumps(item, ensure_ascii=False) + "\n")


def run_comparator(
    manifest_path: Path,
    reference_root: Path,
    report_path: Path,
    whitelist_csv: Path,
    dropped_csv: Path,
    jobs: int,
) -> dict[str, Any]:
    """Invoke the existing music21 comparator and parse its summary event."""
    cmd = [
        sys.executable,
        str(COMPARATOR),
        "--results-jsonl",
        str(manifest_path),
        "--reference-root",
        str(reference_root),
        "--report",
        str(report_path),
        "--whitelist-csv",
        str(whitelist_csv),
        "--dropped-csv",
        str(dropped_csv),
        "--no-cache",
        "--sample-per-category",
        "8",
    ]
    if jobs:
        cmd += ["--jobs", str(jobs)]
    print(f"[reverse] invoking comparator: {' '.join(cmd)}", file=sys.stderr)
    proc = subprocess.run(cmd, capture_output=True, text=True)
    sys.stderr.write(proc.stderr)
    summary: dict[str, Any] = {}
    for line in proc.stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(event, dict) and event.get("event") == "summary":
            summary = event
    if not summary:
        raise SystemExit(
            f"comparator emitted no summary event (rc={proc.returncode}); "
            f"stdout tail:\n{proc.stdout[-2000:]}"
        )
    return summary


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--reference-root", type=Path, default=DEFAULT_REF_ROOT)
    ap.add_argument("--out-root", type=Path, default=DEFAULT_OUT_ROOT)
    ap.add_argument("--croma", default=str(CROMA), help="path to the croma binary")
    ap.add_argument("--jobs", type=int, default=0, help="workers (0 = cpu count)")
    ap.add_argument(
        "--limit", type=int, default=0, help="process only the first N files (0 = all)"
    )
    args = ap.parse_args()

    if not Path(args.croma).exists():
        raise SystemExit(
            f"croma binary not found at {args.croma}; build with "
            "`cargo build -p croma-cli --features musicxml-reader`"
        )

    ref_files = sorted(args.reference_root.glob("tune_*.xml"))
    if args.limit:
        ref_files = ref_files[: args.limit]
    if not ref_files:
        raise SystemExit(f"no tune_*.xml under {args.reference_root}")

    reexport_dir = args.out_root / "reexport"
    reexport_dir.mkdir(parents=True, exist_ok=True)

    jobs = args.jobs or (os.cpu_count() or 1)
    jobs_for_reexport = max(1, jobs)

    print(
        f"[reverse] re-exporting {len(ref_files)} files via `croma read` "
        f"(jobs={jobs_for_reexport})",
        file=sys.stderr,
    )
    jobs_list = [
        (str(ref), str(reexport_dir / f"{ref.stem}.croma.musicxml"))
        for ref in ref_files
    ]

    records: list[dict[str, Any]] = []
    with ProcessPoolExecutor(
        max_workers=jobs_for_reexport,
        initializer=_init_worker,
        initargs=(args.croma,),
    ) as pool:
        for index, record in enumerate(
            pool.map(reexport_one, jobs_list, chunksize=32), 1
        ):
            records.append(record)
            if index % 1000 == 0:
                print(f"  re-export {index}/{len(jobs_list)}", file=sys.stderr)

    # Reader-side stats (the comparator never sees these).
    read_failures = sum(1 for r in records if r["returncode"] != 0)
    trivial = sum(1 for r in records if r["trivial"])
    with_diag = sum(1 for r in records if r["has_diagnostic"])
    diag_code_counts = Counter(
        r["first_diag_code"] for r in records if r["first_diag_code"]
    )
    print(
        f"[reverse] re-export done: {len(records)} files | "
        f"read-cmd failures={read_failures} | empty/trivial={trivial} | "
        f"with-diagnostic={with_diag}",
        file=sys.stderr,
    )
    print(
        f"[reverse] reader diagnostic codes: {dict(diag_code_counts.most_common())}",
        file=sys.stderr,
    )

    manifest_path = args.out_root / "manifest.jsonl"
    build_manifest(records, manifest_path)

    report_path = args.out_root / "report.json"
    whitelist_csv = args.out_root / "whitelist.csv"
    dropped_csv = args.out_root / "dropped.csv"
    if not dropped_csv.exists():
        dropped_csv.write_text("filename\n", encoding="utf-8")

    summary = run_comparator(
        manifest_path=manifest_path,
        reference_root=args.reference_root,
        report_path=report_path,
        whitelist_csv=whitelist_csv,
        dropped_csv=dropped_csv,
        jobs=jobs,
    )

    # Persist the reader-side stats alongside the comparator report so the run
    # is fully reconstructable from docs/untracked.
    reader_stats = {
        "files": len(records),
        "read_cmd_failures": read_failures,
        "empty_or_trivial": trivial,
        "with_diagnostic": with_diag,
        "first_diag_code_counts": dict(diag_code_counts.most_common()),
    }
    (args.out_root / "reader_stats.json").write_text(
        json.dumps(reader_stats, indent=2), encoding="utf-8"
    )

    matches = summary.get("structural_matches", 0)
    mismatches = summary.get("structural_mismatches", 0)
    compared = matches + mismatches
    pct = (matches / compared * 100.0) if compared else 0.0
    category_counts = summary.get("mismatch_category_counts", {})
    top = sorted(category_counts.items(), key=lambda kv: kv[1], reverse=True)[:12]

    print("\n==== REVERSE BASELINE SUMMARY ====")
    print(f"files re-exported: {len(records)}")
    print(f"compared (matches+mismatches): {compared}")
    print(f"matches: {matches} ({pct:.2f}%)")
    print(f"mismatches: {mismatches}")
    print(f"empty/trivial re-exports: {trivial}")
    print(f"reader-diagnostic files: {with_diag}")
    print(f"reader first-diagnostic codes: {dict(diag_code_counts.most_common())}")
    print(f"report: {report_path}")
    print("top mismatch categories:")
    for category, count in top:
        print(f"  {category}: {count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
