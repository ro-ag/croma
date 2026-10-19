//! ABC source formatter built on `croma-core`.
//!
//! Two modes:
//! - [`format`] — a canonical, **idempotent**, **lossless** formatting. Musical
//!   tokens are copied verbatim by source span; only whitespace, blank-line
//!   runs, the final newline, and the *spelling* of croma's own round-trip
//!   carriers ([`FixKind::CarrierCompaction`]) are normalized.
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

#[cfg(test)]
mod corpus_proof;
#[cfg(test)]
mod fmt_first_demo;

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
    /// A bare-number tempo with a non-integer suffix, e.g. `Q:320s` → `Q:320` or
    /// `Q:400.` → `Q:400` — strip the legacy/decimal junk the strict parser
    /// rejects so it reads the bare integer (ABC 2.1 §10.1).
    BareTempoSuffix,
    /// A redundant bar-line run collapsed to its canonical boundary, e.g. the
    /// spaced `| |` → `|` or the run `]||:` → `|:`.
    RedundantBarline,
    /// Whitespace after an information field's colon removed, e.g. `K: C` → `K:C`
    /// (the ABC 2.1 spec's own field notation has no space after the colon).
    FieldSpacing,
    /// Internal whitespace runs collapsed inside an active (column-0) `%%MIDI`
    /// directive's argument region, e.g. `%%MIDI beat 97 87  77 4` →
    /// `%%MIDI beat 97 87 77 4`. `%%MIDI` is an abc2midi convention (not ABC
    /// 2.1); the canonical form follows its whitespace tokenization. The comment
    /// tail and any inert mid-line `%%MIDI` text are left untouched.
    MidiDirectiveSpacing,
    /// A round-trip carrier written in its deprecated long spelling rewritten to
    /// the compact one, e.g. `[I:croma-lyric-extend verse=1]` → `[I:cr le=1]`.
    /// Applied by plain [`format`] (not only [`auto_fix`]): it is a migration of
    /// croma's own output, not a curation of hand-written source.
    CarrierCompaction,
}

impl FixKind {
    /// A short, stable label for reporting.
    pub fn label(self) -> &'static str {
        match self {
            FixKind::DetachedLength => "detached-length",
            FixKind::ChordSymbolInBrackets => "chord-symbol-in-brackets",
            FixKind::DoubledTempo => "doubled-tempo",
            FixKind::BareTempoSuffix => "bare-tempo-suffix",
            FixKind::RedundantBarline => "redundant-barline",
            FixKind::FieldSpacing => "field-spacing",
            FixKind::MidiDirectiveSpacing => "midi-directive-spacing",
            FixKind::CarrierCompaction => "carrier-compaction",
        }
    }

    /// The safety gate a curation of this kind must clear.
    pub(crate) fn gate(self) -> Gate {
        match self {
            // These intentionally restore a dropped duration/structure/tempo, so
            // the rendered MusicXML legitimately changes; only the ordered pitch
            // sequence must be preserved.
            FixKind::DetachedLength
            | FixKind::ChordSymbolInBrackets
            | FixKind::DoubledTempo
            | FixKind::BareTempoSuffix => Gate::Pitch,
            // These must not change ANY rendered aspect; the structure gate
            // reverts e.g. an alignment-sensitive `w:` lyric whose leading
            // whitespace turns out to matter. A carrier respelling belongs here
            // too: the compact form expands back to the same long form the
            // reader already understood, so the rendering must be identical.
            FixKind::RedundantBarline | FixKind::FieldSpacing | FixKind::CarrierCompaction => {
                Gate::Structure
            }
            // `%%MIDI` is not rendered into MusicXML, so neither the pitch nor
            // the structure gate constrains it; a textual directive-token
            // invariant proves the edit changed only collapsible whitespace.
            FixKind::MidiDirectiveSpacing => Gate::DirectiveTokens,
        }
    }
}

/// How strongly a curation must be proven score-preserving before it is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Gate {
    /// The ordered pitch sequence (step+alter+octave) must be unchanged.
    Pitch,
    /// The full MusicXML rendering must be unchanged.
    Structure,
    /// Only whitespace inside active `%%MIDI` argument regions may differ — no
    /// directive token, comment, or other line may change. Used for fixes to
    /// `%%MIDI` directives, which croma does not render into MusicXML.
    DirectiveTokens,
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
///
/// Deprecated long-spelling croma carriers are migrated to the compact spelling
/// first (see [`carrier_migrations`]), then the token-preserving engine runs.
/// The order matters: the engine falls back to the raw line whenever its rebuild
/// does not have the line's exact non-whitespace characters, so a respelling
/// done *inside* the engine would be silently reverted. Doing it to the source
/// text beforehand keeps the engine non-source-changing by construction.
///
/// Like every other source-changing edit in this crate, the migration is gated
/// at runtime: it is kept only if the migrated source renders MusicXML
/// byte-identical to the original's, and dropped wholesale otherwise. That is
/// also what keeps `format` and [`auto_fix`] from disagreeing about whether a
/// file needs changing.
pub fn format(source: &str, options: FormatOptions) -> String {
    let migrated = fixes::migrate_carriers(source, options.parse);
    engine::format(&migrated, options.parse)
}

/// The carrier respellings [`format`] considers for `source`, in source order.
///
/// Exposed so a caller that only *checks* formatting (`croma fmt --check`) can
/// say which deprecated carriers it would migrate instead of reporting a bare
/// "would reformat". These are candidates: `format` keeps them only if the whole
/// set clears its runtime gate.
pub fn carrier_migrations(source: &str, options: FormatOptions) -> Vec<Change> {
    fixes::carrier_migrations(source, options.parse)
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
#[path = "lib_tests.rs"]
mod tests;
