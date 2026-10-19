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

/// `(program, channel, volume_cc, pan_cc)` — a [`MidiInstrumentModel`]
/// stripped of its source span, which legitimately differs after re-parsing.
type MidiFields = (Option<u8>, Option<u8>, Option<u8>, Option<u8>);

/// Score-meaningful MIDI fields per voice, comparable across a write ->
/// re-parse cycle (span excluded).
fn midi_seq(score: &crate::Score) -> Vec<(Option<MidiFields>, Option<i16>)> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            let instrument = voice
                .midi_instrument
                .map(|m| (m.program, m.channel, m.volume_cc, m.pan_cc));
            v.push((instrument, voice.midi_transpose));
        }
    }
    v
}

#[test]
fn midi_instrument_directives_roundtrip() {
    // `%%MIDI program`/`channel`/`control` CC7-10/`transpose` carry
    // score-meaningful sound metadata (forward-translated to MusicXML
    // `<midi-instrument>`). The writer must re-emit them per voice so a Score
    // that carries them — e.g. one built by the MusicXML reader — survives a
    // write -> re-parse cycle instead of collapsing every voice to channel 1.
    let src = concat!(
        "X:1\nL:1/8\nK:C\n",
        "V:1\n%%MIDI program 1 52\n%%MIDI control 7 80\n%%MIDI control 10 64\n",
        "%%MIDI transpose -12\nCDEF\n",
        "V:2\n%%MIDI program 2 0\nGABc\n",
    );
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());
    assert!(
        abc.contains("%%MIDI program 1 52"),
        "writer dropped the MIDI instrument: {abc:?}"
    );
    let s2 = score_of(&abc);
    assert_eq!(midi_seq(&s1), midi_seq(&s2), "MIDI round-trip via {abc:?}");
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

#[test]
fn extreme_octave_marks_roundtrip_without_panicking() {
    // Lowering clamps the octave-mark sum into i8 range, so a 130-comma
    // run stores octave -126 and 200 up-marks store octave 127. The
    // writer's down-mark repeat count `4 - octave` used to overflow i8
    // (debug panic / release capacity overflow); it must widen instead.
    let commas = ",".repeat(130);
    let quotes = "'".repeat(200);
    let src = format!("X:1\nL:1/4\nK:C\nC{commas} c{quotes} D E |\n");
    let (a, b) = roundtrip_pitches(&src);
    assert_eq!(a, b, "pitch round-trip failed for extreme octave marks");
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

fn ending_labels(score: &crate::Score) -> Vec<String> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for e in &voice.events {
                if let crate::TimedEventKind::RepeatEnding(r) = &e.kind {
                    v.push(format!("{:?}", r.endings));
                }
            }
        }
    }
    v
}

#[test]
fn endings_and_ties_roundtrip() {
    let src = "X:1\nL:1/4\nK:C\n|: CDEF |1 GABc :|2 cBAG |]\n";
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());
    let s2 = score_of(&abc);
    assert_eq!(ending_labels(&s1), ending_labels(&s2), "endings: {abc:?}");

    let tie = score_of("X:1\nL:1/4\nK:C\nC2- C2 |\n");
    let tie_abc = write_abc(&tie, AbcWriteOptions::default());
    assert!(tie_abc.contains('-'), "tie not emitted: {tie_abc:?}");
    assert_eq!(pitch_seq(&tie), pitch_seq(&score_of(&tie_abc)));
}

fn slur_roles(score: &crate::Score) -> Vec<String> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for e in &voice.events {
                for s in &e.attachments.slurs {
                    v.push(format!("{:?}", s.role));
                }
            }
        }
    }
    v
}

#[test]
fn slurs_roundtrip() {
    for src in [
        "X:1\nL:1/8\nK:C\n(CDE) F |\n",
        "X:1\nL:1/8\nK:C\n(C (DE) F) G |\n",
        "X:1\nL:1/4\nK:C\nC (D-D) E |\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(
            slur_roles(&s1),
            slur_roles(&s2),
            "slurs for {src:?} -> {abc:?}"
        );
        assert_eq!(
            pitch_seq(&s1),
            pitch_seq(&s2),
            "pitches for {src:?} -> {abc:?}"
        );
    }
}

fn decoration_names(score: &crate::Score) -> Vec<String> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for e in &voice.events {
                for d in &e.attachments.decorations {
                    v.push(d.name.clone());
                }
            }
        }
    }
    v
}

