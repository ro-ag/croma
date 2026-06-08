//! Safe, pitch-preserving curations applied by [`crate::auto_fix`].
//!
//! Each detector proposes candidate edits against the canonically-formatted
//! source. Every candidate is verified at runtime: applied to a trial string,
//! re-parsed, and kept only if the ordered pitch sequence is unchanged from the
//! original. Anything that would change a note is reverted and reported as
//! skipped. Detached *accidentals* (`^ g`) are deliberately not attempted —
//! joining them adds a sharp, which changes a pitch and the gate would revert.

use croma_core::ParseOptions;

use crate::verify::pitch_seq_of;
use crate::{Change, FixResult, FormatOptions};

/// Format `source` canonically, then apply each verified curation.
pub(crate) fn auto_fix(source: &str, options: FormatOptions) -> FixResult {
    let baseline = pitch_seq_of(source, options.parse);
    let mut current = crate::engine::format(source, options.parse);
    let mut changes = Vec::new();
    let mut skipped = Vec::new();

    for candidate in collect_candidates(&current, options.parse) {
        let trial = apply(&current, &candidate);
        if pitch_preserved(&baseline, &trial, options.parse) {
            current = crate::engine::format(&trial, options.parse);
            changes.push(candidate);
        } else {
            skipped.push(candidate);
        }
    }

    FixResult {
        output: current,
        changes,
        skipped,
    }
}

/// Detect every candidate curation in `source`. Filled in by later tasks.
fn collect_candidates(_source: &str, _options: ParseOptions) -> Vec<Change> {
    Vec::new()
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
fn pitch_preserved(
    baseline: &Option<crate::verify::PitchSeq>,
    trial: &str,
    options: ParseOptions,
) -> bool {
    match baseline {
        Some(expected) => pitch_seq_of(trial, options).as_ref() == Some(expected),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_input_yields_no_changes() {
        let options = FormatOptions::default();
        let result = auto_fix("X:1\nK:C\nCDE\n", options);
        assert_eq!(result.output, crate::format("X:1\nK:C\nCDE\n", options));
        assert!(result.changes.is_empty());
        assert!(result.skipped.is_empty());
    }
}
