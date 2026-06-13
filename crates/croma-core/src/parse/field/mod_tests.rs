use super::*;
use crate::parse::parse_document;

#[test]
fn missing_l_defaults_from_meter() {
    let report = parse_document("X:1\nM:2/4\nK:C\nC\n", ParseOptions::default());
    let tune = report.value.fields.tune(0).expect("expected tune fields");
    let unit = tune
        .header
        .unit_note_length
        .as_ref()
        .expect("expected default unit note length");

    assert_eq!(unit.value.fraction, NoteLengthFraction::new(1, 16));
    assert_eq!(unit.value.origin, UnitNoteLengthOrigin::DefaultFromMeter);
    assert_eq!(report.value.source.slice(unit.span), Some("2/4"));
}

#[test]
fn parses_common_cut_and_free_meter() {
    for (source, expected) in [
        ("X:1\nM:C\nK:C\nC\n", MeterKind::CommonTime),
        ("X:1\nM:C|\nK:C\nC\n", MeterKind::CutTime),
        ("X:1\nM:none\nK:C\nC\n", MeterKind::None),
    ] {
        let report = parse_document(source, ParseOptions::default());
        let tune = report.value.fields.tune(0).expect("expected tune fields");
        let meter = tune.header.meter.as_ref().expect("expected meter");
        assert_eq!(meter.value.kind, expected);
        assert_eq!(
            tune.header
                .unit_note_length
                .as_ref()
                .expect("expected unit")
                .value
                .fraction,
            NoteLengthFraction::new(1, 8)
        );
    }
}

#[test]
fn parses_key_modes_and_explicit_accidentals() {
    let report = parse_document("X:1\nK:D Phr ^f _B\nC\n", ParseOptions::default());
    let tune = report.value.fields.tune(0).expect("expected tune fields");
    let key = tune.header.key.as_ref().expect("expected key");

    assert_eq!(
        key.value.tonic,
        Some(KeyTonic {
            step: 'D',
            accidental: None
        })
    );
    assert_eq!(key.value.mode, KeyMode::Phrygian);
    assert_eq!(key.value.accidentals.len(), 2);
    assert_eq!(key.value.accidentals[0].sign, AccidentalSign::Sharp);
    assert_eq!(key.value.accidentals[0].note.value, 'f');
    assert_eq!(key.value.accidentals[1].sign, AccidentalSign::Flat);
    assert_eq!(key.value.accidentals[1].note.value, 'B');

    let explicit = parse_document("X:1\nK:D exp _b _e ^f\nC\n", ParseOptions::default());
    let key = explicit
        .value
        .fields
        .tune(0)
        .and_then(|tune| tune.header.key.as_ref())
        .expect("expected explicit key");
    assert_eq!(key.value.mode, KeyMode::Explicit);
    assert!(key.value.explicit);
    assert_eq!(key.value.accidentals.len(), 3);
}

#[test]
fn parses_explicit_accidental_list_written_without_spaces() {
    // `K:D exp _B^g` — the explicit (exp) accidental list is written space-less,
    // so it arrives as a single token. Every accidental in the token must be
    // parsed (ABC 2.1 §3.1.14), not just the first; dropping `^g` left G natural.
    let report = parse_document("X:1\nK:D exp _B^g\nC\n", ParseOptions::default());
    let key = report
        .value
        .fields
        .tune(0)
        .and_then(|tune| tune.header.key.as_ref())
        .expect("expected explicit key");
    assert_eq!(key.value.mode, KeyMode::Explicit);
    assert!(key.value.explicit);
    assert_eq!(key.value.accidentals.len(), 2);
    assert_eq!(key.value.accidentals[0].sign, AccidentalSign::Flat);
    assert_eq!(key.value.accidentals[0].note.value, 'B');
    assert_eq!(key.value.accidentals[1].sign, AccidentalSign::Sharp);
    assert_eq!(key.value.accidentals[1].note.value, 'g');
}

