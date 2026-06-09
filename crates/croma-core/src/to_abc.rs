//! Canonical `Score` -> ABC writer (the reverse of the MusicXML writer).
//!
//! Emits ABC that is a `croma fmt` fixed point and round-trips through
//! `parse_document` + `lower_score` with an identical structural projection.
use crate::{Rational, Score};

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

fn write_body(_score: &Score, _unit: Rational) -> String {
    String::new() // filled in Task 2+
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LowerOptions, ParseOptions, lower_score, parse_document};

    fn score_of(src: &str) -> crate::Score {
        let doc = parse_document(src, ParseOptions::default());
        lower_score(&doc.value, LowerOptions).value.expect("score")
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
}
