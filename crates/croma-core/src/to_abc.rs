//! Canonical `Score` -> ABC writer (the reverse of the MusicXML writer).
//!
//! Emits ABC that is a `croma fmt` fixed point and round-trips through
//! `parse_document` + `lower_score` with an identical structural projection.
use crate::model::TieRole;
use crate::{
    Accidental, AccidentalMark, BarlineKind, Pitch, Rational, RestVisibility, Score, TimedEventKind,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct AbcWriteOptions {}

/// Emit canonical ABC for `score`. Output is a `croma fmt` fixed point.
pub fn write_abc(score: &Score, _options: AbcWriteOptions) -> String {
    let mut out = String::new();
    let meta = &score.metadata;
    out.push_str(&format!("X:{}\n", meta.reference.text.trim()));
    if let Some(title) = &meta.title {
        out.push_str(&format!("T:{}\n", title.text.trim()));
    }
    let meter_display = meta
        .meter
        .as_ref()
        .map(|m| m.display.clone())
        .unwrap_or_else(|| "4/4".to_string());
    out.push_str(&format!("M:{meter_display}\n"));
    let unit = unit_length(score);
    out.push_str(&format!("L:{}/{}\n", unit.numerator, unit.denominator));
    if let Some(q) = tempo_field(score) {
        out.push_str(&format!("Q:{q}\n"));
    }
    let key_display = meta
        .key
        .as_ref()
        .map(|k| k.display.clone())
        .unwrap_or_else(|| "C".to_string());
    out.push_str(&format!("K:{key_display}\n"));
    out.push_str(&write_body(score, unit));
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Unit note length: ABC 2.1 default — measure duration < 3/4 → 1/16, else 1/8.
fn unit_length(score: &Score) -> Rational {
    let small = Rational::new(1, 16);
    let normal = Rational::new(1, 8);
    match score.metadata.meter.as_ref().and_then(|m| m.duration) {
        Some(dur) if (dur.numerator as u64) * 4 < (dur.denominator as u64) * 3 => small,
        _ => normal,
    }
}

fn tempo_field(score: &Score) -> Option<String> {
    let beat = score.metadata.tempo_model.as_ref()?.beat.as_ref()?;
    Some(format!(
        "{}/{}={}",
        beat.beat_numerator, beat.beat_denominator, beat.bpm
    ))
}

fn write_body(score: &Score, unit: Rational) -> String {
    let mut out = String::new();
    let Some(voice) = score.parts.first().and_then(|p| p.voices.first()) else {
        return out;
    };
    for event in &voice.events {
        match &event.kind {
            TimedEventKind::Note(note) => {
                out.push_str(accidental_str(note.written_accidental.as_ref()));
                out.push_str(&pitch_str(&note.pitch));
                out.push_str(&length_str(event.duration, unit));
                if event
                    .attachments
                    .ties
                    .iter()
                    .any(|t| t.role == TieRole::Start)
                {
                    out.push('-');
                }
                out.push(' ');
            }
            TimedEventKind::Rest(rest) => {
                out.push(match rest.visibility {
                    RestVisibility::Visible => 'z',
                    RestVisibility::Invisible => 'x',
                });
                out.push_str(&length_str(event.duration, unit));
                out.push(' ');
            }
            TimedEventKind::Barline(b) => {
                out.push_str(barline_str(b.kind));
                out.push(' ');
            }
            _ => {} // endings in later tasks
        }
    }
    format!("{}\n", out.trim_end())
}

/// Canonical ABC text for a barline kind.
fn barline_str(kind: BarlineKind) -> &'static str {
    match kind {
        BarlineKind::Regular => "|",
        BarlineKind::Double => "||",
        BarlineKind::Final => "|]",
        BarlineKind::RepeatStart => "|:",
        BarlineKind::RepeatEnd => ":|",
        BarlineKind::RepeatBoth => "::",
        // out of slice-1 scope; emit a plain bar so output still parses
        BarlineKind::Initial
        | BarlineKind::Dotted
        | BarlineKind::Invisible
        | BarlineKind::Liberal => "|",
    }
}

/// ABC accidental prefix for the originally written accidental, if any.
fn accidental_str(mark: Option<&AccidentalMark>) -> &'static str {
    match mark.map(|m| m.kind) {
        Some(Accidental::DoubleFlat) => "__",
        Some(Accidental::Flat) => "_",
        Some(Accidental::Natural) => "=",
        Some(Accidental::Sharp) => "^",
        Some(Accidental::DoubleSharp) => "^^",
        None => "",
    }
}

