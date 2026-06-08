//! Safe, pitch-preserving curations applied by [`crate::auto_fix`].
//!
//! Each detector proposes candidate edits against the canonically-formatted
//! source. Every candidate is verified at runtime: applied to a trial string,
//! re-parsed, and kept only if the ordered pitch sequence is unchanged from the
//! original. Anything that would change a note is reverted and reported as
//! skipped. Detached *accidentals* (`^ g`) are deliberately not attempted —
//! joining them adds a sharp, which changes a pitch and the gate would revert.

use croma_core::{MusicItem, ParseOptions, Span, parse_document};

use crate::verify::{PitchSeq, pitch_seq_of};
use crate::{Change, FixKind, FixResult, FormatOptions};

/// Diagnostic code the parser emits for a length detached from its note/rest.
const DETACHED_LENGTH_CODE: &str = "abc.music.malformed_length";

/// Format `source` canonically, then apply each verified curation.
pub(crate) fn auto_fix(source: &str, options: FormatOptions) -> FixResult {
    let baseline = pitch_seq_of(source, options.parse);
    let formatted = crate::engine::format(source, options.parse);

    let mut candidates = collect_candidates(&formatted, options.parse);
    // Apply edits from the end of the buffer first so earlier byte offsets stay
    // valid as we mutate; candidates never overlap.
    candidates.sort_by_key(|change| std::cmp::Reverse(change.span.start));

    let mut working = formatted;
    let mut changes = Vec::new();
    let mut skipped = Vec::new();

    for candidate in candidates {
        let trial = apply(&working, &candidate);
        if pitch_preserved(&baseline, &trial, options.parse) {
            working = trial;
            changes.push(candidate);
        } else {
            skipped.push(candidate);
        }
    }

    // Restore natural (source order) for reporting.
    changes.reverse();
    skipped.reverse();

    FixResult {
        output: crate::engine::format(&working, options.parse),
        changes,
        skipped,
    }
}

/// Detect every candidate curation in `source`.
fn collect_candidates(source: &str, options: ParseOptions) -> Vec<Change> {
    let report = parse_document(source, options);
    let document = &report.value;

    let mut candidates = Vec::new();
    detached_length(source, &report.diagnostics, &mut candidates);
    chord_symbol_in_brackets(source, document, &mut candidates);
    doubled_tempo(source, document, &mut candidates);
    candidates
}

/// `g 2` → `g2`: remove the whitespace between a note/rest/chord and a length
/// the parser flagged as detached.
fn detached_length(source: &str, diagnostics: &[croma_core::Diagnostic], out: &mut Vec<Change>) {
    let bytes = source.as_bytes();
    for diagnostic in diagnostics {
        if diagnostic.code != DETACHED_LENGTH_CODE {
            continue;
        }
        let digit_start = diagnostic.span.start;
        // Find the run of spaces immediately before the digit.
        let mut ws_start = digit_start;
        while ws_start > 0 && bytes[ws_start - 1] == b' ' {
            ws_start -= 1;
        }
        if ws_start == digit_start {
            continue; // nothing detached to join
        }
        // The character before the gap must be something a length can attach to.
        let Some(prev) = bytes.get(ws_start - 1).copied() else {
            continue;
        };
        if !can_take_length(prev) {
            continue;
        }
        let span = Span {
            start: ws_start,
            end: digit_start,
        };
        out.push(Change {
            kind: FixKind::DetachedLength,
            span,
            before: source.get(span.start..span.end).unwrap_or("").to_string(),
            after: String::new(),
        });
    }
}

/// True if `byte` is a note letter, rest, chord close, or octave mark — i.e. a
/// token a length suffix may legally follow.
fn can_take_length(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b']' | b',' | b'\'')
}

/// `["C"abc]` → `"C"abc`: unwrap a chord whose first member carries a
/// chord-symbol that was written inside the brackets.
fn chord_symbol_in_brackets(
    source: &str,
    document: &croma_core::AbcDocument,
    out: &mut Vec<Change>,
) {
    for tune in &document.music.tunes {
        for line in &tune.lines {
            for item in &line.items {
                let MusicItem::Chord(chord) = item else {
                    continue;
                };
                let Some(close) = chord.close_span else {
                    continue; // unclosed — leave it
                };
                let first_has_symbol = chord
                    .members
                    .first()
                    .is_some_and(|m| !m.note.attachments.chord_symbols.is_empty());
                if !first_has_symbol {
                    continue;
                }
                // Preserve any leading attachment (e.g. a grace group before
                // the `[`), drop only the brackets, keep inner + trailing length.
                let leading = source
                    .get(chord.span.start..chord.open_span.start)
                    .unwrap_or("");
                let inner = source.get(chord.open_span.end..close.start).unwrap_or("");
                let trailing = source.get(close.end..chord.span.end).unwrap_or("");
                let after = format!("{leading}{inner}{trailing}");
                out.push(Change {
                    kind: FixKind::ChordSymbolInBrackets,
                    span: chord.span,
                    before: source
                        .get(chord.span.start..chord.span.end)
                        .unwrap_or("")
                        .to_string(),
                    after,
                });
            }
        }
    }
}

