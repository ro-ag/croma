from __future__ import annotations

import json
import subprocess
import sys
from collections import Counter
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT = REPO_ROOT / "tools" / "music21_polars_corpus_compare.py"
sys.path.insert(0, str(REPO_ROOT / "tools"))


def test_fast_encode_matches_reference_encoder() -> None:
    from music21_compare import encode_fact_value
    from music21_polars_corpus_compare import fast_encode_fact_value

    # Scalars (and >64-bit ints, which fall back to the stdlib encoder)
    # must stay byte-identical to the reference encoder.
    exact_values = [
        None,
        "",
        "plain",
        'quote "inside"',
        "back\\slash",
        "tab\tand\nnewline",
        "control \x01 char",
        "unicode é 日本 ♭",
        0,
        -17,
        10**20,
        True,
        False,
        2.5,
    ]
    for value in exact_values:
        assert fast_encode_fact_value(value) == encode_fact_value(value), repr(value)

    # Containers are compact under orjson; they must stay semantically equal
    # to the reference encoding and keep sorted keys.
    container_values = [
        ["list", 1, None],
        {"nested": {"b": 1, "a": [2, "x"]}},
        [],
        {},
    ]
    for value in container_values:
        encoded = fast_encode_fact_value(value)
        assert json.loads(encoded) == json.loads(encode_fact_value(value)), repr(value)
        assert ", " not in encoded and ": " not in encoded, repr(value)
    assert fast_encode_fact_value({"b": 1, "a": 2}) == '{"a":2,"b":1}'


def test_columnar_comparison_key_matches_python_json() -> None:
    from music21_compare import import_polars
    from music21_polars_corpus_compare import (
        COMPARISON_KEY_FIELDS,
        FACT_COLUMNS,
        facts_frame,
    )

    pl = import_polars()
    key_values_per_row = [
        {
            "component": "pitch",
            "field_name": "step",
            "part_index": 0,
            "measure_index": 12,
            "voice": '1"quoted\\v',
            "staff": "st\taff",
            "event_index": 3,
            "alignment_index": 1,
        },
        {
            "component": "lyric",
            "field_name": "text é日本",
            "part_index": None,
            "measure_index": None,
            "voice": None,
            "staff": None,
            "event_index": None,
            "alignment_index": None,
        },
    ]
    rows = []
    for key_values in key_values_per_row:
        row = {column: None for column in FACT_COLUMNS}
        row.update(
            {
                "relative_path": "tune.abc",
                "filename": "tune.abc",
                "source_side": "croma",
                "value_kind": "str",
                "value_str": "x",
            }
        )
        row.update(key_values)
        rows.append(tuple(row[column] for column in FACT_COLUMNS))

    frame = facts_frame(pl, rows)
    for computed, key_values in zip(frame["comparison_key"].to_list(), key_values_per_row):
        expected = json.dumps(
            {field: key_values[field] for field in COMPARISON_KEY_FIELDS},
            sort_keys=True,
            separators=(",", ":"),
            ensure_ascii=False,
        )
        assert computed == expected


def test_v3_typed_value_columns_and_explicit_raw_values(tmp_path: Path) -> None:
    from music21_polars_corpus_compare import typed_value

    assert typed_value(None) == (None, None, None, None, None)
    assert typed_value(True) == ("bool", None, 1, None, None)
    assert typed_value(3) == ("int", None, 3, None, None)
    assert typed_value(2.5) == ("float", None, None, 2.5, None)
    assert typed_value("C") == ("str", "C", None, None, None)
    assert typed_value([]) == ("json", None, None, None, "[]")

    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["tune"])
    write_musicxml(paths.croma_xml("tune"), [note(step="C", lyric="la")])
    write_musicxml(paths.reference_xml("tune"), [note(step="C", lyric="la")])

    report = run_compare(paths, jobs=1, output_name="typed")

    assert report["schema"] == "croma-music21-polars-corpus-compare-v3"
    assert report["mismatch_category_counts"] == {}
    facts = {
        (row["component"], row["field_name"]): row
        for row in read_jsonl(paths.output / "typed-facts.jsonl")
        if row["source_side"] == "croma"
    }
    dots = facts[("duration", "dots")]
    assert dots["value_kind"] == "int"
    assert dots["value_int"] == 0
    assert dots["value_str"] is None and dots["value_json"] is None
    step = facts[("pitch", "step")]
    assert step["value_kind"] == "str"
    assert step["value_str"] == "C"
    tuplets = facts[("tuplet", "tuplets")]
    assert tuplets["value_kind"] == "json"
    assert tuplets["value_json"] == "[]"
    alter = facts[("pitch", "alter")]
    # The alter fact is the sounding alteration; an unaltered note is 0.0
    # (MusicXML defaults an absent <alter> to 0), not a null fact.
    assert alter["value_kind"] == "float"
    assert alter["value_float"] == 0.0
    # raw_value is populated only when a distinct raw value was captured.
    assert dots["raw_value"] is None
    assert facts[("note", "kind")]["raw_value"] is not None