/// Pitch letter plus octave marks (middle C = octave 4: `C`=4, `c`=5).
fn pitch_str(pitch: &Pitch) -> String {
    let letter = pitch.step.to_ascii_uppercase();
    if pitch.octave >= 5 {
        let mut s = letter.to_ascii_lowercase().to_string();
        s.push_str(&"'".repeat((pitch.octave - 5) as usize));
        s
    } else {
        let mut s = letter.to_string();
        s.push_str(&",".repeat((4 - pitch.octave) as usize));
        s
    }
}

/// Length suffix for `duration` relative to `unit`: `mult = duration / unit`.
fn length_str(duration: Rational, unit: Rational) -> String {
    let mult = Rational::new(
        duration.numerator.saturating_mul(unit.denominator),
        duration.denominator.saturating_mul(unit.numerator),
    );
    match (mult.numerator, mult.denominator) {
        (1, 1) => String::new(),
        (n, 1) => n.to_string(),
        (1, 2) => "/".to_string(),
        (1, d) => format!("/{d}"),
        (n, d) => format!("{n}/{d}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LowerOptions, ParseOptions, lower_score, parse_document};

    fn score_of(src: &str) -> crate::Score {
        let doc = parse_document(src, ParseOptions::default());
        lower_score(&doc.value, LowerOptions).value.expect("score")
    }

    fn pitch_seq(score: &crate::Score) -> Vec<(char, i8, i8)> {
        let mut v = Vec::new();
        for p in &score.parts {
            for voice in &p.voices {
                for e in &voice.events {
                    if let crate::TimedEventKind::Note(n) = &e.kind {
                        v.push((n.pitch.step, n.pitch.alter, n.pitch.octave));
                    }
                }
            }
        }
        v
    }

    type PitchSeq = Vec<(char, i8, i8)>;

    fn roundtrip_pitches(src: &str) -> (PitchSeq, PitchSeq) {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        (pitch_seq(&s1), pitch_seq(&s2))
    }

    #[test]
    fn emits_required_headers() {
        let abc = write_abc(
            &score_of("X:1\nT:Tune\nM:4/4\nL:1/8\nK:C\nC\n"),
            AbcWriteOptions::default(),
        );
        assert!(abc.starts_with("X:1\n"), "got: {abc:?}");
        assert!(abc.contains("\nM:4/4\n"));
        assert!(abc.contains("\nL:1/8\n"));
        assert!(abc.contains("\nK:C\n"));
        assert!(abc.contains("\nT:Tune\n"));
    }

    #[test]
    fn notes_rests_octaves_accidentals_roundtrip() {
        for src in [
            "X:1\nL:1/8\nK:C\nC E G c c' C, z2 ^F _B =c\n",
            "X:1\nM:3/4\nL:1/4\nK:G\nGA B z\n",
        ] {
            let (a, b) = roundtrip_pitches(src);
            assert_eq!(a, b, "pitch round-trip failed for {src:?}");
        }
    }

    fn barline_kinds(score: &crate::Score) -> Vec<crate::BarlineKind> {
        let mut v = Vec::new();
        for p in &score.parts {
            for voice in &p.voices {
                for e in &voice.events {
                    if let crate::TimedEventKind::Barline(b) = &e.kind {
                        v.push(b.kind);
                    }
                }
            }
        }
        v
    }

    #[test]
    fn barlines_roundtrip() {
        for src in [
            "X:1\nL:1/4\nK:C\nCDEF | GABc |\n",
            "X:1\nL:1/4\nK:C\n|: CDEF :| GABc |]\n",
            "X:1\nL:1/4\nK:C\nCDEF || GABc\n",
        ] {
            let s1 = score_of(src);
            let abc = write_abc(&s1, AbcWriteOptions::default());
            let s2 = score_of(&abc);
            assert_eq!(
                barline_kinds(&s1),
                barline_kinds(&s2),
                "barlines for {src:?} -> {abc:?}"
            );
        }
    }
}