#[test]
fn decorations_roundtrip() {
    for src in [
        "X:1\nL:1/8\nK:C\n!fermata!C .D ~E !trill!F |\n",
        "X:1\nL:1/8\nK:C\n!accent!G !upbow!A !downbow!B |\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(
            decoration_names(&s1),
            decoration_names(&s2),
            "decorations for {src:?} -> {abc:?}"
        );
    }
}

fn text_attachments(score: &crate::Score) -> Vec<String> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for e in &voice.events {
                for c in &e.attachments.chord_symbols {
                    v.push(format!("cs:{}", c.text));
                }
                for a in &e.attachments.annotations {
                    v.push(format!("an:{}", a.text));
                }
            }
        }
    }
    v
}

#[test]
fn chord_symbols_and_annotations_roundtrip() {
    for src in [
        "X:1\nL:1/8\nK:C\n\"Gm7\"C \"C\"D \"F#m\"E |\n",
        "X:1\nL:1/8\nK:C\n\"^above\"C \"_below\"D \"<left\"E \">right\"F |\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(
            text_attachments(&s1),
            text_attachments(&s2),
            "text attachments for {src:?} -> {abc:?}"
        );
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
    }
}

fn chord_pitches(score: &crate::Score) -> Vec<Vec<(char, i8, i8)>> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for e in &voice.events {
                if let crate::TimedEventKind::Chord(c) = &e.kind {
                    v.push(
                        c.members
                            .iter()
                            .map(|m| (m.pitch.step, m.pitch.alter, m.pitch.octave))
                            .collect(),
                    );
                }
            }
        }
    }
    v
}

#[test]
fn chords_roundtrip() {
    for src in [
        "X:1\nL:1/8\nK:C\n[CEG]2 [DFA] z |\n",
        "X:1\nL:1/8\nK:C\n[^C_EG]/ [ceg]4 |\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(
            chord_pitches(&s1),
            chord_pitches(&s2),
            "chords for {src:?} -> {abc:?}"
        );
    }
}

fn tuplet_ratios(score: &crate::Score) -> Vec<(u32, u32)> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for e in &voice.events {
                for t in &e.attachments.tuplets {
                    v.push((t.actual_notes, t.normal_notes));
                }
            }
        }
    }
    v
}

/// `(is_rest, role)` per tuplet attachment, in event order — ratios alone
/// can match even when a role landed on the wrong event (or nowhere).
fn tuplet_roles(score: &crate::Score) -> Vec<(bool, crate::model::TupletRole)> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for e in &voice.events {
                let is_rest = matches!(e.kind, crate::TimedEventKind::Rest(_));
                for t in &e.attachments.tuplets {
                    v.push((is_rest, t.role));
                }
            }
        }
    }
    v
}

#[test]
fn tuplets_roundtrip() {
    for src in [
        "X:1\nM:4/4\nL:1/8\nK:C\n(3CDE F2 (3GAB |\n",
        "X:1\nM:4/4\nL:1/8\nK:C\n(5CDEFG (3:2:3 cde |\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(
            tuplet_ratios(&s1),
            tuplet_ratios(&s2),
            "tuplets for {src:?} -> {abc:?}"
        );
        assert_eq!(tuplet_roles(&s1), tuplet_roles(&s2), "roles for {src:?}");
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
    }
}

#[test]
fn nested_tuplets_roundtrip_preserves_outer_and_inner_groups() {
    let src = "X:1\nM:C\nL:1/4\nK:none\n(7:8:8(3A/A/A/ A/A/A/A/A/|\n";
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());

    assert!(abc.contains("(7:8:8"), "{abc}");
    assert!(abc.contains("(3:2:3"), "{abc}");
    assert!(!abc.contains("A8/7"), "{abc}");
    let s2 = score_of(&abc);
    assert_eq!(tuplet_ratios(&s1), tuplet_ratios(&s2), "{abc}");
    assert_eq!(tuplet_roles(&s1), tuplet_roles(&s2), "{abc}");
    assert_eq!(pitch_seq(&s1), pitch_seq(&s2), "{abc}");
}

#[test]
fn overflowing_nested_tuplet_product_does_not_saturate_notated_length() {
    let src = concat!(
        "X:1\n",
        "M:C\n",
        "L:1/8\n",
        "K:C\n",
        "(9:8:1(9:8:1(9:8:1(9:8:1(9:8:1(9:8:1",
        "(9:8:1(9:8:1(9:8:1(9:8:1(9:8:1A|\n",
    );
    let abc = write_abc(&score_of(src), AbcWriteOptions::default());

    assert_eq!(abc.matches("(9:8:1").count(), 11, "{abc}");
    assert!(
        !abc.contains("A8"),
        "overflowing product must not be baked into a saturated length: {abc}"
    );
}

