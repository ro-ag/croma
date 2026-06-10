#!/usr/bin/env python3
"""Content-addressed SQLite cache for the music21 corpus comparison pipeline.

Two layers, both keyed by content rather than by run:

- ``facts``: per MusicXML file (one side of a comparison), keyed by the
  SHA-256 of the file bytes plus an extractor version. Caches the music21
  structural facts so unchanged files are never re-parsed by music21.
- ``results``: per (croma, reference) file pair, keyed by both content
  hashes, the comparison tool version, the relative path, and the
  comparison options. Caches the whole per-file comparison payload so
  unchanged pairs skip fact-row building and the Polars join entirely.

Both tables carry a ``relative_path`` column with an index so entries can
be inspected or invalidated by file name. The cache lives under
``docs/untracked/`` (git-ignored) by default.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sqlite3
import sys
import time
import zlib
from pathlib import Path
from typing import Any

import orjson

CACHE_SCHEMA_VERSION = "1"
DEFAULT_CACHE_DB = Path("docs/untracked/cache/compare-cache.sqlite")
CACHE_DB_ENV_VAR = "CROMA_COMPARE_CACHE_DB"
STALE_ROW_MAX_AGE_DAYS = 14

_SCHEMA_SQL = """
CREATE TABLE IF NOT EXISTS facts_cache (
    content_hash TEXT NOT NULL,
    version TEXT NOT NULL,
    relative_path TEXT,
    source_side TEXT,
    payload BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL,
    PRIMARY KEY (content_hash, version)
);
CREATE INDEX IF NOT EXISTS facts_cache_relative_path
    ON facts_cache(relative_path);
