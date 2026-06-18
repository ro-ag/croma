//! criterion micro-benchmark for the **reader**
//! ([`croma_core::read_musicxml`], MusicXML → `Score`) over the same
//! deterministic size buckets (small ≈ 20 / avg ≈ 200 / large ≈ 1000 lines).
//!
//! The reader is gated behind the `musicxml-reader` cargo feature (it pulls
//! `roxmltree`), so this whole bench target carries
//! `required-features = ["musicxml-reader"]` and is skipped by a plain
//! `cargo bench`. The input MusicXML is produced **once at setup time** by
//! running the forward writer on each ABC fixture, so the timed loop measures
//! only the reader. Throughput is reported in MB/s of *input XML*.
//!
//! Run a quick smoke pass with:
//!
//! ```sh
//! cargo bench -p croma-core --bench reader --features musicxml-reader \
//!   -- --warm-up-time 0.5 --measurement-time 1 --sample-size 10
//! ```

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

use croma_core::{abc_to_musicxml, read_musicxml};

#[path = "common/fixtures.rs"]
mod fixtures;

fn bench_reader(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_musicxml");
    for &(label, body_lines) in fixtures::SIZES {
        let source = fixtures::fixture(body_lines);
        // Produce the reader's input once, outside the timed loop.
        let xml = abc_to_musicxml(&source).expect("fixture must export to MusicXML");
        group.throughput(Throughput::Bytes(xml.len() as u64));
        group.bench_with_input(BenchmarkId::new("read", label), &xml, |b, xml| {
            b.iter(|| {
                let report = read_musicxml(black_box(xml));
                black_box(report);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_reader);
criterion_main!(benches);
