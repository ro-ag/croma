#!/usr/bin/env python3
"""Manage Croma's local progress ledger.

The runtime SQLite database is intentionally ignored:

    docs/untracked/croma-progress.sqlite

The portable project memory is committed as plain SQL:

    docs/progress/croma-progress.sql
"""

from __future__ import annotations

import argparse
import os
import sqlite3
import sys
from pathlib import Path
from typing import Iterable


ROOT = Path(__file__).resolve().parents[2]
DB_PATH = ROOT / "docs" / "untracked" / "croma-progress.sqlite"
DUMP_PATH = ROOT / "docs" / "progress" / "croma-progress.sql"
QUERIES_PATH = ROOT / "docs" / "progress" / "queries.sql"


SCHEMA_SQL = """
PRAGMA user_version = 1;

CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS phase (
  phase_id TEXT PRIMARY KEY,
  branch TEXT,
  status TEXT NOT NULL CHECK (status IN ('planned','in_progress','complete','merged','blocked','unknown')),
  pr_number INTEGER,
  pr_url TEXT,
  commit_hash TEXT,
  selected_target TEXT,
  classification TEXT,
  summary TEXT,
  next_recommended_target TEXT,
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS metric (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  phase_id TEXT NOT NULL REFERENCES phase(phase_id) ON DELETE CASCADE,
  scope TEXT NOT NULL,
  name TEXT NOT NULL,
  before_value TEXT,
  after_value TEXT,
  delta TEXT,
  unit TEXT,
  notes TEXT,
  UNIQUE (phase_id, scope, name)
);

CREATE TABLE IF NOT EXISTS artifact (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  phase_id TEXT NOT NULL REFERENCES phase(phase_id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  path TEXT NOT NULL,
  description TEXT,
  UNIQUE (phase_id, kind, path)
);

CREATE TABLE IF NOT EXISTS validation (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  phase_id TEXT NOT NULL REFERENCES phase(phase_id) ON DELETE CASCADE,
  command TEXT NOT NULL,
  status TEXT NOT NULL,
  notes TEXT,
  UNIQUE (phase_id, command)
);

CREATE TABLE IF NOT EXISTS memory (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  notes TEXT,
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE VIEW IF NOT EXISTS phase_summary AS
SELECT
  phase_id,
  status,
  branch,
  pr_number,
  commit_hash,
  selected_target,
  classification,
  summary,
  next_recommended_target
FROM phase
ORDER BY phase_id;
"""


def connect() -> sqlite3.Connection:
    DB_PATH.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    return conn


def ensure_db() -> None:
    if DB_PATH.exists():
        return
    if DUMP_PATH.exists():
        restore(force=False)
        return
    with connect() as conn:
        conn.executescript(SCHEMA_SQL)


def restore(force: bool) -> None:
    if DB_PATH.exists() and not force:
        print(f"{DB_PATH} already exists; use restore --force to replace it", file=sys.stderr)
        return
    if not DUMP_PATH.exists():
        DB_PATH.parent.mkdir(parents=True, exist_ok=True)
        with connect() as conn:
            conn.executescript(SCHEMA_SQL)
        print(f"initialized empty DB at {DB_PATH}")
        return
    DB_PATH.parent.mkdir(parents=True, exist_ok=True)
    if DB_PATH.exists():
        DB_PATH.unlink()
    with connect() as conn:
        conn.executescript(DUMP_PATH.read_text())
    print(f"restored {DB_PATH} from {DUMP_PATH}")


def export() -> None:
    ensure_db()
    DUMP_PATH.parent.mkdir(parents=True, exist_ok=True)
    with connect() as conn:
        lines = list(conn.iterdump())
    DUMP_PATH.write_text("\n".join(lines) + "\n")
    print(f"exported {DB_PATH} to {DUMP_PATH}")


def print_rows(rows: Iterable[sqlite3.Row]) -> None:
    rows = list(rows)
    if not rows:
        return
    headers = rows[0].keys()
    widths = {h: len(h) for h in headers}
    values: list[dict[str, str]] = []
    for row in rows:
        item = {h: "" if row[h] is None else str(row[h]) for h in headers}
        values.append(item)
        for h, value in item.items():
            widths[h] = max(widths[h], len(value))
    print("  ".join(h.ljust(widths[h]) for h in headers))
    print("  ".join("-" * widths[h] for h in headers))
    for item in values:
        print("  ".join(item[h].ljust(widths[h]) for h in headers))


