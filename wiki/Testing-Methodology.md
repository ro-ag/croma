# Testing & proving methodology

[[How-its-Proven]] lists *what* the gates report; [[Benchmarks]] lists *how
fast*. This page is the **how** behind both — the methodology croma uses to prove
**accuracy** (conversions are spec-faithful, not just plausible) and
**efficiency** (fast at corpus scale, measured honestly). Both are proven the
same way: layered, reproducible, environment-gated, and **adjudicated**.

## Two layers of tests

| Layer | What | When it runs |
| --- | --- | --- |
| **Unit + integration** (~800 tests) | hand-written checks of specific spec points, diagnostics, codes, and byte spans | every change — `cargo test --workspace` |
| **Corpus-scale gates** | the four capabilities + grammar run over a **real-world corpus of 10,000 ABC files** (the [Zenodo ABC dataset](https://doi.org/10.5281/zenodo.17694747)) | environment-gated (`ABC_ROOT`); the full matrix runs from **croma-test** in CI |

Unit tests catch regressions fast; the corpus is what proves the conversions
hold on real music at scale. The corpus-scale gates **skip cleanly** when the
corpus is absent, so croma builds and tests standalone (the **repo split** in
[[How-its-Proven]] — a contributor never needs the heavy proving suite).

## Two principles behind every gate

### 1. Every gate has an *adjudicated* residual

No gate is "pass = byte-identical to another tool." croma **matches** the
baseline where the baseline is spec-correct, and for every remaining divergence
it **decides** (adjudicates) whether croma or the baseline is right and
**documents** the verdict. A gate's residual is therefore a set of *documented,
spec-justified non-bugs*, never unexplained failures.

The discipline that follows: a change may move a gate number **only** when the
new behaviour is more spec-correct **and** the moved files are adjudicated via
the divergence-triage process — never a silent regression. (See
[[Contributing#the-one-rule-that-matters-dont-regress-a-proven-gate]].)

### 2. Prove each capability *two independent ways*

Each promoted capability is proven by **two harnesses that must agree**:

- an **in-process gate** that reuses the crate's own machinery, running over the
  whole corpus in seconds (no client/process spawn), and
- an independent **black-box prover** (a croma-test Python script) that drives
  the **built binary** with a different measurement.

If a bug slipped past one, it has to slip past the other — measured differently —
too. The formatter's in-process `corpus_proof.rs` and black-box
`prove_fmt_lossless.py` agree at `10000 files, 0 notes_changed /
0 not_idempotent / 0 canonical_xml_changed`; the LSP pairs `corpus_proof.rs`
(legs A–E) + an in-memory `Connection` transport test with `prove_lsp_totality.py`
/ `prove_lsp_fidelity.py`.

## Accuracy: how each capability is proven

### Forward writer (ABC → MusicXML) — parity + adjudication

A **raw comparator** diffs croma's MusicXML against
[`abc2xml`](https://wim.vree.org/svgParse/abc2xml.html) (the reference baseline)
over all 10k files. Each file lands as a **structural match** (kept on a
whitelist) or an **adjudicated drop** — a case where croma is strict-correct and
abc2xml is lenient (e.g. abc2xml inserts an empty leading measure; croma
declines it). The worklist of un-triaged files is driven to **zero**, leaving
**9,390 / 9,390** adjudicated matches. The comparator, the whitelist/dropped
baseline, the ABC 2.1 spec knowledge base, and the triage tooling all live in
**croma-test**. See [[abc2xml-Comparison]].

### Formatter — invariants enforced by runtime safety gates

The formatter's contract is two invariants, proven over the corpus
(**10,000 / 10,000**):

- **Idempotent** — `format(format(x)) == format(x)`.
- **Lossless** — plain `format` renders **byte-identical MusicXML**; `auto_fix`
  preserves the **ordered pitch sequence**.

`--auto-fix` goes further: **every curation declares the safety gate it must
clear** (`Pitch`, `Structure`, or `DirectiveTokens`). The candidate edit is
applied to a *trial* string, re-checked against its gate, and **kept only if the
gate holds** — otherwise reverted and reported as `skipped`. A repair therefore
*cannot* silently change the score; the gate is mechanical, not a reviewer's
judgement. ([[Formatter]].)

### Reader (MusicXML → ABC) — self-loop idempotence + a decisive residual test

Three angles:

- **Self-loop idempotence** — `write_musicxml(read_musicxml(xml)) == xml`,
  **9,935 / 9,935**. Because the reader is built as the *inverse of croma's own
  writer*, this is an exact, mechanical check.
- **Foreign-dialect parity** vs **music21** on abc2xml / MuseScore / Finale /
  Sibelius output — **98.50%**.
- **Reader → ABC round-trip** — **97.9%** (9,724 / 9,933). The 209-file residual
  is triaged with a *decisive* test: does any **sounding fact** (pitch + alter +
  octave + duration) get dropped or added? **207 of 209 preserve every sounding
  fact** — valid-but-different structure the lossy XML intermediate cannot
  byte-match, **not** a defect. ([[MusicXML-Reader]].)

### Language server — fidelity + totality

Because `croma-lsp` is a **thin adapter**, its proof is *equality to the core*,
not a re-test of the core:

- **Fidelity** — LSP-path diagnostics / formatting / semantic tokens equal the
  core's, in order and byte-for-byte (**10,000 / 0** mismatches per leg).
- **Totality** — **0 panics / 0 hangs** over the corpus under scripted edits
  (truncate, delete-line, garbage-insert, clear), each analysis
  `catch_unwind`-isolated. Statically, there is **no `unwrap` / `expect` /
  `panic!` / index-panic / `debug_assert!` in the non-test LSP source** — the
  workspace `unwrap_used` lint (denied under CI's `-D warnings`) enforces it, and
  the totality leg proves it dynamically. ([[Language-Server]].)

### Grammar — clean-parse coverage

`tree-sitter-abc` runs `tree-sitter test` plus a corpus clean-parse gate:
**99.46%** of the 10k corpus parses with **no ERROR nodes** (9,946 / 10,000); the
categorized residual is backstopped by `croma-lsp` semantic tokens.
([[Editors-and-Zed]].)

## Efficiency: how speed is proven

Performance is measured with the **same rigour** as correctness, by **additive,
behaviour-preserving** harnesses — they measure, they never change product
output:

- **Criterion micro-benchmarks** — per-call timings on fixed in-process fixtures
  across three size buckets (small ≈ 20, avg ≈ 200, large ≈ 1000 lines).
  Criterion carries the statistics (warmup, sampling, outlier detection); the
  tables quote its **median** plus the **95 % CI**, and `Throughput::Bytes`
  reports MB/s directly.
- **Corpus-scale throughput** — end-to-end over the real 10k corpus,
  **in-process** (corpus held in memory) so the number reflects library
  throughput, not process-spawn overhead.
- **LSP latency distribution** — **p50 / p95 / p99** per request type per size
  bucket, **n = 100 samples per cell**. Leg E is a *gate*, not just a
  measurement: it asserts **p99 < 50 ms** on real-size inputs (with a documented
  150 ms backstop on the synthetic 1000-line stress bucket).
- **Grammar throughput** — `tree-sitter parse --time`, reported as an amortized
  steady-state rate (per-file setup dominates tiny inputs).

The result is committed as a **machine-stamped Markdown baseline** (Apple M4 Max,
Rust 1.96.0, `--release`, a pinned commit) — not the git-ignored
`target/criterion/` HTML. And efficiency findings follow a **measure-don't-fix**
rule: the recorded ABC→MusicXML export super-linearity is filed as a
low-priority backlog item (no real-use impact), **not** silently patched.
([[Benchmarks]].)

## Guardrails that keep the proof honest

- **`ABC_ROOT`-gated** corpus tests, with a **`>= 9000` file-count guard** that
  rejects a vacuous run from a mis-set path — a near-empty corpus can't fake a
  green gate.
- **Skips cleanly** with no corpus, so a contributor never needs the heavy suite
  to build or test croma.
- Every change also runs `cargo test --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo fmt --all --check`; **`unsafe` is forbidden** workspace-wide.
- **CI:** on every pull request the `gates` workflow clones croma-test and runs
  the full corpus matrix against the change, while the cargo-only `ci` workflow
  (and a dependency `audit`) run in parallel.

## Reproduce

The corpus ships with **croma-test** (sourced from Zenodo); point `ABC_ROOT` at
it. The in-process harnesses require an **absolute** path (`cargo test`'s cwd is
the crate dir).

```sh
# Formatter: idempotence + losslessness over 10k (in-process gate)
ABC_ROOT="$PWD/docs/untracked/corpus/zenodo-10k/abc" \
  cargo test -p croma-fmt --release corpus_proof -- --nocapture
# corpus formatter proof: 10000 files, 0 violations

# LSP: promotion legs A–E + totality
ABC_ROOT="$PWD/docs/untracked/corpus/zenodo-10k/abc" \
  cargo test -p croma-lsp --release -- --nocapture

# Corpus-scale throughput (parse / export / fmt)
ABC_ROOT="$(pwd)/docs/untracked/corpus/zenodo-10k/abc" \
  cargo test -p croma-fmt --release --test corpus_throughput -- --ignored --nocapture

# LSP latency distribution (p50/p95/p99)
cargo test -p croma-lsp --release lsp_leg_e_latency_distribution -- --nocapture
```

The complete matrix (writer parity, reader self-loop + foreign parity, grammar
coverage) runs from croma-test's provers — see its `README`. Per-capability
detail:
[`docs/formatter.md`](https://github.com/ro-ag/croma/blob/main/docs/formatter.md),
[`docs/lsp.md`](https://github.com/ro-ag/croma/blob/main/docs/lsp.md),
[`docs/musicxml-reader.md`](https://github.com/ro-ag/croma/blob/main/docs/musicxml-reader.md),
[`docs/benchmarks.md`](https://github.com/ro-ag/croma/blob/main/docs/benchmarks.md).

See also: [[How-its-Proven]] · [[Benchmarks]] · [[abc2xml-Comparison]] ·
[[Contributing]].