def test_redundant_alter_zero_is_not_a_mismatch(tmp_path: Path) -> None:
    # MusicXML defines an absent <alter> as 0; a redundant <alter>0</alter>
    # (abc2xml's serialization for carried naturals) is the same sounding
    # pitch. The alter fact compares the sounding alteration, so this pair
    # must be a structural match, not an "accidental" row.
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["tune"])
    write_musicxml(paths.croma_xml("tune"), [note(step="D", octave=5)])
    write_musicxml(paths.reference_xml("tune"), [note(step="D", octave=5, alter=0)])

    report = run_compare(paths, jobs=1, output_name="alter-eq")

    assert report["mismatch_category_counts"] == {}
    assert report["structural_matches"] == 1


def test_sounding_alteration_difference_is_still_flagged(tmp_path: Path) -> None:
    # D vs D-sharp is a real chromatic difference; normalizing redundant
    # alter-0 serialization must not swallow genuine alteration mismatches.
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["tune"])
    write_musicxml(paths.croma_xml("tune"), [note(step="D", octave=5)])
    write_musicxml(paths.reference_xml("tune"), [note(step="D", octave=5, alter=1)])

    report = run_compare(paths, jobs=1, output_name="alter-diff")

    assert report["mismatch_category_counts"].get("accidental") == 1
    mismatches = read_jsonl(paths.output / "alter-diff-mismatches.jsonl")
    alter_rows = [
        row
        for row in mismatches
        if row["component"] == "pitch" and row["field_name"] == "alter"
    ]
    assert len(alter_rows) == 1
    assert alter_rows[0]["croma_value_float"] == 0.0
    assert alter_rows[0]["reference_value_float"] == 1.0


def test_text_only_tempo_playback_bpm_difference_is_equivalent(tmp_path: Path) -> None:
    # ABC string-only Q: fields have no mandated playback BPM. Croma and
    # abc2xml can choose different <sound tempo> defaults while preserving the
    # same visible words, so the comparator should not count the playback-only
    # MetronomeMark value as a structural mismatch.
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["tempo_text"])
    write_musicxml(
        paths.croma_xml("tempo_text"),
        [tempo_words_with_sound("Allegretto", "120.00"), note()],
    )
    write_musicxml(
        paths.reference_xml("tempo_text"),
        [tempo_words_with_sound("Allegretto", "112.00"), note()],
    )

    report = run_compare(paths, jobs=1, output_name="tempo-text")

    assert report["mismatch_category_counts"] == {}
    assert report["structural_matches"] == 1


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


def test_empty_lyric_extenders_preserve_alignment_slots(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["melisma"])
    write_musicxml(
        paths.croma_xml("melisma"),
        [
            note(lyric="time"),
            note(lyric_xml="<lyric number=\"1\"><extend/></lyric>"),
            note(lyric="day"),
        ],
    )
    write_musicxml(
        paths.reference_xml("melisma"),
        [
            note(lyric="time"),
            note(lyric_xml="<lyric number=\"1\"><extend type=\"stop\"/></lyric>"),
            note(lyric="day"),
        ],
    )

    report = run_compare(paths, jobs=1, output_name="melisma")

    assert report["mismatch_category_counts"] == {}
    lyric_rows = [
        row
        for row in read_jsonl(paths.output / "melisma-facts.jsonl")
        if row["component"] == "lyric"
    ]
    croma_lyrics = [
        row
        for row in lyric_rows
        if row["source_side"] == "croma" and row["field_name"] == "text"
    ]
    assert [row["value_str"] for row in croma_lyrics] == ["time", "", "day"]
    assert {row["value_kind"] for row in croma_lyrics} == {"str"}


def test_lyric_syllabic_participates_in_comparison(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["syllabic"])
    write_musicxml(
        paths.croma_xml("syllabic"),
        [note(lyric_xml="<lyric number=\"1\"><syllabic>begin</syllabic><text>A</text></lyric>")],
    )
    write_musicxml(
        paths.reference_xml("syllabic"),
        [note(lyric_xml="<lyric number=\"1\"><syllabic>single</syllabic><text>A</text></lyric>")],
    )

    report = run_compare(paths, jobs=1, output_name="syllabic")

    assert report["mismatch_category_counts"] == {"lyric": 1}
    mismatches = read_jsonl(paths.output / "syllabic-mismatches.jsonl")
    assert mismatches[0]["field_name"] == "syllabic"


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


