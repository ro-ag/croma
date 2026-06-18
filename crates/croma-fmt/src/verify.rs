//! Pitch-sequence extraction — the losslessness gate for the formatter.
//!
//! The ordered sequence of `(step, alter, octave)` over every sounded note is
//! the canonical "did the notes change?" signal, mirroring
//! croma-test's `tools/prove_divergences.py`'s `pitch_seq`. Rests contribute nothing; each
//! chord member contributes one entry in order. A formatting that preserves this
//! sequence is lossless in the sense the project's corpus proof uses.

use croma_core::{
    LowerOptions, ParseOptions, Score, TimedEventKind, lower_score, parse_document, write_musicxml,
};

/// Ordered `(step, alter, octave)` for every sounded note.
pub(crate) type PitchSeq = Vec<(char, i8, i8)>;

/// The full MusicXML rendering of `source`, or `None` if it does not lower to a
/// score. This is the strongest "did the rendered score change?" signal — used
/// to gate cosmetic curations (e.g. redundant bar-line collapse) that must not
/// change ANY rendered aspect, not merely the pitches. The MusicXML embeds no
/// source byte offsets, so byte-equality of two renderings means identical
/// scores.
pub(crate) fn musicxml_of(source: &str, options: ParseOptions) -> Option<String> {
    let report = parse_document(source, options);
    let score = lower_score(&report.value, LowerOptions).value?;
    Some(write_musicxml(&score).musicxml)
}

/// Parse + lower `source`, returning its pitch sequence, or `None` if the
/// document does not lower to a score (e.g. it has hard errors).
pub(crate) fn pitch_seq_of(source: &str, options: ParseOptions) -> Option<PitchSeq> {
    let report = parse_document(source, options);
    let score = lower_score(&report.value, LowerOptions).value?;
    Some(pitch_seq(&score))
}

/// Extract the ordered pitch sequence from a lowered [`Score`].
pub(crate) fn pitch_seq(score: &Score) -> PitchSeq {
    let mut out = PitchSeq::new();
    for part in &score.parts {
        for voice in &part.voices {
            for event in &voice.events {
                match &event.kind {
                    TimedEventKind::Note(note) => {
                        out.push((note.pitch.step, note.pitch.alter, note.pitch.octave));
                    }
                    TimedEventKind::Chord(chord) => {
                        for member in &chord.members {
                            out.push((member.pitch.step, member.pitch.alter, member.pitch.octave));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> ParseOptions {
        ParseOptions::default()
    }

    #[test]
    fn extracts_sequential_notes() {
        let seq = pitch_seq_of("X:1\nK:C\nCEG\n", opts()).expect("score");
        assert_eq!(seq, vec![('C', 0, 4), ('E', 0, 4), ('G', 0, 4)]);
    }

    #[test]
    fn chord_members_are_in_order_and_rests_skipped() {
        let seq = pitch_seq_of("X:1\nK:C\n[CEG] z\n", opts()).expect("score");
        assert_eq!(seq, vec![('C', 0, 4), ('E', 0, 4), ('G', 0, 4)]);
    }

    #[test]
    fn accidental_changes_the_sequence() {
        let natural = pitch_seq_of("X:1\nK:C\nG\n", opts()).expect("score");
        let sharp = pitch_seq_of("X:1\nK:C\n^G\n", opts()).expect("score");
        assert_ne!(natural, sharp);
    }
}