#[test]
fn parses_nospace_key_global_accidentals_after_tonic() {
    let report = parse_document("X:1\nK:D_B^g\nC\n", ParseOptions::default());
    let tune = report.value.fields.tune(0).expect("expected tune fields");
    let key = tune.header.key.as_ref().expect("expected key");

    assert_eq!(
        key.value.tonic,
        Some(KeyTonic {
            step: 'D',
            accidental: None
        })
    );
    assert_eq!(key.value.mode, KeyMode::Major);
    assert!(
        key.value.accidentals.is_empty(),
        "compact accidental tail is nonstandard and ignored"
    );
    assert!(key.value.compact_accidentals_ignored);
}

#[test]
fn parses_nospace_key_global_accidentals_with_sharp_first_as_base_key() {
    let report = parse_document("X:1\nK:D^f_B_e\nC\n", ParseOptions::default());
    let tune = report.value.fields.tune(0).expect("expected tune fields");
    let key = tune.header.key.as_ref().expect("expected key");

    assert_eq!(
        key.value.tonic,
        Some(KeyTonic {
            step: 'D',
            accidental: None
        })
    );
    assert_eq!(key.value.mode, KeyMode::Major);
    assert!(
        key.value.accidentals.is_empty(),
        "compact accidental tail is nonstandard and ignored"
    );
    assert!(key.value.compact_accidentals_ignored);
}

#[test]
fn key_field_captures_octave_property() {
    let report = parse_document("X:1\nK: Dm octave=1\nC\n", ParseOptions::default());
    let tune = report.value.fields.tune(0).expect("expected tune fields");
    let key = tune.header.key.as_ref().expect("expected key");

    assert_eq!(
        key.value.tonic,
        Some(KeyTonic {
            step: 'D',
            accidental: None
        })
    );
    assert_eq!(key.value.mode, KeyMode::Minor);
    assert_eq!(
        key.value
            .properties
            .octave
            .as_ref()
            .map(|value| value.value.as_str()),
        Some("1")
    );
    // The tonic/mode token (`Dm`) must not be misread as a clef shorthand.
    assert!(key.value.properties.clef.is_none());
}

#[test]
fn key_field_captures_bare_clef_shorthand() {
    let report = parse_document("X:1\nK:C treble+8\nC\n", ParseOptions::default());
    let tune = report.value.fields.tune(0).expect("expected tune fields");
    let key = tune.header.key.as_ref().expect("expected key");

    assert_eq!(
        key.value.tonic,
        Some(KeyTonic {
            step: 'C',
            accidental: None
        })
    );
    assert_eq!(
        key.value
            .properties
            .clef
            .as_ref()
            .map(|value| value.value.as_str()),
        Some("treble+8")
    );
}

#[test]
fn unknown_fields_warn_and_stay_out_of_music() {
    let report = parse_document("X:1\nK:C\nY:ABC\nC\n", ParseOptions::default());

    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.field.unknown")
    );
    assert_eq!(
        report
            .value
            .surface
            .tokens_of_kind(crate::syntax::tune::SurfaceKind::Note)
            .count(),
        1
    );
}

#[test]
fn version_lines_and_version_instructions_update_interpretation_mode() {
    let strict = parse_document("%abc-2.1\nX:1\nK:C\nC\n", ParseOptions::default());
    assert_eq!(
        strict.value.fields.file_header.dialect.mode,
        ParseMode::Strict
    );

    let loose = parse_document("%abc\nX:1\nK:C\nC\n", ParseOptions::default());
    assert_eq!(
        loose.value.fields.file_header.dialect.mode,
        ParseMode::Loose
    );

    let tune_loose = parse_document(
        "%abc-2.1\nX:1\nI:abc-version 2.0\nK:C\nC\n",
        ParseOptions::default(),
    );
    let tune = tune_loose.value.fields.tune(0).expect("expected tune");
    assert_eq!(tune.header.dialect.mode, ParseMode::Loose);
}

