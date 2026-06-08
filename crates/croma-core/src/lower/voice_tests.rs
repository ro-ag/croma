use super::middle_octave_shift;

#[test]
fn middle_octave_shift_matches_abc2xml() {
    assert_eq!(middle_octave_shift("d"), -2);
    assert_eq!(middle_octave_shift("D"), -1);
    assert_eq!(middle_octave_shift("B"), 0);
    assert_eq!(middle_octave_shift("C"), 0);
    assert_eq!(middle_octave_shift("c"), -1);
    assert_eq!(middle_octave_shift("D,"), 0);
}

#[test]
fn middle_octave_shift_ignores_malformed_input() {
    assert_eq!(middle_octave_shift(""), 0);
    assert_eq!(middle_octave_shift("x"), 0);
    assert_eq!(middle_octave_shift("3"), 0);
}