def test_baseline_mismatch_loader_handles_sparse_jsonl_columns(tmp_path: Path) -> None:
    from music21_polars_corpus_compare import load_mismatch_file_counts

    baseline = tmp_path / "baseline-mismatches.jsonl"
    rows = [{"filename": "early.abc", "late_value": None} for _ in range(100)]
    rows.append({"filename": "late.abc", "late_value": 1})
    write_jsonl(baseline, rows)

    assert load_mismatch_file_counts(baseline) == Counter(
        {"early.abc": 100, "late.abc": 1}
    )


def test_cache_warm_report_only_run_replays_pair_results(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["pitch_mismatch", "match"])
    write_musicxml(paths.croma_xml("pitch_mismatch"), [note(step="C")])
    write_musicxml(paths.reference_xml("pitch_mismatch"), [note(step="D")])
    write_musicxml(paths.croma_xml("match"), [note(step="E")])
    write_musicxml(paths.reference_xml("match"), [note(step="E")])

    cold = run_compare(paths, jobs=2, output_name="cold", tables=False)
    warm = run_compare(paths, jobs=2, output_name="warm", tables=False)
    uncached = run_compare(paths, jobs=2, output_name="uncached", tables=False, no_cache=True)

    assert cold["cache"]["enabled"] is True
    assert cold["cache"]["result_hits"] == 0
    assert cold["cache"]["result_misses"] == 2
    assert warm["cache"]["result_hits"] == 2
    assert warm["cache"]["result_misses"] == 0
    assert uncached["cache"]["enabled"] is False

    for left, right in [(cold, warm), (cold, uncached)]:
        assert comparable_report(left) == comparable_report(right)
    assert (paths.output / "cold-per-file.jsonl").read_text() == (
        paths.output / "warm-per-file.jsonl"
    ).read_text()


def test_cache_facts_layer_serves_table_writing_runs(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["pitch_mismatch"])
    write_musicxml(paths.croma_xml("pitch_mismatch"), [note(step="C")])
    write_musicxml(paths.reference_xml("pitch_mismatch"), [note(step="D")])

    cold = run_compare(paths, jobs=1, output_name="cold")
    warm = run_compare(paths, jobs=1, output_name="warm")

    assert cold["cache"]["facts_hits"] == 0
    assert cold["cache"]["facts_misses"] == 2
    assert warm["cache"]["facts_hits"] == 2
    assert warm["cache"]["facts_misses"] == 0
    # Table-writing runs bypass the pair-result layer but reuse cached facts.
    assert warm["cache"]["result_hits"] == 0
    assert comparable_report(cold) == comparable_report(warm)
    for name in ["facts.jsonl", "comparison.jsonl", "mismatches.jsonl"]:
        assert (paths.output / f"cold-{name}").read_text() == (
            paths.output / f"warm-{name}"
        ).read_text()


def test_cache_invalidates_when_xml_content_changes(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["tune"])
    write_musicxml(paths.croma_xml("tune"), [note(step="D")])
    write_musicxml(paths.reference_xml("tune"), [note(step="D")])

    before = run_compare(paths, jobs=1, output_name="before", tables=False)
    assert before["mismatch_category_counts"] == {}

    write_musicxml(paths.croma_xml("tune"), [note(step="C")])
    after = run_compare(paths, jobs=1, output_name="after", tables=False)

    assert after["mismatch_category_counts"] == {"pitch": 1}
    assert after["cache"]["result_hits"] == 0
    # The unchanged reference side still hits the facts layer.
    assert after["cache"]["facts_hits"] == 1
    assert after["cache"]["facts_misses"] == 1


def test_cache_replay_patches_run_specific_paths(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["tune"])
    write_musicxml(paths.croma_xml("tune"), [note(step="C")])
    write_musicxml(paths.reference_xml("tune"), [note(step="D")])

    run_compare(paths, jobs=1, output_name="cold", tables=False)

    moved_root = paths.root / "croma-xml-moved"
    moved_root.mkdir()
    moved = moved_root / "tune.croma.musicxml"
    moved.write_bytes(paths.croma_xml("tune").read_bytes())
    warm = run_compare(
        paths,
        jobs=1,
        output_name="warm",
        tables=False,
        croma_xml_root=moved_root,
    )

    assert warm["cache"]["result_hits"] == 1
    [per_file_row] = read_jsonl(paths.output / "warm-per-file.jsonl")
    assert per_file_row["croma_xml_path"] == str(moved)


def test_corrupt_cache_db_is_recreated(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["tune"])
    write_musicxml(paths.croma_xml("tune"), [note(step="C")])
    write_musicxml(paths.reference_xml("tune"), [note(step="D")])
    cache_db = paths.output / "cache.sqlite"
    cache_db.parent.mkdir(parents=True, exist_ok=True)
    cache_db.write_bytes(b"this is not a sqlite database")

    report = run_compare(paths, jobs=1, tables=False)

    assert report["mismatch_category_counts"] == {"pitch": 1}
    assert report["cache"]["enabled"] is True
    assert report["cache"]["result_misses"] == 1