#[test]
fn interpretation_fields_update_decoration_and_linebreak_state() {
    let decoration = parse_document("I:decoration +\n\nX:1\nK:C\nC\n", ParseOptions::default());
    let tune = decoration.value.fields.tune(0).expect("expected tune");
    assert_eq!(
        tune.inherited_file_header.dialect.decoration_delimiter,
        DecorationDelimiter::Plus
    );

    let linebreak = parse_document("X:1\nI:linebreak !\nK:C\nC\n", ParseOptions::default());
    let tune = linebreak.value.fields.tune(0).expect("expected tune");
    assert!(tune.header.dialect.line_break.uses_bang());
    assert_eq!(
        tune.header.dialect.decoration_delimiter,
        DecorationDelimiter::Plus
    );
}

#[test]
fn parses_voice_properties_with_source_spans() {
    let report = parse_document(
        "X:1\nV:T1 name=\"Tenor 1\" nm=T subname=\"Line A\" snm=TA clef=treble stem=up octave=-1 transpose=_B\nK:C\nC\n",
        ParseOptions::default(),
    );
    let tune = report.value.fields.tune(0).expect("expected tune fields");
    let voice = tune
        .header
        .voices
        .first()
        .expect("expected voice definition");
    let properties = &voice.value.parsed_properties;

    assert_eq!(voice.value.id.value, "T1");
    assert_eq!(
        properties.name.as_ref().map(|value| value.value.as_str()),
        Some("Tenor 1")
    );
    assert_eq!(
        properties.nm.as_ref().map(|value| value.value.as_str()),
        Some("T")
    );
    assert_eq!(
        properties
            .subname
            .as_ref()
            .map(|value| value.value.as_str()),
        Some("Line A")
    );
    assert_eq!(
        properties.snm.as_ref().map(|value| value.value.as_str()),
        Some("TA")
    );
    assert_eq!(
        properties.clef.as_ref().map(|value| value.value.as_str()),
        Some("treble")
    );
    assert_eq!(
        properties.stem.as_ref().map(|value| value.value),
        Some(StemDirection::Up)
    );
    assert_eq!(
        report
            .value
            .source
            .slice(properties.name.as_ref().expect("expected name").span),
        Some("Tenor 1")
    );
    assert_eq!(
        properties.octave.as_ref().map(|value| value.value.as_str()),
        Some("-1")
    );
    assert_eq!(
        properties
            .transpose
            .as_ref()
            .map(|value| value.value.as_str()),
        Some("_B")
    );
}

#[test]
fn parses_i_score_as_structured_directive() {
    let report = parse_document("X:1\nK:C\nI:score (T1 T2)\nC\n", ParseOptions::default());
    let score = report
        .value
        .fields
        .fields
        .iter()
        .find_map(|field| match &field.kind {
            ParsedFieldKind::Interpretation(InterpretationField::Score { directive }) => {
                Some(directive)
            }
            _ => None,
        })
        .expect("expected I:score directive");

    assert_eq!(score.value.value, "(T1 T2)");
    assert_eq!(score.tokens.len(), 4);
}

#[test]
fn invalid_voice_stem_is_preserved_as_other_property_with_span() {
    let report = parse_document(
        "X:1\nV:bad stem=sideways clef=bass\nK:C\nC\n",
        ParseOptions::default(),
    );
    let voice = report
        .value
        .fields
        .tune(0)
        .and_then(|tune| tune.header.voices.first())
        .expect("expected voice definition");
    let properties = &voice.value.parsed_properties;

    assert!(properties.stem.is_none());
    let stem = properties
        .other
        .iter()
        .find(|property| property.key.value == "stem")
        .expect("expected preserved invalid stem property");
    assert_eq!(stem.value.value, "sideways");
    assert_eq!(report.value.source.slice(stem.span), Some("stem=sideways"));
}