def run_query(sql: str, params: tuple[object, ...] = ()) -> None:
    ensure_db()
    with connect() as conn:
        rows = conn.execute(sql, params).fetchall()
    print_rows(rows)


def status() -> None:
    run_query(
        """
        SELECT phase_id, status, branch, pr_number, selected_target, next_recommended_target
        FROM phase_summary
        ORDER BY
          CASE
            WHEN phase_id = 'phase-9' THEN 9
            WHEN phase_id = 'phase-10' THEN 10
            ELSE 100
          END,
          phase_id;
        """
    )


def metrics(phase_id: str | None) -> None:
    if phase_id:
        run_query(
            """
            SELECT phase_id, scope, name, before_value, after_value, delta, unit, notes
            FROM metric
            WHERE phase_id = ?
            ORDER BY scope, name;
            """,
            (phase_id,),
        )
    else:
        run_query(
            """
            SELECT phase_id, scope, name, before_value, after_value, delta, unit
            FROM metric
            ORDER BY phase_id, scope, name;
            """
        )


def artifacts(phase_id: str | None) -> None:
    if phase_id:
        run_query(
            """
            SELECT phase_id, kind, path, description
            FROM artifact
            WHERE phase_id = ?
            ORDER BY kind, path;
            """,
            (phase_id,),
        )
    else:
        run_query("SELECT phase_id, kind, path, description FROM artifact ORDER BY phase_id, kind;")


def validations(phase_id: str | None) -> None:
    if phase_id:
        run_query(
            """
            SELECT phase_id, status, command, notes
            FROM validation
            WHERE phase_id = ?
            ORDER BY command;
            """,
            (phase_id,),
        )
    else:
        run_query("SELECT phase_id, status, command FROM validation ORDER BY phase_id, command;")


def memory() -> None:
    run_query("SELECT key, value, notes FROM memory ORDER BY key;")


def sql(query: str | None) -> None:
    ensure_db()
    text = query
    if text is None:
        text = sys.stdin.read()
    if not text.strip():
        raise SystemExit("empty SQL query")
    with connect() as conn:
        rows = conn.execute(text).fetchall()
    print_rows(rows)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="cmd", required=True)

    restore_p = sub.add_parser("restore", help="restore ignored SQLite DB from committed SQL")
    restore_p.add_argument("--force", action="store_true", help="replace an existing runtime DB")

    sub.add_parser("init", help="create an empty runtime DB if no dump exists")
    sub.add_parser("export", help="export ignored runtime DB to committed SQL dump")
    sub.add_parser("status", help="show phase status summary")
    sub.add_parser("memory", help="show persistent project memories")

    metrics_p = sub.add_parser("metrics", help="show metrics")
    metrics_p.add_argument("--phase", help="phase id, for example phase-10i")

    artifacts_p = sub.add_parser("artifacts", help="show evidence artifacts")
    artifacts_p.add_argument("--phase", help="phase id, for example phase-10i")

    validations_p = sub.add_parser("validations", help="show validation commands")
    validations_p.add_argument("--phase", help="phase id, for example phase-10i")

    sql_p = sub.add_parser("sql", help="run a read query against the runtime DB")
    sql_p.add_argument("query", nargs="?", help="SQL query; reads stdin if omitted")

    args = parser.parse_args()
    os.chdir(ROOT)

    if args.cmd == "restore":
        restore(force=args.force)
    elif args.cmd == "init":
        ensure_db()
        print(f"runtime DB ready at {DB_PATH}")
    elif args.cmd == "export":
        export()
    elif args.cmd == "status":
        status()
    elif args.cmd == "memory":
        memory()
    elif args.cmd == "metrics":
        metrics(args.phase)
    elif args.cmd == "artifacts":
        artifacts(args.phase)
    elif args.cmd == "validations":
        validations(args.phase)
    elif args.cmd == "sql":
        sql(args.query)
    else:
        parser.error(f"unknown command: {args.cmd}")


if __name__ == "__main__":
    main()
