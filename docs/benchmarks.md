# croma performance baseline

This is croma's committed **performance** baseline — a machine-stamped, reproducible
snapshot of how fast the four shipped capabilities (forward writer, `croma fmt`,
MusicXML→ABC reader, `croma-lsp`) and the `tree-sitter-abc` grammar run. croma's
*correctness* is proven elsewhere (corpus parity, LSP legs A–E, grammar
`tree-sitter test`); this document is purely about throughput and latency.

It is the citable artifact for the benchmark epic (decisions in
[`docs/superpowers/specs/2026-06-17-benchmark-suite-decisions.md`](superpowers/specs/2026-06-17-benchmark-suite-decisions.md)).
The benchmark *harnesses* are additive and behavior-preserving — they measure, they
never change product output. The numbers below are from a single deliberate
`--release` run on the recording machine; **criterion carries the statistics**
(warmup, sampling, outlier detection — the micro-benchmark tables quote its median
plus the low/high of its 95 % confidence interval), and the LSP percentile harness
carries the latency distribution (n = 100 samples per cell). We commit this report,
not the git-ignored `target/criterion/` HTML.

## Recording machine

| | |
| --- | --- |
| **CPU** | Apple M4 Max (16 cores) |
| **RAM** | 64 GB |
| **OS** | macOS 26.5.1 (Darwin 25.5.0, arm64) |
| **Toolchain** | Rust 1.96.0 (pinned by `rust-toolchain.toml`) |
| **Commit** | `cb9c099` (current HEAD; this report adds no code, so every number reflects this code state) |
| **Build** | `--release` |

**Corpus** (used by the corpus-scale and grammar layers): `zenodo-10k` —
10,000 ABC files, ~5.5 MB total (5,533,070 bytes). Per-file line counts:
min 4 / median 14 / p90 34 / p99 80 / **max 244** (`tune_013458.abc`, 4,665 bytes).
The micro-benchmarks need **no** corpus: their fixtures are synthesized in-process.

---

## 1. Micro-benchmarks (criterion, fixed fixtures, no corpus)

Statistically-robust per-call timings on deterministic in-process fixtures
(`crates/croma-core/benches/common/fixtures.rs`): a fixed 7-line ABC header plus a
cycled representative body (notes, chords, grace groups, decorations, tuplets, chord
symbols, barlines, broken rhythm, rests, octave marks, accidentals). Three size
buckets — **small ≈ 20 lines**, **avg ≈ 200 lines**, **large ≈ 1000 lines**. Each
fixture parses with zero errors and exports cleanly, so every target gets real
input. `Throughput::Bytes(len)` makes criterion report MB/s directly. Times are the
criterion **median**; the bracket is the 95 % CI [low … high]. MB/s is at the median.

### Parser — `parse_document(src, ParseOptions::default())` (croma-core)

| size | median time | 95 % CI | throughput |
| --- | --- | --- | --- |
| small | 27.0 µs | [26.9 … 27.2 µs] | 23.8 MiB/s |
| avg | 348.2 µs | [345.9 … 350.4 µs] | 25.4 MiB/s |
| large | 1.796 ms | [1.792 … 1.799 ms] | 25.2 MiB/s |

### Forward writer — `export_musicxml(src)` ABC→MusicXML (croma-core)

| size | median time | 95 % CI | throughput |
| --- | --- | --- | --- |
| small | 203.0 µs | [202.4 … 203.7 µs] | 3.17 MiB/s |
| avg | 4.105 ms | [4.081 … 4.132 ms] | 2.15 MiB/s |
| large | 54.10 ms | [53.98 … 54.23 ms] | 0.84 MiB/s (856.8 KiB/s) |

### Reader — `read_musicxml(xml)` MusicXML→Score (croma-core, `musicxml-reader` feature)

