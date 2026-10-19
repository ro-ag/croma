//! Stem, rest-placement, slur and tie tests: the examples of
//! `docs/engraving/rules.md` §3.1, §3.2 (S2 table and S6 examples), §3.3, the
//! ST1–ST15 table of §3.4, the §4.3 table and §4.4 examples, and the
//! SL1–SL7 / TI1–TI6 table of §5.3, plus their listed edge cases.

use super::*;

// ---- §3.1 abs_step --------------------------------------------------------

#[test]
fn abs_step_matches_the_listed_examples() {
    // rules.md §3.1 examples, including case-insensitivity.
    assert_eq!(abs_step('C', 4), 35);
    assert_eq!(abs_step('b', 4), 41);
    assert_eq!(abs_step('G', 5), 46);
    assert_eq!(abs_step('D', 3), 29);
    assert_eq!(abs_step('A', -1), 5);
}

#[test]
fn abs_step_maps_unknown_letters_to_c() {
    // rules.md §3.1 [choice]: an unknown letter is index 0.
    assert_eq!(abs_step('x', 4), 35);
    assert_eq!(abs_step('\u{1F3B5}', 4), 35);
}

#[test]
fn abs_step_is_safe_at_the_octave_extremes() {
    // rules.md §3.1: the i8 extremes do not overflow.
    assert_eq!(abs_step('B', 127), 902);
    assert_eq!(abs_step('C', -128), -889);
}

// ---- §3.2 middle_distance -------------------------------------------------

#[test]
fn middle_line_steps_follow_sign_and_line() {
    // rules.md S2 table, including clefs clef_model cannot produce.
    let cases = [
        ("G", "2", 41), // treble, B4
        ("G", "1", 43), // French violin, D5
        ("F", "4", 29), // bass, D3
        ("F", "3", 31), // baritone, F3
        ("C", "3", 35), // alto, C4
        ("C", "4", 33), // tenor, A3
        ("C", "1", 39), // soprano, G4
        ("C", "2", 37), // mezzo, E4
    ];
    for (sign, line, middle) in cases {
        assert_eq!(middle_step_for(sign, line), middle, "{sign}/{line}");
    }
}

#[test]
fn percussion_reads_as_treble_on_any_line() {
    // rules.md S3.
    for line in ["1", "2", "3", "5", ""] {
        assert_eq!(middle_step_for("percussion", line), 41);
    }
}

#[test]
fn bad_lines_and_unknown_signs_fall_back() {
    // rules.md S4 [choice]: bad line → the sign's standard line; unknown sign
    // → treble.
    assert_eq!(middle_step_for("G", ""), 41);
    assert_eq!(middle_step_for("F", "0"), 29);
    assert_eq!(middle_step_for("C", "6"), 35);
    assert_eq!(middle_step_for("F", "four"), 29);
    assert_eq!(middle_step_for("TAB", "5"), 41);
    assert_eq!(middle_step_for("", ""), 41);
}

#[test]
fn middle_distance_matches_the_listed_examples() {
    // rules.md S6 examples, through clef_model's ABC clef names.
    let treble = Some("treble");
    let bass = Some("bass");
    let alto = Some("alto");
    let tenor = Some("tenor");
    let perc = Some("perc");
    assert_eq!(middle_distance(treble, 'B', 4), 0);
    assert_eq!(middle_distance(treble, 'C', 4), 6);
    assert_eq!(middle_distance(treble, 'G', 5), -5);
    assert_eq!(middle_distance(treble, 'A', 3), 8);
    assert_eq!(middle_distance(bass, 'D', 3), 0);
    assert_eq!(middle_distance(bass, 'C', 4), -6);
    assert_eq!(middle_distance(bass, 'G', 2), 4);
    assert_eq!(middle_distance(alto, 'C', 4), 0);
    assert_eq!(middle_distance(alto, 'F', 3), 4);
    assert_eq!(middle_distance(tenor, 'A', 3), 0);
    assert_eq!(middle_distance(tenor, 'C', 4), -2);
    assert_eq!(middle_distance(perc, 'B', 4), 0);
    assert_eq!(middle_distance(perc, 'E', 4), 4);
    assert_eq!(middle_distance(None, 'b', 4), 0);
}

