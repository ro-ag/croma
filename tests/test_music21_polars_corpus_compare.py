from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT = REPO_ROOT / "tools" / "music21_polars_corpus_compare.py"


def test_failure_paths_and_missing_files_are_reported(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["bad_croma", "bad_reference", "missing_croma"])
    write_malformed(paths.croma_xml("bad_croma"))
    write_musicxml(paths.reference_xml("bad_croma"), [note()])
    write_musicxml(paths.croma_xml("bad_reference"), [note()])
    write_malformed(paths.reference_xml("bad_reference"))
    write_musicxml(paths.reference_xml("missing_croma"), [note()])

    report = run_compare(
        paths,
        jobs=2,
        extra=[
            "--facts-parquet",
            str(paths.output / "facts.parquet"),
            "--comparison-parquet",
            str(paths.output / "comparison.parquet"),
            "--mismatches-parquet",
            str(paths.output / "mismatches.parquet"),
            "--per-file-summary-parquet",
            str(paths.output / "per-file.parquet"),
            "--per-component-summary-parquet",
            str(paths.output / "per-component.parquet"),
        ],
    )
    mismatches = read_jsonl(paths.mismatches_jsonl)

    assert report["croma_musicxml_import_failures"] == 1
    assert report["reference_musicxml_import_failures"] == 1
    assert report["comparison_harness_issues"] == 1
    assert report["mismatch_category_counts"] == {
        "comparison_harness_issue": 1,
        "import_failure": 2,
    }
    assert {
        (row["filename"], row["source_side"], row["mismatch_category"])
        for row in mismatches
    } == {
        ("bad_croma.abc", "croma", "import_failure"),
        ("bad_reference.abc", "reference", "import_failure"),
        ("missing_croma.abc", "croma", "comparison_harness_issue"),
    }
    for artifact in [
        paths.output / "facts.parquet",
        paths.output / "comparison.parquet",
        paths.output / "mismatches.parquet",
        paths.output / "per-file.parquet",
        paths.output / "per-component.parquet",
    ]:
        assert artifact.exists()


def test_output_is_deterministic_for_serial_and_parallel_jobs(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["pitch_mismatch", "match"])
    write_musicxml(paths.croma_xml("pitch_mismatch"), [note(step="C")])
    write_musicxml(paths.reference_xml("pitch_mismatch"), [note(step="D")])
    write_musicxml(paths.croma_xml("match"), [note(step="E")])
    write_musicxml(paths.reference_xml("match"), [note(step="E")])

    serial = run_compare(paths, jobs=1, output_name="serial")
    parallel = run_compare(paths, jobs=2, output_name="parallel")

    assert serial["mismatch_category_counts"] == parallel["mismatch_category_counts"]
    assert serial["fact_rows"] == parallel["fact_rows"]
    assert serial["comparison_rows"] == parallel["comparison_rows"]
    assert serial["mismatch_rows"] == parallel["mismatch_rows"]
    assert (paths.output / "serial-mismatches.jsonl").read_text() == (
        paths.output / "parallel-mismatches.jsonl"
    ).read_text()


def test_component_filtering_and_precise_mismatch_categories(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["mixed"])
    write_musicxml(paths.croma_xml("mixed"), [note(step="C", duration=4, type_="quarter", lyric="la")])
    write_musicxml(paths.reference_xml("mixed"), [note(step="D", duration=8, type_="half", lyric="do")])

    full_report = run_compare(paths, jobs=1, output_name="full")
    assert {"duration", "lyric", "pitch"} <= set(full_report["mismatch_category_counts"])

    lyric_report = run_compare(
        paths,
        jobs=1,
        output_name="lyric",
        extra=["--component", "lyric"],
    )
    assert lyric_report["component_filter"] == ["lyric"]
    assert lyric_report["mismatch_category_counts"] == {"lyric": 1}
    assert {row["component"] for row in read_jsonl(paths.output / "lyric-facts.jsonl")} == {
        "lyric"
    }


def test_baseline_delta_classification(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["delta", "new"])
    write_musicxml(paths.croma_xml("delta"), [note(lyric="la")])
    write_musicxml(paths.reference_xml("delta"), [note(lyric="do")])
    write_musicxml(paths.croma_xml("new"), [note(step="C")])
    write_musicxml(paths.reference_xml("new"), [note(step="D")])

    baseline = paths.output / "baseline-mismatches.jsonl"
    write_jsonl(
        baseline,
        [
            {"filename": "delta.abc", "mismatch_category": "lyric"},
            {"filename": "delta.abc", "mismatch_category": "pitch"},
            {"filename": "resolved.abc", "mismatch_category": "duration"},
        ],
    )

    report = run_compare(
        paths,
        jobs=1,
        extra=["--baseline-mismatches", str(baseline)],
    )

    improved = {row["filename"]: row["classification"] for row in report["baseline"]["improved"]}
    regressed = {row["filename"]: row["classification"] for row in report["baseline"]["regressed"]}
    assert improved["delta.abc"] == "improved"
    assert improved["resolved.abc"] == "resolved"
    assert regressed["new.abc"] == "new_regression"


