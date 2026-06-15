//! Diagnostic and spec-reference builders for the lowering stage.

use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
use crate::model::{BarlineKind, Fraction};

pub(crate) fn invalid_tuplet_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.invalid_tuplet",
        "Tuplet specifier is outside the supported ABC range",
        span,
    )
    .with_spec_reference(abc_tuplet_reference())
    .with_recovery_note(RecoveryNote::new(
        "The tuplet syntax was preserved and ignored during lowering.",
    ))
}

pub(crate) fn barline_export_policy_info(span: Span, kind: BarlineKind) -> Diagnostic {
    Diagnostic::new(
        Severity::Info,
        "abc.musicxml.barline_policy",
        match kind {
            BarlineKind::Dotted => "Dotted barline is exported as a MusicXML dotted bar-style",
            BarlineKind::Invisible => "Invisible barline is exported as a MusicXML none bar-style",
            _ => "Barline export policy applied",
        },
        span,
    )
    .with_spec_reference(abc_barline_reference())
}

pub(crate) fn free_meter_multirest_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.multirest.free_meter",
        "Multi-measure rest in free meter has no measure duration; recovered using unit note length",
        span,
    )
    .with_spec_reference(abc_rest_reference())
    .with_recovery_note(RecoveryNote::new(
        "The rest count was preserved and each measure was lowered as one unit note length.",
    ))
}

pub(crate) fn overlay_incomplete_measure_warning(
    span: Span,
    actual: Fraction,
    expected: Fraction,
) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.voice.overlay_incomplete_measure",
        format!(
            "Overlay voice duration {}/{} is shorter than the measure-local duration {}/{}",
            actual.numerator, actual.denominator, expected.numerator, expected.denominator
        ),
        span,
    )
    .with_spec_reference(abc_overlay_reference())
    .with_recovery_note(RecoveryNote::new(
        "The overlay segment was preserved as a temporary measure-local voice.",
    ))
}

pub(crate) fn overlay_overfull_measure_warning(
    span: Span,
    actual: Fraction,
    expected: Fraction,
) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.voice.overlay_overfull_measure",
        format!(
            "Overlay voice duration {}/{} is longer than the measure-local duration {}/{}",
            actual.numerator, actual.denominator, expected.numerator, expected.denominator
        ),
        span,
    )
    .with_spec_reference(abc_overlay_reference())
    .with_recovery_note(RecoveryNote::new(
        "The overlay segment was preserved as a temporary measure-local voice.",
    ))
}

pub(crate) fn lyric_syllable_count_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.lyric.syllable_count",
        "Lyric syllable count does not match the available notes",
        span,
    )
    .with_spec_reference(abc_lyric_reference())
    .with_recovery_note(RecoveryNote::new(
        "The excess lyric token was preserved but not aligned to a note.",
    ))
}

pub(crate) fn symbol_count_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.symbol.count",
        "Symbol line has more symbols than available notes",
        span,
    )
    .with_spec_reference(abc_symbol_reference())
    .with_recovery_note(RecoveryNote::new(
        "The excess symbol was preserved but not aligned to a note.",
    ))
}

pub(crate) fn invalid_meter_change_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.field.invalid_m",
        "Invalid M: field value was ignored during lowering",
        span,
    )
    .with_spec_reference(abc_field_reference())
    .with_recovery_note(RecoveryNote::new(
        "Lowering continued with the previous valid meter.",
    ))
}

pub(crate) fn unsupported_complex_meter_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.meter.unsupported_complex",
        "Complex meter is preserved but has no fixed measure duration yet",
        span,
    )
    .with_spec_reference(abc_field_reference())
    .with_recovery_note(RecoveryNote::new(
        "Measure construction continued as free meter until a supported meter appears.",
    ))
}

pub(crate) fn inline_instruction_ignored_warning(directive: &str, span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.field.inline_ignored",
        format!("Inline I: instruction `{directive}` was ignored"),
        span,
    )
    .with_spec_reference(abc_field_reference())
    .with_recovery_note(RecoveryNote::new(
        "The instruction field was preserved but did not change lowering state.",
    ))
}

pub(crate) fn invalid_key_change_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.field.invalid_k",
        "Invalid K: field value was ignored during lowering",
        span,
    )
    .with_spec_reference(abc_field_reference())
    .with_recovery_note(RecoveryNote::new(
        "Lowering continued with the previous valid key signature.",
    ))
}

pub(crate) fn compact_key_accidentals_ignored_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.field.key.compact_accidentals_ignored",
        "No-space K: global accidentals were ignored",
        span,
    )
    .with_spec_reference(abc_field_reference())
    .with_recovery_note(RecoveryNote::new(
        "The valid base key was preserved; write global accidentals separated by spaces to apply them.",
    ))
}

pub(crate) fn key_tonic_trailing_junk_ignored_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.field.key.tonic_trailing_junk_ignored",
        "Characters after the K: tonic were ignored",
        span,
    )
    .with_spec_reference(abc_field_reference())
    .with_recovery_note(RecoveryNote::new(
        "The valid leading tonic was preserved and the trailing characters discarded.",
    ))
}

pub(crate) fn abc_barline_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.8 repeat/bar symbols")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_rest_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.5 rests")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_chord_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.11 chords")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_tuplet_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.13 tuplets")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_broken_rhythm_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.7 broken rhythm")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_slur_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.10 ties and slurs")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_overlay_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 7.4 voice overlay")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_lyric_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 5.1 lyrics alignment")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_symbol_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.15 symbol lines")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_field_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 information fields")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_annotation_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 sections 4.18 chord symbols / 4.19 annotations")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

pub(crate) fn abc_grace_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.12 grace notes")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}
