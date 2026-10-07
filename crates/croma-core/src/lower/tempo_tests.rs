//! Tempo (`Q:`) beat-fraction parsing tests.

use super::parse_tempo_beat;
use crate::model::Fraction;

#[test]
fn summed_beats_with_large_denominators_reduce_without_overflow() {
    // `1/65536 + 1/65536` has a cross-multiplied denominator of 2^32, one past
    // u32. The sum must be computed wide and reduced: 1/32768.
    let beat = parse_tempo_beat("1/65536 1/65536=120", Fraction::new(1, 8)).expect("beat");
    assert_eq!((beat.beat_numerator, beat.beat_denominator), (1, 32768));
    assert_eq!(beat.bpm, 120);
}

#[test]
fn summed_beat_that_does_not_fit_u32_is_not_a_numeric_tempo() {
    // Two coprime denominators near u32::MAX: the reduced sum still needs a
    // denominator beyond u32, so the field is not a numeric tempo (the writer
    // keeps it as words), rather than panicking or wrapping.
    assert!(parse_tempo_beat("1/4294967291 1/4294967279=120", Fraction::new(1, 8)).is_none());
}

#[test]
fn overflowing_q_field_exports_without_panicking() {
    let source = "X:1\nQ:1/65536 1/65536=120\nK:C\nC|\n";
    assert!(crate::export_musicxml(source).is_ok());
}
