use super::{middle_octave_shift, voice_octave_shift};
use crate::diagnostic::Span;
use crate::model::{TextLine, VoicePropertiesModel};

fn shift_props(
    clef: Option<&str>,
    octave: Option<&str>,
    middle: Option<&str>,
) -> VoicePropertiesModel {
    let text_line = |text: &str| TextLine {
        text: text.to_string(),
        span: Span::new(0, 0),
    };
    VoicePropertiesModel {
        clef: clef.map(text_line),
        octave: octave.map(text_line),
        middle: middle.map(text_line),
        ..VoicePropertiesModel::default()
    }
}

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

#[test]
fn voice_octave_shift_clamps_oversized_modifiers() {
    // `octave=` values beyond abc2xml's single-digit domain clamp to ±9
    // (previously values outside i8 were silently ignored).
    assert_eq!(
        voice_octave_shift(&shift_props(None, Some("99999"), None)),
        9
    );
    assert_eq!(
        voice_octave_shift(&shift_props(None, Some("-99999"), None)),
        -9
    );
    // An in-i8-range but absurd value clamps the same way instead of
    // overflowing the per-note base+shift addition downstream.
    assert_eq!(
        voice_octave_shift(&shift_props(Some("treble+15"), Some("125"), None)),
        11
    );
    // The combined clef+octave+middle total clamps to ±12.
    assert_eq!(
        voice_octave_shift(&shift_props(Some("treble-15"), Some("-9"), Some("d"))),
        -12
    );
}

#[test]
fn voice_octave_shift_ignores_malformed_octave_value() {
    // Mirrors `middle_octave_shift_ignores_malformed_input`: a non-numeric
    // `octave=` value contributes nothing, leaving only the clef suffix.
    assert_eq!(
        voice_octave_shift(&shift_props(Some("treble+8"), Some("x"), None)),
        1
    );
}
