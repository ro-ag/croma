# Croma Progress Tracker

This directory stores the committed, plain-text snapshot of the project progress
ledger. The queryable SQLite database is local and ignored.

Paths:

- `docs/progress/croma-progress.sql`: committed SQL dump, used as portable memory.
- `docs/untracked/croma-progress.sqlite`: ignored runtime SQLite database.
- `tools/progress/progress.py`: restore/query/export helper.

Typical workflow:

```sh
# Restore local query DB from the committed SQL snapshot.
uv run python tools/progress/progress.py restore

# Show current phase status.
uv run python tools/progress/progress.py status

# Show metrics for one phase.
uv run python tools/progress/progress.py metrics --phase phase-10i

# Export updated runtime DB back to committed plain text.
uv run python tools/progress/progress.py export
```

Agents should restore/query this tracker before generating new phase prompts.
After a completed phase, update the runtime DB, export the SQL snapshot, and commit
the SQL snapshot when the phase's completion criteria are met.

Generated corpus reports, XML, parquet, JSONL, and runtime DB files remain under
`docs/untracked/` and are not committed.
