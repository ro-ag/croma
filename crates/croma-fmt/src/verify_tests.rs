use super::*;

fn opts() -> ParseOptions {
    ParseOptions::default()
}

#[test]
fn extracts_sequential_notes() {
    let seq = pitch_seq_of("X:1\nK:C\nCEG\n", opts()).expect("score");
    assert_eq!(seq, vec![('C', 0, 4), ('E', 0, 4), ('G', 0, 4)]);
}

#[test]
fn chord_members_are_in_order_and_rests_skipped() {
    let seq = pitch_seq_of("X:1\nK:C\n[CEG] z\n", opts()).expect("score");
    assert_eq!(seq, vec![('C', 0, 4), ('E', 0, 4), ('G', 0, 4)]);
}

#[test]
fn accidental_changes_the_sequence() {
    let natural = pitch_seq_of("X:1\nK:C\nG\n", opts()).expect("score");
    let sharp = pitch_seq_of("X:1\nK:C\n^G\n", opts()).expect("score");
    assert_ne!(natural, sharp);
}
