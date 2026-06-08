//! ABC source formatter built on `croma-core`.
//!
//! Two modes:
//! - [`format`] — a canonical, **idempotent**, **lossless** formatting. Musical
//!   tokens are copied verbatim by source span; only whitespace, blank-line
//!   runs, and the final newline are normalized.
//! - [`auto_fix`] — additionally applies safe curations of malformed input.
//!   Every change is gated at runtime: it is kept only if the ordered pitch
//!   sequence (step+alter+octave) is unchanged, otherwise reverted.
//!
//! Guarantees, exercised in tests and (locally) over the 10k corpus:
//! - idempotent: `format(format(x)) == format(x)`;
//! - lossless: `pitch_seq(x) == pitch_seq(format(x))` and
//!   `pitch_seq(x) == pitch_seq(auto_fix(x).output)`.

use croma_core::{ParseOptions, Span};

mod engine;
mod fixes;
mod verify;

/// Options controlling how the formatter parses its input. Defaults to the same
/// strict ABC 2.1 parse the rest of the toolkit uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormatOptions {
    /// Parse options (spec version + mode) used to interpret the source.
    pub parse: ParseOptions,
}

/// A single curation applied (or skipped) by [`auto_fix`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Which curation produced this change.
    pub kind: FixKind,
    /// Location in the (canonically formatted) source the change applies to.
    pub span: Span,
    /// The text before the change.
    pub before: String,
    /// The text after the change.
    pub after: String,
}

/// The class of a curation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixKind {
    /// A length detached from its note/rest by whitespace, e.g. `g 2` → `g2`.
    DetachedLength,
    /// A chord-symbol written inside chord brackets, e.g. `["C"abc]` → `"C"abc`.
    ChordSymbolInBrackets,
    /// A tempo whose beat spec is doubled, e.g. `Q:1/4=1/4=160` → `Q:1/4=160`.
    DoubledTempo,
}

impl FixKind {
    /// A short, stable label for reporting.
    pub fn label(self) -> &'static str {
        match self {
            FixKind::DetachedLength => "detached-length",
            FixKind::ChordSymbolInBrackets => "chord-symbol-in-brackets",
            FixKind::DoubledTempo => "doubled-tempo",
        }
    }
}

/// The result of [`auto_fix`]: the formatted output plus the curations that were
/// applied and the candidate curations that were reverted by the safety gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixResult {
    /// The formatted, curated source.
    pub output: String,
    /// Curations that were applied (each verified pitch-preserving).
    pub changes: Vec<Change>,
    /// Candidate curations reverted because they would change the notes.
    pub skipped: Vec<Change>,
}

/// Format `source` into its canonical form. Idempotent and lossless.
pub fn format(source: &str, options: FormatOptions) -> String {
    engine::format(source, options.parse)
}

/// True when `source` is already in canonical form.
pub fn is_formatted(source: &str, options: FormatOptions) -> bool {
    format(source, options) == source
}

/// Format `source` and apply safe, pitch-preserving curations.
pub fn auto_fix(source: &str, options: FormatOptions) -> FixResult {
    fixes::auto_fix(source, options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(source: &str) -> String {
        format(source, FormatOptions::default())
    }

    #[test]
    fn collapses_music_spaces_but_preserves_beaming_breaks() {
        let out = fmt("X:1\nK:C\nCDE   FGA  |  c2\n");
        assert!(out.contains("CDE FGA | c2"), "got: {out:?}");
        assert!(out.ends_with('\n') && !out.ends_with("\n\n"));
    }

    #[test]
    fn is_idempotent() {
        let src = "X:1\nT:Tune\nK:C\n  CDE   FGA  |  \"C\"  c2  z2  % c o m\n\n\nX:2\nK:G\nGABc\n";
        let once = fmt(src);
        let twice = fmt(&once);
        assert_eq!(once, twice, "not idempotent");
    }

    #[test]
    fn is_lossless() {
        let src = "X:1\nT:Tune\nK:C\n  CDE   FGA  |  c2  z2\n";
        let before = verify::pitch_seq_of(src, ParseOptions::default());
        let after = verify::pitch_seq_of(&fmt(src), ParseOptions::default());
        assert_eq!(before, after);
        assert!(before.is_some());
    }

    #[test]
    fn lyrics_and_directives_are_byte_stable() {
        let out = fmt("X:1\nK:C\nCDE\nw: do  re   mi\n%%MIDI program 1\n");
        assert!(out.contains("w: do  re   mi"), "got: {out:?}");
        assert!(out.contains("%%MIDI program 1"), "got: {out:?}");
    }

    #[test]
    fn trims_trailing_whitespace_and_collapses_blank_runs() {
        let out = fmt("X:1  \nK:C\t\nC   \n\n\n\nD\n");
        assert_eq!(out, "X:1\nK:C\nC\n\nD\n");
    }

    #[test]
    fn empty_source_formats_to_empty() {
        assert_eq!(fmt(""), "");
    }

    #[test]
    fn chords_and_grace_groups_are_not_duplicated() {
        // Top-level runs collapse to one space; bracket/brace contents and a
        // chord length are preserved verbatim and never emitted twice.
        let src = "X:1\nK:C\n[\"Cmaj\"abc]   [CE]2  |\n";
        let out = fmt(src);
        assert_eq!(out, "X:1\nK:C\n[\"Cmaj\"abc] [CE]2 |\n");
        assert_eq!(fmt(&out), out, "not idempotent");
        assert_eq!(
            verify::pitch_seq_of(src, ParseOptions::default()),
            verify::pitch_seq_of(&out, ParseOptions::default()),
        );
    }

    #[test]
    fn grace_group_appears_once() {
        let out = fmt("X:1\nK:C\n{ge}A B\n");
        assert_eq!(out.matches("{ge}").count(), 1, "got: {out:?}");
    }

    #[test]
    fn inline_voice_prefix_is_never_dropped() {
        // Regression: a leading `V:1` voice marker on a music line is not covered
        // by the music tokens; it must survive (else notes move between voices).
        let src = "X:1\nM:4/4\nL:1/8\nV:1\nV:2\nK:C\nV:1  CDE   FGA |\nV:2  C,2  x4    |\n";
        let out = fmt(src);
        assert!(out.contains("V:1  CDE FGA |"), "got: {out:?}");
        assert!(out.contains("V:2  C,2 x4 |"), "got: {out:?}");
        assert_eq!(fmt(&out), out, "not idempotent");
        assert_eq!(
            verify::pitch_seq_of(src, ParseOptions::default()),
            verify::pitch_seq_of(&out, ParseOptions::default()),
        );
    }
}
