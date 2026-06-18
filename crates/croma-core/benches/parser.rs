//! criterion micro-benchmark for the forward **parser**
//! ([`croma_core::parse_document`]) over deterministic, in-process,
//! size-bucketed ABC fixtures (small ≈ 20 / avg ≈ 200 / large ≈ 1000 lines).
//!
//! Throughput is reported in MB/s via [`Throughput::Bytes`]; no corpus is
//! needed. Run a quick smoke pass with:
//!
//! ```sh
//! cargo bench -p croma-core --bench parser \
//!   -- --warm-up-time 0.5 --measurement-time 1 --sample-size 10
//! ```

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

use croma_core::{ParseOptions, parse_document};

#[path = "common/fixtures.rs"]
mod fixtures;

fn bench_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_document");
    for &(label, body_lines) in fixtures::SIZES {
        let source = fixtures::fixture(body_lines);
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(BenchmarkId::new("parse", label), &source, |b, src| {
            b.iter(|| {
                let report = parse_document(black_box(src), ParseOptions::default());
                black_box(report);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parser);
criterion_main!(benches);