#[test]
fn rest_tuplets_roundtrip() {
    // Rests carry tuplet roles like notes do, so a rest-led (or rest-closed,
    // or all-rest) tuplet keeps its marker and span across a round-trip.
    for src in [
        "X:1\nM:4/4\nL:1/8\nK:C\n(3zBA F2 |\n",
        "X:1\nM:4/4\nL:1/8\nK:C\n(3BAz F2 |\n",
        "X:1\nM:4/4\nL:1/8\nK:C\n(3zzz F2 |\n",
        "X:1\nM:4/4\nL:1/8\nK:C\n(3:2:3z B A F2 |\n",
        "X:1\nM:4/4\nL:1/8\nK:C\n(3z>BA F2 |\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(
            tuplet_ratios(&s1),
            tuplet_ratios(&s2),
            "tuplets for {src:?} -> {abc:?}"
        );
        assert_eq!(
            tuplet_roles(&s1),
            tuplet_roles(&s2),
            "roles for {src:?} -> {abc:?}"
        );
        assert_eq!(
            pitch_seq(&s1),
            pitch_seq(&s2),
            "pitches for {src:?} -> {abc:?}"
        );
        let durations = |s: &crate::Score| {
            s.parts[0].voices[0]
                .events
                .iter()
                .filter_map(|e| match e.kind {
                    crate::TimedEventKind::Note(_) | crate::TimedEventKind::Rest(_) => {
                        Some(e.duration)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            durations(&s1),
            durations(&s2),
            "durations for {src:?} -> {abc:?}"
        );
    }
    // `(3BAz` previously dropped the Stop (it sat on the rest) and emitted a
    // wrong span of 2; the rest must count, giving `(3:2:3`.
    let s1 = score_of("X:1\nM:4/4\nL:1/8\nK:C\n(3BAz F2 |\n");
    let abc = write_abc(&s1, AbcWriteOptions::default());
    assert!(abc.contains("(3:2:3"), "expected (3:2:3 marker in {abc:?}");
}

fn grace_pitches(score: &crate::Score) -> Vec<(char, i8, i8, bool, bool)> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for e in &voice.events {
                for (after_grace, g) in e
                    .attachments
                    .grace_groups
                    .iter()
                    .map(|group| (false, group))
                    .chain(
                        e.attachments
                            .after_grace_groups
                            .iter()
                            .map(|group| (true, group)),
                    )
                {
                    let slash = g.slash.is_some();
                    for ge in &g.events {
                        if let crate::model::GraceEventKind::Note(n) = &ge.kind {
                            v.push((
                                n.pitch.step,
                                n.pitch.alter,
                                n.pitch.octave,
                                slash,
                                after_grace,
                            ));
                        }
                    }
                }
            }
        }
    }
    v
}

fn grace_slur_roles(score: &crate::Score) -> Vec<(bool, crate::model::SlurRole)> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for e in &voice.events {
                for (after_grace, g) in e
                    .attachments
                    .grace_groups
                    .iter()
                    .map(|group| (false, group))
                    .chain(
                        e.attachments
                            .after_grace_groups
                            .iter()
                            .map(|group| (true, group)),
                    )
                {
                    v.extend(g.slurs.iter().map(|slur| (after_grace, slur.role)));
                    for ge in &g.events {
                        v.extend(ge.slurs.iter().map(|slur| (after_grace, slur.role)));
                    }
                }
            }
        }
    }
    v
}

#[test]
fn grace_notes_roundtrip() {
    for src in [
        "X:1\nL:1/8\nK:C\n{ge}C {/d}E F2 |\n",
        "X:1\nL:1/8\nK:C\n{gege}A {^c}B |\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(
            grace_pitches(&s1),
            grace_pitches(&s2),
            "grace for {src:?} -> {abc:?}"
        );
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
    }
}

#[test]
fn grace_internal_slurs_roundtrip() {
    let src = "X:1\nL:1/8\nK:C\n{(fg)}a2 {(ef)}g2|]\n";
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());
    let s2 = score_of(&abc);

    assert!(
        abc.contains("{(fg)}a2 {(ef)}g2"),
        "grace-internal slurs missing from ABC: {abc:?}"
    );
    assert_eq!(grace_pitches(&s1), grace_pitches(&s2));
    assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
}