| size | median time | 95 % CI | throughput |
| --- | --- | --- | --- |
| small | 234.5 µs | [233.7 … 235.2 µs] | 217.0 MiB/s |
| avg | 3.516 ms | [3.503 … 3.529 ms] | 210.7 MiB/s |
| large | 21.08 ms | [21.01 … 21.15 ms] | 180.7 MiB/s |

> **Reading the reader's MB/s.** The reader's input is **MusicXML**, i.e. the
> *writer's output*, which is ~80× larger than the ABC it came from. So its MB/s is
> measured over XML-input bytes and is **not** comparable to the ABC-input MB/s of
> the other targets. Compare **per-call time** across targets, not MB/s: e.g. on the
> avg fixture the writer (ABC→XML) takes 4.1 ms and the reader (XML→Score) 3.5 ms.

### Formatter — `format(src, FormatOptions::default())` (croma-fmt)

| size | median time | 95 % CI | throughput |
| --- | --- | --- | --- |
| small | 46.1 µs | [45.9 … 46.3 µs] | 13.9 MiB/s |
| avg | 612.7 µs | [610.9 … 614.6 µs] | 14.4 MiB/s |
| large | 3.300 ms | [3.289 … 3.311 ms] | 13.7 MiB/s |

### Auto-fixer — `auto_fix(src, FormatOptions::default())` (croma-fmt)

| size | median time | 95 % CI | throughput |
| --- | --- | --- | --- |
| small | 221.3 µs | [220.7 … 221.9 µs] | 2.90 MiB/s |
| avg | 2.921 ms | [2.913 … 2.928 ms] | 3.03 MiB/s |
| large | 18.06 ms | [18.00 … 18.11 ms] | 2.51 MiB/s |

**Per-call snapshot, avg (~200-line) fixture:** parse 348 µs · writer (ABC→XML)
4.1 ms · reader (XML→Score) 3.5 ms · format 613 µs · auto_fix 2.9 ms.

Parser, formatter and reader hold a flat MB/s across sizes (linear in input). The
writer and auto_fix slope down as input grows — see
[§6](#6-finding-diagnostics--export-super-linearity).

---

## 2. Corpus-scale throughput (in-process, `ABC_ROOT`-gated)

End-to-end throughput over the **real 10k corpus**, in-process (one process, corpus
held in memory) so the number reflects library throughput, not process-spawn
overhead. Harness: `crates/croma-fmt/tests/corpus_throughput.rs`, driven by
croma-test's `tools/bench_corpus_throughput.py`; it skips cleanly when `ABC_ROOT`
is unset and asserts ≥ 9,000 files when set.

| path | entry point | files | wall | files/s | MB/s |
| --- | --- | --- | --- | --- | --- |
| parse | `parse_document` | 10,000 | 0.23 s | **43,247** | **23.9** |
| export | `export_musicxml` (ABC→XML) | 10,000 | 1.41 s | **7,081** | **3.9** |
| fmt | `format` | 10,000 | 0.36 s | **27,450** | **15.2** |

These agree with the per-call micro-benchmarks: corpus parse ≈ 24 MB/s matches the
fixture parser, corpus fmt ≈ 15 MB/s matches the formatter, and corpus export
≈ 3.9 MB/s sits between the writer's small/avg fixtures (the corpus skews small —
median 14 lines — so export amortizes near its fast end).

---

## 3. LSP latency distribution (p50/p95/p99)

`croma-lsp` is a thin adapter over croma-core/croma-fmt. Leg E of its corpus-proof
harness (`crates/croma-lsp/src/corpus_proof.rs`,
`lsp_leg_e_latency_distribution`) times each request type against each size bucket,
n = 100 samples per cell (UTF-8). The avg subject is the synthetic 200-line
document; small ≈ 20 lines, large ≈ 1000 lines. All values in **milliseconds**.