/// `Q:1/4=1/4=160` → `Q:1/4=160`: collapse a tempo whose beat spec is doubled.
fn doubled_tempo(source: &str, document: &croma_core::AbcDocument, out: &mut Vec<Change>) {
    for field in &document.fields.fields {
        if field.code != 'Q' {
            continue;
        }
        let raw = source
            .get(field.value_span.start..field.value_span.end)
            .unwrap_or("");
        let Some(collapsed) = collapse_doubled_tempo(raw.trim()) else {
            continue;
        };
        out.push(Change {
            kind: FixKind::DoubledTempo,
            span: field.value_span,
            before: raw.to_string(),
            after: collapsed,
        });
    }
}

/// If `value` is exactly `BEAT=BEAT=BPM` with the two beat specs equal, return
/// `BEAT=BPM`; otherwise `None`.
fn collapse_doubled_tempo(value: &str) -> Option<String> {
    let parts: Vec<&str> = value.split('=').collect();
    if parts.len() != 3 {
        return None;
    }
    let (first, second, bpm) = (parts[0], parts[1], parts[2]);
    if first != second || !is_beat_spec(first) || !is_positive_integer(bpm) {
        return None;
    }
    Some(format!("{first}={bpm}"))
}

/// True for a `numerator/denominator` beat spec like `1/4`.
fn is_beat_spec(value: &str) -> bool {
    match value.split_once('/') {
        Some((num, den)) => is_positive_integer(num) && is_positive_integer(den),
        None => false,
    }
}

/// True for a non-empty run of ASCII digits.
fn is_positive_integer(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}

/// Apply a candidate edit (a `before` → `after` replacement at `span`).
fn apply(source: &str, change: &Change) -> String {
    let mut out = String::with_capacity(source.len());
    out.push_str(source.get(..change.span.start).unwrap_or(""));
    out.push_str(&change.after);
    out.push_str(source.get(change.span.end..).unwrap_or(""));
    out
}

/// True if `trial` lowers to the same pitch sequence as the baseline.
fn pitch_preserved(baseline: &Option<PitchSeq>, trial: &str, options: ParseOptions) -> bool {
    match baseline {
        Some(expected) => pitch_seq_of(trial, options).as_ref() == Some(expected),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> FormatOptions {
        FormatOptions::default()
    }

    #[test]
    fn clean_input_yields_no_changes() {
        let result = auto_fix("X:1\nK:C\nCDE\n", opts());
        assert_eq!(result.output, crate::format("X:1\nK:C\nCDE\n", opts()));
        assert!(result.changes.is_empty());
        assert!(result.skipped.is_empty());
    }

    #[test]
    fn joins_detached_length() {
        let result = auto_fix("X:1\nK:C\ng 2\n", opts());
        assert!(result.output.contains("g2"), "got: {:?}", result.output);
        assert_eq!(result.changes.len(), 1);
        assert_eq!(result.changes[0].kind, FixKind::DetachedLength);
        // pitch sequence unchanged
        let base = pitch_seq_of("X:1\nK:C\ng 2\n", ParseOptions::default());
        let after = pitch_seq_of(&result.output, ParseOptions::default());
        assert_eq!(base, after);
    }

    #[test]
    fn detached_accidental_is_never_touched() {
        // `^ g` would change G natural -> G#, a pitch change: must not be fixed.
        let result = auto_fix("X:1\nK:C\n^ g\n", opts());
        assert!(result.changes.is_empty(), "changes: {:?}", result.changes);
        assert!(result.output.contains("^ g"), "got: {:?}", result.output);
    }

    #[test]
    fn unwraps_chord_symbol_in_brackets() {
        let result = auto_fix("X:1\nK:C\n[\"Cmaj\"abc]\n", opts());
        assert!(
            result.output.contains("\"Cmaj\"abc"),
            "got: {:?}",
            result.output
        );
        assert!(
            !result.output.contains("[\"Cmaj\""),
            "brackets remain: {:?}",
            result.output
        );
        assert_eq!(result.changes.len(), 1);
        assert_eq!(result.changes[0].kind, FixKind::ChordSymbolInBrackets);
    }

    #[test]
    fn collapses_doubled_tempo() {
        let result = auto_fix("X:1\nQ:1/4=1/4=160\nK:C\nC\n", opts());
        assert!(
            result.output.contains("Q:1/4=160"),
            "got: {:?}",
            result.output
        );
        assert!(
            !result.output.contains("1/4=1/4"),
            "still doubled: {:?}",
            result.output
        );
        assert_eq!(result.changes.len(), 1);
        assert_eq!(result.changes[0].kind, FixKind::DoubledTempo);
    }

    #[test]
    fn collapse_doubled_tempo_only_fires_on_exact_double() {
        assert_eq!(
            collapse_doubled_tempo("1/4=1/4=160"),
            Some("1/4=160".to_string())
        );
        assert_eq!(collapse_doubled_tempo("1/4=160"), None);
        assert_eq!(collapse_doubled_tempo("1/4=1/8=160"), None);
        assert_eq!(collapse_doubled_tempo("\"Allegro\""), None);
    }

    #[test]
    fn output_is_idempotent_after_fixes() {
        let once = auto_fix("X:1\nK:C\ng 2\n", opts()).output;
        let twice = crate::format(&once, opts());
        assert_eq!(once, twice);
    }
}
