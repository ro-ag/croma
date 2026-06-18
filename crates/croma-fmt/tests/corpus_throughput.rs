//! Corpus-scale **throughput** harness for the three forward paths — parse,
//! export (ABC→MusicXML), and fmt — over the external 10k ABC corpus. It reports
//! **files/s** and **MB/s** so the benchmark suite has a real end-to-end library
//! throughput number to sit beside criterion's per-call micro-benchmarks.
//!
//! It runs **in-process** (one process, the corpus held in memory) so the number
//! reflects library throughput, not 10k process spawns. Like the `corpus_proof`
//! gates it is `ABC_ROOT`-gated and additionally `#[ignore]`d, so a plain
//! `cargo test --workspace` skips the slow loop entirely; it runs only when asked
//! for explicitly:
//!
//! ```sh
//! ABC_ROOT=/abs/path/to/zenodo-10k/abc \
//!   cargo test -p croma-fmt --release --test corpus_throughput -- --ignored --nocapture
//! ```
//!
//! `tools/bench_corpus_throughput.py` is the thin wrapper that sets `ABC_ROOT`
//! (absolute), runs this in release, and parses the `bench corpus …` summary
//! lines. LOCAL ONLY — the corpus is external (provision it per AGENTS.md).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use croma_core::{ParseOptions, abc_to_musicxml, parse_document};
use croma_fmt::{FormatOptions, format};

/// The smallest corpus we accept as a non-vacuous measurement; mirrors the
/// `corpus_proof` gates so a mis-set `ABC_ROOT` cannot silently "pass" by
/// timing nothing.
const MIN_CORPUS_FILES: usize = 9_000;

/// Collect every `*.abc` file directly under `dir` (sorted for determinism).
/// Mirrors `croma-lsp`'s `corpus_proof::abc_files` — no `.unwrap()`, errors are
/// reported and treated as an empty set so the non-vacuity guard fires.
fn abc_files(dir: &Path) -> Vec<PathBuf> {
    let read = match fs::read_dir(dir) {
        Ok(read) => read,
        Err(error) => {
            eprintln!("cannot read ABC_ROOT {}: {error}", dir.display());
            return Vec::new();
        }
    };
    let mut files: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "abc"))
        .collect();
    files.sort();
    files
}

/// Print a stable throughput summary line for one path. Format (parsed by
/// `tools/bench_corpus_throughput.py`):
///
/// ```text
/// bench corpus <path>: <files> files, <MB> MB total, <s> s, <files/s> files/s, <MB/s> MB/s
/// ```
///
/// `errors` is appended only when non-zero (the corpus is known-good, so the
/// happy path stays clean) — e.g. `… MB/s, 3 errors`.
fn report(path: &str, files: usize, bytes: usize, secs: f64, errors: usize) {
    let mb = bytes as f64 / 1_000_000.0;
    let files_per_s = if secs > 0.0 { files as f64 / secs } else { 0.0 };
    let mb_per_s = if secs > 0.0 { mb / secs } else { 0.0 };
    let tail = if errors > 0 {
        format!(", {errors} errors")
    } else {
        String::new()
    };
    eprintln!(
        "bench corpus {path}: {files} files, {mb:.1} MB total, {secs:.2} s, {files_per_s:.0} files/s, {mb_per_s:.1} MB/s{tail}"
    );
}

#[ignore = "slow corpus loop; run explicitly with --ignored and ABC_ROOT set"]
#[test]
fn corpus_throughput_parse_export_fmt() {
    // ABC_ROOT-gated: skip cleanly (even under --ignored) when the external
    // corpus is not provisioned, exactly like the `corpus_proof` gates.
    let Ok(root) = std::env::var("ABC_ROOT") else {
        eprintln!("bench corpus: skipped (ABC_ROOT unset)");
        return;
    };
    let root = PathBuf::from(root);
    let files = abc_files(&root);

    // Load the whole corpus into memory once (this read is the warm-up: the OS
    // page cache is primed and every source is owned before any path is timed).
    let mut sources: Vec<String> = Vec::with_capacity(files.len());
    let mut total_bytes = 0usize;
    for path in &files {
        let Ok(bytes) = fs::read(path) else { continue };
        total_bytes += bytes.len();
        sources.push(String::from_utf8_lossy(&bytes).into_owned());
    }
    let n = sources.len();

    assert!(
        n >= MIN_CORPUS_FILES,
        "only {n} .abc files under {} — expected >= {MIN_CORPUS_FILES}; is ABC_ROOT correct?",
        root.display(),
    );

    eprintln!(
        "bench corpus: {n} files, {:.1} MB total loaded from {}",
        total_bytes as f64 / 1_000_000.0,
        root.display(),
    );

    // parse: croma_core::parse_document(src, ParseOptions::default()).
    let parse_opts = ParseOptions::default();
    let start = Instant::now();
    for src in &sources {
        let parsed = parse_document(src, parse_opts);
        std::hint::black_box(&parsed);
    }
    let parse_secs = start.elapsed().as_secs_f64();
    report("parse", n, total_bytes, parse_secs, 0);

    // export: croma_core::abc_to_musicxml(src) (ABC→MusicXML). The corpus is
    // known-good, but a stray hard error must not abort the timed loop, so we
    // count Err and report it rather than `.expect`-ing every file.
    let mut export_errors = 0usize;
    let start = Instant::now();
    for src in &sources {
        match abc_to_musicxml(src) {
            Ok(xml) => {
                std::hint::black_box(xml);
            }
            Err(_) => export_errors += 1,
        }
    }
    let export_secs = start.elapsed().as_secs_f64();
    report("export", n, total_bytes, export_secs, export_errors);

    // fmt: croma_fmt::format(src, FormatOptions::default()).
    let fmt_opts = FormatOptions::default();
    let start = Instant::now();
    for src in &sources {
        let out = format(src, fmt_opts);
        std::hint::black_box(out);
    }
    let fmt_secs = start.elapsed().as_secs_f64();
    report("fmt", n, total_bytes, fmt_secs, 0);
}