def comparable_report(report: dict[str, Any]) -> dict[str, Any]:
    volatile = {"started_at", "finished_at", "elapsed_seconds", "cache", "tables", "jobs"}
    return {key: value for key, value in report.items() if key not in volatile}


def test_jsonl_logging_is_default_with_start_progress_and_summary(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["pitch_mismatch"])
    write_musicxml(paths.croma_xml("pitch_mismatch"), [note(step="C")])
    write_musicxml(paths.reference_xml("pitch_mismatch"), [note(step="D")])

    report, stdout, stderr = run_compare_capture(
        paths,
        jobs=1,
        tables=False,
        extra=["--progress-every", "1"],
    )

    events = [
        json.loads(line)
        for line in stderr.splitlines()
        if line.startswith("{")
    ]
    starts = [event for event in events if event["event"] == "start"]
    progress = [event for event in events if event["event"] == "progress"]
    assert starts and starts[0]["files_selected"] == 1
    assert starts[0]["cache_enabled"] is True
    assert progress and progress[-1] == {"completed": 1, "event": "progress", "total": 1}

    summary = json.loads(stdout.splitlines()[-1])
    assert summary["event"] == "summary"
    assert summary["report"] == str(paths.output / "report.json")
    assert summary["mismatch_category_counts"] == {"pitch": 1}
    assert summary["structural_mismatches"] == 1
    assert summary["cache"]["enabled"] is True
    assert summary["elapsed_seconds"] == report["elapsed_seconds"]


def test_text_log_format_keeps_legacy_lines(tmp_path: Path) -> None:
    paths = FixturePaths.create(tmp_path)
    write_result_set(paths, ["match"])
    write_musicxml(paths.croma_xml("match"), [note(step="E")])
    write_musicxml(paths.reference_xml("match"), [note(step="E")])

    _, stdout, stderr = run_compare_capture(
        paths,
        jobs=1,
        tables=False,
        extra=["--log-format", "text", "--progress-every", "1"],
    )

    assert "processed 1/1" in stderr
    assert stdout.splitlines()[0].startswith("report: ")
    assert "structural matches: 1" in stdout


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
    tables: bool = True,
    no_cache: bool = False,
    croma_xml_root: Path | None = None,
) -> dict[str, Any]:
    report, _, _ = run_compare_capture(
        paths,
        jobs=jobs,
        output_name=output_name,
        extra=extra,
        tables=tables,
        no_cache=no_cache,
        croma_xml_root=croma_xml_root,
    )
    return report


def run_compare_capture(
    paths: FixturePaths,
    *,
    jobs: int,
    output_name: str | None = None,
    extra: list[str] | None = None,
    tables: bool = True,
    no_cache: bool = False,
    croma_xml_root: Path | None = None,
) -> tuple[dict[str, Any], str, str]:
    prefix = f"{output_name}-" if output_name else ""
    report = paths.output / f"{prefix}report.json"
    command = [
        sys.executable,
        str(SCRIPT),
        "--results-jsonl",
        str(paths.results_jsonl),
        "--croma-xml-root",
        str(croma_xml_root if croma_xml_root is not None else paths.croma_root),
        "--reference-root",
        str(paths.reference_root),
        "--report",
        str(report),
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
    if tables:
        command.extend(
            [
                "--facts-jsonl",
                str(paths.output / f"{prefix}facts.jsonl"),
                "--comparison-jsonl",
                str(paths.output / f"{prefix}comparison.jsonl"),
                "--mismatches-jsonl",
                str(paths.output / f"{prefix}mismatches.jsonl"),
            ]
        )
    if no_cache:
        command.append("--no-cache")
    else:
        command.extend(["--cache-db", str(paths.output / "cache.sqlite")])
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
    return (
        json.loads(report.read_text(encoding="utf-8")),
        completed.stdout,
        completed.stderr,
    )


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


def tempo_words_with_sound(text: str, tempo: str) -> str:
    return f"""
      <direction placement="above">
        <direction-type><words>{text}</words></direction-type>
        <sound tempo="{tempo}"/>
      </direction>
"""


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
    lyric_xml: str | None = None,
) -> str:
    alter_xml = f"<alter>{alter}</alter>" if alter is not None else ""
    lyric_body = lyric_xml if lyric_xml is not None else (
        f"<lyric><syllabic>single</syllabic><text>{lyric}</text></lyric>"
        if lyric is not None
        else ""
    )
    return f"""
      <note>
        <pitch><step>{step}</step>{alter_xml}<octave>{octave}</octave></pitch>
        <duration>{duration}</duration>
        <type>{type_}</type>
        {lyric_body}
      </note>
"""