#[test]
fn middle_distance_is_safe_at_the_octave_extremes() {
    // rules.md §0.3: totality.
    assert_eq!(middle_distance(None, 'B', 127), 41 - 902);
    assert_eq!(middle_distance(Some("bass"), 'C', -128), 29 + 889);
}

// ---- §3.3 note_type_has_stem ----------------------------------------------

#[test]
fn half_and_shorter_carry_stems() {
    // rules.md N1.
    for note_type in [
        "half", "quarter", "eighth", "16th", "32nd", "64th", "128th", "256th", "512th", "1024th",
    ] {
        assert!(note_type_has_stem(note_type), "{note_type}");
    }
}

#[test]
fn whole_and_longer_and_unknown_types_have_no_stem() {
    // rules.md N1, N2 [choice] (long, maxima), N3 [choice] (exact match).
    for note_type in [
        "whole", "breve", "long", "maxima", "", "Quarter", "2048th", "eighth ",
    ] {
        assert!(!note_type_has_stem(note_type), "{note_type:?}");
    }
}

// ---- §3.4 stem_up ---------------------------------------------------------

#[test]
fn stem_up_matches_the_st_table() {
    // rules.md §3.4 worked examples ST1–ST15.
    type Case = (
        &'static str,
        &'static [i32],
        Option<&'static [i32]>,
        bool,
        usize,
        bool,
    );
    let cases: [Case; 15] = [
        ("ST1", &[6], None, false, 0, true),
        ("ST2", &[-5], None, false, 0, false),
        ("ST3", &[0], None, false, 0, false),
        ("ST4", &[3, -5], None, false, 0, false),
        ("ST5", &[5, -3], None, false, 0, true),
        ("ST6", &[4, -2, -4], None, false, 0, false),
        ("ST7", &[4, 2, -4], None, false, 0, true),
        ("ST8", &[4, -4], None, false, 0, false),
        ("ST9", &[1], Some(&[1, 2, -6]), false, 0, false),
        ("ST10", &[-1], Some(&[-1, 3, 4]), false, 0, true),
        ("ST11", &[-6], None, true, 0, true),
        ("ST12", &[6], None, true, 1, false),
        ("ST13", &[0], None, true, 2, true),
        ("ST14", &[0], None, true, 3, false),
        ("ST15", &[i32::MIN, i32::MAX], None, false, 0, false),
    ];
    for (name, own, beam, multivoice, slot, expected) in cases {
        assert_eq!(stem_up(own, beam, multivoice, slot), expected, "{name}");
    }
}

#[test]
fn stem_up_edge_cases() {
    // rules.md §3.4 edge cases: all on the middle line → down; Some(&[]) beam
    // distances fall back to own; empty own and no beam → down.
    assert!(!stem_up(&[0, 0], None, false, 0));
    assert!(stem_up(&[6], Some(&[]), false, 0));
    assert!(!stem_up(&[], None, false, 0));
    assert!(!stem_up(&[], Some(&[]), false, 0));
    assert!(stem_up(&[i32::MAX], None, false, 0));
    assert!(!stem_up(&[i32::MIN], None, false, 0));
}

#[test]
fn every_member_of_a_beam_group_gets_the_same_direction() {
    // rules.md D5: the group's distances decide, whatever the member's own.
    let group = [-1, 3, 4, -2];
    let directions: Vec<bool> = group
        .iter()
        .map(|&own| stem_up(&[own], Some(&group), false, 0))
        .collect();
    assert_eq!(directions, [true, true, true, true]);
}

#[test]
fn multivoice_slot_parity_holds_for_huge_slots() {
    // rules.md D6 [choice]: parity, for any slot.
    assert!(!stem_up(&[-9], None, true, usize::MAX));
    assert!(stem_up(&[9], None, true, usize::MAX - 1));
}