def test_only_files_and_candidate_file_list(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["keep", "skip"])
    write_musicxml(paths.croma_xml("keep"), [note(step="C")])
    write_musicxml(paths.reference_xml("keep"), [note(step="D")])
    write_musicxml(paths.croma_xml("skip"), [note(step="E")])
    write_musicxml(paths.reference_xml("skip"), [note(step="F")])
    only_files = paths.output / "only-files.txt"
    only_files.write_text("keep.abc\n", encoding="utf-8")
    file_list = paths.output / "candidates.txt"

    report = run_compare(
        paths,
        jobs=1,
        extra=[
            "--only-files",
            str(only_files),
            "--write-file-list",
            str(file_list),
        ],
    )

    assert report["files_selected"] == 1
    assert [row["filename"] for row in report["candidate_files"]] == ["keep.abc"]
    assert file_list.read_text(encoding="utf-8") == "keep.abc\n"


class FixturePaths:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.sources = root / "abc"
        self.croma_root = root / "croma-xml"
        self.reference_root = root / "reference-xml"
        self.output = root / "out"
        self.results_jsonl = root / "results.jsonl"
        self.report = self.output / "report.json"
        self.facts_jsonl = self.output / "facts.jsonl"
        self.comparison_jsonl = self.output / "comparison.jsonl"
        self.mismatches_jsonl = self.output / "mismatches.jsonl"
        self.per_file_jsonl = self.output / "per-file.jsonl"
        self.per_component_jsonl = self.output / "per-component.jsonl"

    @classmethod
    def create(cls, root: Path) -> "FixturePaths":
        paths = cls(root)
        for directory in [paths.sources, paths.croma_root, paths.reference_root, paths.output]:
            directory.mkdir(parents=True, exist_ok=True)
        return paths

    def croma_xml(self, stem: str) -> Path:
        return self.croma_root / f"{stem}.croma.musicxml"

    def reference_xml(self, stem: str) -> Path:
        return self.reference_root / f"{stem}.musicxml"


def run_compare(
    paths: FixturePaths,
    *,
    jobs: int,
    output_name: str | None = None,
    extra: list[str] | None = None,
) -> dict[str, Any]:
    prefix = f"{output_name}-" if output_name else ""
    report = paths.output / f"{prefix}report.json"
    command = [
        sys.executable,
        str(SCRIPT),
        "--results-jsonl",
        str(paths.results_jsonl),
        "--croma-xml-root",
        str(paths.croma_root),
        "--reference-root",
        str(paths.reference_root),
        "--report",
        str(report),
        "--facts-jsonl",
        str(paths.output / f"{prefix}facts.jsonl"),
        "--comparison-jsonl",
        str(paths.output / f"{prefix}comparison.jsonl"),
        "--mismatches-jsonl",
        str(paths.output / f"{prefix}mismatches.jsonl"),
        "--per-file-summary-jsonl",
        str(paths.output / f"{prefix}per-file.jsonl"),
        "--per-component-summary-jsonl",
        str(paths.output / f"{prefix}per-component.jsonl"),
        "--jobs",
        str(jobs),
        "--worker-chunk-size",
        "1",
        "--progress-every",
        "0",
    ]
    if extra:
        command.extend(extra)
    completed = subprocess.run(
        command,
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    assert completed.returncode == 0, completed.stderr + completed.stdout
    return json.loads(report.read_text(encoding="utf-8"))


def write_result_set(paths: FixturePaths, stems: list[str]) -> None:
    rows = []
    for stem in stems:
        source = paths.sources / f"{stem}.abc"
        source.write_text("X:1\nT:fixture\nM:4/4\nK:C\nC\n", encoding="utf-8")
        rows.append(
            {
                "schema": "croma-corpus-result-v1",
                "status": "success",
                "mode": "xml",
                "relative_path": f"{stem}.abc",
                "path": str(source),
                "returncode": 0,
                "panic": False,
                "hard_error": False,
                "timeout": False,
                "diagnostics": [],
            }
        )
    write_jsonl(paths.results_jsonl, rows)


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, sort_keys=True))
            handle.write("\n")


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    return [
        json.loads(line)
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]


def write_malformed(path: Path) -> None:
    path.write_text("<score-partwise><part>", encoding="utf-8")


def write_musicxml(path: Path, notes: list[str]) -> None:
    path.write_text(
        f"""<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <part-list>
    <score-part id="P1">
      <part-name>Fixture</part-name>
    </score-part>
  </part-list>
  <part id="P1">
    <measure number="1">
      <attributes>
        <divisions>4</divisions>
        <key><fifths>0</fifths></key>
        <time><beats>4</beats><beat-type>4</beat-type></time>
        <clef><sign>G</sign><line>2</line></clef>
      </attributes>
      {''.join(notes)}
    </measure>
  </part>
</score-partwise>
""",
        encoding="utf-8",
    )


def note(
    *,
    step: str = "C",
    alter: int | None = None,
    octave: int = 4,
    duration: int = 4,
    type_: str = "quarter",
    lyric: str | None = None,
) -> str:
    alter_xml = f"<alter>{alter}</alter>" if alter is not None else ""
    lyric_xml = (
        f"<lyric><syllabic>single</syllabic><text>{lyric}</text></lyric>"
        if lyric is not None
        else ""
    )
    return f"""
      <note>
        <pitch><step>{step}</step>{alter_xml}<octave>{octave}</octave></pitch>
        <duration>{duration}</duration>
        <type>{type_}</type>
        {lyric_xml}
      </note>
"""