#[test]
fn bare_grace_slur_roundtrips() {
    let src = "X:1\nL:1/8\nK:C\n({Bc})D|\n";
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());
    let s2 = score_of(&abc);

    assert!(
        abc.contains("({Bc})D"),
        "bare grace slur missing from ABC: {abc:?}"
    );
    assert_eq!(grace_pitches(&s1), grace_pitches(&s2));
    assert_eq!(grace_slur_roles(&s1), grace_slur_roles(&s2));
    assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
}

#[test]
fn bare_grace_slur_before_barline_roundtrips() {
    let src = "X:1\nL:1/8\nK:C\nA2({Bc})|\n";
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());
    let s2 = score_of(&abc);

    assert!(
        abc.contains("A2([I:croma-after-grace]{Bc})"),
        "bare grace before barline missing from ABC: {abc:?}"
    );
    assert_eq!(grace_pitches(&s1), grace_pitches(&s2));
    assert_eq!(grace_slur_roles(&s1), grace_slur_roles(&s2));
    assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
}

#[test]
fn after_grace_carrier_roundtrips_on_chords() {
    let src = "X:1\nL:1/8\nK:C\n[CEG]8[I:croma-after-grace]{d} | D8 |\n";
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());
    let s2 = score_of(&abc);

    assert!(
        abc.contains("[CEG]8[I:croma-after-grace]{d}"),
        "chord after-grace carrier missing from ABC: {abc:?}"
    );
    assert_eq!(
        grace_pitches(&s1),
        grace_pitches(&s2),
        "grace for {src:?} -> {abc:?}"
    );
    assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
}

#[test]
fn trailing_trill_grace_notes_roundtrip_after_principal_note() {
    let src = "X:1\nT:Trailing Grace\nM:4/4\nL:1/8\nK:C\nTe6{de}|d2f f2f|\n";
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());
    let s2 = score_of(&abc);

    assert!(
        abc.contains("!trill!e6[I:croma-after-grace]{de}"),
        "after-grace suffix missing from ABC: {abc:?}"
    );
    assert_eq!(
        grace_pitches(&s1),
        grace_pitches(&s2),
        "grace for {src:?} -> {abc:?}"
    );
    assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
}

#[test]
fn chord_symbol_after_slur_open_survives() {
    // The writer emits the slur-open first (`("G7"DE)`), the canonical
    // order; the parser keeps the chord symbol in either order.
    let src = "X:1\nL:1/8\nK:C\n(\"G7\"DE) F |\n";
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());
    let s2 = score_of(&abc);
    assert_eq!(
        text_attachments(&s1),
        text_attachments(&s2),
        "chord symbol lost: {abc:?}"
    );
    assert!(!text_attachments(&s1).is_empty());
}

#[test]
fn chord_symbol_after_grace_survives() {
    // The writer emits the grace group first (`{AB}"F"c`), the canonical
    // order; the parser keeps the chord symbol in either order.
    let src = "X:1\nL:1/8\nK:C\n{AB}\"F\"c2 d2 |\n";
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());
    let s2 = score_of(&abc);
    assert_eq!(
        text_attachments(&s1),
        text_attachments(&s2),
        "chord symbol lost: {abc:?}"
    );
    assert!(!text_attachments(&s1).is_empty());
}

fn member_tie_starts(score: &crate::Score) -> Vec<Vec<bool>> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for e in &voice.events {
                if let crate::TimedEventKind::Chord(c) = &e.kind {
                    v.push(
                        c.members
                            .iter()
                            .map(|m| m.attachments.ties.iter().any(|t| t.role == TieRole::Start))
                            .collect(),
                    );
                }
            }
        }
    }
    v
}

#[test]
fn chord_member_ties_stay_per_member() {
    // `[dg-]` ties only g; the writer must not promote it to a whole-chord
    // tie (`[dg]-`), which would also tie d.
    for src in [
        "X:1\nL:1/8\nK:C\n[dg-]2 [dg]2 |\n",
        "X:1\nL:1/8\nK:C\n[C-E]2 [CE]2 |\n",
        "X:1\nL:1/8\nK:C\n[CE]2- [CE]2 |\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(
            member_tie_starts(&s1),
            member_tie_starts(&s2),
            "member ties for {src:?} -> {abc:?}"
        );
    }
}

fn harmony_texts(score: &crate::Score) -> Vec<String> {
    use crate::model::AlignedSymbolKind;
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for e in &voice.events {
                for c in &e.attachments.chord_symbols {
                    v.push(c.text.clone());
                }
                for s in &e.attachments.symbols {
                    if s.kind == AlignedSymbolKind::ChordSymbol {
                        v.push(s.text.clone());
                    }
                }
            }
        }
    }
    v
}

