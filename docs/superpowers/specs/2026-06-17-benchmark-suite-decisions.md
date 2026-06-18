# Benchmark suite — decisions (Epic B)

**Date:** 2026-06-17 · **Phase:** phase-66-bench · **Status:** locked, pre-code

croma's four capabilities (forward writer, `croma fmt`, MusicXML→ABC reader,
`croma-lsp`) plus the `tree-sitter-abc` grammar are shipped and corpus-proven for
*correctness*. The only performance number that exists is the LSP leg-E latency
probe (~1 ms median, ~200-line file). This epic adds a formal, reproducible
**performance** picture: statistical micro-benchmarks for the hot paths,
corpus-scale throughput, and an LSP latency distribution — committed as a
machine-stamped baseline (`docs/benchmarks.md`). It also **hardens leg E**
(median → p50/p95/p99 across file sizes).

This epic is **additive and behavior-preserving**: benchmarks *measure*, they
never change product output. If a bench reveals a perf bug, it is **filed**, not
fixed here.

## What we measure (and the exact entry point)

| Target | Crate | Entry point | Unit |
| --- | --- | --- | --- |
| Parser | `croma-core` | `parse_document(src, ParseOptions::default())` | MB/s, files/s |
| Forward writer | `croma-core` | `export_musicxml(src)` (ABC→MusicXML) | MB/s, files/s |
| Formatter | `croma-fmt` | `format(src, FormatOptions::default())` | MB/s |
| Auto-fixer | `croma-fmt` | `auto_fix(src, FormatOptions::default())` | MB/s |
| Reader | `croma-core` (`musicxml-reader` feat) | `read_musicxml(xml)` (MusicXML→Score) | MB/s, files/s |
| LSP | `croma-lsp` | `diagnostics` / `semantic_tokens` / `formatting` / `hover` / `completion` / `code_actions` | p50/p95/p99 ms |
| Grammar | `tree-sitter-abc` | `tree-sitter parse` | bytes/ms |

Size buckets for fixtures and the LSP distribution: **small ≈ 20 lines**,
**average ≈ 200 lines**, **large ≈ 1000 lines** (deterministically synthesized so
the micro-benchmarks need no corpus).

## Decision 1 — Framework: **criterion as a dev-dependency**

Use [`criterion`](https://crates.io/crates/criterion) (statistical sampling,
warmup, outlier detection, `Throughput` → MB/s) as a **`[dev-dependencies]`** entry
plus per-crate `benches/` with a `[[bench]]` `harness = false` target.

- **Dev-deps are excluded from the published crate's normal-dependency graph.** The
  CI zero-dep guard is `cargo tree -p croma-core --edges normal | wc -l == 1`;
  `--edges normal` drops dev (and build) edges, so a criterion dev-dep leaves it at
  **1** (verified: baseline is `1` today, croma-core alone). A runtime dep on any
  bench crate is **rejected**.
- Pin **`criterion = { version = "0.5", default-features = false, features =
  ["cargo_bench_support"] }`**. Dropping the default `plotters` feature avoids a
  heavy/HTML/plotting dependency chain (no system cairo/gnuplot on the CI macОS
  runner); keeping `cargo_bench_support` is what lets the bench run under plain
  `cargo bench`. We commit a human-readable report, not the git-ignored
  `target/criterion/` HTML, so plots are unneeded.
- Hand-rolled `std::time` is the fallback **only** if criterion cannot be a clean
  dev-dep — not expected.

## Decision 2 — Layout: **per-crate `benches/`** (no central bench crate)

Each benchmark lives with the code it measures:

```
crates/croma-core/benches/parser.rs     # parse_document
crates/croma-core/benches/writer.rs     # export_musicxml
crates/croma-core/benches/reader.rs     # read_musicxml  (required-features = ["musicxml-reader"])
crates/croma-fmt/benches/fmt.rs         # format + auto_fix
crates/croma-lsp/  (latency lives in the existing corpus_proof harness, extended)
```

- Idiomatic, and it keeps the workspace **member count unchanged** (no
  `crates/croma-bench`). No new `exclude`/`default-members` churn.
- The reader bench carries `required-features = ["musicxml-reader"]`, so a plain
  `cargo bench`/`cargo build --all-targets` (no features) **skips** it, preserving
  the zero-dep default; it is exercised by the existing CI lines
  `cargo {test,clippy} -p croma-core … --features musicxml-reader`.
- Fixtures are generated **in-process** by a small deterministic generator (S/M/L
  ABC of known byte length) so the micro-benchmarks are **always runnable without
  the corpus**.