| request | small p50/p95/p99 | avg p50/p95/p99 | large p50/p95/p99 |
| --- | --- | --- | --- |
| diagnostics | 0.23 / 0.25 / 0.31 | 4.47 / 4.69 / 4.76 | 59.44 / 62.37 / 62.77 |
| semantic_tokens | 0.04 / 0.05 / 0.05 | 0.56 / 0.60 / 0.62 | 2.99 / 3.17 / 3.27 |
| formatting | 0.05 / 0.06 / 0.08 | 0.64 / 0.71 / 0.76 | 3.41 / 3.60 / 3.67 |
| hover | 0.03 / 0.03 / 0.03 | 0.38 / 0.45 / 0.52 | 2.08 / 2.24 / 2.31 |
| completion | 0.01 / 0.01 / 0.01 | 0.01 / 0.01 / 0.01 | 0.03 / 0.03 / 0.03 |
| code_action | 0.23 / 0.25 / 0.29 | 3.00 / 3.19 / 3.23 | 18.36 / 18.99 / 19.22 |

**Representative bar (what musicians actually edit).** On small **and** avg inputs,
every request — including the two heaviest, `diagnostics` and `semantic_tokens` —
clears the leg-E release ceiling of **p99 < 50 ms** with a wide margin: the worst
real-size cell is diagnostics @ avg = **4.76 ms p99**, ~10× under the bar. The leg-E
gate asserts this ceiling on small+avg and is retained, so leg E stays a gate, not
just a measurement.

