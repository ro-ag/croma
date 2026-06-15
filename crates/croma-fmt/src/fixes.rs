//! Safe, pitch-preserving curations applied by [`crate::auto_fix`].
//!
//! Each detector proposes candidate edits against the canonically-formatted
//! source. Every candidate is verified at runtime: applied to a trial string,
//! re-parsed, and kept only if the ordered pitch sequence is unchanged from the
//! original. Anything that would change a note is reverted and reported as
//! skipped. Detached *accidentals* (`^ g`) are deliberately not attempted —
//! joining them adds a sharp, which changes a pitch and the gate would revert.

use croma_core::{MusicItem, MusicTokenKind, ParseOptions, Span, parse_document};

use crate::verify::{PitchSeq, musicxml_of, pitch_seq_of};
use crate::{Change, FixKind, FixResult, FormatOptions, Gate};

/// Diagnostic code the parser emits for a length detached from its note/rest.
const DETACHED_LENGTH_CODE: &str = "abc.music.malformed_length";

/// Format `source` canonically, then apply each verified curation.
pub(crate) fn auto_fix(source: &str, options: FormatOptions) -> FixResult {
    let baseline = pitch_seq_of(source, options.parse);
    let formatted = crate::engine::format(source, options.parse);

    let mut candidates = resolve_overlaps(collect_candidates(&formatted, options.parse));
    // Apply edits from the end of the buffer first so earlier byte offsets stay
    // valid as we mutate; resolve_overlaps guarantees they never overlap.
    candidates.sort_by_key(|change| std::cmp::Reverse(change.span.start));

    let mut working = formatted;
    let mut changes = Vec::new();
    let mut skipped = Vec::new();

    for candidate in candidates {
        let trial = apply(&working, &candidate);
        let preserved = match candidate.kind.gate() {
            Gate::Pitch => pitch_preserved(&baseline, &trial, options.parse),
            // Structure fixes must not change the rendering relative to the
            // current state (which prior pitch fixes may legitimately have
            // altered), so compare `working` to `trial`, not to the original.
            Gate::Structure => structure_preserved(&working, &trial, options.parse),
        };
        if preserved {
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

/// Drop candidates whose spans overlap, preferring the larger span (e.g. a
/// whole-value doubled-tempo collapse over the field-spacing trim of its leading
/// whitespace). The cumulative apply assumes non-overlapping edits.
fn resolve_overlaps(mut candidates: Vec<Change>) -> Vec<Change> {
    candidates.sort_by(|a, b| {
        a.span
            .start
            .cmp(&b.span.start)
            .then(b.span.end.cmp(&a.span.end))
    });
    let mut kept: Vec<Change> = Vec::new();
    let mut last_end = 0;
    for candidate in candidates {
        if candidate.span.start >= last_end {
            last_end = candidate.span.end;
            kept.push(candidate);
        }
    }
    kept
}

/// Detect every candidate curation in `source`.
fn collect_candidates(source: &str, options: ParseOptions) -> Vec<Change> {
    let report = parse_document(source, options);
    let document = &report.value;

    let mut candidates = Vec::new();
    detached_length(source, &report.diagnostics, &mut candidates);
    chord_symbol_in_brackets(source, document, &mut candidates);
    doubled_tempo(source, document, &mut candidates);
    redundant_barlines(source, document, &mut candidates);
    field_spacing(source, document, &mut candidates);
    candidates
}

/// `K: C` → `K:C`: remove whitespace between an information field's colon and
/// its value. The ABC 2.1 spec writes fields with no space after the colon.
/// Structure-gated, so an alignment-sensitive value (`w:`/`s:`) whose leading
/// whitespace actually matters is reverted rather than mangled.
fn field_spacing(source: &str, document: &croma_core::AbcDocument, out: &mut Vec<Change>) {
    for field in &document.fields.fields {
        let value = source
            .get(field.value_span.start..field.value_span.end)
            .unwrap_or("");
        let leading = value.len() - value.trim_start().len();
        if leading == 0 {
            continue;
        }
        let span = Span {
            start: field.value_span.start,
            end: field.value_span.start + leading,
        };
        out.push(Change {
            kind: FixKind::FieldSpacing,
            span,
            before: source.get(span.start..span.end).unwrap_or("").to_string(),
            after: String::new(),
        });
    }
}

/// `| |` → `|`, `]||:` → `|:`: collapse a run of bar-line tokens (contiguous or
/// whitespace-separated) to its canonical single boundary. The candidate is a
/// best-effort canonical form; the structure gate (MusicXML equality) proves the
/// collapse rendering-neutral and reverts it otherwise, so legitimate complex
/// bar lines (a real `||` double bar, a final `|]`) are left untouched.
fn redundant_barlines(source: &str, document: &croma_core::AbcDocument, out: &mut Vec<Change>) {
    for tune in &document.music.tunes {
        for line in &tune.lines {
            collect_barline_runs(source, line, out);
        }
    }
}

/// Scan one music line's tokens for maximal `{Barline, Whitespace}` runs and
/// propose a canonical collapse for each that is shorter than the original.
fn collect_barline_runs(source: &str, line: &croma_core::MusicLine, out: &mut Vec<Change>) {
    let tokens = &line.tokens;
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].kind != MusicTokenKind::Barline {
            index += 1;
            continue;
        }
        // Extend over a maximal run of bar-line and interleaved whitespace
        // tokens, remembering the span of the first and last *bar-line* token.
        let run_start = tokens[index].span.start;
        let mut run_end = tokens[index].span.end;
        let mut has_internal_whitespace = false;
        let mut cursor = index + 1;
        let mut pending_whitespace = false;
        while cursor < tokens.len() {
            match tokens[cursor].kind {
                MusicTokenKind::Whitespace => pending_whitespace = true,
                MusicTokenKind::Barline => {
                    has_internal_whitespace |= pending_whitespace;
                    pending_whitespace = false;
                    run_end = tokens[cursor].span.end;
                }
                _ => break,
            }
            cursor += 1;
        }

        let original = source.get(run_start..run_end).unwrap_or("");
        let core: String = original.chars().filter(|c| !c.is_whitespace()).collect();
        // Only bother when there is redundancy to remove: internal spacing, or a
        // run longer than a plain two-character bar line (`||`, `|]`, `[|`),
        // which we leave to the structure gate to keep verbatim.
        if has_internal_whitespace || core.chars().count() > 2 {
            let candidate = canonical_barline(&core);
            if candidate.len() < original.len() {
                out.push(Change {
                    kind: FixKind::RedundantBarline,
                    span: Span {
                        start: run_start,
                        end: run_end,
                    },
                    before: original.to_string(),
                    after: candidate.to_string(),
                });
            }
        }

        index = cursor;
    }
}

/// The canonical single bar line for a whitespace-stripped run, derived from its
/// repeat markers: a trailing `:` opens a repeat (`|:`), a leading `:` closes one
/// (`:|`). A thick `]` in the run is a light-heavy final bar that must survive the
/// collapse — `]||:` is `|]:` (final + repeat-start), not a bare `|:` — otherwise
/// the structure gate rejects the lossy candidate and no simplification is made.
fn canonical_barline(core: &str) -> &'static str {
    let opens_repeat = core.ends_with(':');
    let closes_repeat = core.starts_with(':');
    let has_final = core.contains(']');
    match (has_final, closes_repeat, opens_repeat) {
        (false, true, true) => ":|:",
        (false, false, true) => "|:",
        (false, true, false) => ":|",
        (false, false, false) => "|",
        (true, true, true) => ":|]:",
        (true, false, true) => "|]:",
        (true, true, false) => ":|]",
        (true, false, false) => "|]",
    }
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

/// True if `before` and `trial` render to byte-identical MusicXML — i.e. the
/// edit changed no rendered aspect of the score at all.
fn structure_preserved(before: &str, trial: &str, options: ParseOptions) -> bool {
    match musicxml_of(before, options) {
        Some(rendering) => musicxml_of(trial, options).as_ref() == Some(&rendering),
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

    #[test]
    fn collapses_spaced_double_barline() {
        let result = auto_fix("X:1\nL:1/4\nK:C\nCDE| |FGA\n", opts());
        assert!(
            result.output.contains("CDE|FGA"),
            "got: {:?}",
            result.output
        );
        assert!(
            result
                .changes
                .iter()
                .any(|c| c.kind == FixKind::RedundantBarline),
            "changes: {:?}",
            result.changes
        );
        // Structure gate: rendering identical.
        assert_eq!(
            musicxml_of("X:1\nL:1/4\nK:C\nCDE| |FGA\n", ParseOptions::default()),
            musicxml_of(&result.output, ParseOptions::default()),
        );
    }

    #[test]
    fn collapses_thick_thin_repeat_run() {
        // `]||:` = a thick-thin final bar fused with a forward repeat (lowers to
        // [Final, RepeatStart]). The redundant `||` collapses away, but the `]`
        // light-heavy closer must survive: the canonical form is `|]:`, NOT a
        // bare `|:` (which would drop the final bar and fail the structure gate).
        let result = auto_fix("X:1\nL:1/4\nK:C\nab ]||: cd\n", opts());
        assert!(
            result.output.contains("ab |]: cd"),
            "got: {:?}",
            result.output
        );
        assert!(
            result
                .changes
                .iter()
                .any(|c| c.kind == FixKind::RedundantBarline)
        );
    }

    #[test]
    fn keeps_real_double_bar_and_final_bar() {
        // `||` is a meaningful light-light double bar; `|]` a final bar. Neither
        // is redundant — the structure gate must leave them untouched.
        let double = auto_fix("X:1\nL:1/4\nK:C\nCDE||FGA\n", opts());
        assert!(
            double.output.contains("CDE||FGA"),
            "got: {:?}",
            double.output
        );
        assert!(
            !double
                .changes
                .iter()
                .any(|c| c.kind == FixKind::RedundantBarline)
        );

        let final_bar = auto_fix("X:1\nL:1/4\nK:C\nCDE|FGA|]\n", opts());
        assert!(
            final_bar.output.contains("|]"),
            "got: {:?}",
            final_bar.output
        );
    }

    #[test]
    fn trims_space_after_field_colon() {
        let result = auto_fix("X:1\nT: My Tune\nK: C\nCDE\n", opts());
        assert!(
            result.output.contains("T:My Tune"),
            "got: {:?}",
            result.output
        );
        assert!(result.output.contains("K:C"), "got: {:?}", result.output);
        assert!(
            result
                .changes
                .iter()
                .filter(|c| c.kind == FixKind::FieldSpacing)
                .count()
                >= 2
        );
        // Structure gate: rendering identical.
        assert_eq!(
            musicxml_of("X:1\nT: My Tune\nK: C\nCDE\n", ParseOptions::default()),
            musicxml_of(&result.output, ParseOptions::default()),
        );
    }

    #[test]
    fn field_spacing_does_not_overlap_doubled_tempo() {
        // `Q: 1/4=1/4=160` triggers both detectors; overlap resolution keeps the
        // whole-value tempo collapse, which already drops the leading space.
        let result = auto_fix("X:1\nQ: 1/4=1/4=160\nK:C\nC\n", opts());
        assert!(
            result.output.contains("Q:1/4=160"),
            "got: {:?}",
            result.output
        );
        assert!(
            !result.output.contains("1/4=1/4"),
            "got: {:?}",
            result.output
        );
    }

    #[test]
    fn does_not_drop_a_real_boundary() {
        // `| |]` is a single bar then a final bar (two boundaries); collapsing to
        // `|]` would delete a boundary and change the rendering — gate reverts.
        let result = auto_fix("X:1\nL:1/4\nK:C\nCDE| |]\n", opts());
        assert_eq!(
            musicxml_of("X:1\nL:1/4\nK:C\nCDE| |]\n", ParseOptions::default()),
            musicxml_of(&result.output, ParseOptions::default()),
        );
    }
}