#[test]
fn aligned_chord_symbols_inline_roundtrip() {
    // `s:`-aligned chord symbols re-emit inline; the <harmony> sequence
    // (inline + aligned, in exporter order) must survive the round-trip.
    let src = "X:1\nL:1/4\nK:C\nC \"D7\"D [EG] F |\ns:\"Gm\" * \"C\"\n";
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());
    let s2 = score_of(&abc);
    assert_eq!(
        harmony_texts(&s1),
        harmony_texts(&s2),
        "harmony for {abc:?}"
    );
    assert_eq!(harmony_texts(&s1).len(), 3);
    assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
}

fn symbol_tokens(score: &crate::Score) -> Vec<(String, String)> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for e in &voice.events {
                for s in &e.attachments.symbols {
                    if s.kind != crate::model::AlignedSymbolKind::ChordSymbol {
                        v.push((format!("{:?}", s.kind), s.text.clone()));
                    }
                }
            }
            for measure in &voice.measures {
                for segment in &measure.overlays {
                    for e in &segment.events {
                        for s in &e.attachments.symbols {
                            if s.kind != crate::model::AlignedSymbolKind::ChordSymbol {
                                v.push((format!("{:?}", s.kind), s.text.clone()));
                            }
                        }
                    }
                }
            }
        }
    }
    v
}

#[test]
fn aligned_symbols_reemit_as_symbol_lines() {
    // Decoration/Annotation/Raw aligned symbols re-emit as s: lines (their
    // inline forms render different MusicXML), preserving kind + text +
    // note alignment; layered (adjacent) s: lines round-trip too.
    for src in [
        "X:1\nL:1/4\nK:C\nCDEF|\ns:!trill! * \"^slow\" foo\n",
        "X:1\nL:1/4\nK:C\nCD z EF|\ns:!trill! * !fermata!\ns:* +mordent+\n",
        "X:1\nL:1/4\nK:C\nC & E F|\ns:* \"^overlay\" !trill!\n",
        "X:1\nL:1/4\nK:C\nC & E & G|\ns:* \"^first\" \"^second\"\n",
        "X:1\nL:1/4\nK:C\nC D & z E|\ns:* * \"^after-rest\"\n",
        "X:1\nL:1/4\nK:C\nC & [EG] F|\ns:* \"^chord\" !trill!\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(
            symbol_tokens(&s1),
            symbol_tokens(&s2),
            "aligned symbols for {src:?} -> {abc:?}"
        );
        assert!(!symbol_tokens(&s1).is_empty());
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
    }
}

fn measure_count(score: &crate::Score) -> u32 {
    score.parts[0].voices[0]
        .events
        .iter()
        .map(|e| e.measure.index)
        .max()
        .map_or(0, |m| m + 1)
}

#[test]
fn exotic_barlines_roundtrip() {
    // Dotted and Invisible keep their kind; Initial and Liberal normalize
    // to Regular (structurally identical — neither renders a <barline>).
    for (src, same_kinds) in [
        ("X:1\nL:1/4\nK:C\nCD .| EF |\n", true),
        ("X:1\nL:1/4\nK:C\nCD [|] EF |\n", true),
        ("X:1\nL:1/4\nK:C\n[| CD EF |\n", false),
        ("X:1\nL:1/4\nK:C\nCD ||| EF |\n", false),
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2), "{abc:?}");
        assert_eq!(measure_count(&s1), measure_count(&s2), "{abc:?}");
        if same_kinds {
            assert_eq!(barline_kinds(&s1), barline_kinds(&s2), "{abc:?}");
        }
    }
}

#[test]
fn split_token_barline_pairs_rejoin() {
    // `||:` lowers to [Double, RepeatStart] sharing one span; emitting the
    // pair as two spaced tokens would phantom an empty leading measure.
    for src in [
        "X:1\nL:1/4\nK:C\n||: CDEF :|\n",
        "X:1\nL:1/4\nK:C\n[|: CDEF :|\n",
        "X:1\nL:1/4\nK:C\nCDEF ||: GABc :|\n",
        // `|]:` lowers to [Final, RepeatStart] sharing one span (a final bar
        // fused with a forward repeat); it must rejoin to `|]:`, not split
        // into a spaced `|] |:` that phantoms an empty measure.
        "X:1\nL:1/4\nK:C\nCDEF |]: GABc :|\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(measure_count(&s1), measure_count(&s2), "{src:?} -> {abc:?}");
        assert_eq!(barline_kinds(&s1), barline_kinds(&s2), "{src:?} -> {abc:?}");
    }
    // A real two-token `|| |:` pair mid-tune has distinct spans and must
    // NOT be rejoined (its phantom measure is real).
    let src = "X:1\nL:1/4\nK:C\nCDEF || |: GABc :|\n";
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());
    let s2 = score_of(&abc);
    assert_eq!(measure_count(&s1), measure_count(&s2), "{abc:?}");
}

