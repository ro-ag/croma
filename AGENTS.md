# AGENTS.md

Guidance for AI agents (Codex, Claude Code, etc.) working in this repository.
Read this first, every session.

## Start every session here

Do **not** assume project state from chat history. Self-discover it. Run the
bootstrap, which is idempotent and recreates the progress database and (when the
corpus is available) the testbed:

```sh
tools/session_bootstrap.sh
```

It reports git state, provisions the toolchain (`rustup` + `uv sync`), builds
`target/debug/croma`, restores the progress tracker DB, and reports testbed
status. To also rebuild the full 10k testbed (needs the external corpus):

```sh
ABC_ROOT=/path/to/abc REF_ROOT=/path/to/musicxml tools/session_bootstrap.sh --testbed
```

Then read the current state before planning any work:

```sh
uv run python tools/progress/progress.py status
uv run python tools/progress/progress.py memory
uv run python tools/progress/progress.py sql \
  "select * from phase_summary order by updated_at desc limit 5;"
```

Pick the next target from the tracker's `next_recommended_target` and evidence
artifacts — not from intuition or memory.

## Environment

Two interchangeable environments, same pinned toolchain (details:
[`docs/development-environment.md`](docs/development-environment.md)):

- **Linux cloud sandbox** — provisioned with `rustup` + `uv`. Ephemeral: commit
  and push anything worth keeping.
- **Local Nix flake** — `nix develop` / direnv, any OS.

Rust 1.96.0 is pinned by `rust-toolchain.toml`; plain `cargo`/`rustc` select it
on any host. Never hardcode an absolute toolchain path. Use `uv` for all Python.

## Standing rules

- **Never work on `main`.** Branch per phase as `codex/<phase>-<slug>`.
- Generated corpus XML / JSONL / Parquet / reports and the runtime SQLite DB
  stay under `docs/untracked/` (git-ignored). Do not commit them.
- The corpus (ABC + reference MusicXML) is **external** and not committed; drive
  tooling via `ABC_ROOT` / `REF_ROOT`. See
  [`docs/testing/corpus-reproducibility.md`](docs/testing/corpus-reproducibility.md).
- `croma-core` must stay crates.io-publishable — no path-only/local runtime
  assumptions in library code.
- **Formatter and LSP are gated** until parser quality is proven. Parser /
  corpus / music21 comparison work remains the priority.

## Progress tracker

The committed SQL snapshot at `docs/progress/croma-progress.sql` is the portable
project memory; `docs/untracked/croma-progress.sqlite` is the ignored runtime
DB. Workflow and schema: [`docs/progress/README.md`](docs/progress/README.md).

```sh
uv run python tools/progress/progress.py restore   # rebuild runtime DB
uv run python tools/progress/progress.py status     # phase summary
# ...update the runtime DB via `sql` UPDATE/INSERT...
uv run python tools/progress/progress.py export     # write SQL snapshot
```

After a completed phase, update the runtime DB, export the snapshot, and commit
the SQL snapshot together with the source/docs/test changes.

## Validate before committing

```sh
cargo test --workspace        # Rust unit + integration tests
cargo run -p croma-cli -- xml examples/basic.abc
uv run pytest                 # Python tooling tests, if present
```

## Phase completion criteria

A parser/export phase is done only when:

- the target was selected from tracker/evidence;
- no-happy-path tests were added for any fixed bug;
- a targeted before/after comparison is reported when relevant, and a full
  report-only comparison is run when parser/export/comparison behavior changes;
- `cargo test --workspace` passes;
- the tracker DB is updated and the SQL snapshot exported;
- changes are committed and pushed.

Open a pull request **only** when these criteria are met and the user asks for
one.
