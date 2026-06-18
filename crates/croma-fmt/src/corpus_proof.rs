//! Corpus-scale proof of the formatter's two core invariants — **idempotence**
//! and **losslessness** — over the external 10k ABC corpus.
//!
//! This is the evidence that promotes `croma fmt` out of the gated tier: for
//! every real corpus file we assert that plain `format` is a lossless fixed
//! point and that `auto_fix` preserves the score and likewise settles to a fixed
//! point. It runs **in-process**, reusing the crate's own `engine`/`verify`, so
//! the full sweep takes seconds rather than 10k subprocess spawns.
//!
//! Env-gated: it runs only when `ABC_ROOT` points at the corpus directory, so a
//! normal `cargo test` (which has no corpus) skips it cleanly. Drive it with:
//!
//! ```sh
//! ABC_ROOT=docs/untracked/corpus/zenodo-10k/abc \
//!   cargo test -p croma-fmt --release corpus_proof -- --nocapture
//! ```
//!
//! croma-test's `tools/prove_fmt_lossless.py` is the complementary **black-box** proof: it
//! drives the built binary over the same corpus and writes a JSON report. The
//! two are independent (in-process gate reuse vs. binary + regex pitch-seq) and
//! must agree.

use std::fs;
use std::path::{Path, PathBuf};

use croma_core::ParseOptions;

use crate::verify::{musicxml_of, pitch_seq_of};
use crate::{FormatOptions, auto_fix, format};

/// The smallest corpus we accept as a non-vacuous proof. The zenodo set is
/// 10,000 files; this guards against a mis-set `ABC_ROOT` silently "passing" by
/// processing nothing.
const MIN_CORPUS_FILES: usize = 9_000;

/// Run all four invariant checks on one source, returning a human-readable
/// reason for each that fails (empty = the file upholds every invariant).
fn violations(source: &str) -> Vec<&'static str> {
    let opts = FormatOptions::default();
    let parse = ParseOptions::default();
    let mut out = Vec::new();

    // 1. Plain `format` is idempotent: format(format(x)) == format(x).
    let once = format(source, opts);
    let twice = format(&once, opts);
    if once != twice {
        out.push("plain format is not idempotent");
    }

    // 2. Plain `format` is lossless: it changes no rendered aspect, so the
    //    MusicXML is byte-identical. Files that do not lower (hard errors)
    //    render `None` on both sides and are inherently equal.
    if musicxml_of(source, parse) != musicxml_of(&once, parse) {
        out.push("plain format changed the rendered MusicXML");
    }

    let fixed = auto_fix(source, opts);

    // 3. `auto_fix` preserves the ordered pitch sequence (the lossless promise).
    //    Only assertable when the source itself lowers to a score; a source that
    //    does not lower has no pitch sequence to preserve, and the per-fix gates
    //    apply no curation in that case anyway.
    if let Some(before) = pitch_seq_of(source, parse)
        && pitch_seq_of(&fixed.output, parse).as_ref() != Some(&before)
    {
        out.push("auto_fix changed the pitch sequence");
    }

    // 4. `auto_fix` output is itself a `format` fixed point.
    if format(&fixed.output, opts) != fixed.output {
        out.push("auto_fix output is not a format fixed point");
    }

    out
}

/// Collect every `*.abc` file directly under `dir`.
fn abc_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read ABC_ROOT {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "abc"))
        .collect();
    files.sort();
    files
}

#[test]
fn fmt_is_idempotent_and_lossless_over_the_corpus() {
    let Ok(root) = std::env::var("ABC_ROOT") else {
        eprintln!("ABC_ROOT unset — skipping corpus-scale formatter proof");
        return;
    };
    let root = PathBuf::from(root);
    let files = abc_files(&root);

    let mut processed = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for path in &files {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let source = String::from_utf8_lossy(&bytes);
        processed += 1;
        for reason in violations(&source) {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            failures.push(format!("{name}: {reason}"));
        }
    }

    eprintln!(
        "corpus formatter proof: {processed} files, {} violations",
        failures.len()
    );
    assert!(
        processed >= MIN_CORPUS_FILES,
        "only {processed} .abc files under {} — expected >= {MIN_CORPUS_FILES}; is ABC_ROOT correct?",
        root.display(),
    );
    assert!(
        failures.is_empty(),
        "formatter invariant violations ({} total); first {}:\n{}",
        failures.len(),
        failures.len().min(25),
        failures
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