#[test]
fn spacers_roundtrip() {
    // `y` spacers are zero-duration layout events; dropping them collapses
    // spacer-only measures, so they are emitted. Also inside a tuplet the
    // explicit `(p:q:r` span counts events, keeping the group intact.
    for src in [
        "X:1\nL:1/4\nK:C\nCD y EF | GA y2 BC |\n",
        "X:1\nM:4/4\nL:1/8\nK:C\n(3CDE y F2 |\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2), "{src:?} -> {abc:?}");
        assert_eq!(measure_count(&s1), measure_count(&s2), "{src:?} -> {abc:?}");
        let spacers = |s: &crate::Score| {
            s.parts[0].voices[0]
                .events
                .iter()
                .filter(|e| matches!(e.kind, crate::TimedEventKind::Spacer))
                .count()
        };
        assert_eq!(spacers(&s1), spacers(&s2), "{src:?} -> {abc:?}");
    }
}

fn lyric_tokens(score: &crate::Score) -> Vec<(u32, String, String)> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for e in &voice.events {
                for l in &e.attachments.lyrics {
                    v.push((l.verse, format!("{:?}", l.control), l.text.clone()));
                }
            }
            for measure in &voice.measures {
                for segment in &measure.overlays {
                    for e in &segment.events {
                        for l in &e.attachments.lyrics {
                            v.push((l.verse, format!("{:?}", l.control), l.text.clone()));
                        }
                    }
                }
            }
        }
    }
    v
}

#[test]
fn lyrics_roundtrip() {
    for src in [
        // syllables, hyphen, extender, skip, '~' space, across barlines
        "X:1\nL:1/4\nK:C\nCDEF|GAB c|\nw:doe-ray me_ fa * la~la ti\n",
        // multi-verse adjacency
        "X:1\nL:1/4\nK:C\nCD EF|\nw:one two three four\nw:uno dos tres cuatro\n",
        // verse gap (1 and 3) + chord consumes one position + rest skipped
        "X:1\nL:1/4\nK:C\nC z [EG] F|\nw:a b c\nw:*\nw:x y z\n",
        // escaped metacharacters in syllables
        "X:1\nL:1/4\nK:C\nCD|\nw:a\\-b c\\*d\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(
            lyric_tokens(&s1),
            lyric_tokens(&s2),
            "lyrics for {src:?} -> {abc:?}"
        );
        assert!(
            !lyric_tokens(&s1).is_empty(),
            "fixture has no lyrics: {src:?}"
        );
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
    }
}

#[test]
fn overlay_lyrics_roundtrip_in_source_order() {
    for src in [
        "X:1\nM:C\nL:1/8\nK:G\nD2D2 D2D>D&x6C>D|E2FE D2D2|\nw:And tis down by the green-wood side-oh!\n",
        "X:1\nM:C|\nL:1/4\nK:C\nV:1\nd\"^Duo:p\"3/4d/4de&dG3/4G/4Ge|\nw:the king of\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(
            lyric_tokens(&s1),
            lyric_tokens(&s2),
            "lyrics for {src:?} -> {abc:?}"
        );
        assert_eq!(overlay_pitches(&s1), overlay_pitches(&s2));
    }
}

fn voice_shape(score: &crate::Score) -> Vec<(String, Vec<Option<String>>, usize)> {
    score
        .parts
        .iter()
        .flat_map(|p| {
            p.voices.iter().map(|v| {
                let p = &v.properties;
                let texts = vec![
                    p.name.as_ref().map(|t| t.text.clone()),
                    p.nm.as_ref().map(|t| t.text.clone()),
                    p.subname.as_ref().map(|t| t.text.clone()),
                    p.snm.as_ref().map(|t| t.text.clone()),
                    p.clef.as_ref().map(|t| t.text.clone()),
                    p.stem.map(|s| format!("{s:?}")),
                    p.octave.as_ref().map(|t| t.text.clone()),
                    p.transpose.as_ref().map(|t| t.text.clone()),
                    p.middle.as_ref().map(|t| t.text.clone()),
                ];
                (v.id.value.clone(), texts, v.events.len())
            })
        })
        .collect()
}