**Large is a synthetic stress bucket.** 1000 lines is **4× the 244-line maximum real
corpus file**; no real input reaches it. Here `diagnostics` measures **62.77 ms
p99** because it runs the full ABC→MusicXML export (see
[§6](#6-finding-diagnostics--export-super-linearity)); this is over the 50 ms *real-size*
ceiling but well under the documented **150 ms backstop** for the stress bucket, and
every other request stays in the single-to-low-double-digit ms range. `completion` is
effectively constant (≤ 0.03 ms) across all sizes.

---

## 4. Grammar throughput (`tree-sitter-abc`)

`tree-sitter parse --time` reports `Parse: <ms>  <bytes/ms>` per file. **bytes/ms ==
KB/s**, so MB/s = bytes/ms ÷ 1000. Runs are from `tree-sitter-abc/` (the CLI
auto-detects the local generated grammar; the "you have not configured any parser
directories" warning is benign). tree-sitter CLI 0.26.9.

**Individual real corpus files** (per-file setup dominates small inputs):

| file | size | parse | throughput |
| --- | --- | --- | --- |
| `tune_001012.abc` (4 lines) | 31 B | 0.02 ms | ~1,300 bytes/ms (~1.3 MB/s) |
| `tune_010875.abc` (14 lines, median) | 314 B | 0.07 ms | ~4,300 bytes/ms (~4.3 MB/s) |
| `tune_013458.abc` (244 lines, **largest real**) | 4,665 B | 0.57 ms | ~8,300 bytes/ms (~8.3 MB/s) |

**Amortized steady state (clean headline).** A 419,940-byte input built by repeating
the largest clean real file (parses with **zero ERROR nodes**) measures
**~8,500–8,940 bytes/ms ≈ 8.5–8.9 MB/s** steady-state — the honest per-byte rate
once fixed per-file setup is amortized away.

> *Aside:* a 445,717-byte concatenation of 400 diverse corpus files measures higher
> (~17,000 bytes/ms ≈ 17 MB/s) but is **not** clean — gluing independent tunes
> end-to-end introduces a handful of boundary ERROR nodes. It is recorded only as a
> rough upper bound; the clean single-file-repeat figure above is the headline.

---

## 5. Summary

| layer | headline |
| --- | --- |
| parser (micro, avg) | 348 µs · 25.4 MiB/s |
| writer ABC→XML (micro, avg) | 4.1 ms · 2.15 MiB/s |
| reader XML→Score (micro, avg) | 3.5 ms · 210.7 MiB/s (XML-input bytes) |
| formatter (micro, avg) | 613 µs · 14.4 MiB/s |
| auto_fixer (micro, avg) | 2.9 ms · 3.03 MiB/s |
| corpus parse (10k, in-proc) | 43,247 files/s · 23.9 MB/s |
| corpus export (10k, in-proc) | 7,081 files/s · 3.9 MB/s |
| corpus fmt (10k, in-proc) | 27,450 files/s · 15.2 MB/s |
| LSP diagnostics p99 (real-size: small/avg) | 0.31 / 4.76 ms |
| LSP semantic_tokens p99 (real-size: small/avg) | 0.05 / 0.62 ms |
| grammar (clean steady state) | ~8.5–8.9 MB/s |

---

## 6. Finding: diagnostics / export super-linearity

The **ABC→MusicXML export path** — exercised by the forward writer, and inside the
LSP `diagnostics` request (`analyze_document`) — is **super-linear** in input size:

| input | export-bearing measurement | rate |
| --- | --- | --- |
| avg ≈ 200 lines | writer 4.1 ms · LSP diagnostics 4.76 ms p99 | ~2.1 MiB/s |
| large ≈ 1000 lines | writer 54.1 ms · LSP diagnostics 62.77 ms p99 | ~0.84 MiB/s |

~5× the input → ~13× the time (and auto_fix, which formats then re-exports, shows the
same downward slope). The parser, formatter, and reader stay **flat** in MB/s across
the same size sweep, so the non-linearity is localized to the MusicXML export stage,
not parsing or formatting.

**Impact: none for real use.** The corpus maximum is 244 lines, where export stays a
few ms (largest real file ≈ a few ms; avg diagnostics 4.76 ms p99). The effect only
appears on the synthetic 1000-line stress bucket (4× the largest real file), and even
there it sits under the 150 ms LSP backstop. Per the epic's **measure-don't-fix**
rule, this is **recorded as a low-priority perf-backlog item**, not fixed here. A
future optimization of the export emitter (e.g. reducing per-element allocation or
string growth) would flatten it; there is no correctness implication.

---

## 7. Reproduce

All commands run from the repo root unless noted. `--release` matters — debug numbers
are several× slower and unrepresentative.

**Micro-benchmarks (criterion).** Default settings reproduce these; `--measurement-time 3`
bounds wall time without dropping to a smoke sample size. Read `time:` and `thrpt:`
from criterion's stdout.

```sh
cargo bench -p croma-core --bench parser
cargo bench -p croma-core --bench writer
cargo bench -p croma-core --bench reader --features musicxml-reader   # reader is feature-gated
cargo bench -p croma-fmt  --bench fmt
```

The reader bench carries `required-features = ["musicxml-reader"]`; a plain
`cargo bench` (no features) skips it, preserving the zero-dep default build.

**Corpus-scale throughput.** Point `ABC_ROOT` at the corpus; the wrapper resolves it
to an **absolute** path (the in-process harness requires absolute, because
`cargo test` runs with cwd = the crate dir). From the croma-test repo:

```sh
uv run python tools/bench_corpus_throughput.py \
  --abc-root docs/untracked/corpus/zenodo-10k/abc
```

Equivalent direct invocation (from the croma repo):

```sh
ABC_ROOT="$(pwd)/docs/untracked/corpus/zenodo-10k/abc" \
  cargo test -p croma-fmt --release --test corpus_throughput -- --ignored --nocapture
```

**LSP latency distribution.**

```sh
cargo test -p croma-lsp --release lsp_leg_e_latency_distribution -- --nocapture
```

**Grammar throughput.** Run from the grammar directory (it auto-detects the local
generated parser):

```sh
cd tree-sitter-abc
npx tree-sitter parse --quiet --time <path-to-abc-file>
```

For an amortized steady-state number, parse a single large input (e.g. one of the
larger corpus files, or many copies of one concatenated into a temp file) rather than
a tiny file, since per-file setup dominates small inputs.

> **Not committed:** `target/criterion/` (the HTML report and raw samples) is
> git-ignored and intentionally **not** part of the repo. This Markdown file is the
> committed baseline. Re-run any layer above to regenerate the live numbers.
