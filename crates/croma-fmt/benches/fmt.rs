//! criterion micro-benchmarks for the **formatter** ([`croma_fmt::format`]) and
//! the **auto-fixer** ([`croma_fmt::auto_fix`]) over deterministic, in-process
//! size-bucketed ABC fixtures (small ≈ 20 / avg ≈ 200 / large ≈ 1000 lines).
//!
//! Two benchmark groups — `format` and `auto_fix` — each reporting MB/s of input
//! ABC via [`Throughput::Bytes`]; no corpus is needed. Run a quick smoke pass
//! with:
//!
//! ```sh
//! cargo bench -p croma-fmt --bench fmt \
//!   -- --warm-up-time 0.5 --measurement-time 1 --sample-size 10
//! ```

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

use croma_fmt::{FormatOptions, auto_fix, format};

#[path = "common/fixtures.rs"]
mod fixtures;

fn bench_format(c: &mut Criterion) {
    let mut group = c.benchmark_group("format");
    for &(label, body_lines) in fixtures::SIZES {
        let source = fixtures::fixture(body_lines);
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(BenchmarkId::new("format", label), &source, |b, src| {
            b.iter(|| {
                let out = format(black_box(src), FormatOptions::default());
                black_box(out);
            });
        });
    }
    group.finish();
}

fn bench_auto_fix(c: &mut Criterion) {
    let mut group = c.benchmark_group("auto_fix");
    for &(label, body_lines) in fixtures::SIZES {
        let source = fixtures::fixture(body_lines);
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(BenchmarkId::new("auto_fix", label), &source, |b, src| {
            b.iter(|| {
                let result = auto_fix(black_box(src), FormatOptions::default());
                black_box(result);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_format, bench_auto_fix);
criterion_main!(benches);
