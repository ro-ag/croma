#!/usr/bin/env python3
"""Provision Croma's external real-world ABC corpus.

Generated corpus files stay under docs/untracked/ by default. This script owns
only reproducible download/import steps; it does not make the corpus part of the
tracked Rust crate.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import re
import ssl
import subprocess
import sys
import tarfile
import urllib.request
import zipfile
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = ROOT / "docs" / "untracked" / "corpus" / "zenodo-10k"
DEFAULT_LFS_ARCHIVE = ROOT / "docs" / "corpus" / "zenodo-10k-abc.tar.gz"
ZENODO_10K_URL = "https://zenodo.org/records/17694747/files/dataset_10k.json?download=1"
ZENODO_10K_RECORD = "https://zenodo.org/records/17694747"
ZENODO_10K_DOI = "https://doi.org/10.5281/zenodo.17694747"
ABC2XML_URL = "https://wim.vree.org/svgParse/abc2xml.py-268.zip"
LICENSE_METADATA_KEYS = ("license", "license_url", "rights", "usage_rights")
LICENSE_EXCLUSION_PATTERNS = {
    "noncommercial": re.compile(r"\b(?:non[-\s]?commercial|nc)\b", re.IGNORECASE),
    "all-rights-reserved": re.compile(r"\ball\s+rights\s+reserved\b", re.IGNORECASE),
    "no-derivatives": re.compile(r"\b(?:no[-\s]?derivatives|nd)\b", re.IGNORECASE),
}


def clean_id(value: str) -> str:
    cleaned = re.sub(r"[^a-zA-Z0-9_.-]+", "-", value.strip())
    if not cleaned:
        raise ValueError("empty tune id")
    return cleaned


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def display_path(path: Path) -> str:
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


def license_metadata(metadata: dict[str, Any]) -> dict[str, str]:
    values = {}
    for key in LICENSE_METADATA_KEYS:
        value = metadata.get(key)
        if value is not None and str(value).strip():
            values[key] = str(value).strip()
    return values


def license_exclusion_reason(metadata: dict[str, Any]) -> str | None:
    text = " ".join(license_metadata(metadata).values())
    if not text:
        return None
    for reason, pattern in LICENSE_EXCLUSION_PATTERNS.items():
        if pattern.search(text):
            return reason
    return None


def download(url: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    context = ssl.create_default_context(cafile=os.environ.get("SSL_CERT_FILE"))
    request = urllib.request.Request(
        url,
        headers={"User-Agent": "Croma corpus provisioner"},
    )
    with urllib.request.urlopen(request, timeout=120, context=context) as response:
        destination.write_bytes(response.read())


def expected_sha256(path: Path) -> str:
    text = path.read_text(encoding="utf-8").strip()
    if not text:
        raise ValueError(f"empty checksum file: {path}")
    return text.split()[0]


def verify_archive(archive: Path, sha256_path: Path) -> str:
    expected = expected_sha256(sha256_path)
    actual = sha256_file(archive)
    if actual != expected:
        raise SystemExit(
            f"archive checksum mismatch for {archive}: expected {expected}, got {actual}. "
            "If this is a Git LFS pointer, run `git lfs pull --include docs/corpus/zenodo-10k-abc.tar.gz`."
        )
    return actual


def safe_extract_tar(archive: Path, output_dir: Path) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    output_root = output_dir.resolve()
    with tarfile.open(archive, "r:gz") as tar:
        members = tar.getmembers()
        for member in members:
            member_path = output_root / member.name
            try:
                member_path.resolve().relative_to(output_root)
            except ValueError as error:
                raise SystemExit(f"refusing unsafe archive member path: {member.name}") from error
            if member.issym() or member.islnk():
                raise SystemExit(f"refusing link in corpus archive: {member.name}")
        try:
            tar.extractall(output_root, members=members, filter="data")
        except TypeError:
            tar.extractall(output_root, members=members)


def import_archive(archive: Path, sha256_path: Path, output_dir: Path) -> int:
    digest = verify_archive(archive, sha256_path)
    safe_extract_tar(archive, output_dir)
    abc_count = len(list((output_dir / "abc").glob("*.abc")))
    if abc_count == 0:
        raise SystemExit(f"archive did not produce any .abc files under {output_dir / 'abc'}")
    print(f"verified {display_path(archive)} sha256={digest}")
    print(f"imported {abc_count} ABC files into {display_path(output_dir / 'abc')}")
    return abc_count


def normalized_tar_info(path: Path, arcname: str) -> tarfile.TarInfo:
    info = tarfile.TarInfo(arcname)
    stat = path.stat()
    info.size = stat.st_size if path.is_file() else 0
    info.mtime = 0
    info.uid = 0
    info.gid = 0
    info.uname = ""
    info.gname = ""
    info.mode = 0o755 if path.is_dir() else 0o644
    info.type = tarfile.DIRTYPE if path.is_dir() else tarfile.REGTYPE
    return info


def add_tar_directory(tar: tarfile.TarFile, path: Path, arcname: str) -> None:
    tar.addfile(normalized_tar_info(path, arcname))


def add_tar_file(tar: tarfile.TarFile, path: Path, arcname: str) -> None:
    with path.open("rb") as handle:
        tar.addfile(normalized_tar_info(path, arcname), handle)


def build_archive(corpus_root: Path, archive: Path, sha256_path: Path) -> str:
    abc_root = corpus_root / "abc"
    required = [abc_root, corpus_root / "manifest.jsonl", corpus_root / "license-report.json"]
    for path in required:
        if not path.exists():
            raise SystemExit(f"required corpus archive input is missing: {path}")

    archive.parent.mkdir(parents=True, exist_ok=True)
    with archive.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as gz:
            with tarfile.open(fileobj=gz, mode="w") as tar:
                add_tar_directory(tar, abc_root, "abc")
                for abc_file in sorted(abc_root.glob("*.abc")):
                    add_tar_file(tar, abc_file, f"abc/{abc_file.name}")
                add_tar_file(tar, corpus_root / "manifest.jsonl", "manifest.jsonl")
                add_tar_file(tar, corpus_root / "license-report.json", "license-report.json")

    digest = sha256_file(archive)
    sha256_path.parent.mkdir(parents=True, exist_ok=True)
    sha256_path.write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    return digest


def write_license_report(
    output_dir: Path,
    total: int,
    imported: int,
    excluded_entries: list[dict[str, Any]],
    included_without_license: int,
) -> None:
    excluded_by_reason: dict[str, int] = {}
    for entry in excluded_entries:
        reason = entry["reason"]
        excluded_by_reason[reason] = excluded_by_reason.get(reason, 0) + 1

    report = {
        "schema": "croma-corpus-license-report-v1",
        "source": {
            "name": "ABC Notation Dataset (10k samples)",
            "doi": ZENODO_10K_DOI,
            "record": ZENODO_10K_RECORD,
            "dataset_url": ZENODO_10K_URL,
            "license": "Creative Commons Attribution 4.0 International",
            "license_url": "https://creativecommons.org/licenses/by/4.0/",
        },
        "total": total,
        "imported": imported,
        "excluded": len(excluded_entries),
        "included_without_license": included_without_license,
        "excluded_by_reason": excluded_by_reason,
        "excluded_entries": excluded_entries,
    }
    (output_dir / "license-report.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def write_corpus(
    dataset_path: Path,
    output_dir: Path,
    limit: int | None,
    include_license_excluded: bool,
) -> int:
    data: list[dict[str, Any]] = json.loads(dataset_path.read_text(encoding="utf-8"))
    if limit is not None:
        data = data[:limit]

    abc_dir = output_dir / "abc"
    abc_dir.mkdir(parents=True, exist_ok=True)
    manifest_path = output_dir / "manifest.jsonl"
    excluded_entries: list[dict[str, Any]] = []
    included_without_license = 0
    imported = 0

    with manifest_path.open("w", encoding="utf-8") as manifest:
        for ordinal, item in enumerate(data, start=1):
            tune_id = clean_id(str(item["id"]))
            metadata = item.get("metadata", {})
            if not isinstance(metadata, dict):
                metadata = {}
            license_values = license_metadata(metadata)
            reason = license_exclusion_reason(metadata)
            if reason is not None and not include_license_excluded:
                excluded_entries.append(
                    {
                        "id": tune_id,
                        "ordinal": ordinal,
                        "reason": reason,
                        "license": license_values,
                    }
                )
                continue
            if not license_values:
                included_without_license += 1

            abc = item["abc_notation"].rstrip() + "\n"
            abc_path = abc_dir / f"{tune_id}.abc"
            abc_path.write_text(abc, encoding="utf-8")
            manifest.write(
                json.dumps(
                    {
                        "ordinal": ordinal,
                        "id": tune_id,
                        "path": display_path(abc_path),
                        "sha256": sha256_text(abc),
                        "title": metadata.get("title"),
                        "key": metadata.get("key"),
                        "meter": metadata.get("meter"),
                        "rhythm": metadata.get("rhythm"),
                        "source_url": metadata.get("source_url"),
                        "page_url": metadata.get("page_url"),
                        "original_id": metadata.get("original_id"),
                        "license": license_values or None,
                        "license_exclusion_reason": reason,
                    },
                    ensure_ascii=False,
                    sort_keys=True,
                )
                + "\n"
            )
            imported += 1

    write_license_report(output_dir, len(data), imported, excluded_entries, included_without_license)
    return imported


def ensure_abc2xml_tool(url: str, output_dir: Path) -> Path:
    cache_dir = output_dir / "tools" / "abc2xml"
    script_path = cache_dir / "abc2xml_268" / "abc2xml.py"
    if script_path.exists():
        return script_path

    zip_path = cache_dir / "abc2xml.py-268.zip"
    download(url, zip_path)
    with zipfile.ZipFile(zip_path) as archive:
        archive.extractall(cache_dir)

    if not script_path.exists():
        raise FileNotFoundError(f"abc2xml.py was not found after extracting {zip_path}")
    return script_path


def batched(values: list[Path], batch_size: int) -> list[list[Path]]:
    return [values[index : index + batch_size] for index in range(0, len(values), batch_size)]


def convert_with_abc2xml(
    abc_root: Path,
    output_dir: Path,
    report_path: Path,
    tool_path: Path,
    batch_size: int,
) -> tuple[int, int]:
    abc_files = sorted(abc_root.glob("*.abc"))
    if not abc_files:
        raise SystemExit(f"no .abc files found in {abc_root}; provision the corpus first")

    output_dir.mkdir(parents=True, exist_ok=True)
    report_path.parent.mkdir(parents=True, exist_ok=True)
    successes = 0
    failures = 0

    with report_path.open("w", encoding="utf-8") as report:
        for batch_number, batch in enumerate(batched(abc_files, batch_size), start=1):
            completed = subprocess.run(
                [sys.executable, str(tool_path), "-o", str(output_dir), *map(str, batch)],
                check=False,
                capture_output=True,
                text=True,
            )
            expected = [output_dir / f"{path.stem}.xml" for path in batch]
            missing = [path for path in expected if not path.exists()]
            converted = len(batch) - len(missing)
            successes += converted
            failures += len(missing)
            report.write(
                json.dumps(
                    {
                        "batch": batch_number,
                        "returncode": completed.returncode,
                        "input_count": len(batch),
                        "converted_count": converted,
                        "missing_count": len(missing),
                        "missing": [display_path(path) for path in missing],
                        "stdout": completed.stdout[-4000:],
                        "stderr": completed.stderr[-4000:],
                    },
                    ensure_ascii=False,
                    sort_keys=True,
                )
                + "\n"
            )
            print(f"batch {batch_number}: converted {converted}/{len(batch)}", flush=True)

    return successes, failures


def add_common_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="provision_corpus.py")
    subcommands = parser.add_subparsers(dest="command", required=True)

    fetch_parser = subcommands.add_parser("fetch-zenodo-10k", help="Download and import the Zenodo ABC 10k corpus")
    add_common_arguments(fetch_parser)
    fetch_parser.add_argument("--url", default=ZENODO_10K_URL)
    fetch_parser.add_argument("--cache", type=Path)
    fetch_parser.add_argument("--limit", type=int)
    fetch_parser.add_argument("--include-license-excluded", action="store_true")

    import_parser = subcommands.add_parser("import-zenodo-10k", help="Import an already downloaded Zenodo JSON")
    add_common_arguments(import_parser)
    import_parser.add_argument("dataset", type=Path)
    import_parser.add_argument("--limit", type=int)
    import_parser.add_argument("--include-license-excluded", action="store_true")

    archive_parser = subcommands.add_parser("import-archive", help="Import a verified compressed ABC corpus archive")
    add_common_arguments(archive_parser)
    archive_parser.add_argument("--archive", type=Path, default=DEFAULT_LFS_ARCHIVE)
    archive_parser.add_argument("--sha256-file", type=Path)

    build_archive_parser = subcommands.add_parser("build-archive", help="Build a compressed ABC corpus archive")
    add_common_arguments(build_archive_parser)
    build_archive_parser.add_argument("--archive", type=Path, default=DEFAULT_LFS_ARCHIVE)
    build_archive_parser.add_argument("--sha256-file", type=Path)

    reference_parser = subcommands.add_parser("abc2xml-real", help="Generate reference MusicXML with abc2xml.py")
    add_common_arguments(reference_parser)
    reference_parser.add_argument("--abc-root", type=Path)
    reference_parser.add_argument("--reference-root", type=Path)
    reference_parser.add_argument("--report", type=Path)
    reference_parser.add_argument("--tool", type=Path)
    reference_parser.add_argument("--tool-url", default=ABC2XML_URL)
    reference_parser.add_argument("--batch-size", type=int, default=100)

    args = parser.parse_args(argv)
    output = args.output.resolve()

    if args.command == "fetch-zenodo-10k":
        cache_path = args.cache or output / "cache" / "dataset_10k.json"
        download(args.url, cache_path)
        count = write_corpus(
            cache_path,
            output,
            args.limit,
            include_license_excluded=args.include_license_excluded,
        )
        print(f"downloaded {display_path(cache_path)}")
        print(f"imported {count} ABC files into {display_path(output / 'abc')}")
        return 0

    if args.command == "import-zenodo-10k":
        count = write_corpus(
            args.dataset,
            output,
            args.limit,
            include_license_excluded=args.include_license_excluded,
        )
        print(f"imported {count} ABC files into {display_path(output / 'abc')}")
        return 0

    if args.command == "import-archive":
        archive = args.archive.resolve()
        sha256_path = (args.sha256_file or Path(f"{archive}.sha256")).resolve()
        import_archive(archive, sha256_path, output)
        return 0

    if args.command == "build-archive":
        archive = args.archive.resolve()
        sha256_path = (args.sha256_file or Path(f"{archive}.sha256")).resolve()
        digest = build_archive(output, archive, sha256_path)
        print(f"built {display_path(archive)}")
        print(f"wrote {display_path(sha256_path)}")
        print(f"sha256={digest}")
        return 0

    if args.command == "abc2xml-real":
        abc_root = (args.abc_root or output / "abc").resolve()
        reference_root = (args.reference_root or output / "musicxml").resolve()
        report = (args.report or output / "abc2xml-report.jsonl").resolve()
        tool = args.tool.resolve() if args.tool is not None else ensure_abc2xml_tool(args.tool_url, output)
        successes, failures = convert_with_abc2xml(
            abc_root=abc_root,
            output_dir=reference_root,
            report_path=report,
            tool_path=tool,
            batch_size=args.batch_size,
        )
        print(f"converted {successes} ABC files to MusicXML; missing {failures}")
        return 1 if failures else 0

    raise AssertionError(args.command)


if __name__ == "__main__":
    raise SystemExit(main())
