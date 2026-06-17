//! Corpus-scale proof of the LSP analysis layer's **totality** (promotion-bar
//! leg C) over the external 10k ABC corpus.
//!
//! For every real corpus file we assert that the pure analysis layer is
//! panic-free and emits only in-bounds [`lsp_types::Range`]s — not only on the
//! pristine file, but across a scripted "type as you go" + malformed-mid-edit
//! sequence driven through the [`DocumentStore`]. Each file's whole sequence is
//! wrapped in [`std::panic::catch_unwind`] so a single panic is counted, not
//! fatal, and the gate asserts **0 panics**.
//!
//! It runs **in-process**, reusing the crate's own [`diagnostics`] and
//! [`DocumentStore`], so the full sweep takes seconds rather than 10k subprocess
//! spawns — mirroring `croma-fmt`'s `corpus_proof`. Env-gated on `ABC_ROOT`:
//!
//! ```sh
//! ABC_ROOT=docs/untracked/corpus/zenodo-10k/abc \
//!   cargo test -p croma-lsp --release -- --nocapture
//! ```
//!
//! `tools/prove_lsp_totality.py` is the complementary black-box wrapper that
//! runs this harness and parses its summary line.

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use croma_core::SourceText;
use lsp_types::{Position, Range, TextDocumentContentChangeEvent, Url};

use crate::position::PositionEncoding;
use crate::{DocumentStore, diagnostics};

/// The smallest corpus we accept as a non-vacuous proof; guards a mis-set
/// `ABC_ROOT` from silently "passing" by processing nothing.
const MIN_CORPUS_FILES: usize = 9_000;

/// Assert every diagnostic range is in-bounds for `text`. Returns a reason on the
/// first violation, else `None`. "In-bounds" = both endpoints land on a line
/// that exists and a character no wider than that line under the encoding, and
/// `start <= end`.
fn ranges_in_bounds(text: &str, encoding: PositionEncoding) -> Option<String> {
    let diags = diagnostics(text, encoding);
    let source = SourceText::new(text);
    let line_count = source.line_count();
    for d in &diags {
        let r = d.range;
        if !position_in_bounds(&source, r.start, encoding, line_count) {
            return Some(format!("start {:?} out of bounds", r.start));
        }
        if !position_in_bounds(&source, r.end, encoding, line_count) {
            return Some(format!("end {:?} out of bounds", r.end));
        }
        if (r.start.line, r.start.character) > (r.end.line, r.end.character) {
            return Some(format!("range not well-formed: {r:?}"));
        }
    }
    None
}

/// Whether `pos` addresses a real line and a character within that line's width
/// (inclusive of the end, where an edit can land on the line break).
fn position_in_bounds(
    source: &SourceText,
    pos: Position,
    encoding: PositionEncoding,
    line_count: usize,
) -> bool {
    let line_index = pos.line as usize;
    // An empty document has line_count 1 ("" line); a position on line 0 is fine.
    if line_index >= line_count.max(1) {
        return false;
    }
    let Some(line) = source.line(line_index) else {
        return line_index == 0 && pos.character == 0;
    };
    let text = source.as_str();
    let slice = text.get(line.start()..line.end()).unwrap_or("");
    let width: usize = match encoding {
        PositionEncoding::Utf8 => slice.len(),
        PositionEncoding::Utf16 => slice.chars().map(char::len_utf16).sum(),
    };
    (pos.character as usize) <= width
}

/// A full-document content change (no range) carrying `text`.
fn full_change(text: &str) -> TextDocumentContentChangeEvent {
    TextDocumentContentChangeEvent {
        range: None,
        range_length: None,
        text: text.to_string(),
    }
}

/// A ranged content change.
fn ranged_change(start: Position, end: Position, text: &str) -> TextDocumentContentChangeEvent {
    TextDocumentContentChangeEvent {
        range: Some(Range { start, end }),
        range_length: None,
        text: text.to_string(),
    }
}

