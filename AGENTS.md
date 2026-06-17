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
status.

If the external corpus is missing, bootstrap can recreate the original 10k ABC
sources from Zenodo under ignored storage:

```sh
tools/session_bootstrap.sh --fetch-corpus
tools/session_bootstrap.sh --fetch-corpus --fetch-reference
```

Bootstrap prefers the verified Git LFS archive at
`docs/corpus/zenodo-10k-abc.tar.gz` when present, and falls back to Zenodo when
the archive is absent or fails checksum validation. If LFS smudge was skipped,
fetch only that archive with:

```sh
git lfs pull --include docs/corpus/zenodo-10k-abc.tar.gz
```

The second bootstrap command also generates reference MusicXML with `abc2xml.py`.
To rebuild the full 10k testbed after the corpus exists:

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
- The **formatter (`croma fmt` + `--auto-fix`) is promoted** (un-gated): parser
  quality is proven (raw whitelist 9,390 / dropped 545 / worklist 0) and the
  formatter is idempotent + lossless over the full 10k corpus — see
  [`docs/formatter.md`](docs/formatter.md) and the `corpus_proof` test +
  `tools/prove_fmt_lossless.py`.
- The **MusicXML→Score reader (`croma read` / `croma musicxml2abc`) is promoted**
  (un-gated in the CLI): self-loop XML re-emission idempotence **9,934/9,935** (1
  adjudicated residual), totality 0-panic over croma's own 10k + the
  abc2xml-reference 10k + malformed inputs, reference-dialect **music21 parity
  98.50%** (above the 93.9% forward floor; residual adjudicated), and a reader→ABC
  structural round-trip of **95.8%** — see
  [`docs/musicxml-reader.md`](docs/musicxml-reader.md). The **default CLI build now
  ships the reader** (pulling `roxmltree`), but **`croma-core` keeps
  `musicxml-reader` opt-in** so the library's default build stays zero-dependency +
  crates.io-publishable (`cargo build -p croma-core` pulls nothing; roxmltree is a
  dep of the CLI *binary* only). Build a reader-less CLI with
  `cargo build -p croma-cli --no-default-features`.
- The **LSP (`croma-lsp`, a stdio language server) is promoted** (un-gated in the
  default build). It is a thin adapter over the proven core/formatter — never a
  re-implementation — proven over the full 10k corpus: diagnostics fidelity
  **10,000/10,000** (LSP path == core, codes+spans), formatting identity
  **10,000/10,000** (== `croma fmt`, byte-exact), totality **0 panics / 0 hangs**
  (didOpen/didChange incl. malformed mid-edit), semantic tokens
  exhaustive/non-overlapping/in-bounds **10,000/0**, and latency **~1 ms** for
  diagnostics + semantic tokens on a ~200-line file (ceiling 50 ms) — see
  [`docs/lsp.md`](docs/lsp.md) and the `croma-lsp` `corpus_proof` harness +
  `tools/prove_lsp_totality.py` / `tools/prove_lsp_fidelity.py`. The **default
  build now ships the `croma-lsp` binary** (pulling `lsp-server`/`lsp-types`), but
  those deps live on the **LSP binary only**; **`croma-core` stays
  zero-dependency + crates.io-publishable** (`cargo build -p croma-core` pulls
  nothing). Any LSP-vs-core mismatch is a bug, not a new spec; re-prove the legs
  above after any touch.

## Parser recovery policy

The parser is **strict to ABC 2.1**. When it meets malformed input, it follows
one three-tier rule (loose source is the formatter's job, not the parser's):

1. **Default: reject.** Input that does not match the spec grammar is not
   silently accepted.
2. **Recover *and warn* — only for a clear intention spoiled by a minimal,
   mechanical slip** (a stray space/comma, a missing space). Recover the obvious
   intent and **always emit a diagnostic** — recovery is **never silent**. A
   silent recovery is indistinguishable from mimicking `abc2xml`; the warning is
   what makes recovery defensible as transparent strict-recognition.
3. **Otherwise: strict reject.** If the intention is not unambiguous, or the
   mistake is not a trivial slip, reject it. Repair belongs in
   `croma fmt --auto-fix`, which sanitises loose source into canonical spelling
   the strict parser then reads cleanly.

Corpus impact: warnings are stderr diagnostics, so adding one is
**whitelist-neutral** (the MusicXML is unchanged) and always safe to land.
Converting a recovery into a reject **changes the MusicXML**, so it can drop
files out of `whitelist.csv`; that is acceptable only as an **adjudicated** move
to `dropped.csv` (croma is strict-correct, `abc2xml` is lenient — see
[`spec-is-driver`](docs/comparison/abc2xml-divergences/README.md)), never a
silent regression. Triage each such file with the divergence-triage process.

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

## Corpus comparison quick facts

Full recipe: [`docs/testing/corpus-reproducibility.md`](docs/testing/corpus-reproducibility.md).
What agents should know up front about `tools/music21_polars_corpus_compare.py`:

- **The comparator is RAW (2026-06).** The six match-forcing normalizations were
  stripped — it reports raw structural differences and forces no matches. **Do not
  re-add normalizations to "recover" the match rate.** Files are partitioned into
  `whitelist.csv` (raw matches; the regression baseline) and `dropped.csv`
  (adjudicated non-croma-bugs), both under
  [`docs/comparison/abc2xml-divergences/`](docs/comparison/abc2xml-divergences/README.md);
  the worklist (mismatches) is triaged **one file at a time** by an **investigator
  agent** (reads the ABC, runs croma/abc2xml/music21, consults the ABC 2.1 spec KB,
  returns a structured verdict) driven by a **triage process** that decides
  fix-croma / fix-comparator / drop. Protocol and verdict schema are runner-neutral
  — start from
  [`docs/comparison/abc2xml-divergences/TRIAGE.md`](docs/comparison/abc2xml-divergences/TRIAGE.md);
  the Claude Code binding is `.claude/agents/abc-divergence-investigator.md` +
  `.claude/skills/divergence-triage/`. Pass `--whitelist-csv` to emit the whitelist
  and `--dropped-csv` to exclude dropped files (the report shows
  `whitelist_files` / `dropped_files`).
- **It is cheap to re-run.** A content-addressed SQLite cache
  (`docs/untracked/cache/compare-cache.sqlite`, git-ignored, disposable)
  makes a full 10k rerun ~0.7 s when nothing changed, ~2-3 s after a parser
  change, ~23 s cold. Versioning is automatic (tool sources + music21/polars
  versions), so code edits self-invalidate; pass `--no-cache` if you suspect
  staleness. Do not skip a full comparison to save time — it costs seconds.
- **Logs are JSONL by default.** Progress events stream on stderr; stdout
  ends with one `{"event":"summary",...}` object whose fields mirror the
  report JSON (report path, counters, `mismatch_category_counts`, cache
  hits). Parse that line instead of scraping text; `--log-format text`
  restores the legacy lines.
- **Tables are schema v3 with typed values.** Filter mismatch/fact tables
  natively on `value_kind` plus `value_str`/`value_int`/`value_float`
  (`value_json` only for nested values) — values are not JSON-quoted
  strings. `--jobs` defaults to auto (CPU count minus one).
- Cache maintenance: `uv run python tools/compare_cache.py stats|invalidate <file>|prune`.

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