#[test]
fn multi_voice_roundtrip() {
    for src in [
        "X:1\nL:1/4\nK:C\nV:1\nCDEF|\nV:2\nE,F,G,A,|\n",
        "X:1\nL:1/4\nK:C\nV:T name=\"Tenor\" clef=treble-8\nCDEF|\nV:B clef=bass\nC,D,E,F,|\n",
        "X:1\nL:1/4\nK:C\nV:1 octave=-1\nCDEF|\nV:2 octave=1\nGABc|\n",
        // Oversized modifiers clamp (octave=99999 -> +9, treble+15
        // octave=125 -> +11) and must clamp IDENTICALLY in lowering and
        // in the writer's mirrored shift, or pitches drift per pass.
        "X:1\nL:1/4\nK:C\nV:1 octave=99999\nCDEF|\nV:2 clef=treble+15 octave=125\nGABc|\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2), "{src:?} -> {abc:?}");
        assert_eq!(voice_shape(&s1), voice_shape(&s2), "{src:?} -> {abc:?}");
    }
}

fn overlay_pitches(score: &crate::Score) -> Vec<(u32, char, i8)> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for m in &voice.measures {
                for seg in &m.overlays {
                    for e in &seg.events {
                        if let crate::model::TimelineEventKind::Note { step, octave, .. } = e.kind {
                            v.push((seg.measure_index, step, octave));
                        }
                    }
                }
            }
        }
    }
    v
}

#[test]
fn overlays_roundtrip() {
    for src in [
        "X:1\nL:1/4\nK:C\nC2 E2 & G,2 B,2 |\n",
        "X:1\nL:1/4\nK:C\nCDEF | G2 A2 & B,2 C2 | E4 |\n",
        "X:1\nL:1/4\nK:C\nC2 E2 & G,2 B,2 & E,2 F,2 |\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2), "{src:?} -> {abc:?}");
        assert_eq!(
            overlay_pitches(&s1),
            overlay_pitches(&s2),
            "{src:?} -> {abc:?}"
        );
    }
}

#[test]
fn review_regressions_roundtrip() {
    // Repros from the adversarial review: each must round-trip with an
    // identical structural shape AND be write-idempotent.
    for src in [
        // C1: grace pitches are stored unshifted in octave-shifted voices
        "X:1\nL:1/4\nK:C\nV:1 clef=treble-8\n{fg}A B C D|\n",
        "X:1\nL:1/4\nK:C\nV:1 middle=d\n{fg}A B C D|\n",
        // I1: `+:` continuation joins with a newline inside the syllable
        "X:1\nL:1/4\nK:C\nCDEF|\nw:a b\n+:c d\n",
        // I2a: spacer inside a tuplet consumes no tuplet slot
        "X:1\nM:4/4\nL:1/8\nK:C\nC (3D y E F|\n",
        // I2b: tuplet inside an overlay segment
        "X:1\nL:1/4\nK:C\nC2 E2 & (3G,B,D G,2|\n",
        // I3: overlay in a measure with no primary-voice content
        "X:1\nL:1/4\nK:C\n| & CDEF|\n",
        // I4: backslash in a quoted voice property
        "X:1\nL:1/4\nK:C\nV:1 name=\"a\\\\b\"\nCDEF|\n",
    ] {
        let s1 = score_of(src);
        let abc1 = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc1);
        let abc2 = write_abc(&s2, AbcWriteOptions::default());
        assert_eq!(abc1, abc2, "not idempotent for {src:?}");
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2), "{src:?} -> {abc1:?}");
        assert_eq!(
            measure_count(&s1),
            measure_count(&s2),
            "{src:?} -> {abc1:?}"
        );
        assert_eq!(
            grace_pitches(&s1),
            grace_pitches(&s2),
            "{src:?} -> {abc1:?}"
        );
        assert_eq!(
            tuplet_ratios(&s1),
            tuplet_ratios(&s2),
            "{src:?} -> {abc1:?}"
        );
        assert_eq!(
            overlay_pitches(&s1),
            overlay_pitches(&s2),
            "{src:?} -> {abc1:?}"
        );
        assert_eq!(voice_shape(&s1), voice_shape(&s2), "{src:?} -> {abc1:?}");
    }
}

fn change_events(score: &crate::Score) -> Vec<(usize, String)> {
    let mut v = Vec::new();
    for p in &score.parts {
        for voice in &p.voices {
            for (i, e) in voice.events.iter().enumerate() {
                match &e.kind {
                    crate::TimedEventKind::KeyChange(k) => {
                        v.push((i, format!("K:{}", k.display)));
                    }
                    crate::TimedEventKind::MeterChange(m) => {
                        v.push((i, format!("M:{}", m.display)));
                    }
                    _ => {}
                }
            }
        }
    }
    v
}

