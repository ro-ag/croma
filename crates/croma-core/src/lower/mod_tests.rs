use super::*;
use crate::model::{
    Accidental, AlignedSymbolKind, LyricControl, RestVisibility, SlurRole, TieRole, TimedEvent,
    TimedEventKind, TupletRole,
};
use crate::options::ParseOptions;
use crate::parse::{parse_document, parse_tune_report_from_document};
use crate::syntax::{
    AnnotationPlacement, DecorationKind, MalformedSyntax, MalformedSyntaxKind, OctaveMark,
    QuotedTextKind, VariantEndingPart,
};

fn events_for(source: &str) -> (Vec<Event>, Vec<Diagnostic>) {
    let document = parse_document(source, ParseOptions::default()).value;
    let report = parse_tune_report_from_document(&document);
    (
        report.value.expect("expected tune").events,
        report.diagnostics,
    )
}

fn count_diagnostics(diagnostics: &[Diagnostic], code: &'static str) -> usize {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == code)
        .count()
}

#[test]
fn normalizes_pitch_case_and_mixed_octave_marks() {
    let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\nC C' c C,',\n");

    assert!(diagnostics.is_empty());
    let octaves = events
        .iter()
        .filter_map(|event| match event {
            Event::Note { octave, .. } => Some(*octave),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(octaves, vec![4, 5, 5, 3]);
}

#[test]
fn recovers_standalone_octave_marks_without_attaching_to_neighbor_notes() {
    let document_report = parse_document("X:1\nL:1/8\nK:C\n' , C\n", ParseOptions::default());
    assert_eq!(
        count_diagnostics(&document_report.diagnostics, "abc.music.malformed_octave"),
        2
    );

    let tune_music = document_report
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");
    let malformed = tune_music.lines[0]
        .items
        .iter()
        .filter_map(|item| match item {
            MusicItem::Malformed(item) => Some(item),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(malformed.len(), 2);
    assert!(malformed.iter().all(|item| !item.span.is_empty()));

    let tune_report = parse_tune_report_from_document(&document_report.value);
    let events = tune_report.value.expect("expected tune").events;
    assert!(matches!(
        events.as_slice(),
        [Event::Note {
            step: 'C',
            octave: 4,
            accidental: None,
            ..
        }]
    ));
}

#[test]
fn preserves_explicit_accidentals_in_semantic_events() {
    let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\n^C _D =E ^^F __G\n");

    assert!(diagnostics.is_empty());
    let accidentals = events
        .iter()
        .filter_map(|event| match event {
            Event::Note { accidental, .. } => Some(*accidental),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        accidentals,
        vec![
            Some(Accidental::Sharp),
            Some(Accidental::Flat),
            Some(Accidental::Natural),
            Some(Accidental::DoubleSharp),
            Some(Accidental::DoubleFlat),
        ]
    );
}

#[test]
fn recovers_dangling_accidentals_without_leaking_into_later_notes() {
    let document_report = parse_document("X:1\nL:1/8\nK:C\n^ _ = C\n", ParseOptions::default());
    assert_eq!(
        count_diagnostics(
            &document_report.diagnostics,
            "abc.music.malformed_accidental"
        ),
        3
    );

    let tune_report = parse_tune_report_from_document(&document_report.value);
    let events = tune_report.value.expect("expected tune").events;
    assert!(matches!(
        events.as_slice(),
        [Event::Note {
            step: 'C',
            accidental: None,
            ..
        }]
    ));
}

#[test]
fn lowers_fractional_lengths_and_slash_shorthand() {
    let document = parse_document(
        "X:1\nL:1/8\nK:C\nA2 A/ A// A3/2 A/4\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");

    assert!(report.diagnostics.is_empty());
    assert_eq!(tune.divisions, 8);
    let durations = tune
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Note { duration, .. } => Some(*duration),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(durations, vec![8, 2, 1, 6, 1]);
}

#[test]
fn recovers_malformed_lengths_and_preserves_valid_neighbors() {
    let document_report =
        parse_document("X:1\nL:1/8\nK:C\nA0 B/0 C 3 / D\n", ParseOptions::default());
    assert_eq!(
        count_diagnostics(&document_report.diagnostics, "abc.music.malformed_length"),
        4
    );

    let tune_music = document_report
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");
    let malformed_spans = tune_music.lines[0]
        .items
        .iter()
        .filter_map(|item| match item {
            MusicItem::Malformed(item) => Some(item.span),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(malformed_spans.len(), 2);
    assert!(
        malformed_spans
            .iter()
            .all(|span| document_report.value.source.slice(*span).is_some())
    );

    let tune_report = parse_tune_report_from_document(&document_report.value);
    let tune = tune_report.value.expect("expected tune");
    let durations = tune
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Note { duration, .. } => Some(*duration),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(durations, vec![4, 4, 4, 4]);
}

#[test]
fn lowers_multi_measure_rests_in_known_and_free_meter() {
    let (known_events, known_diagnostics) = events_for("X:1\nM:2/4\nL:1/8\nK:C\nZ2 X\n");
    assert!(known_diagnostics.is_empty());
    let known_durations = known_events
        .iter()
        .filter_map(|event| match event {
            Event::Rest {
                duration,
                visibility,
                ..
            } => Some((*duration, *visibility)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        known_durations,
        vec![
            (32, RestVisibility::Visible),
            (16, RestVisibility::Invisible),
        ]
    );

    let document = parse_document("X:1\nM:none\nL:1/8\nK:C\nZ3\n", ParseOptions::default()).value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.multirest.free_meter")
    );
    assert_eq!(tune.events.len(), 1);
    assert!(matches!(tune.events[0], Event::Rest { duration: 12, .. }));
}

#[test]
fn lowers_visible_invisible_rests_and_spacers() {
    let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\nz x y C\n");

    assert!(diagnostics.is_empty());
    let rests = events
        .iter()
        .filter_map(|event| match event {
            Event::Rest {
                visibility,
                duration,
                ..
            } => Some((*visibility, *duration)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rests,
        vec![(RestVisibility::Visible, 4), (RestVisibility::Invisible, 4),]
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Spacer { .. }))
            .count(),
        1
    );
}

#[test]
fn malformed_rest_lengths_recover_to_safe_durations() {
    let document_report = parse_document("X:1\nL:1/8\nK:C\nz0 x/0\n", ParseOptions::default());
    assert_eq!(
        count_diagnostics(&document_report.diagnostics, "abc.music.malformed_length"),
        2
    );

    let tune_report = parse_tune_report_from_document(&document_report.value);
    let rests = tune_report
        .value
        .expect("expected tune")
        .events
        .into_iter()
        .filter_map(|event| match event {
            Event::Rest {
                visibility,
                duration,
                ..
            } => Some((visibility, duration)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rests,
        vec![(RestVisibility::Visible, 4), (RestVisibility::Invisible, 4),]
    );
}

#[test]
fn lowers_basic_double_and_repeat_barlines() {
    let (events, diagnostics) = events_for("X:1\nK:C\nC|D||E|:F:|G::A[|B|]c\n");

    assert!(diagnostics.is_empty());
    let barlines = events
        .iter()
        .filter_map(|event| match event {
            Event::Barline { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        barlines,
        vec![
            BarlineKind::Regular,
            BarlineKind::Double,
            BarlineKind::RepeatStart,
            BarlineKind::RepeatEnd,
            BarlineKind::RepeatBoth,
            BarlineKind::Initial,
            BarlineKind::Final,
        ]
    );
}

#[test]
fn recovers_invalid_barline_fragments_as_skipped_malformed_items() {
    let document_report = parse_document("X:1\nK:C\nC : D\n", ParseOptions::default());
    assert_eq!(
        count_diagnostics(&document_report.diagnostics, "abc.music.invalid_barline"),
        1
    );

    let tune_music = document_report
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");
    assert!(tune_music.lines[0].items.iter().any(|item| matches!(
        item,
        MusicItem::Malformed(MalformedSyntax {
            kind: MalformedSyntaxKind::InvalidBarline,
            ..
        })
    )));

    let tune_report = parse_tune_report_from_document(&document_report.value);
    let notes = tune_report
        .value
        .expect("expected tune")
        .events
        .into_iter()
        .filter(|event| matches!(event, Event::Note { .. }))
        .count();
    assert_eq!(notes, 2);
}

#[test]
fn parses_liberal_dotted_and_invisible_barlines_with_diagnostics() {
    let report = parse_document("X:1\nK:C\nC |[| D .| E [|] F\n", ParseOptions::default());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.barline.liberal")
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.barline.policy")
    );

    let tune_report = parse_tune_report_from_document(&report.value);
    assert!(
        tune_report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.musicxml.barline_policy")
    );
}

#[test]
fn unclosed_inline_fields_groups_and_strings_are_recoverable_syntax() {
    let document_report = parse_document(
        "X:1\nK:C\nC [M:3/4\nD {ef\nE \"Am\nF [CE\nG\n",
        ParseOptions::default(),
    );

    for code in [
        "abc.music.unclosed_inline_field",
        "abc.music.unclosed_grace",
        "abc.music.unclosed_quoted_text",
        "abc.music.unclosed_chord",
    ] {
        assert!(
            document_report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code),
            "expected diagnostic {code}"
        );
    }

    let tune_report = parse_tune_report_from_document(&document_report.value);
    let notes = tune_report
        .value
        .expect("expected tune")
        .events
        .into_iter()
        .filter_map(|event| match event {
            Event::Note { step, .. } => Some(step),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(notes, vec!['C', 'D', 'E', 'F', 'G']);
}

#[test]
fn parses_spec_attachment_order_around_note_group() {
    let document_report =
        parse_document("X:1\nL:1/8\nK:C\n\"Gm7\"v.=G,2\n", ParseOptions::default());
    assert!(document_report.diagnostics.is_empty());
    let tune_music = document_report
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");
    let note = tune_music.lines[0]
        .items
        .iter()
        .find_map(|item| match item {
            MusicItem::Note(note) => Some(note),
            _ => None,
        })
        .expect("expected note");

    assert_eq!(note.attachments.chord_symbols[0].text, "Gm7");
    assert_eq!(
        note.attachments
            .decorations
            .iter()
            .map(|decoration| decoration.name.as_str())
            .collect::<Vec<_>>(),
        // `v` (down-bow shorthand) normalizes to its canonical decoration
        // name; `.` (staccato) is handled separately and stays as-is.
        vec!["downbow", "."]
    );
    assert_eq!(
        note.accidental.map(|accidental| accidental.sign),
        Some(Accidental::Natural)
    );
    assert_eq!(note.octave_marks[0].mark, OctaveMark::Lower);
    assert_eq!(
        note.length.as_ref().map(|length| length.raw.as_str()),
        Some("2")
    );
}

#[test]
fn classifies_quoted_chord_symbols_and_annotations() {
    let document_report = parse_document(
        "X:1\nL:1/8\nK:C\n\"Am7\"C \"^above\"D \"_below\"E \"<left\"F \">right\"G \"@free\"A\n",
        ParseOptions::default(),
    );
    assert!(document_report.diagnostics.is_empty());
    let tune_music = document_report
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");
    let notes = tune_music.lines[0]
        .items
        .iter()
        .filter_map(|item| match item {
            MusicItem::Note(note) => Some(note),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(notes[0].attachments.chord_symbols[0].text, "Am7");
    let placements = notes[1..]
        .iter()
        .map(|note| note.attachments.annotations[0].kind)
        .collect::<Vec<_>>();
    assert_eq!(
        placements,
        vec![
            QuotedTextKind::Annotation(AnnotationPlacement::Above),
            QuotedTextKind::Annotation(AnnotationPlacement::Below),
            QuotedTextKind::Annotation(AnnotationPlacement::Left),
            QuotedTextKind::Annotation(AnnotationPlacement::Right),
            QuotedTextKind::Annotation(AnnotationPlacement::Free),
        ]
    );
}

#[test]
fn quoted_text_before_grace_group_stays_in_main_note_bundle() {
    // `"F"{AB}c`: the syntax tree binds the quoted text to the MAIN note's
    // attachment bundle, alongside the grace group — the first grace note
    // inside the braces must not steal it.
    let document_report = parse_document("X:1\nL:1/8\nK:C\n\"F\"{AB}c\n", ParseOptions::default());
    assert!(document_report.diagnostics.is_empty());
    let tune_music = document_report
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");
    let note = tune_music.lines[0]
        .items
        .iter()
        .find_map(|item| match item {
            MusicItem::Note(note) => Some(note),
            _ => None,
        })
        .expect("expected note");

    assert_eq!(note.pitch.step, 'c');
    assert_eq!(note.attachments.chord_symbols[0].text, "F");
    assert_eq!(note.attachments.grace_groups.len(), 1);
    let grace = &note.attachments.grace_groups[0];
    assert!(grace.elements.iter().all(|element| match element {
        crate::syntax::GraceElementSyntax::Note(grace_note) =>
            grace_note.attachments.chord_symbols.is_empty(),
        _ => true,
    }));
}

#[test]
fn parses_user_defined_and_legacy_decoration_symbols_from_dialect_state() {
    let user_symbol = parse_document("X:1\nU:W=!trill!\nK:C\nWC\n", ParseOptions::default());
    assert!(user_symbol.diagnostics.is_empty());
    let tune_music = user_symbol
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");
    let note = tune_music.lines[0]
        .items
        .iter()
        .find_map(|item| match item {
            MusicItem::Note(note) => Some(note),
            _ => None,
        })
        .expect("expected note");
    assert_eq!(
        note.attachments.decorations[0].kind,
        DecorationKind::UserDefined
    );
    // The `U:`-defined symbol expands to its canonical decoration name so it
    // maps through the same export path as the long-form `!trill!`.
    assert_eq!(note.attachments.decorations[0].name, "trill");

    let legacy_allowed = parse_document(
        "X:1\nI:decoration +\nK:C\n+trill+C\n",
        ParseOptions::default(),
    );
    assert!(legacy_allowed.diagnostics.is_empty());
    let tune_music = legacy_allowed
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");
    let note = tune_music.lines[0]
        .items
        .iter()
        .find_map(|item| match item {
            MusicItem::Note(note) => Some(note),
            _ => None,
        })
        .expect("expected note");
    assert_eq!(
        note.attachments.decorations[0].kind,
        DecorationKind::LegacyNamed
    );

    let legacy_rejected = parse_document("X:1\nK:C\n+trill+C\n", ParseOptions::default());
    assert!(
        legacy_rejected
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.invalid_decoration")
    );
}

#[test]
fn parses_chord_with_inside_and_outside_decorations() {
    let document_report =
        parse_document("X:1\nL:1/8\nK:C\n!trill![.CEG]\n", ParseOptions::default());
    assert!(document_report.diagnostics.is_empty());
    let tune_music = document_report
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");
    let chord = tune_music.lines[0]
        .items
        .iter()
        .find_map(|item| match item {
            MusicItem::Chord(chord) => Some(chord),
            _ => None,
        })
        .expect("expected chord");

    assert_eq!(chord.attachments.decorations[0].name, "trill");
    assert_eq!(chord.members.len(), 3);
    assert_eq!(chord.members[0].note.attachments.decorations[0].name, ".");
}

#[test]
fn parses_chord_internal_tie_marker_on_member() {
    let document_report = parse_document("X:1\nL:1/8\nK:C\n[DA-]\n", ParseOptions::default());
    assert_eq!(
        count_diagnostics(
            &document_report.diagnostics,
            "abc.music.unknown_chord_token"
        ),
        0,
        "chord-internal tie must not be reported as an unknown chord token"
    );
    let tune_music = document_report
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");
    let chord = tune_music.lines[0]
        .items
        .iter()
        .find_map(|item| match item {
            MusicItem::Chord(chord) => Some(chord),
            _ => None,
        })
        .expect("expected chord");

    assert_eq!(chord.members.len(), 2);
    assert!(
        chord.members[0].tie.is_none(),
        "D member must not carry a tie"
    );
    assert!(
        chord.members[1].tie.is_some(),
        "A member must carry the internal tie marker"
    );
}

#[test]
fn unclosed_chord_scan_stops_at_barline() {
    // An unclosed `[` followed by a space then a barline must not consume the
    // following measures. The chord scan stops at `|`, emitting an
    // unclosed-chord diagnostic for the partial run, and the real notes after
    // the barline survive as ordinary music.
    let document_report = parse_document(
        "X:1\nL:1/8\nK:C\n[ |: CDEF | GABc |\n",
        ParseOptions::default(),
    );
    assert_eq!(
        count_diagnostics(&document_report.diagnostics, "abc.music.unclosed_chord"),
        1,
        "the unclosed bracket should yield exactly one unclosed-chord diagnostic"
    );
    let tune_music = document_report
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");
    let note_letters = tune_music.lines[0]
        .items
        .iter()
        .filter_map(|item| match item {
            MusicItem::Note(note) => Some(note.pitch.step),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        note_letters,
        vec!['C', 'D', 'E', 'F', 'G', 'A', 'B', 'c'],
        "all notes after the unclosed bracket must survive"
    );
}

#[test]
fn valid_chord_before_barline_does_not_swallow_following_note() {
    // Guard: a closed chord immediately followed by a barline parses as a chord
    // then a barline, with no unclosed-chord diagnostic and the following note
    // intact.
    let document_report = parse_document("X:1\nL:1/8\nK:C\n[CEG]|D\n", ParseOptions::default());
    assert_eq!(
        count_diagnostics(&document_report.diagnostics, "abc.music.unclosed_chord"),
        0,
        "a closed chord must not report an unclosed-chord diagnostic"
    );
    let tune_music = document_report
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");
    let chord = tune_music.lines[0]
        .items
        .iter()
        .find_map(|item| match item {
            MusicItem::Chord(chord) => Some(chord),
            _ => None,
        })
        .expect("expected chord");
    assert_eq!(chord.members.len(), 3);
    let trailing_note = tune_music.lines[0]
        .items
        .iter()
        .filter_map(|item| match item {
            MusicItem::Note(note) => Some(note.pitch.step),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        trailing_note,
        vec!['D'],
        "the D after the barline must survive"
    );
}

#[test]
fn lowers_chord_member_and_outer_duration_multipliers() {
    let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\n[C2E2G2]3\n");
    assert!(diagnostics.is_empty());
    let notes = events
        .iter()
        .filter_map(|event| match event {
            Event::Note {
                step,
                duration,
                chord,
                ..
            } => Some((*step, *duration, *chord)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        notes,
        vec![('C', 24, false), ('E', 24, true), ('G', 24, true)]
    );
}

#[test]
fn variable_duration_chord_members_emit_diagnostic() {
    let document = parse_document("X:1\nL:1/8\nK:C\n[E2G,6]\n", ParseOptions::default()).value;
    let report = parse_tune_report_from_document(&document);

    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.chord.variable_duration")
    );
}

#[test]
fn broken_rhythm_is_transparent_across_grace_groups() {
    let (left_events, left_diagnostics) = events_for("X:1\nL:1/8\nK:C\nA<{g}A\n");
    let (right_events, right_diagnostics) = events_for("X:1\nL:1/8\nK:C\nA{g}<A\n");

    assert!(left_diagnostics.is_empty());
    assert!(right_diagnostics.is_empty());
    let durations = |events: Vec<Event>| {
        events
            .into_iter()
            .filter_map(|event| match event {
                Event::Note { duration, .. } => Some(duration),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(durations(left_events), durations(right_events));
}

#[test]
fn parses_staccato_triplet_without_spaces() {
    let document_report = parse_document("X:1\nL:1/8\nK:C\n(3.a.b.c\n", ParseOptions::default());
    assert!(document_report.diagnostics.is_empty());
    let tune_music = document_report
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");

    assert!(matches!(tune_music.lines[0].items[0], MusicItem::Tuplet(_)));
    let staccato_count = tune_music.lines[0]
        .items
        .iter()
        .filter_map(|item| match item {
            MusicItem::Note(note) => Some(&note.attachments.decorations),
            _ => None,
        })
        .filter(|decorations| decorations.iter().any(|decoration| decoration.name == "."))
        .count();
    assert_eq!(staccato_count, 3);
}

#[test]
fn parses_adjacent_repeat_endings_after_barlines() {
    let document_report = parse_document("X:1\nK:C\n:|2 C|1D A:|2B\n", ParseOptions::default());
    assert!(document_report.diagnostics.is_empty());
    let tune_music = document_report
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");

    let endings = tune_music.lines[0]
        .items
        .iter()
        .filter(|item| matches!(item, MusicItem::VariantEnding(_)))
        .count();
    let repeat_ends = tune_music.lines[0]
        .items
        .iter()
        .filter(|item| {
            matches!(
                item,
                MusicItem::Barline(BarlineSyntax {
                    kind: BarlineKind::RepeatEnd,
                    ..
                })
            )
        })
        .count();
    assert_eq!(endings, 3);
    assert_eq!(repeat_ends, 2);
}

#[test]
fn parses_bracketed_variant_ending_lists_and_ranges() {
    let document_report = parse_document(
        "X:1\nK:C\n[1 C | [2 D | [1,3] E | [1-3] F | [1,3,5-7] G\n",
        ParseOptions::default(),
    );
    assert!(document_report.diagnostics.is_empty());
    let tune_music = document_report
        .value
        .music
        .tune(0)
        .expect("expected parsed tune music");
    let endings = tune_music.lines[0]
        .items
        .iter()
        .filter_map(|item| match item {
            MusicItem::VariantEnding(ending) => Some(ending),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(endings.len(), 5);
    assert_eq!(endings[0].endings.len(), 1);
    assert_eq!(endings[2].endings.len(), 2);
    assert!(matches!(
        endings[3].endings[0],
        VariantEndingPart::Range { .. }
    ));
    assert_eq!(endings[4].endings.len(), 3);
}

#[test]
fn repeat_ending_shorthand_must_be_adjacent() {
    let legal = parse_document("X:1\nK:C\nC| [1D\n", ParseOptions::default());
    assert!(
        !legal
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.invalid_repeat_ending")
    );

    let spaced = parse_document("X:1\nK:C\nC| 1D\n", ParseOptions::default());
    assert!(
        spaced
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.invalid_repeat_ending")
    );
}

#[test]
fn unclosed_slurs_are_recoverable_in_lowering() {
    let document = parse_document("X:1\nK:C\n(C D\n", ParseOptions::default()).value;
    let report = parse_tune_report_from_document(&document);

    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.unclosed_slur")
    );
    assert_eq!(
        report
            .value
            .expect("expected tune")
            .events
            .iter()
            .filter(|event| matches!(event, Event::Note { .. }))
            .count(),
        2
    );
}

#[test]
fn non_music_lines_and_chords_do_not_leak_comments_or_directives() {
    let document_report = parse_document(
        "X:1\nT:ABC\n+:DEF\nK:C\n%%text GAB\n[CDE] C % FED\n",
        ParseOptions::default(),
    );
    let report = parse_tune_report_from_document(&document_report.value);
    let events = report.value.expect("expected tune").events;

    let notes = events
        .iter()
        .filter(|event| matches!(event, Event::Note { .. }))
        .count();
    assert_eq!(notes, 4);
}

#[test]
fn lowers_sequential_body_voice_blocks_to_explicit_timelines() {
    let document =
        parse_document("X:1\nK:C\nV:1\nC D|\nV:2\nE F|\n", ParseOptions::default()).value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");

    assert_eq!(
        tune.voices
            .iter()
            .map(|voice| voice.id.value.as_str())
            .collect::<Vec<_>>(),
        vec!["1", "2"]
    );
    let note_counts = tune
        .voices
        .iter()
        .map(|voice| {
            voice
                .measures
                .iter()
                .flat_map(|measure| &measure.events)
                .filter(|event| matches!(event.kind, TimelineEventKind::Note { .. }))
                .count()
        })
        .collect::<Vec<_>>();
    assert_eq!(note_counts, vec![2, 2]);
}

#[test]
fn lowers_inline_voice_switches_to_interleaved_timelines() {
    let document = parse_document(
        "X:1\nK:C\n[V:T1] C D| [V:T2] E F|\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");

    assert_eq!(tune.voices.len(), 2);
    assert!(tune.voices.iter().any(|voice| voice.id.value == "T1"));
    assert!(tune.voices.iter().any(|voice| voice.id.value == "T2"));
    let inline_voice = document
        .music
        .tune(0)
        .expect("expected parsed tune music")
        .lines
        .first()
        .expect("expected music line")
        .items
        .iter()
        .find_map(|item| match item {
            MusicItem::InlineField(field) if field.code == 'V' => Some(field),
            _ => None,
        })
        .expect("expected inline V field");
    assert_eq!(inline_voice.value.value, "T1");
}

#[test]
fn inline_field_after_barline_is_not_swallowed_into_a_liberal_barline() {
    // `|[M:3/8]` must parse as a plain barline followed by an inline field,
    // not as a liberal `|[` combined barline. The old greedy barline scan
    // ate the `[`, mangling the field and inserting a spurious empty bar.
    let document = parse_document(
        "X:1\nL:1/4\nM:6/8\nK:C\nC3|[M:3/8]E2E|[M:6/8]F2G|\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");

    let non_empty: Vec<usize> = tune.voices[0]
        .measures
        .iter()
        .map(|measure| {
            measure
                .events
                .iter()
                .filter(|event| event.alignable)
                .count()
        })
        .collect();
    assert_eq!(
        non_empty,
        vec![1, 2, 2],
        "no spurious empty measures: {non_empty:?}"
    );

    let line = document
        .music
        .tune(0)
        .expect("expected parsed tune music")
        .lines
        .first()
        .expect("expected music line");
    let inline_codes: Vec<char> = line
        .items
        .iter()
        .filter_map(|item| match item {
            MusicItem::InlineField(field) => Some(field.code),
            _ => None,
        })
        .collect();
    assert_eq!(inline_codes, vec!['M', 'M']);
}

#[test]
fn aligns_postponed_and_adjacent_lyrics_under_abc21_cursor_rules() {
    let document = parse_document(
            "X:1\nK:C\nC D E F|\nG A B c|\nw: doh re mi fa sol la ti doh\nw: alt verse words here more text ok done\n",
            ParseOptions::default(),
        )
        .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");
    let lyrics = tune.voices[0]
        .measures
        .iter()
        .flat_map(|measure| &measure.events)
        .flat_map(|event| &event.lyrics)
        .filter(|lyric| lyric.control == LyricControl::Syllable)
        .map(|lyric| (lyric.verse, lyric.text.as_str()))
        .collect::<Vec<_>>();

    assert!(lyrics.contains(&(1, "doh")));
    assert!(lyrics.contains(&(1, "doh")));
    assert!(lyrics.contains(&(2, "alt")));
}

#[test]
fn empty_lyric_line_consumes_notes_and_later_lyrics_start_after_them() {
    let document = parse_document(
        "X:1\nK:C\nC D E F|\nw:\nG A B c|\nw: sol la ti doh\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");
    let lyrics = tune.voices[0]
        .measures
        .iter()
        .flat_map(|measure| &measure.events)
        .filter_map(|event| {
            event
                .lyrics
                .iter()
                .find(|lyric| lyric.control == LyricControl::Syllable)
                .map(|lyric| lyric.text.as_str())
        })
        .collect::<Vec<_>>();

    assert_eq!(lyrics, vec!["sol", "la", "ti", "doh"]);
}

#[test]
fn lyrics_skip_rests_spacers_grace_notes_and_bar_marker_advances() {
    let document = parse_document(
        "X:1\nK:C\nC z y {g}D|E F|\nw: one | two three\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");
    let aligned = tune.voices[0]
        .measures
        .iter()
        .flat_map(|measure| &measure.events)
        .filter_map(|event| {
            event
                .lyrics
                .iter()
                .find(|lyric| lyric.control == LyricControl::Syllable)
                .map(|lyric| lyric.text.as_str())
        })
        .collect::<Vec<_>>();

    assert_eq!(aligned, vec!["one", "two", "three"]);
}

#[test]
fn overlay_rewinds_to_previous_barline_and_warns_when_incomplete() {
    let document = parse_document("X:1\nL:1/8\nK:C\nC D & E|\n", ParseOptions::default()).value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");

    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.voice.overlay_incomplete_measure")
    );
    let overlay = &tune.voices[0].measures[0].overlays[0];
    assert_eq!(overlay.events.len(), 1);
    assert_eq!(overlay.events[0].onset, Fraction::zero());
}

#[test]
fn symbol_lines_align_to_notes_and_preserve_symbol_kinds() {
    let document = parse_document(
        "X:1\nK:C\nC z D E F|\ns: \"C\" * !>! \"^slow\"\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");
    let symbols = tune.voices[0]
        .measures
        .iter()
        .flat_map(|measure| &measure.events)
        .flat_map(|event| &event.symbols)
        .map(|symbol| (symbol.text.as_str(), symbol.kind))
        .collect::<Vec<_>>();

    assert_eq!(
        symbols,
        vec![
            ("C", AlignedSymbolKind::ChordSymbol),
            (">", AlignedSymbolKind::Decoration),
            ("^slow", AlignedSymbolKind::Annotation),
        ]
    );
}

#[test]
fn preserves_score_directives_and_post_tune_words_in_lowered_tune() {
    let document = parse_document(
        "X:1\nK:C\n%%score (T1 T2)\nC\nW:after words\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");

    assert_eq!(tune.score_directives.len(), 1);
    assert_eq!(tune.score_directives[0].value.text, "(T1 T2)");
    assert_eq!(tune.post_tune_lyrics[0].text, "after words");
}

#[test]
fn preserves_header_score_directive_and_header_words() {
    let document = parse_document(
        "X:1\nI:score (A B)\nW:header words\nK:C\nC\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");

    assert_eq!(tune.score_directives.len(), 1);
    assert_eq!(tune.score_directives[0].value.text, "(A B)");
    assert_eq!(tune.post_tune_lyrics[0].text, "header words");
}

#[test]
fn body_voice_properties_override_header_definition_in_timeline() {
    let document = parse_document(
        "X:1\nV:1 name=Header\nK:C\nV:1 name=Body stem=down\nC\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");

    assert_eq!(
        tune.voices[0]
            .properties
            .name
            .as_ref()
            .map(|name| name.text.as_str()),
        Some("Body")
    );
    assert_eq!(
        tune.voices[0].properties.stem,
        Some(StemDirectionModel::Down)
    );
}

#[test]
fn lyrics_controls_preserve_text_and_diagnose_excess_tokens() {
    let source = "X:1\nK:C\nC D E F G A|\nw: time__ of~the \\-dash * extra too\n";
    let document = parse_document(source, ParseOptions::default()).value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");
    let note_lyrics = tune.voices[0].measures[0]
        .events
        .iter()
        .filter(|event| event.alignable)
        .map(|event| {
            event
                .lyrics
                .iter()
                .map(|lyric| (lyric.control, lyric.text.as_str()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(note_lyrics[0], vec![(LyricControl::Syllable, "time")]);
    assert_eq!(note_lyrics[1], vec![(LyricControl::Extender, "")]);
    assert_eq!(note_lyrics[2], vec![(LyricControl::Extender, "")]);
    assert_eq!(note_lyrics[3], vec![(LyricControl::Syllable, "of the")]);
    assert_eq!(note_lyrics[4], vec![(LyricControl::Syllable, "-dash")]);
    assert!(note_lyrics[5].is_empty());
    assert_eq!(
        count_diagnostics(&report.diagnostics, "abc.lyric.syllable_count"),
        2
    );
}

#[test]
fn lyric_bar_marker_is_ignored_when_cursor_is_already_at_measure_boundary() {
    let document = parse_document(
        "X:1\nK:C\nC D|E F|\nw: one two | three four\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");
    let aligned = tune.voices[0]
        .measures
        .iter()
        .flat_map(|measure| &measure.events)
        .filter_map(|event| {
            event
                .lyrics
                .iter()
                .find(|lyric| lyric.control == LyricControl::Syllable)
                .map(|lyric| lyric.text.as_str())
        })
        .collect::<Vec<_>>();

    assert_eq!(aligned, vec!["one", "two", "three", "four"]);
}

fn syllables_per_measure(source: &str) -> Vec<Vec<String>> {
    let document = parse_document(source, ParseOptions::default()).value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");
    tune.voices[0]
        .measures
        .iter()
        .map(|measure| {
            measure
                .events
                .iter()
                .filter(|event| event.alignable)
                .map(|event| {
                    event
                        .lyrics
                        .iter()
                        .find(|lyric| {
                            matches!(
                                lyric.control,
                                LyricControl::Syllable | LyricControl::Extender
                            )
                        })
                        .map(|lyric| lyric.text.clone())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
}

#[test]
fn leading_bar_marker_advances_past_a_filled_first_measure() {
    // Per ABC 2.1 section 5.1 a `|` "advances to the next bar". When a lyric
    // line opens with `|`, the notes of the first measure carry no syllable
    // and the first word lands on the downbeat of the second measure. The
    // earlier cursor model wrongly treated the line-start boundary as an
    // already-synced barline and kept the word on the pickup note.
    let per_measure = syllables_per_measure("X:1\nL:1/4\nK:C\nG z|c d|e f|\nw: |Oh well\n");
    assert_eq!(per_measure[0], vec!["".to_owned()]);
    assert_eq!(per_measure[1], vec!["Oh".to_owned(), "well".to_owned()]);
}

#[test]
fn consecutive_bar_markers_each_advance_one_measure() {
    // `|||` must skip three bars, not collapse into a single no-op at the
    // first boundary. The first word therefore lands in the fourth measure.
    let per_measure = syllables_per_measure("X:1\nL:1/4\nK:C\nG|c d|e f|g a|b c|\nw:|||All done\n");
    assert!(per_measure[0].iter().all(|text| text.is_empty()));
    assert!(per_measure[1].iter().all(|text| text.is_empty()));
    assert!(per_measure[2].iter().all(|text| text.is_empty()));
    assert_eq!(per_measure[3], vec!["All".to_owned(), "done".to_owned()]);
}

#[test]
fn double_hyphen_holds_a_blank_note_instead_of_a_literal_dash() {
    // `tri--umph` spans three notes with the middle one blank (ABC 2.1
    // section 5.1: a hyphen preceded by another hyphen is a separate, empty
    // syllable). The middle note must not export the literal "-" text.
    let document = parse_document(
        "X:1\nL:1/4\nK:C\nc d e|\nw: tri--umph\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");
    let texts = tune.voices[0].measures[0]
        .events
        .iter()
        .filter(|event| event.alignable)
        .map(|event| {
            event
                .lyrics
                .iter()
                .find(|lyric| lyric.control == LyricControl::Syllable)
                .map(|lyric| lyric.text.clone())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        texts,
        vec![Some("tri".to_owned()), None, Some("umph".to_owned())]
    );
}

#[test]
fn symbol_bar_boundary_and_excess_symbol_are_diagnosed_without_realigning() {
    let document = parse_document(
        "X:1\nK:C\nC D|E F|\ns: \"C\" !>! | \"^slow\" !fermata! !extra!\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");
    let aligned = tune.voices[0]
        .measures
        .iter()
        .flat_map(|measure| &measure.events)
        .flat_map(|event| &event.symbols)
        .map(|symbol| (symbol.text.as_str(), symbol.kind))
        .collect::<Vec<_>>();

    assert_eq!(
        aligned,
        vec![
            ("C", AlignedSymbolKind::ChordSymbol),
            (">", AlignedSymbolKind::Decoration),
            ("^slow", AlignedSymbolKind::Annotation),
            ("fermata", AlignedSymbolKind::Decoration),
        ]
    );
    assert_eq!(
        count_diagnostics(&report.diagnostics, "abc.symbol.count"),
        1
    );
}

#[test]
fn lyrics_use_the_current_voice_cursor_in_interleaved_body_fields() {
    let document = parse_document(
        "X:1\nK:C\nV:1\nC D|\nw: one two\nV:2\nE F|\nw: three four\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");
    let lyrics_for_voice = |voice_id: &str| {
        tune.voices
            .iter()
            .find(|voice| voice.id.value == voice_id)
            .expect("expected voice")
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .flat_map(|event| &event.lyrics)
            .filter(|lyric| lyric.control == LyricControl::Syllable)
            .map(|lyric| lyric.text.as_str())
            .collect::<Vec<_>>()
    };

    assert_eq!(lyrics_for_voice("1"), vec!["one", "two"]);
    assert_eq!(lyrics_for_voice("2"), vec!["three", "four"]);
}

#[test]
fn body_voice_field_can_switch_voice_and_carry_same_line_music() {
    let document = parse_document(
        "X:1\nL:1/8\nK:C\nV:1 C D|\nV:2 E F|\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    assert!(!report.has_errors());
    let tune = report
        .value
        .as_ref()
        .expect("expected same-line voice music");
    let notes_for_voice = |voice_id: &str| {
        tune.voices
            .iter()
            .find(|voice| voice.id.value == voice_id)
            .expect("expected voice")
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .filter_map(|event| match &event.kind {
                TimelineEventKind::Note { step, .. } => Some(*step),
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(notes_for_voice("1"), vec!['C', 'D']);
    assert_eq!(notes_for_voice("2"), vec!['E', 'F']);
}

#[test]
fn body_voice_properties_are_not_treated_as_same_line_music() {
    let document =
        parse_document("X:1\nK:C\nV:1 clef=treble\nC D|\n", ParseOptions::default()).value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected following music line");
    let voice = tune
        .voices
        .iter()
        .find(|voice| voice.id.value == "1")
        .expect("expected voice");

    assert_eq!(
        voice
            .properties
            .clef
            .as_ref()
            .map(|clef| clef.text.as_str()),
        Some("treble")
    );
    assert_eq!(
        voice.measures[0]
            .events
            .iter()
            .filter(|event| matches!(event.kind, TimelineEventKind::Note { .. }))
            .count(),
        2
    );
}

#[test]
fn body_voice_property_words_are_not_treated_as_same_line_music() {
    let document = parse_document(
        "X:1\nK:C\nV:1 Program 1 110 alto\nC D|\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected following music line");
    let voice = tune
        .voices
        .iter()
        .find(|voice| voice.id.value == "1")
        .expect("expected voice");

    assert_eq!(
        count_diagnostics(&report.diagnostics, "abc.music.unknown_token"),
        0
    );
    assert_eq!(
        voice.measures[0]
            .events
            .iter()
            .filter(|event| matches!(event.kind, TimelineEventKind::Note { .. }))
            .count(),
        2
    );
}

#[test]
fn body_voice_field_same_line_music_with_leading_accidental() {
    for accidental in ["=", "^", "_"] {
        let source = format!("X:1\nL:1/8\nK:C\nV:1 C D|\nV:2 {accidental}E F|\n");
        let document_report = parse_document(&source, ParseOptions::default());
        assert_eq!(
            count_diagnostics(
                &document_report.diagnostics,
                "abc.field.voice_property_ignored"
            ),
            0,
            "accidental `{accidental}` is music, not a discarded property"
        );
        let report = parse_tune_report_from_document(&document_report.value);
        assert!(!report.has_errors());
        let tune = report.value.expect("expected same-line voice music");
        let steps = tune
            .voices
            .iter()
            .find(|voice| voice.id.value == "2")
            .expect("expected voice 2")
            .measures
            .iter()
            .flat_map(|measure| &measure.events)
            .filter_map(|event| match &event.kind {
                TimelineEventKind::Note { step, .. } => Some(*step),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(steps, vec!['E', 'F'], "accidental `{accidental}`");
    }
}

#[test]
fn body_voice_field_same_line_music_with_accidental_inside_first_token() {
    let document = parse_document(
        "X:1\nL:1/8\nK:C\nV:1 C D|\nV:2 E2=F2 G2A2|\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    assert!(!report.has_errors());
    let tune = report.value.expect("expected same-line voice music");
    let steps = tune
        .voices
        .iter()
        .find(|voice| voice.id.value == "2")
        .expect("expected voice 2")
        .measures
        .iter()
        .flat_map(|measure| &measure.events)
        .filter_map(|event| match &event.kind {
            TimelineEventKind::Note { step, .. } => Some(*step),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(steps, vec!['E', 'F', 'G', 'A']);
}

#[test]
fn body_voice_field_unparseable_property_token_warns() {
    let report = parse_document(
        "X:1\nK:C\nV:1 Program =C2D2\nC D|\n",
        ParseOptions::default(),
    );
    assert_eq!(
        count_diagnostics(&report.diagnostics, "abc.field.voice_property_ignored"),
        1
    );
}

#[test]
fn overfull_overlay_measure_duration_emits_diagnostic() {
    let document = parse_document("X:1\nL:1/8\nK:C\nC & D E|\n", ParseOptions::default()).value;
    let report = parse_tune_report_from_document(&document);

    assert_eq!(
        count_diagnostics(&report.diagnostics, "abc.voice.overlay_overfull_measure"),
        1
    );
}

#[test]
fn ampersand_in_lyric_and_symbol_lines_is_not_music_overlay_syntax() {
    let document = parse_document(
        "X:1\nK:C\nC D E|\nw: Tom & Jerry\ns: & * !>!\n",
        ParseOptions::default(),
    )
    .value;
    let report = parse_tune_report_from_document(&document);
    let tune = report.value.expect("expected tune");
    let lyrics = tune.voices[0]
        .measures
        .iter()
        .flat_map(|measure| &measure.events)
        .flat_map(|event| &event.lyrics)
        .filter(|lyric| lyric.control == LyricControl::Syllable)
        .map(|lyric| lyric.text.as_str())
        .collect::<Vec<_>>();
    let symbols = tune.voices[0]
        .measures
        .iter()
        .flat_map(|measure| &measure.events)
        .flat_map(|event| &event.symbols)
        .map(|symbol| (symbol.text.as_str(), symbol.kind))
        .collect::<Vec<_>>();

    assert_eq!(lyrics, vec!["Tom", "&", "Jerry"]);
    assert_eq!(
        symbols,
        vec![
            ("&", AlignedSymbolKind::Raw),
            (">", AlignedSymbolKind::Decoration)
        ]
    );
    assert!(tune.voices[0].measures[0].overlays.is_empty());
}

fn tune_for(source: &str) -> (crate::model::Tune, Vec<Diagnostic>) {
    let document = parse_document(source, ParseOptions::default());
    let mut diagnostics = document.diagnostics;
    let report = parse_tune_report_from_document(&document.value);
    diagnostics.extend(report.diagnostics);
    (report.value.expect("expected tune"), diagnostics)
}

fn diagnostic_span<'a>(
    source: &'a str,
    diagnostics: &'a [Diagnostic],
    code: &'static str,
) -> &'a str {
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == code)
        .expect("expected diagnostic");
    &source[diagnostic.span.start..diagnostic.span.end]
}

fn semantic_note_events(tune: &crate::model::Tune) -> Vec<&TimedEvent> {
    tune.score.parts[0].voices[0]
        .events
        .iter()
        .filter(|event| matches!(event.kind, TimedEventKind::Note(_)))
        .collect()
}

fn semantic_note_alters(tune: &crate::model::Tune) -> Vec<i8> {
    semantic_note_events(tune)
        .into_iter()
        .map(|event| match &event.kind {
            TimedEventKind::Note(note) => note.pitch.alter,
            _ => unreachable!(),
        })
        .collect()
}

#[test]
fn semantic_score_marks_pickup_and_keeps_fixed_measure_numbers() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\nC|D E F G|A B c d|\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty());
    let measures = &tune.score.parts[0].voices[0].measures;
    assert_eq!(measures[0].expected_duration, Some(Fraction::new(4, 4)));
    assert_eq!(measures[0].actual_duration, Fraction::new(1, 4));
    assert!(measures[0].pickup);
    assert_eq!(
        measures
            .iter()
            .map(|measure| measure.id.number)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(measures[1].actual_duration, Fraction::new(4, 4));
    assert!(measures[1].complete);
}

#[test]
fn leading_repeat_start_stays_on_first_measure() {
    let source = "X:1\nM:3/4\nL:1/4\nK:C\n|: G c d | E D C |\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty());
    let measures = &tune.score.parts[0].voices[0].measures;
    assert_eq!(measures.len(), 2);
    assert_eq!(measures[0].actual_duration, Fraction::new(3, 4));
    assert_eq!(measures[1].actual_duration, Fraction::new(3, 4));
    assert!(
        measures[0]
            .barlines
            .iter()
            .any(|barline| barline.kind == BarlineKind::RepeatStart)
    );
    let first_measure_steps = semantic_note_events(&tune)
        .into_iter()
        .take(3)
        .map(|event| match &event.kind {
            TimedEventKind::Note(note) => note.pitch.step,
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    assert_eq!(first_measure_steps, vec!['G', 'C', 'D']);
}

#[test]
fn plain_leading_barline_does_not_create_empty_pickup_measure() {
    let source = "X:1\nM:4/4\nL:1/4\nK:C\n| E | F G A B |\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty());
    let measures = &tune.score.parts[0].voices[0].measures;
    assert_eq!(measures.len(), 2);
    assert_eq!(measures[0].actual_duration, Fraction::new(1, 4));
    assert!(measures[0].pickup);
    assert_eq!(measures[1].actual_duration, Fraction::new(4, 4));
}

#[test]
fn semantic_accidentals_propagate_within_measure_and_reset_at_barline() {
    let source = "X:1\nL:1/8\nK:C\n^F F|F\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty());
    assert_eq!(semantic_note_alters(&tune), vec![1, 1, 0]);
    assert!(tune.score.accidental_policy.reset_at_barlines);
}

#[test]
fn semantic_accidental_carry_persists_across_standalone_meter_change() {
    // ABC 2.1 §11.3 (`%%propagate-accidentals` default `pitch`): an explicit
    // accidental applies to same-pitch notes up to the END of the bar. A
    // mid-tune `M:` field line is not a bar line, so it must not clear the
    // measure accidental ledger (abc2xml carries the flat through too).
    let source = "X:1\nL:1/4\nK:C\n_e\nM:3/2\ne\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty());
    assert_eq!(semantic_note_alters(&tune), vec![-1, -1]);
}

#[test]
fn semantic_accidental_carry_persists_across_inline_meter_change() {
    let source = "X:1\nL:1/4\nK:C\n_e [M:3/2] e\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty());
    assert_eq!(semantic_note_alters(&tune), vec![-1, -1]);
}

#[test]
fn semantic_accidental_carry_persists_across_same_key_change() {
    // A mid-tune `K:` field is not a bar line either; even a same-key K:C
    // restatement must keep the carried flat (ABC 2.1 §11.3, abc2xml parity).
    let source = "X:1\nL:1/4\nK:C\n_e\nK:C\ne\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty());
    assert_eq!(semantic_note_alters(&tune), vec![-1, -1]);
}

#[test]
fn semantic_accidental_carry_persists_across_real_key_change() {
    // K:G alters F (sharp) but says nothing about E, so the explicitly
    // flattened E keeps its in-bar carry across the key change while the F
    // after it picks up the new signature.
    let source = "X:1\nL:1/4\nK:C\n_e\nK:G\ne f\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty());
    assert_eq!(semantic_note_alters(&tune), vec![-1, -1, 1]);
}

#[test]
fn semantic_tuplets_and_broken_rhythm_keep_rational_durations() {
    let source = "X:1\nL:1/8\nK:C\n(3CDE F>G\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty());
    let durations = semantic_note_events(&tune)
        .into_iter()
        .map(|event| event.duration)
        .collect::<Vec<_>>();
    assert_eq!(
        durations,
        vec![
            Fraction::new(1, 12),
            Fraction::new(1, 12),
            Fraction::new(1, 12),
            Fraction::new(3, 16),
            Fraction::new(1, 16),
        ]
    );
}

#[test]
fn semantic_voices_have_explicit_onsets_and_durations() {
    let source = "X:1\nL:1/4\nK:C\nV:1\nC D|\nV:2\nE2 F|\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty());
    let voice = tune
        .score
        .parts
        .iter()
        .flat_map(|part| &part.voices)
        .find(|voice| voice.id.value == "2")
        .expect("expected voice 2");
    let notes = voice
        .events
        .iter()
        .filter(|event| matches!(event.kind, TimedEventKind::Note(_)))
        .map(|event| (event.onset, event.duration))
        .collect::<Vec<_>>();
    assert_eq!(
        notes,
        vec![
            (Fraction::zero(), Fraction::new(2, 4)),
            (Fraction::new(2, 4), Fraction::new(1, 4)),
        ]
    );
}

#[test]
fn semantic_lyrics_symbols_and_prefix_attachments_stay_on_intended_event() {
    let source = "X:1\nL:1/8\nK:C\n\"Am\"!trill!C D\nw: one two\ns: \"C\" !>!\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty());
    let notes = semantic_note_events(&tune);
    let first = notes[0];
    assert_eq!(first.attachments.chord_symbols[0].text, "Am");
    assert_eq!(first.attachments.decorations[0].name, "trill");
    assert_eq!(first.attachments.lyrics[0].text, "one");
    assert_eq!(first.attachments.symbols[0].text, "C");
}

#[test]
fn chord_symbol_before_grace_group_binds_to_main_note() {
    // ABC 2.1 §4.20: in `"F"{AB}c` the grace group attaches to the main note
    // `c`, and the chord symbol written before the grace binds to `c` too —
    // not to a note inside the braces.
    let source = "X:1\nL:1/8\nK:C\n\"F\"{AB}c d|\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
    let notes = semantic_note_events(&tune);
    let first = notes[0];
    assert_eq!(first.attachments.chord_symbols[0].text, "F");
    assert_eq!(first.attachments.grace_groups.len(), 1);
    assert_eq!(first.attachments.grace_groups[0].note_count, 2);
    assert!(notes[1].attachments.chord_symbols.is_empty());
}

#[test]
fn chord_symbol_before_slur_open_binds_to_first_slurred_note() {
    // `"G7"(DE)`: the chord symbol rides across the slur-open and binds to
    // `D`, which also carries the slur start; `E` carries the slur stop.
    let source = "X:1\nL:1/8\nK:C\n\"G7\"(DE) F|\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
    let notes = semantic_note_events(&tune);
    let first = notes[0];
    assert_eq!(first.attachments.chord_symbols[0].text, "G7");
    assert!(
        first
            .attachments
            .slurs
            .iter()
            .any(|slur| slur.role == SlurRole::Start)
    );
    assert!(
        notes[1]
            .attachments
            .slurs
            .iter()
            .any(|slur| slur.role == SlurRole::Stop)
    );
    assert!(notes[2].attachments.chord_symbols.is_empty());
}

#[test]
fn chord_symbol_before_grace_and_slur_binds_to_main_note() {
    // `"F"{AB}(cd)`: chord symbol, grace group, and slur start all land on the
    // main note `c`.
    let source = "X:1\nL:1/8\nK:C\n\"F\"{AB}(cd)|\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
    let notes = semantic_note_events(&tune);
    let first = notes[0];
    assert_eq!(first.attachments.chord_symbols[0].text, "F");
    assert_eq!(first.attachments.grace_groups.len(), 1);
    assert!(
        first
            .attachments
            .slurs
            .iter()
            .any(|slur| slur.role == SlurRole::Start)
    );
    assert!(
        notes[1]
            .attachments
            .slurs
            .iter()
            .any(|slur| slur.role == SlurRole::Stop)
    );
}

#[test]
fn chord_symbol_before_tuplet_marker_binds_to_first_tuplet_note() {
    // `"F"(3CDE F`: the chord symbol rides across the tuplet marker and binds
    // to `C`; the tuplet roles stay intact.
    let source = "X:1\nL:1/8\nK:C\n\"F\"(3CDE F|\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
    let notes = semantic_note_events(&tune);
    let first = notes[0];
    assert_eq!(first.attachments.chord_symbols[0].text, "F");
    assert!(
        first
            .attachments
            .tuplets
            .iter()
            .any(|tuplet| tuplet.role == TupletRole::Start)
    );
    assert!(
        notes[2]
            .attachments
            .tuplets
            .iter()
            .any(|tuplet| tuplet.role == TupletRole::Stop)
    );
    assert!(notes[3].attachments.chord_symbols.is_empty());
    assert!(notes[3].attachments.tuplets.is_empty());
}

/// `(kind, tuplet roles)` for every note/rest/chord in the first voice.
/// `semantic_note_events` filters to notes only, which would hide the rests
/// these tests are about.
fn timed_tuplet_roles(tune: &crate::model::Tune) -> Vec<(&'static str, Vec<TupletRole>)> {
    tune.score.parts[0].voices[0]
        .events
        .iter()
        .filter_map(|event| {
            let kind = match &event.kind {
                TimedEventKind::Note(_) => "note",
                TimedEventKind::Rest(_) => "rest",
                TimedEventKind::Chord(_) => "chord",
                _ => return None,
            };
            let roles = event
                .attachments
                .tuplets
                .iter()
                .map(|tuplet| tuplet.role)
                .collect();
            Some((kind, roles))
        })
        .collect()
}

#[test]
fn rest_led_tuplet_carries_start_role_on_the_rest() {
    // `(3zBA`: the leading rest is the tuplet's first group, so it carries the
    // Start role just as a note would.
    let source = "X:1\nL:1/8\nK:C\n(3zBA|\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
    assert_eq!(
        timed_tuplet_roles(&tune),
        vec![
            ("rest", vec![TupletRole::Start]),
            ("note", vec![TupletRole::Continue]),
            ("note", vec![TupletRole::Stop]),
        ]
    );
}

#[test]
fn rest_closed_tuplet_carries_stop_role_on_the_rest() {
    // `(3BAz`: the trailing rest closes the tuplet, so it carries Stop.
    let source = "X:1\nL:1/8\nK:C\n(3BAz|\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
    assert_eq!(
        timed_tuplet_roles(&tune),
        vec![
            ("note", vec![TupletRole::Start]),
            ("note", vec![TupletRole::Continue]),
            ("rest", vec![TupletRole::Stop]),
        ]
    );
}

#[test]
fn all_rest_tuplet_carries_roles_on_every_rest() {
    let source = "X:1\nL:1/8\nK:C\n(3zzz|\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
    assert_eq!(
        timed_tuplet_roles(&tune),
        vec![
            ("rest", vec![TupletRole::Start]),
            ("rest", vec![TupletRole::Continue]),
            ("rest", vec![TupletRole::Stop]),
        ]
    );
}

#[test]
fn rest_led_tuplet_keeps_prefix_attachments_on_the_rest() {
    // `(3"C"zBA`: the chord symbol rides across the tuplet marker and binds to
    // the leading rest, which still carries the Start role.
    let source = "X:1\nL:1/8\nK:C\n(3\"C\"zBA|\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
    let rest = tune.score.parts[0].voices[0]
        .events
        .iter()
        .find(|event| matches!(event.kind, TimedEventKind::Rest(_)))
        .expect("expected a rest event");
    assert_eq!(rest.attachments.chord_symbols[0].text, "C");
    assert!(
        rest.attachments
            .tuplets
            .iter()
            .any(|tuplet| tuplet.role == TupletRole::Start)
    );
}

#[test]
fn quoted_texts_before_and_after_grace_group_keep_order() {
    // `"F"{AB}"G"c`: both chord symbols bind to `c`, in source order.
    let source = "X:1\nL:1/8\nK:C\n\"F\"{AB}\"G\"c d|\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
    let notes = semantic_note_events(&tune);
    let first = notes[0];
    assert_eq!(
        first
            .attachments
            .chord_symbols
            .iter()
            .map(|symbol| symbol.text.as_str())
            .collect::<Vec<_>>(),
        vec!["F", "G"]
    );
    assert_eq!(first.attachments.grace_groups.len(), 1);
}

#[test]
fn decoration_before_grace_group_binds_to_main_note() {
    // `!trill!{AB}c`: the decoration rides the same pending bundle as quoted
    // text, so it must survive the grace group and bind to `c`.
    let source = "X:1\nL:1/8\nK:C\n!trill!{AB}c d|\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
    let notes = semantic_note_events(&tune);
    let first = notes[0];
    assert_eq!(first.attachments.decorations[0].name, "trill");
    assert_eq!(first.attachments.grace_groups.len(), 1);
    assert!(notes[1].attachments.decorations.is_empty());
}

#[test]
fn quoted_text_after_grace_or_inside_slur_stays_bound() {
    // Controls that already worked before the prefix-attachment fix: quoted
    // text AFTER the grace group, and INSIDE the slur parens.
    let (tune, diagnostics) = tune_for("X:1\nL:1/8\nK:C\n{AB}\"F\"c d|\n");
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
    let notes = semantic_note_events(&tune);
    assert_eq!(notes[0].attachments.chord_symbols[0].text, "F");
    assert_eq!(notes[0].attachments.grace_groups.len(), 1);

    let (tune, diagnostics) = tune_for("X:1\nL:1/8\nK:C\n(\"G7\"DE) F|\n");
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
    let notes = semantic_note_events(&tune);
    assert_eq!(notes[0].attachments.chord_symbols[0].text, "G7");
    assert!(
        notes[0]
            .attachments
            .slurs
            .iter()
            .any(|slur| slur.role == SlurRole::Start)
    );
}

#[test]
fn unclosed_grace_group_after_quoted_text_still_diagnoses() {
    // `"F"{AB c`: the unclosed grace group swallows the rest of the line and
    // must keep emitting its diagnostic; the pending chord symbol must not
    // suppress it or panic, and it must not leak onto the next line's notes.
    let source = "X:1\nL:1/8\nK:C\n\"F\"{AB c\nd e|\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        count_diagnostics(&diagnostics, "abc.music.unclosed_grace"),
        1
    );
    let notes = semantic_note_events(&tune);
    assert!(
        notes
            .iter()
            .all(|note| note.attachments.chord_symbols.is_empty())
    );
}

#[test]
fn broken_rhythm_without_neighbors_diagnoses_and_keeps_timing_stable() {
    let source = "X:1\nL:1/8\nK:C\n< C D >|\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(source, &diagnostics, "abc.music.broken_rhythm.missing_left"),
        "<"
    );
    assert_eq!(
        diagnostic_span(
            source,
            &diagnostics,
            "abc.music.broken_rhythm.missing_right"
        ),
        ">"
    );
    let durations = semantic_note_events(&tune)
        .into_iter()
        .map(|event| event.duration)
        .collect::<Vec<_>>();
    assert_eq!(durations, vec![Fraction::new(1, 8), Fraction::new(1, 8)]);
    assert_eq!(
        tune.score.parts[0].voices[0].measures[0].actual_duration,
        Fraction::new(2, 8)
    );
}

#[test]
fn broken_rhythm_after_barline_does_not_bind_across_bar() {
    // ABC 2.1 §4.4: `>` dots the *previous* note. After a barline there is no
    // previous note for a leading `>`, so it must be void: `c2` keeps 1/4 and
    // `d` keeps 1/8 instead of dotting `c2` across the bar.
    let source = "X:1\nL:1/8\nK:C\nc2|>d e\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(source, &diagnostics, "abc.music.broken_rhythm.missing_left"),
        ">"
    );
    let durations = semantic_note_events(&tune)
        .into_iter()
        .map(|event| event.duration)
        .collect::<Vec<_>>();
    assert_eq!(
        durations,
        vec![
            Fraction::new(2, 8),
            Fraction::new(1, 8),
            Fraction::new(1, 8)
        ]
    );
}

#[test]
fn broken_rhythm_after_barline_single_note_left_does_not_bind() {
    let source = "X:1\nL:1/8\nK:C\nB|>d e\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(source, &diagnostics, "abc.music.broken_rhythm.missing_left"),
        ">"
    );
    let durations = semantic_note_events(&tune)
        .into_iter()
        .map(|event| event.duration)
        .collect::<Vec<_>>();
    assert_eq!(
        durations,
        vec![
            Fraction::new(1, 8),
            Fraction::new(1, 8),
            Fraction::new(1, 8)
        ]
    );
}

#[test]
fn broken_rhythm_within_measure_still_applies() {
    let source = "X:1\nL:1/8\nK:C\na>b\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty());
    let durations = semantic_note_events(&tune)
        .into_iter()
        .map(|event| event.duration)
        .collect::<Vec<_>>();
    assert_eq!(durations, vec![Fraction::new(3, 16), Fraction::new(1, 16)]);
}

#[test]
fn broken_rhythm_dangling_right_still_void() {
    let source = "X:1\nL:1/8\nK:C\nA>|B c\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(
            source,
            &diagnostics,
            "abc.music.broken_rhythm.missing_right"
        ),
        ">"
    );
    let durations = semantic_note_events(&tune)
        .into_iter()
        .map(|event| event.duration)
        .collect::<Vec<_>>();
    assert_eq!(
        durations,
        vec![
            Fraction::new(1, 8),
            Fraction::new(1, 8),
            Fraction::new(1, 8)
        ]
    );
}

#[test]
fn broken_rhythm_same_measure_multi_unit_left_still_applies() {
    let source = "X:1\nL:1/8\nK:C\nc2>d e\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty());
    let durations = semantic_note_events(&tune)
        .into_iter()
        .map(|event| event.duration)
        .collect::<Vec<_>>();
    assert_eq!(
        durations,
        vec![
            Fraction::new(3, 8),
            Fraction::new(1, 16),
            Fraction::new(1, 8)
        ]
    );
}

#[test]
fn broken_rhythm_grace_transparency_still_applies() {
    let source = "X:1\nL:1/8\nK:C\nc>{g}d\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(diagnostics.is_empty());
    let durations = semantic_note_events(&tune)
        .into_iter()
        .map(|event| event.duration)
        .collect::<Vec<_>>();
    assert_eq!(durations, vec![Fraction::new(3, 16), Fraction::new(1, 16)]);
}

#[test]
fn short_tuplet_does_not_consume_notes_after_barline() {
    let source = "X:1\nL:1/8\nK:C\n(3C|D E|\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(source, &diagnostics, "abc.music.tuplet.too_few_notes"),
        "(3"
    );
    let durations = semantic_note_events(&tune)
        .into_iter()
        .map(|event| event.duration)
        .collect::<Vec<_>>();
    assert_eq!(
        durations,
        vec![
            Fraction::new(1, 12),
            Fraction::new(1, 8),
            Fraction::new(1, 8)
        ]
    );
    assert_eq!(
        tune.score.parts[0].voices[0].measures[1].actual_duration,
        Fraction::new(2, 8)
    );
}

#[test]
fn unmatched_tie_preserves_unmerged_note_events() {
    let source = "X:1\nL:1/8\nK:C\nC- D E\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(source, &diagnostics, "abc.music.unmatched_tie"),
        "-"
    );
    let notes = semantic_note_events(&tune);
    assert_eq!(notes.len(), 3);
    assert!(notes.iter().all(|event| event.attachments.ties.is_empty()));
    assert_eq!(
        notes.iter().map(|event| event.duration).collect::<Vec<_>>(),
        vec![
            Fraction::new(1, 8),
            Fraction::new(1, 8),
            Fraction::new(1, 8)
        ]
    );
}

#[test]
fn ties_resolve_across_barlines_without_changing_measure_timing() {
    let source = "X:1\nM:2/4\nL:1/4\nK:C\nC- | C D |\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.unmatched_tie")
    );
    let notes = semantic_note_events(&tune);
    assert_eq!(notes.len(), 3);
    assert_eq!(
        notes
            .iter()
            .map(|event| event.attachments.ties.first().map(|tie| tie.role))
            .collect::<Vec<_>>(),
        vec![Some(TieRole::Start), Some(TieRole::Stop), None]
    );
    assert_eq!(
        notes[0].attachments.ties[0].pair_id,
        notes[1].attachments.ties[0].pair_id
    );
    assert_eq!(
        notes.iter().map(|event| event.duration).collect::<Vec<_>>(),
        vec![
            Fraction::new(1, 4),
            Fraction::new(1, 4),
            Fraction::new(1, 4)
        ]
    );
    let measures = &tune.score.parts[0].voices[0].measures;
    assert_eq!(
        measures
            .iter()
            .map(|measure| measure.actual_duration)
            .collect::<Vec<_>>(),
        vec![Fraction::new(1, 4), Fraction::new(2, 4)]
    );
    assert!(measures[0].pickup);
    assert!(measures[1].complete);
}

#[test]
fn dropped_tie_does_not_leak_accidental_across_barline() {
    // The tie on `^a-` finds no matching stop note (`b` mismatches) and is
    // dropped; the accidental carry preserved across the barline on the tie's
    // behalf must be undone with it, so the measure-2 `a`s stay natural.
    let source = "X:1\nL:1/8\nK:C\n^a- | b a a\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(source, &diagnostics, "abc.music.unmatched_tie"),
        "-"
    );
    assert_eq!(semantic_note_alters(&tune), vec![1, 0, 0, 0]);
    let notes = semantic_note_events(&tune);
    assert!(notes.iter().all(|event| event.attachments.ties.is_empty()));
}

#[test]
fn dropped_tie_at_rest_does_not_leak_accidental_across_barline() {
    // The pending tie is dropped when the rest arrives; the barline-preserved
    // accidental carry must be dropped with it.
    let source = "X:1\nL:1/8\nK:C\n^a- | z a\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(source, &diagnostics, "abc.music.unmatched_tie"),
        "-"
    );
    assert_eq!(semantic_note_alters(&tune), vec![1, 0]);
}

#[test]
fn chord_tie_partial_drop_keeps_matched_tie_and_drops_leaked_accidental() {
    // The C tie matches the next `c`; the A tie is dropped in the same drain.
    // The dropped A tie must not leak its sharp onto the trailing `a`, while
    // the matched C tie stays intact.
    let source = "X:1\nL:1/8\nK:C\n[^ac]- | c a\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(source, &diagnostics, "abc.music.unmatched_tie"),
        "-"
    );
    let chord = tune.score.parts[0].voices[0]
        .events
        .iter()
        .find_map(|event| match &event.kind {
            TimedEventKind::Chord(chord) => Some(chord),
            _ => None,
        })
        .expect("expected chord");
    assert_eq!(
        chord
            .members
            .iter()
            .map(|member| (member.pitch.step, member.pitch.alter))
            .collect::<Vec<_>>(),
        vec![('A', 1), ('C', 0)]
    );
    // The A tie is dropped (no matching stop note); the C tie survives.
    assert!(chord.members[0].attachments.ties.is_empty());
    assert_eq!(chord.members[1].attachments.ties.len(), 1);
    assert_eq!(chord.members[1].attachments.ties[0].role, TieRole::Start);

    // Measure 2: the tie-stop `c` and the trailing `a`, which must NOT
    // inherit the dropped A tie's sharp.
    assert_eq!(semantic_note_alters(&tune), vec![0, 0]);
    let notes = semantic_note_events(&tune);
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].attachments.ties.len(), 1);
    assert_eq!(notes[0].attachments.ties[0].role, TieRole::Stop);
    assert_eq!(
        notes[0].attachments.ties[0].pair_id,
        chord.members[1].attachments.ties[0].pair_id
    );
    assert!(notes[1].attachments.ties.is_empty());
}

#[test]
fn rewritten_accidental_after_dropped_tie_carries_within_measure() {
    // After the dropped tie's leaked carry is undone, a freshly written `^a`
    // in measure 2 must still propagate to the rest of that measure.
    let source = "X:1\nL:1/8\nK:C\n^a- | b ^a a\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(source, &diagnostics, "abc.music.unmatched_tie"),
        "-"
    );
    assert_eq!(semantic_note_alters(&tune), vec![1, 0, 1, 1]);
}

#[test]
fn tie_across_barline_preserves_accidental_carry() {
    // Legit cross-bar tie: the stop note keeps the start note's sharp.
    let source = "X:1\nL:1/8\nK:C\n^a- | a\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.unmatched_tie")
    );
    assert_eq!(semantic_note_alters(&tune), vec![1, 1]);
    let notes = semantic_note_events(&tune);
    assert_eq!(
        notes
            .iter()
            .map(|event| event.attachments.ties.first().map(|tie| tie.role))
            .collect::<Vec<_>>(),
        vec![Some(TieRole::Start), Some(TieRole::Stop)]
    );
    assert_eq!(
        notes[0].attachments.ties[0].pair_id,
        notes[1].attachments.ties[0].pair_id
    );
}

#[test]
fn same_measure_dropped_tie_keeps_written_accidental_carry() {
    // A tie dropped within the measure must not cancel the carry that comes
    // from the WRITTEN accidental itself: `^a-b a` stays A#, B, A#.
    let source = "X:1\nL:1/8\nK:C\n^a-b a\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(source, &diagnostics, "abc.music.unmatched_tie"),
        "-"
    );
    assert_eq!(semantic_note_alters(&tune), vec![1, 0, 1]);
}

#[test]
fn matched_tie_stop_accidental_persists_for_rest_of_measure() {
    // A matched cross-bar tie carries the sharp onto the stop note, and the
    // carried accidental persists for the rest of the stop note's measure.
    let source = "X:1\nL:1/8\nK:C\n^a- | a a\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.unmatched_tie")
    );
    assert_eq!(semantic_note_alters(&tune), vec![1, 1, 1]);
}

#[test]
fn tie_across_barline_with_rewritten_accidental_keeps_carry() {
    // The stop note re-writes the same sharp; the tie still matches and the
    // carry persists for the rest of the measure.
    let source = "X:1\nL:1/8\nK:C\n^a- | ^a a\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.unmatched_tie")
    );
    assert_eq!(semantic_note_alters(&tune), vec![1, 1, 1]);
}

#[test]
fn tie_across_double_barline_preserves_accidental_carry() {
    let source = "X:1\nL:1/8\nK:C\n^a- || a\n";
    let (tune, diagnostics) = tune_for(source);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "abc.music.unmatched_tie")
    );
    assert_eq!(semantic_note_alters(&tune), vec![1, 1]);
    let notes = semantic_note_events(&tune);
    assert_eq!(
        notes
            .iter()
            .map(|event| event.attachments.ties.first().map(|tie| tie.role))
            .collect::<Vec<_>>(),
        vec![Some(TieRole::Start), Some(TieRole::Stop)]
    );
}

#[test]
fn crossing_slurs_diagnose_but_preserve_notes() {
    let source = "X:1\nL:1/8\nK:C\n.(C (D .) E)\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(source, &diagnostics, "abc.music.crossing_slur"),
        ".)"
    );
    assert_eq!(semantic_note_events(&tune).len(), 3);
    assert_eq!(
        semantic_note_events(&tune)
            .into_iter()
            .map(|event| event.duration)
            .collect::<Vec<_>>(),
        vec![
            Fraction::new(1, 8),
            Fraction::new(1, 8),
            Fraction::new(1, 8)
        ]
    );
}

#[test]
fn variable_chord_members_preserve_chord_shape_and_following_timing() {
    let source = "X:1\nL:1/8\nK:C\n[E2G,6] C\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(source, &diagnostics, "abc.music.chord.variable_duration"),
        "[E2G,6]"
    );
    let events = &tune.score.parts[0].voices[0].events;
    let chord = events
        .iter()
        .find_map(|event| match &event.kind {
            TimedEventKind::Chord(chord) => Some(chord),
            _ => None,
        })
        .expect("expected chord");
    assert_eq!(chord.members.len(), 2);
    assert_eq!(
        chord
            .members
            .iter()
            .map(|member| member.duration)
            .collect::<Vec<_>>(),
        vec![Fraction::new(2, 8), Fraction::new(6, 8)]
    );
    let note_after = events
        .iter()
        .find(|event| {
            matches!(event.kind, TimedEventKind::Note(_)) && event.onset == Fraction::new(2, 8)
        })
        .expect("expected following note");
    assert_eq!(note_after.onset, Fraction::new(2, 8));
}

#[test]
fn overlay_incomplete_duration_does_not_shift_base_timeline() {
    let source = "X:1\nL:1/8\nK:C\nC D & E|\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(source, &diagnostics, "abc.voice.overlay_incomplete_measure"),
        "&"
    );
    let base_notes = semantic_note_events(&tune)
        .into_iter()
        .map(|event| (event.onset, event.duration))
        .collect::<Vec<_>>();
    assert_eq!(
        base_notes,
        vec![
            (Fraction::zero(), Fraction::new(1, 8)),
            (Fraction::new(1, 8), Fraction::new(1, 8)),
        ]
    );
}

#[test]
fn lyric_count_mismatches_attach_valid_syllables_only() {
    let source = "X:1\nL:1/8\nK:C\nC D E F|\nw: one two\nw: a b c d e\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        count_diagnostics(&diagnostics, "abc.lyric.syllable_count"),
        2
    );
    let lyrics = semantic_note_events(&tune)
        .into_iter()
        .flat_map(|event| event.attachments.lyrics.iter())
        .filter(|lyric| lyric.control == LyricControl::Syllable)
        .map(|lyric| lyric.text.as_str())
        .collect::<Vec<_>>();
    assert_eq!(lyrics, vec!["one", "a", "two", "b", "c", "d"]);
    assert_eq!(semantic_note_events(&tune).len(), 4);
}

#[test]
fn invalid_state_changes_keep_previous_state_and_valid_music() {
    let source = "X:1\nM:2/4\nL:1/8\nK:C\nC D|\nM:bad\nK:???\nL:bad\nE F|\n";
    let document = parse_document(source, ParseOptions::default());
    assert_eq!(
        diagnostic_span(source, &document.diagnostics, "abc.field.invalid_l"),
        "bad"
    );
    let report = parse_tune_report_from_document(&document.value);
    let tune = report.value.expect("expected tune");

    assert_eq!(
        diagnostic_span(source, &report.diagnostics, "abc.field.invalid_m"),
        "bad"
    );
    assert_eq!(
        diagnostic_span(source, &report.diagnostics, "abc.field.invalid_k"),
        "???"
    );
    let notes = semantic_note_events(&tune);
    assert_eq!(notes.len(), 4);
    assert!(
        notes
            .iter()
            .all(|event| event.duration == Fraction::new(1, 8))
    );
    assert_eq!(
        tune.score.parts[0].voices[0].measures[1].expected_duration,
        Some(Fraction::new(2, 4))
    );
}

#[test]
fn malformed_repeat_ending_keeps_barline_timing_intact() {
    let source = "X:1\nL:1/8\nK:C\nC|[1- D|E|\n";
    let document = parse_document(source, ParseOptions::default());
    assert_eq!(
        diagnostic_span(
            source,
            &document.diagnostics,
            "abc.music.invalid_repeat_ending"
        ),
        // `|[1-` parses as a barline plus the `[1` variant ending, so the
        // malformed-ending diagnostic spans the whole `[1-`.
        "[1-"
    );
    let report = parse_tune_report_from_document(&document.value);
    let tune = report.value.expect("expected tune");

    assert_eq!(semantic_note_events(&tune).len(), 3);
    assert_eq!(
        tune.score.parts[0].voices[0]
            .measures
            .iter()
            .map(|measure| measure.actual_duration)
            .collect::<Vec<_>>(),
        vec![
            Fraction::new(1, 8),
            Fraction::new(1, 8),
            Fraction::new(1, 8)
        ]
    );
}

#[test]
fn inline_instruction_field_warns_and_changes_nothing() {
    // `[I:tuplets 1 0 0]` is an abcm2ps DISPLAY directive: it cannot change
    // how `(3CDE` parses (abc2xml skips it too). Lowering drops it but must
    // say so — previously the `_ => {}` arm swallowed it silently — and the
    // music on either side must lower identically.
    let source = "X:1\nL:1/8\nK:C\n(3CDE [I:tuplets 1 0 0](3CDE|\n";
    let document = parse_document(source, ParseOptions::default());
    let report = parse_tune_report_from_document(&document.value);
    let tune = report.value.expect("expected tune");

    assert_eq!(
        diagnostic_span(source, &report.diagnostics, "abc.field.inline_ignored"),
        "[I:tuplets 1 0 0]"
    );
    let notes = tune
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Note {
                step,
                octave,
                duration,
                ..
            } => Some((*step, *octave, *duration)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(notes.len(), 6);
    assert_eq!(notes[..3], notes[3..], "music differs across the inline I:");
}

#[test]
fn unsupported_directive_is_metadata_not_music() {
    let source = "X:1\nK:C\n%%foo bar\nC D|\n";
    let (tune, diagnostics) = tune_for(source);

    assert_eq!(
        diagnostic_span(source, &diagnostics, "abc.directive.unsupported"),
        "foo"
    );
    assert_eq!(tune.score.metadata.preserved_directives[0].name.text, "foo");
    assert_eq!(semantic_note_events(&tune).len(), 2);
    assert_eq!(
        tune.score.parts[0].voices[0].measures[0].actual_duration,
        Fraction::new(2, 8)
    );
}

#[test]
fn mid_tune_key_and_meter_changes_reach_the_score() {
    let source = "X:1\nL:1/4\nK:C\nCDEF|[K:F]GAB_B|[M:3/4]ABc|\n";
    let doc = crate::parse_document(source, crate::ParseOptions::default());
    let score = crate::lower_score(&doc.value, crate::LowerOptions)
        .value
        .expect("score");
    let events = &score.parts[0].voices[0].events;
    let keys: Vec<&crate::model::KeySignatureModel> = events
        .iter()
        .filter_map(|e| match &e.kind {
            crate::TimedEventKind::KeyChange(k) => Some(k),
            _ => None,
        })
        .collect();
    let meters: Vec<&crate::model::MeterModel> = events
        .iter()
        .filter_map(|e| match &e.kind {
            crate::TimedEventKind::MeterChange(m) => Some(m),
            _ => None,
        })
        .collect();
    assert_eq!(keys.len(), 1, "one key change event");
    assert_eq!(keys[0].display, "F");
    assert_eq!(keys[0].fifths, -1);
    assert_eq!(meters.len(), 1, "one meter change event");
    assert_eq!(meters[0].display, "3/4");
    // header metadata stays the header values
    assert_eq!(
        score.metadata.key.as_ref().map(|k| k.display.as_str()),
        Some("C")
    );
    // positions: key change directly after the first barline, before the G
    let key_idx = events
        .iter()
        .position(|e| matches!(e.kind, crate::TimedEventKind::KeyChange(_)))
        .expect("key change present");
    assert!(key_idx > 0, "key change is not the first event");
    assert!(matches!(
        events[key_idx - 1].kind,
        crate::TimedEventKind::Barline(_)
    ));
}

#[test]
fn standalone_body_key_line_broadcasts_to_all_voices() {
    let source = "X:1\nL:1/4\nK:C\nV:1\nCDEF|\nK:D\nV:1\nFGAF|\nV:2\nCDEF|\nFGAF|\n";
    let doc = crate::parse_document(source, crate::ParseOptions::default());
    let score = crate::lower_score(&doc.value, crate::LowerOptions)
        .value
        .expect("score");
    let key_changes_per_voice: Vec<usize> = score
        .parts
        .iter()
        .flat_map(|p| &p.voices)
        .map(|v| {
            v.events
                .iter()
                .filter(|e| matches!(e.kind, crate::TimedEventKind::KeyChange(_)))
                .count()
        })
        .collect();
    assert!(
        key_changes_per_voice.iter().all(|&n| n == 1),
        "every voice records the broadcast key change: {key_changes_per_voice:?}"
    );
}
