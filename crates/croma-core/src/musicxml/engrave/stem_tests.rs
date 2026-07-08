use super::*;

const TREBLE: Option<&str> = Some("treble");
const BASS: Option<&str> = Some("bass");

#[test]
fn middle_line_distances_by_clef() {
    // Treble middle line is B4 → distance 0; a note below is positive, above negative.
    assert_eq!(middle_distance(TREBLE, 'B', 4), 0);
    assert_eq!(middle_distance(TREBLE, 'G', 4), 2); // below → stem up
    assert_eq!(middle_distance(TREBLE, 'C', 5), -1); // above → stem down
    // Bass middle line is D3.
    assert_eq!(middle_distance(BASS, 'D', 3), 0);
    assert_eq!(middle_distance(BASS, 'F', 3), -2);
}

#[test]
fn single_note_auto_direction() {
    assert!(auto_stem_up(&[2])); // below middle → up
    assert!(!auto_stem_up(&[-1])); // above middle → down
    assert!(!auto_stem_up(&[0])); // on the middle line → down
}

#[test]
fn chord_uses_outermost_pair() {
    // G4 (+2) and E5 (-5): net -3 → down.
    assert!(!auto_stem_up(&[2, -5]));
    // G4 (+2) and A4 (+1): both below → up.
    assert!(auto_stem_up(&[2, 1]));
    // Balanced pair peels inward: D5(-2), B4(0), G4(+2) → 0 sum, then inner 0 → down.
    assert!(!auto_stem_up(&[-2, 0, 2]));
}

#[test]
fn voice_parity_overrides_in_multivoice() {
    // Distances say "down", but multi-voice slot 0 forces up.
    assert!(stem_up(&[-5], None, true, 0));
    assert!(!stem_up(&[5], None, true, 1));
    assert!(stem_up(&[], None, true, 2));
    assert!(!stem_up(&[], None, true, 3));
}

#[test]
fn beam_direction_uses_whole_beam() {
    // Single-voice: a lone note stems by its own distance...
    assert!(stem_up(&[2], None, false, 0));
    // ...but a beamed note follows the whole beam's extremes.
    assert!(!stem_up(&[2], Some(&[2, -6]), false, 0));
}

#[test]
fn stemless_note_types() {
    assert!(note_type_has_stem("quarter"));
    assert!(note_type_has_stem("eighth"));
    assert!(!note_type_has_stem("whole"));
    assert!(!note_type_has_stem("breve"));
}

#[test]
fn multivoice_rest_offsets() {
    assert_eq!(rest_display(TREBLE, 0), ('D', 5)); // upper voice, above middle
    assert_eq!(rest_display(TREBLE, 1), ('G', 4)); // lower voice, below middle
    assert_eq!(rest_display(BASS, 0), ('F', 3));
}

#[test]
fn slur_placement_rules() {
    assert_eq!(slur_placement(Some(true), false, 0), "below"); // opposite stem-up
    assert_eq!(slur_placement(Some(false), false, 0), "above"); // opposite stem-down
    assert_eq!(slur_placement(Some(true), true, 0), "above"); // multivoice upper
    assert_eq!(slur_placement(Some(true), true, 1), "below"); // multivoice lower
}

#[test]
fn tie_orientation_rules() {
    assert_eq!(tie_orientation(Some(true), false), "under"); // single, opposite stem
    assert_eq!(tie_orientation(Some(false), false), "over");
    assert_eq!(tie_orientation(Some(true), true), "over"); // multivoice, stem side
    assert_eq!(tie_orientation(Some(false), true), "under");
}
