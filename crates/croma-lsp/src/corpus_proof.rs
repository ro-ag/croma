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

use croma_core::{Diagnostic, SourceText, Span};
use croma_fmt::{FormatOptions, format};
use lsp_types::{Position, Range, SemanticToken, TextDocumentContentChangeEvent, Url};

use crate::position::{PositionEncoding, position_to_byte};
use crate::{
    DocumentStore, analyze_document, code_actions, completion, diagnostics, document_symbols,
    folding_ranges, formatting, hover, semantic_tokens,
};

/// The smallest corpus we accept as a non-vacuous proof; guards a mis-set
/// `ABC_ROOT` from silently "passing" by processing nothing.
const MIN_CORPUS_FILES: usize = 9_000;

/// Assert every diagnostic range is in-bounds for `text`, AND that every R2
/// analysis function runs to completion on `text` (leg C, extended to the new
/// request handlers). Returns a reason on the first violation, else `None`.
/// "In-bounds" = both endpoints land on a line that exists and a character no
/// wider than that line under the encoding, and `start <= end`.
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

    // Exercise the R2 request handlers' pure cores so the totality sweep also
    // covers formatting / semantic tokens / symbols / folding on every mid-edit
    // state (the whole sequence is already panic-isolated by the caller).
    let _ = formatting(text, encoding);
    let _ = document_symbols(text, encoding);
    let _ = folding_ranges(text, encoding);
    let tokens = semantic_tokens(text, encoding);
    if let Some(reason) = semantic_token_violation(&source, &tokens.data, encoding) {
        return Some(format!("semantic tokens: {reason}"));
    }

    // R3 handlers (hover / completion / codeAction) must also be total on every
    // mid-edit state, probed at a spread of positions including out-of-bounds.
    exercise_r3_handlers(text, &source, encoding);

    None
}

/// Drive the R3 request handlers (`hover`, `completion`, `code_actions`) over
/// `text` at a spread of positions — every line start, a couple of interior
/// columns per line, and a deliberately out-of-bounds position — so the totality
/// sweep proves they never panic on a real (or mid-edit) corpus buffer. The
/// caller wraps the whole sequence in `catch_unwind`, so a panic here is counted.
fn exercise_r3_handlers(text: &str, source: &SourceText, encoding: PositionEncoding) {
    let uri = Url::parse("file:///probe.abc").expect("valid probe uri");
    // code_actions ignores position; run it once.
    let _ = code_actions(
        &uri,
        text,
        Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 0,
            },
        },
        encoding,
    );

    let line_count = source.line_count().max(1);
    for line_index in 0..line_count {
        let width = line_width(source, line_index, encoding);
        // Line start, a column mid-line, the line end, and one past the end.
        let cols = [0u32, width / 2, width, width + 5];
        for character in cols {
            let pos = Position {
                line: line_index as u32,
                character,
            };
            let _ = hover(text, pos, encoding);
            let _ = completion(text, pos, encoding);
        }
    }
    // A line well past EOF (out of bounds) — both must stay total.
    let past = Position {
        line: (line_count as u32) + 50,
        character: 99,
    };
    let _ = hover(text, past, encoding);
    let _ = completion(text, past, encoding);
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

// ---------------------------------------------------------------------------
// Leg D: semantic-token correctness (exhaustive, non-overlapping, in-bounds,
// monotonic delta-encoding). Used both by the totality sweep above and the
// fidelity test below.
// ---------------------------------------------------------------------------

/// Decode a delta-encoded semantic-token stream into absolute
/// `(line, start, length, token_type)` tuples.
fn decode_tokens(data: &[SemanticToken]) -> Vec<(u32, u32, u32, u32)> {
    let mut out = Vec::with_capacity(data.len());
    let mut line = 0u32;
    let mut start = 0u32;
    for t in data {
        if t.delta_line == 0 {
            start = start.saturating_add(t.delta_start);
        } else {
            line = line.saturating_add(t.delta_line);
            start = t.delta_start;
        }
        out.push((line, start, t.length, t.token_type));
    }
    out
}

