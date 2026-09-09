# Benchmarks

croma ships a committed, reproducible **performance** baseline — a
machine-stamped snapshot of how fast the four shipped capabilities and the
`tree-sitter-abc` grammar run. (Correctness is proven separately —
[[How-its-Proven]].) How these numbers are measured — criterion statistics,
corpus throughput, the n = 100 LSP percentile harness, measure-don't-fix — is in
[[Testing-Methodology#efficiency-how-speed-is-proven]].

## Headline throughput / latency

Apple M4 Max, Rust 1.96.0, `--release`:

| Layer | Headline |
| --- | --- |
| Parser (corpus, in-process) | **43,247 files/s** · 23.9 MB/s |
| Formatter (corpus, in-process) | **27,450 files/s** · 15.2 MB/s |
| ABC → MusicXML writer (corpus) | **7,081 files/s** · 3.9 MB/s |
| LSP diagnostics, real-size p99 | **≤ 4.76 ms** (release ceiling 50 ms) |
| LSP semantic tokens, real-size p99 | **≤ 0.62 ms** |
| `tree-sitter-abc` (steady state) | **~8.5–8.9 MB/s** |

The corpus median file is **14 lines** (max **244**); at those sizes every
operation is in the low-millisecond range.

## Recording context

The numbers are from a single deliberate `--release` run on the recording
machine — **Apple M4 Max (16 cores), 64 GB, macOS 26.5.1, Rust 1.96.0** (pinned)
— at commit `cb9c099`. The benchmark harnesses are additive and
behaviour-preserving: they measure, they never change product output. Criterion
carries the micro-benchmark statistics (warmup, sampling, outlier detection);
the LSP percentile harness carries the latency distribution.

## A recorded finding (measure-don't-fix)

The **ABC → MusicXML export path** is **super-linear** on synthetic large input:
~5× the input → ~13× the time (~2.1 MiB/s at avg ≈ 200 lines vs ~0.84 MiB/s at
≈ 1000 lines). The parser, formatter, and reader stay flat across the same sweep,
so the effect is localised to the export stage.

**Impact: none for real use.** The corpus maximum is 244 lines, where export
stays a few ms; the slope only appears on the synthetic 1000-line stress bucket
(4× the largest real file), and even there it sits under the documented 150 ms
LSP backstop. Per the benchmark epic's measure-don't-fix rule, it is recorded as
a low-priority perf-backlog item, not fixed — there is no correctness
implication.

## Full reference & reproduction

Per-call criterion micro-benchmarks, corpus-scale throughput, the full LSP
p50/p95/p99 latency table, grammar throughput, and exact reproduction commands
are in
[**`docs/benchmarks.md`**](https://github.com/ro-ag/croma/blob/main/docs/benchmarks.md).

```sh
# Micro-benchmarks (criterion); --release matters
cargo bench -p croma-core --bench parser
cargo bench -p croma-core --bench writer
cargo bench -p croma-core --bench reader --features musicxml-reader   # feature-gated
cargo bench -p croma-fmt  --bench fmt
```