## Decision 3 — Three reporting layers

**(a) criterion micro-benchmarks on fixed fixtures** — always runnable, no corpus.
`Throughput::Bytes(len)` so criterion reports MB/s directly. These are the
statistically-robust per-call numbers (parser/writer/reader/fmt/auto_fix × S/M/L).

**(b) corpus-scale throughput** — an **in-process**, `ABC_ROOT`-gated harness
(mirrors `corpus_proof`: skips cleanly when `ABC_ROOT` is unset, asserts a
non-vacuous file count when set) that loops the real 10k corpus and reports
**files/s + MB/s** for parse / export / fmt. It prints a stable summary line; the
thin Python wrapper `tools/bench_corpus_throughput.py` runs it
(`cargo test --release … -- --ignored --nocapture`) and parses that line — exactly
the pattern `tools/prove_lsp_totality.py` uses over `corpus_proof`. In-process (not
per-file subprocess) so the number reflects library throughput, not process
spawn overhead.

**(c) LSP latency p50/p95/p99** — extend leg E from a single median into a
**distribution per request × size bucket** (small/avg/large) for diagnostics,
semantic tokens, formatting, hover, completion, code action. Reported p50/p95/p99;
the release ceiling assertion (diagnostics + semantic tokens **p99 < 50 ms**, with
wide margin) is retained so leg E stays a gate, not just a measurement.

**The committed report** is `docs/benchmarks.md`: every number, stamped with
**machine (CPU/OS/cores), toolchain (1.96.0), and commit**, plus the **exact
commands to reproduce each layer**. We do **not** commit `target/criterion/`
(git-ignored). Numbers are taken from a deliberate `--release` run on the recording
machine — no cherry-picked single samples; criterion's statistics and the
percentile harness carry the distribution.

## Decision 4 — `croma-core` default build stays zero-dep

criterion is **dev-only**; the reader bench is **feature-gated**. The published
crate's normal-dependency graph is unchanged:
`cargo tree -p croma-core --edges normal` stays **1 line**. Verified baseline today
= `1`. This is re-checked as a gate in every stage.

## Staging (each landed via `tools/land.py <branch> -y`; orchestrator stays small)

- **B1 — framework + core/fmt/reader micro-benchmarks.** criterion dev-dep +
  per-crate `benches/` for parser / writer / fmt / auto_fix / reader on S/M/L
  fixtures. **Gate:** `cargo bench` compiles + runs each; `clippy --all-targets`
  clean; zero-dep guard still 1.
- **B2 — LSP latency distribution + corpus throughput.** Extend leg E into
  p50/p95/p99 per request × {small,avg,large}; add
  `tools/bench_corpus_throughput.py` (+ in-process harness) for corpus files/s +
  MB/s. **Gate:** numbers produced; **leg E re-confirmed** (diagnostics + semantic
  tokens p99 < 50 ms with margin).
- **B3 — grammar throughput + baseline report.** `tree-sitter-abc` bytes/ms; write
  `docs/benchmarks.md` (all numbers + machine/toolchain/commit stamp + reproduce
  commands); tracker + memory. **Gate:** report committed + reproducible.

All bench/harness code is delegated to **subagents** (one per stage); the
orchestrator holds this doc + the tracker, verifies gates, and lands PRs.

## Guardrails (must stay green — additive epic, no behavior change)

- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`
  (lints benches), `cargo fmt --all --check` — all clean.
- No proven-gate regression: LSP legs A–E, fmt 10000/0, raw whitelist 9390/0,
  reader 9935/9935 + 98.50%, grammar `tree-sitter test` 17/17 + ≥99.46% — unchanged
  by construction; re-verify the crate whose bench is added still test-passes.
- `croma-core` zero-dep: `cargo tree -p croma-core --edges normal` = 1 line.
- Corpus-driven benches/harnesses **skip cleanly when `ABC_ROOT` is unset** (mirror
  `corpus_proof`), so a plain `cargo bench` without the corpus still runs the
  fixture benches.
- No AI co-author trailer on any commit.

## Recording machine (this session's baseline)

- **CPU:** Apple M4 Max (16 cores) · **RAM:** 64 GB
- **OS:** macOS 26.5.1 (Darwin 25.5.0, arm64)
- **Toolchain:** Rust 1.96.0 (pinned by `rust-toolchain.toml`)
- **Baseline commit:** `f81d5c0` (epic start)

`docs/benchmarks.md` re-stamps these at measurement time.