// ---- §4 rest_display ------------------------------------------------------

#[test]
fn rest_display_matches_the_table() {
    // rules.md §4.3 table via sign/line (the middle-step part) and §4.2.
    let cases = [
        ("G", "2", ('F', 5), ('E', 4)),
        ("F", "4", ('A', 3), ('G', 2)),
        ("C", "3", ('G', 4), ('F', 3)),
        ("C", "4", ('E', 4), ('D', 3)),
        ("percussion", "2", ('F', 5), ('E', 4)),
        ("C", "1", ('D', 5), ('C', 4)),
        ("G", "1", ('A', 5), ('G', 4)),
    ];
    let display = |middle: i32| {
        let octave = i8::try_from(middle.div_euclid(7) - 1).unwrap_or(0);
        let index = usize::try_from(middle.rem_euclid(7)).unwrap_or(0);
        (STEP_LETTERS[index], octave)
    };
    for (sign, line, upper, lower) in cases {
        let middle = middle_step_for(sign, line);
        assert_eq!(display(middle + 4), upper, "{sign}/{line} upper");
        assert_eq!(display(middle - 4), lower, "{sign}/{line} lower");
    }
}

#[test]
fn rest_display_matches_the_listed_examples() {
    // rules.md §4.4 examples through clef_model's ABC clef names.
    assert_eq!(rest_display(None, 0), ('F', 5));
    assert_eq!(rest_display(None, 1), ('E', 4));
    assert_eq!(rest_display(Some("bass"), 0), ('A', 3));
    assert_eq!(rest_display(Some("bass"), 1), ('G', 2));
    assert_eq!(rest_display(Some("alto"), 0), ('G', 4));
    assert_eq!(rest_display(Some("alto"), 1), ('F', 3));
    assert_eq!(rest_display(Some("tenor"), 1), ('D', 3));
    assert_eq!(rest_display(Some("perc"), 0), ('F', 5));
    assert_eq!(rest_display(None, 2), ('F', 5));
    assert_eq!(rest_display(None, 3), ('E', 4));
}

#[test]
fn rest_display_edge_cases() {
    // rules.md §4.4: a huge slot uses parity; an unknown clef name is treble.
    assert_eq!(rest_display(None, usize::MAX), ('E', 4));
    assert_eq!(rest_display(Some("no-such-clef"), 0), ('F', 5));
    assert_eq!(rest_display(Some(""), 1), ('E', 4));
}

// ---- §5 slurs and ties ----------------------------------------------------

#[test]
fn slur_placement_matches_the_sl_table() {
    // rules.md §5.3 SL1–SL7 (L1, L2, L3).
    let cases = [
        ("SL1", Some(true), false, 0, "below"),
        ("SL2", Some(false), false, 0, "above"),
        ("SL3", None, false, 0, "above"),
        ("SL4", Some(false), true, 0, "above"),
        ("SL5", Some(true), true, 1, "below"),
        ("SL6", None, true, 3, "below"),
        ("SL7", None, true, 2, "above"),
    ];
    for (name, stem, multivoice, slot, expected) in cases {
        assert_eq!(slur_placement(stem, multivoice, slot), expected, "{name}");
    }
}

#[test]
fn slur_placement_slot_parity_for_huge_slots() {
    // rules.md L2: parity, for any slot.
    assert_eq!(slur_placement(Some(true), true, usize::MAX), "below");
    assert_eq!(slur_placement(Some(true), false, usize::MAX), "below");
}

#[test]
fn tie_orientation_matches_the_ti_table() {
    // rules.md §5.3 TI1–TI6 (K1, K2, K3).
    let cases = [
        ("TI1", Some(true), false, "under"),
        ("TI2", Some(false), false, "over"),
        ("TI3", None, false, "over"),
        ("TI4", Some(true), true, "over"),
        ("TI5", Some(false), true, "under"),
        ("TI6", None, true, "over"),
    ];
    for (name, stem, multivoice, expected) in cases {
        assert_eq!(tie_orientation(stem, multivoice), expected, "{name}");
    }
}