/// The union of non-`Whitespace` `MusicToken` byte ranges the parser produced
/// for `source`, as a sorted, merged set of `[start, end)` byte intervals (the
/// end of each clamped to its start line, mirroring the emitter). This is the
/// ground truth a correct token stream must *cover* exactly: every highlighted
/// byte, nothing invented. Working in bytes makes it encoding-independent.
fn expected_token_byte_coverage(source: &SourceText) -> Vec<(usize, usize)> {
    use croma_core::syntax::MusicTokenKind;
    use croma_core::{ParseOptions, parse_document};

    let report = parse_document(source.as_str(), ParseOptions::default());
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for tune in &report.value.music.tunes {
        for line in &tune.lines {
            for tok in &line.tokens {
                if matches!(tok.kind, MusicTokenKind::Whitespace) {
                    continue;
                }
                let (s, e) = if tok.span.start <= tok.span.end {
                    (tok.span.start, tok.span.end)
                } else {
                    (tok.span.end, tok.span.start)
                };
                let line_text_end = byte_to_position_line_text_end(source, s);
                let e = e.min(line_text_end);
                if e > s {
                    spans.push((s, e));
                }
            }
        }
    }
    merge_intervals(&mut spans)
}

/// The text-end byte of the line containing byte `offset` (terminator excluded).
fn byte_to_position_line_text_end(source: &SourceText, offset: usize) -> usize {
    let pos = crate::position::byte_to_position(source, offset, PositionEncoding::Utf8);
    source
        .line(pos.line as usize)
        .map(|l| l.text_end())
        .unwrap_or_else(|| source.len())
}

