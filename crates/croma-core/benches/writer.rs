//! criterion micro-benchmark for the forward **writer**
//! ([`croma_core::abc_to_musicxml`], ABC → MusicXML) over the same deterministic
//! size-bucketed fixtures as the parser bench (small ≈ 20 / avg ≈ 200 /
//! large ≈ 1000 lines).
//!
//! Throughput is reported in MB/s of *input ABC* via [`Throughput::Bytes`]; no
//! corpus is needed. Run a quick smoke pass with:
//!
//! ```sh
//! cargo bench -p croma-core --bench writer \
//!   -- --warm-up-time 0.5 --measurement-time 1 --sample-size 10
//! ```

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

use croma_core::abc_to_musicxml;

#[path = "common/fixtures.rs"]
mod fixtures;

fn bench_writer(c: &mut Criterion) {
    let mut group = c.benchmark_group("abc_to_musicxml");
    for &(label, body_lines) in fixtures::SIZES {
        let source = fixtures::fixture(body_lines);
        // Fail fast at setup if a fixture ever stops exporting cleanly, rather
        // than benchmarking an error path.
        abc_to_musicxml(&source).expect("fixture must export to MusicXML");
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(BenchmarkId::new("export", label), &source, |b, src| {
            b.iter(|| {
                let xml = abc_to_musicxml(black_box(src)).expect("fixture exports cleanly");
                black_box(xml);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_writer);
criterion_main!(benches);