/// Run the scripted edit sequence for one source and assert in-bounds ranges
/// after every state. Returns a reason on the first failure, else `None`.
///
/// The sequence simulates real editing plus deliberately hostile mid-edit
/// states: a safe mid-point truncation, a middle-line deletion, an unbalanced
/// `"[[[\n"` insertion, and a clear-to-empty.
fn scripted_sequence(uri: &Url, source: &str, encoding: PositionEncoding) -> Option<String> {
    // State 0: the pristine file as opened.
    let mut store = DocumentStore::new();
    store.open(uri.clone(), source.to_string());
    if let Some(text) = store.get(uri)
        && let Some(reason) = ranges_in_bounds(text, encoding)
    {
        return Some(format!("pristine: {reason}"));
    }

    // State 1: truncate to a safe mid-point char boundary (mid-edit buffer).
    let mid = safe_midpoint(source);
    let truncated = source.get(..mid).unwrap_or("").to_string();
    store.change(uri, vec![full_change(&truncated)], encoding);
    if let Some(text) = store.get(uri)
        && let Some(reason) = ranges_in_bounds(text, encoding)
    {
        return Some(format!("truncated: {reason}"));
    }

    // State 2: restore full text, then delete a middle line via a ranged edit.
    store.change(uri, vec![full_change(source)], encoding);
    if let Some(reason) = delete_middle_line(&mut store, uri, source, encoding) {
        return Some(reason);
    }

    // State 3: insert an unbalanced bracket run at the very start.
    store.change(
        uri,
        vec![ranged_change(
            Position {
                line: 0,
                character: 0,
            },
            Position {
                line: 0,
                character: 0,
            },
            "[[[\n",
        )],
        encoding,
    );
    if let Some(text) = store.get(uri)
        && let Some(reason) = ranges_in_bounds(text, encoding)
    {
        return Some(format!("bracket-insert: {reason}"));
    }

    // State 4: clear to empty.
    store.change(uri, vec![full_change("")], encoding);
    if let Some(text) = store.get(uri)
        && let Some(reason) = ranges_in_bounds(text, encoding)
    {
        return Some(format!("cleared: {reason}"));
    }

    // State 5: "type as you go" a minimal tune from empty, incrementally.
    let keystrokes = ["X", ":", "1", "\n", "K", ":", "C", "\n", "C", "D", "E", "F"];
    let mut col = 0u32;
    let mut line = 0u32;
    for key in keystrokes {
        store.change(
            uri,
            vec![ranged_change(
                Position {
                    line,
                    character: col,
                },
                Position {
                    line,
                    character: col,
                },
                key,
            )],
            encoding,
        );
        if key == "\n" {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        if let Some(text) = store.get(uri)
            && let Some(reason) = ranges_in_bounds(text, encoding)
        {
            return Some(format!("typing: {reason}"));
        }
    }

    None
}

/// Restore-and-delete a middle source line through a ranged edit, asserting the
/// result is in-bounds.
fn delete_middle_line(
    store: &mut DocumentStore,
    uri: &Url,
    source: &str,
    encoding: PositionEncoding,
) -> Option<String> {
    let source_text = SourceText::new(source);
    let lines = source_text.line_count();
    if lines >= 3 {
        let target = lines / 2;
        let start = Position {
            line: target as u32,
            character: 0,
        };
        let end = Position {
            line: (target + 1) as u32,
            character: 0,
        };
        store.change(uri, vec![ranged_change(start, end, "")], encoding);
    }
    store
        .get(uri)
        .and_then(|text| ranges_in_bounds(text, encoding))
        .map(|reason| format!("delete-line: {reason}"))
}

/// The largest char boundary at or below half the source length.
fn safe_midpoint(source: &str) -> usize {
    let mut mid = source.len() / 2;
    while mid > 0 && !source.is_char_boundary(mid) {
        mid -= 1;
    }
    mid
}

/// Collect every `*.abc` file directly under `dir`.
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

#[test]
fn lsp_analysis_is_total_over_the_corpus() {
    let Ok(root) = std::env::var("ABC_ROOT") else {
        eprintln!("ABC_ROOT unset — skipping corpus-scale LSP totality proof");
        return;
    };
    let root = PathBuf::from(root);
    let files = abc_files(&root);

    let mut processed = 0usize;
    let mut panics = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in &files {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let source = String::from_utf8_lossy(&bytes).into_owned();
        processed += 1;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let uri = Url::parse(&format!("file:///{name}"))
            .unwrap_or_else(|_| Url::parse("file:///tune.abc").expect("fallback uri is valid"));

        // Exercise both encodings; the whole sequence is panic-isolated.
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            for enc in [PositionEncoding::Utf8, PositionEncoding::Utf16] {
                if let Some(reason) = scripted_sequence(&uri, &source, enc) {
                    return Some(format!("[{enc:?}] {reason}"));
                }
            }
            None
        }));
        match outcome {
            Ok(Some(reason)) => failures.push(format!("{name}: {reason}")),
            Ok(None) => {}
            Err(_) => {
                panics += 1;
                failures.push(format!("{name}: PANIC"));
            }
        }
    }

    // The summary line parsed by tools/prove_lsp_totality.py. Keep the format
    // stable: "lsp totality: N files, P panics".
    eprintln!("lsp totality: {processed} files, {panics} panics");
    if !failures.is_empty() {
        eprintln!(
            "first {} failures:\n{}",
            failures.len().min(25),
            failures
                .iter()
                .take(25)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    assert!(
        processed >= MIN_CORPUS_FILES,
        "only {processed} .abc files under {} — expected >= {MIN_CORPUS_FILES}; is ABC_ROOT correct?",
        root.display(),
    );
    assert_eq!(panics, 0, "{panics} files panicked during analysis");
    assert!(
        failures.is_empty(),
        "{} totality failures (incl. panics); see the list above",
        failures.len(),
    );
}