/// Sort and merge `[start, end)` byte intervals into a disjoint, ordered set.
fn merge_intervals(spans: &mut [(usize, usize)]) -> Vec<(usize, usize)> {
    spans.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for &(s, e) in spans.iter() {
        match merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    merged
}

/// Validate one document's semantic tokens against leg D's four properties.
/// Returns a reason on the first violation, else `None`.
fn semantic_token_violation(
    source: &SourceText,
    data: &[SemanticToken],
    encoding: PositionEncoding,
) -> Option<String> {
    // (iv) delta-encoding monotonic: deltaLine >= 0 (u32, always), and within a
    // line deltaStart >= 0 (u32, always). The risk is a *negative* logical step,
    // which would surface as an overlap below; check the raw stream is decodable.
    let decoded = decode_tokens(data);

    // (iii) in-bounds + (ii) non-overlapping: positions strictly increasing and
    // each token fits within its line's width under the encoding.
    let line_count = source.line_count();
    let mut prev_end: Option<(u32, u32)> = None;
    for &(line, start, length, _ty) in &decoded {
        if (line as usize) >= line_count.max(1) {
            return Some(format!("token line {line} out of bounds"));
        }
        let width = line_width(source, line as usize, encoding);
        if start + length > width {
            return Some(format!(
                "token at {line}:{start} len {length} exceeds line width {width}"
            ));
        }
        let this_start = (line, start);
        if let Some(prev) = prev_end
            && this_start < prev
        {
            return Some(format!(
                "token at {line}:{start} overlaps previous end {prev:?}"
            ));
        }
        prev_end = Some((line, start + length));
    }

    // (i) exhaustive (as coverage): the union of emitted byte ranges equals the
    // union of non-whitespace parser-token byte ranges — every highlighted byte
    // covered, nothing invented. Reconstruct each emitted token's byte range by
    // reversing its (line, start) and (line, start+length) endpoints.
    let mut emitted: Vec<(usize, usize)> = Vec::new();
    for &(line, start, length, _ty) in &decoded {
        let start_byte = position_to_byte(
            source,
            Position {
                line,
                character: start,
            },
            encoding,
        );
        let end_byte = position_to_byte(
            source,
            Position {
                line,
                character: start + length,
            },
            encoding,
        );
        if end_byte > start_byte {
            emitted.push((start_byte, end_byte));
        }
    }
    let emitted = merge_intervals(&mut emitted);
    let expected = expected_token_byte_coverage(source);
    if emitted != expected {
        return Some(format!(
            "coverage: {} emitted byte-ranges vs {} expected (union mismatch)",
            emitted.len(),
            expected.len()
        ));
    }

    None
}

/// The width of line `index` in `encoding` units (its text, excluding the
/// terminator).
fn line_width(source: &SourceText, index: usize, encoding: PositionEncoding) -> u32 {
    source
        .line(index)
        .and_then(|l| source.slice(Span::new(l.start(), l.text_end())))
        .map(|slice| match encoding {
            PositionEncoding::Utf8 => slice.len() as u32,
            PositionEncoding::Utf16 => slice.chars().map(char::len_utf16).sum::<usize>() as u32,
        })
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Leg A: diagnostics fidelity. The LSP diagnostics set must equal the core
// analyze_document diagnostics: same count, matching (severity, code) in order,
// and every LSP Range reversing to the originating core byte span.
// ---------------------------------------------------------------------------

fn severity_matches(core: croma_core::Severity, lsp: lsp_types::DiagnosticSeverity) -> bool {
    use croma_core::Severity;
    use lsp_types::DiagnosticSeverity;
    matches!(
        (core, lsp),
        (Severity::Error, DiagnosticSeverity::ERROR)
            | (Severity::Warning, DiagnosticSeverity::WARNING)
            | (Severity::Info, DiagnosticSeverity::INFORMATION)
    )
}

/// Compare LSP diagnostics with core diagnostics for `source` under `encoding`.
/// Returns a reason on the first divergence, else `None`.
fn diagnostics_fidelity_violation(source: &str, encoding: PositionEncoding) -> Option<String> {
    let core: Vec<Diagnostic> = analyze_document(source).diagnostics;
    let lsp = diagnostics(source, encoding);
    let text = SourceText::new(source);

    if core.len() != lsp.len() {
        return Some(format!("count: core {} vs lsp {}", core.len(), lsp.len()));
    }
    for (i, (c, l)) in core.iter().zip(lsp.iter()).enumerate() {
        // severity
        match l.severity {
            Some(sev) if severity_matches(c.severity, sev) => {}
            other => {
                return Some(format!(
                    "[{i}] severity core {:?} vs lsp {other:?}",
                    c.severity
                ));
            }
        }
        // code
        let lsp_code = match &l.code {
            Some(lsp_types::NumberOrString::String(s)) => s.as_str(),
            other => return Some(format!("[{i}] lsp code not a string: {other:?}")),
        };
        if lsp_code != c.code {
            return Some(format!("[{i}] code core {:?} vs lsp {lsp_code:?}", c.code));
        }
        // range reverses to the core byte span
        let start_byte = position_to_byte(&text, l.range.start, encoding);
        let end_byte = position_to_byte(&text, l.range.end, encoding);
        let (span_start, span_end) = if c.span.start <= c.span.end {
            (c.span.start, c.span.end)
        } else {
            (c.span.end, c.span.start)
        };
        // Clamp the expected span to the document length / char boundaries the
        // way span_to_range -> position_to_byte does, so an out-of-range core
        // span (e.g. EOF anchor) reverses consistently.
        let clamp = |b: usize| {
            let mut b = b.min(text.len());
            while b > 0 && !text.as_str().is_char_boundary(b) {
                b -= 1;
            }
            b
        };
        if start_byte != clamp(span_start) || end_byte != clamp(span_end) {
            return Some(format!(
                "[{i}] range {:?} reverses to {start_byte}..{end_byte}, expected {}..{}",
                l.range,
                clamp(span_start),
                clamp(span_end),
            ));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Leg B: formatting identity. Applying the formatting() edit to the source must
// equal croma_fmt::format(source) byte-for-byte.
// ---------------------------------------------------------------------------

/// Apply the (single, full-document or empty) formatting edit to `source` and
/// compare with `croma_fmt::format`. Returns a reason on mismatch, else `None`.
fn formatting_identity_violation(source: &str, encoding: PositionEncoding) -> Option<String> {
    let expected = format(source, FormatOptions::default());
    let edits = formatting(source, encoding);
    let applied = match edits.as_slice() {
        [] => source.to_string(),
        [edit] => {
            let text = SourceText::new(source);
            let start = position_to_byte(&text, edit.range.start, encoding);
            let end = position_to_byte(&text, edit.range.end, encoding);
            if start > end || end > source.len() {
                return Some(format!(
                    "edit range {start}..{end} invalid for len {}",
                    source.len()
                ));
            }
            let mut out = source.to_string();
            out.replace_range(start..end, &edit.new_text);
            out
        }
        many => return Some(format!("expected <=1 edit, got {}", many.len())),
    };
    if applied != expected {
        return Some(format!(
            "applied formatting != croma_fmt::format ({} vs {} bytes)",
            applied.len(),
            expected.len()
        ));
    }
    None
}

/// Run all R2 fidelity legs (A, B, D) on one source under one encoding,
/// returning the first violation reason per leg.
fn fidelity_violations(source: &str, encoding: PositionEncoding) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(reason) = diagnostics_fidelity_violation(source, encoding) {
        out.push(format!("A {reason}"));
    }
    if let Some(reason) = formatting_identity_violation(source, encoding) {
        out.push(format!("B {reason}"));
    }
    let text = SourceText::new(source);
    let tokens = semantic_tokens(source, encoding);
    if let Some(reason) = semantic_token_violation(&text, &tokens.data, encoding) {
        out.push(format!("D {reason}"));
    }
    out
}

/// The corpus fidelity gate: legs A (diagnostics), B (formatting), D (semantic
/// tokens) over the 10k. Reports stable summary lines parsed by
/// `tools/prove_lsp_fidelity.py`:
///
/// ```text
/// lsp leg A diagnostics: <N> files, <M> mismatches
/// lsp leg B formatting:  <N> files, <M> mismatches
/// lsp leg D tokens:      <N> files, <M> violations
/// ```
#[test]
fn lsp_fidelity_legs_abd_over_the_corpus() {
    let Ok(root) = std::env::var("ABC_ROOT") else {
        eprintln!("ABC_ROOT unset — skipping corpus-scale LSP fidelity proof (A/B/D)");
        return;
    };
    let root = PathBuf::from(root);
    let files = abc_files(&root);

    let mut processed = 0usize;
    let mut leg_a = 0usize;
    let mut leg_b = 0usize;
    let mut leg_d = 0usize;
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

        // Leg A and D are encoding-sensitive; check both. Leg B is encoding-
        // independent in effect (whole-doc replace) but we drive UTF-8 (exact)
        // and UTF-16 to be safe.
        for enc in [PositionEncoding::Utf8, PositionEncoding::Utf16] {
            for v in fidelity_violations(&source, enc) {
                if let Some(rest) = v.strip_prefix("A ") {
                    leg_a += 1;
                    failures.push(format!("{name} [{enc:?}] A: {rest}"));
                } else if let Some(rest) = v.strip_prefix("B ") {
                    leg_b += 1;
                    failures.push(format!("{name} [{enc:?}] B: {rest}"));
                } else if let Some(rest) = v.strip_prefix("D ") {
                    leg_d += 1;
                    failures.push(format!("{name} [{enc:?}] D: {rest}"));
                }
            }
        }
    }

    eprintln!("lsp leg A diagnostics: {processed} files, {leg_a} mismatches");
    eprintln!("lsp leg B formatting: {processed} files, {leg_b} mismatches");
    eprintln!("lsp leg D tokens: {processed} files, {leg_d} violations");
    if !failures.is_empty() {
        eprintln!(
            "first {} fidelity failures:\n{}",
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
    assert_eq!(leg_a, 0, "leg A diagnostics-fidelity mismatches");
    assert_eq!(leg_b, 0, "leg B formatting-identity mismatches");
    assert_eq!(leg_d, 0, "leg D semantic-token violations");
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

// ---------------------------------------------------------------------------
// Leg E: latency. diagnostics + semantic tokens on an average ~200-line file
// must complete well under ~50 ms on a CI machine. We measure the median of 20
// iterations (robust to a slow box / scheduler noise) and print the actual ms.
// Run with `--release` for a representative number.
// ---------------------------------------------------------------------------

/// The latency ceiling (ms). The spec budget is ~50 ms on a CI machine; the real
/// figure is expected to be «1 ms. Generous so a slow shared CI box still passes.
const LATENCY_CEILING_MS: f64 = 50.0;

/// How many iterations to time; we report the median.
const LATENCY_ITERATIONS: usize = 20;

/// Choose the timing subject: if `ABC_ROOT` is set, the real corpus file whose
/// line count is closest to 200; otherwise a synthesized ~200-line ABC document.
/// Returns `(text, label)`.
fn latency_subject() -> (String, String) {
    if let Ok(root) = std::env::var("ABC_ROOT") {
        let files = abc_files(&PathBuf::from(&root));
        let mut best: Option<(usize, PathBuf, String)> = None;
        for path in &files {
            let Ok(bytes) = fs::read(path) else { continue };
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let lines = text.lines().count();
            let distance = lines.abs_diff(200);
            let take = match &best {
                Some((best_distance, _, _)) => distance < *best_distance,
                None => true,
            };
            if take {
                best = Some((distance, path.clone(), text));
            }
        }
        if let Some((_, path, text)) = best {
            let lines = text.lines().count();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            return (text, format!("corpus {name} ({lines} lines)"));
        }
    }
    let text = synthetic_abc_200();
    let lines = text.lines().count();
    (text, format!("synthetic ({lines} lines)"))
}

/// A plausible ~200-line ABC document for when no corpus is available: a header
/// plus ~190 music-body lines with a representative mix of notes, chords, grace
/// groups, decorations, tuplets, and barlines.
fn synthetic_abc_200() -> String {
    let mut out = String::with_capacity(8 * 1024);
    out.push_str("X:1\nT:Latency Probe\nC:croma\nM:4/4\nL:1/8\nQ:1/4=120\nK:C\n");
    let bodies = [
        "CDEF GABc | defg abc'd' | !trill!c2 B2 A2 G2 |",
        "[CEG]2 {ab}c2 | (3def (3gab c4 | \"Am\"A2 \"G\"G2 F4 |",
        ".C.D.E.F | G>A B<c d2 e2 | z2 c2 B2 A2 |]",
        "T2 A,B,C,D, E,F,G,A, | =c ^d _e f | C/2D/2E/2F/2 G2 |",
    ];
    // ~190 body lines so the total is ~200 incl. the 7 header lines.
    for i in 0..190 {
        out.push_str(bodies[i % bodies.len()]);
        out.push('\n');
    }
    out
}

/// The median of a slice of f64 (sorted copy); empty slices report 0.
fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

#[test]
fn lsp_leg_e_latency_under_ceiling() {
    let (text, label) = latency_subject();
    // Use UTF-8 (the negotiated-preferred encoding) for the headline number.
    let encoding = PositionEncoding::Utf8;

    // Warm up once (page in code paths / allocator) so the first sample is not
    // an outlier; the warm-up result is discarded.
    let _ = diagnostics(&text, encoding);
    let _ = semantic_tokens(&text, encoding);

    let mut samples = Vec::with_capacity(LATENCY_ITERATIONS);
    for _ in 0..LATENCY_ITERATIONS {
        let start = std::time::Instant::now();
        let diags = diagnostics(&text, encoding);
        let tokens = semantic_tokens(&text, encoding);
        let elapsed = start.elapsed();
        // Touch the results so the optimiser cannot elide the work.
        std::hint::black_box((&diags, &tokens.data));
        samples.push(elapsed.as_secs_f64() * 1_000.0);
    }

    let median_ms = median(&samples);
    // The stable line a tools/ wrapper can parse.
    eprintln!(
        "lsp leg E latency: {median_ms:.2} ms (~200-line file, median of {LATENCY_ITERATIONS}) [{label}]"
    );

    // The leg E bar is a *release* figure on a CI machine ("Run it with
    // `--release`"). A plain `cargo test --workspace` builds unoptimized, where
    // the same work is an order of magnitude slower and unrepresentative — so the
    // ceiling is only enforced for optimized builds. The number is always
    // measured and printed regardless, so the gate is observable in either mode.
    if cfg!(debug_assertions) {
        eprintln!("lsp leg E: debug build — ceiling not enforced (run with --release for the bar)");
        return;
    }
    assert!(
        median_ms < LATENCY_CEILING_MS,
        "leg E latency {median_ms:.2} ms exceeds ceiling {LATENCY_CEILING_MS} ms on {label}"
    );
}