CREATE TABLE IF NOT EXISTS result_cache (
    pair_key TEXT NOT NULL PRIMARY KEY,
    relative_path TEXT,
    payload BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS result_cache_relative_path
    ON result_cache(relative_path);
"""


def file_sha256(path: Path) -> str | None:
    digest = hashlib.sha256()
    try:
        with path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1 << 20), b""):
                digest.update(chunk)
    except OSError:
        return None
    return digest.hexdigest()


def text_sha256(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def installed_version(package: str) -> str:
    try:
        from importlib.metadata import PackageNotFoundError, version
    except ImportError:  # pragma: no cover - importlib.metadata is stdlib
        return "unknown"
    try:
        return version(package)
    except PackageNotFoundError:
        module = sys.modules.get(package)
        return getattr(module, "__version__", "unknown")


def facts_cache_version() -> str:
    extractor_source = (Path(__file__).resolve().parent / "music21_compare.py").read_bytes()
    parts = [
        CACHE_SCHEMA_VERSION,
        hashlib.sha256(extractor_source).hexdigest(),
        installed_version("music21"),
        f"py{sys.version_info.major}.{sys.version_info.minor}",
    ]
    return text_sha256("|".join(parts))[:16]


def result_cache_version(facts_version: str) -> str:
    tool_source = (
        Path(__file__).resolve().parent / "music21_polars_corpus_compare.py"
    ).read_bytes()
    parts = [
        CACHE_SCHEMA_VERSION,
        facts_version,
        hashlib.sha256(tool_source).hexdigest(),
        installed_version("polars"),
    ]
    return text_sha256("|".join(parts))[:16]


def pair_result_key(
    croma_hash: str,
    reference_hash: str,
    result_version: str,
    relative_path: str,
    options: dict[str, Any],
) -> str:
    options_json = json.dumps(options, sort_keys=True, separators=(",", ":"))
    return text_sha256(
        "|".join([croma_hash, reference_hash, result_version, relative_path, options_json])
    )


def encode_payload(value: Any) -> bytes:
    # orjson over stdlib json: ~6x faster and emits utf-8 bytes directly.
    # Output stays plain JSON, so blobs written by either codec interoperate.
    return zlib.compress(orjson.dumps(value))


def decode_payload(blob: bytes) -> Any:
    return orjson.loads(zlib.decompress(blob))


class CompareCache:
    """One SQLite connection per process; safe for concurrent workers via WAL."""

    def __init__(self, path: Path, connection: sqlite3.Connection) -> None:
        self.path = path
        self._connection = connection

    @classmethod
    def open(cls, path: Path) -> "CompareCache":
        path.parent.mkdir(parents=True, exist_ok=True)
        try:
            return cls(path, cls._connect(path))
        except sqlite3.DatabaseError:
            path.unlink(missing_ok=True)
            for suffix in ["-wal", "-shm"]:
                Path(f"{path}{suffix}").unlink(missing_ok=True)
            return cls(path, cls._connect(path))

    @staticmethod
    def _connect(path: Path) -> sqlite3.Connection:
        connection = sqlite3.connect(path, timeout=60.0)
        connection.execute("PRAGMA journal_mode=WAL")
        connection.execute("PRAGMA synchronous=NORMAL")
        connection.executescript(_SCHEMA_SQL)
        connection.commit()
        return connection

    def close(self) -> None:
        self._connection.close()

    def get_facts(self, content_hash: str, version: str) -> Any | None:
        row = self._connection.execute(
            "SELECT payload FROM facts_cache WHERE content_hash = ? AND version = ?",
            (content_hash, version),
        ).fetchone()
        if row is None:
            return None
        payload = decode_payload(row[0])
        self._touch("facts_cache", "content_hash = ? AND version = ?", (content_hash, version))
        return payload

    def put_facts(
        self,
        content_hash: str,
        version: str,
        relative_path: str | None,
        source_side: str | None,
        facts: Any,
    ) -> None:
        now = int(time.time())
        self._connection.execute(
            "INSERT OR REPLACE INTO facts_cache"
            " (content_hash, version, relative_path, source_side, payload,"
            "  created_at, last_used_at)"
            " VALUES (?, ?, ?, ?, ?, ?, ?)",
            (content_hash, version, relative_path, source_side, encode_payload(facts), now, now),
        )
        self._connection.commit()

    def get_result(self, pair_key: str) -> Any | None:
        row = self._connection.execute(
            "SELECT payload FROM result_cache WHERE pair_key = ?",
            (pair_key,),
        ).fetchone()
        if row is None:
            return None
        payload = decode_payload(row[0])
        self._touch("result_cache", "pair_key = ?", (pair_key,))
        return payload

    def put_result(self, pair_key: str, relative_path: str | None, payload: Any) -> None:
        now = int(time.time())
        self._connection.execute(
            "INSERT OR REPLACE INTO result_cache"
            " (pair_key, relative_path, payload, created_at, last_used_at)"
            " VALUES (?, ?, ?, ?, ?)",
            (pair_key, relative_path, encode_payload(payload), now, now),
        )
        self._connection.commit()

    def _touch(self, table: str, where: str, parameters: tuple[Any, ...]) -> None:
        now = int(time.time())
        self._connection.execute(
            f"UPDATE {table} SET last_used_at = ? WHERE {where} AND last_used_at < ?",
            (now, *parameters, now - 86400),
        )
        self._connection.commit()

    def invalidate_relative_path(self, relative_path: str) -> int:
        deleted = 0
        for table in ["facts_cache", "result_cache"]:
            cursor = self._connection.execute(
                f"DELETE FROM {table} WHERE relative_path = ?", (relative_path,)
            )
            deleted += cursor.rowcount
        self._connection.commit()
        return deleted

    def prune_stale(self, max_age_days: int = STALE_ROW_MAX_AGE_DAYS) -> int:
        cutoff = int(time.time()) - max_age_days * 86400
        deleted = 0
        for table in ["facts_cache", "result_cache"]:
            cursor = self._connection.execute(
                f"DELETE FROM {table} WHERE last_used_at < ?", (cutoff,)
            )
            deleted += cursor.rowcount
        self._connection.commit()
        return deleted

    def stats(self) -> dict[str, Any]:
        rows = {}
        for table in ["facts_cache", "result_cache"]:
            count, payload_bytes = self._connection.execute(
                f"SELECT COUNT(*), COALESCE(SUM(LENGTH(payload)), 0) FROM {table}"
            ).fetchone()
            rows[table] = {"rows": int(count), "payload_bytes": int(payload_bytes)}
        return rows


def main() -> int:
    parser = argparse.ArgumentParser(description="Inspect or maintain the comparison cache")
    parser.add_argument(
        "--cache-db",
        type=Path,
        default=Path(os.environ.get(CACHE_DB_ENV_VAR, str(DEFAULT_CACHE_DB))),
    )
    subcommands = parser.add_subparsers(dest="command", required=True)
    subcommands.add_parser("stats")
    invalidate = subcommands.add_parser("invalidate")
    invalidate.add_argument("relative_path")
    prune = subcommands.add_parser("prune")
    prune.add_argument("--max-age-days", type=int, default=STALE_ROW_MAX_AGE_DAYS)
    args = parser.parse_args()

    cache = CompareCache.open(args.cache_db)
    try:
        if args.command == "stats":
            print(json.dumps(cache.stats(), indent=2))
        elif args.command == "invalidate":
            print(f"deleted {cache.invalidate_relative_path(args.relative_path)} rows")
        elif args.command == "prune":
            print(f"deleted {cache.prune_stale(args.max_age_days)} rows")
    finally:
        cache.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