#[test]
fn mid_tune_key_meter_changes_roundtrip() {
    for src in [
        // inline at a barline + meter change
        "X:1\nL:1/4\nK:C\nCDEF|[K:F]GAB_B|[M:3/4]ABc|\n",
        // mid-measure key change
        "X:1\nL:1/4\nK:C\n^FGA[K:D]F|FGAF|\n",
        // standalone body lines
        "X:1\nL:1/4\nK:C\nCDEF|\nK:D\nFGAF|\n",
        // mode + cut-time displays echo verbatim
        "X:1\nL:1/4\nK:C\nCDEF|[K:Ador]ABcd|[M:C|]ABcd|\n",
        // no-op restatement of the header key (49 corpus files do this)
        "X:1\nL:1/4\nK:C\nCDEF|[K:C]GABc|\n",
        // same-measure ledger interaction: natural F before [K:D] holds
        "X:1\nL:1/4\nK:C\n=FGA[K:D]F|FGAF|\n",
        // multi-voice broadcast: standalone K:D between voice bodies
        "X:1\nL:1/4\nK:C\nV:1\nCDEF|\nK:D\nV:1\nFGAF|\nV:2\nCDEF|\nFGAF|\n",
    ] {
        let s1 = score_of(src);
        let abc1 = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc1);
        let abc2 = write_abc(&s2, AbcWriteOptions::default());
        assert_eq!(abc1, abc2, "not idempotent for {src:?}");
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2), "{src:?} -> {abc1:?}");
        assert_eq!(
            change_events(&s1),
            change_events(&s2),
            "{src:?} -> {abc1:?}"
        );
        assert_eq!(
            measure_count(&s1),
            measure_count(&s2),
            "{src:?} -> {abc1:?}"
        );
    }
}

#[test]
fn output_is_a_write_abc_fixed_point() {
    // Idempotency form (avoids a croma-fmt dev-dep cycle): writing the
    // re-parsed Score yields byte-identical ABC.
    for src in [
        "X:1\nL:1/8\nK:C\nCDE FGA | c2 z2\n",
        "X:1\nL:1/4\nK:C\n|: CDEF |1 GABc :|2 cBAG |]\n",
        "X:1\nL:1/4\nK:C\nC2- C2 |\n",
    ] {
        let abc = write_abc(&score_of(src), AbcWriteOptions::default());
        let rewritten = write_abc(&score_of(&abc), AbcWriteOptions::default());
        assert_eq!(abc, rewritten, "write_abc not idempotent for {src:?}");
    }
}

#[test]
fn section_label_with_brackets_emits_whole_line_not_inline() {
    // A `P:` label whose text contains `[` or `]` cannot be safely emitted
    // as an inline field `[P:{label}]` — the parser reads up to the first `]`,
    // truncating the label.  The fix is to emit it as a whole-line body field
    // `P:{label}` (on its own line) which allows `]` in the value.
    // This is the tune_013528 case: P:Avokatrilli   [Am]
    let src = "X:1\nT:T\nM:2/4\nL:1/8\nK:Am\nP:Avokatrilli   [Am]\nAB cd |\n";
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());
    // Must NOT contain the broken inline form.
    assert!(
        !abc.contains("[P:Avokatrilli"),
        "label containing `[` must not be emitted as inline `[P:...]`; got:\n{abc}"
    );
    // Must contain a whole-line `P:` with the full label (including `[Am]`).
    assert!(
        abc.contains("\nP:Avokatrilli   [Am]\n"),
        "label containing `[` must be emitted as whole-line `P:...[Am]`; got:\n{abc}"
    );
    // Re-parsing must recover the identical SectionLabel event.
    let s2 = score_of(&abc);
    let labels1: Vec<_> = s1
        .parts
        .iter()
        .flat_map(|p| p.voices.iter())
        .flat_map(|v| v.events.iter())
        .filter_map(|e| {
            if let crate::TimedEventKind::SectionLabel(l) = &e.kind {
                Some(l.clone())
            } else {
                None
            }
        })
        .collect();
    let labels2: Vec<_> = s2
        .parts
        .iter()
        .flat_map(|p| p.voices.iter())
        .flat_map(|v| v.events.iter())
        .filter_map(|e| {
            if let crate::TimedEventKind::SectionLabel(l) = &e.kind {
                Some(l.clone())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        labels1, labels2,
        "section label must survive the ABC → write_abc → re-parse round-trip"
    );
}
