use super::*;

const EIGHTH: i64 = 240;
const SIXTEENTH: i64 = 120;

fn note(rtick: i64, dur: i64, beams: u8) -> BeamInput {
    BeamInput {
        rtick,
        dur,
        beams,
        is_rest: false,
        tuplet_id: None,
    }
}

fn tuplet_note(rtick: i64, dur: i64, beams: u8, id: u32) -> BeamInput {
    BeamInput {
        tuplet_id: Some(id),
        ..note(rtick, dur, beams)
    }
}

fn rest(rtick: i64, dur: i64) -> BeamInput {
    BeamInput {
        rtick,
        dur,
        beams: 1,
        is_rest: true,
        tuplet_id: None,
    }
}

fn meter(n: u32, d: u32) -> Meter {
    Meter {
        numerator: n,
        denominator: d,
    }
}

/// Compact rendering of a note's segments: e.g. `"1:begin"` or `"1:continue 2:begin"`.
fn seg_text(segs: &[BeamSegment]) -> String {
    segs.iter()
        .map(|s| format!("{}:{}", s.level, s.text.as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn plan_text(notes: &[BeamInput], m: Meter) -> Vec<String> {
    plan(notes, m)
        .segments
        .iter()
        .map(|segs| seg_text(segs))
        .collect()
}

#[test]
fn four_four_eighths_group_in_fours() {
    let notes: Vec<_> = (0..8).map(|i| note(i * EIGHTH, EIGHTH, 1)).collect();
    assert_eq!(
        plan_text(&notes, meter(4, 4)),
        [
            "1:begin",
            "1:continue",
            "1:continue",
            "1:end", // beat 1-2
            "1:begin",
            "1:continue",
            "1:continue",
            "1:end", // beat 3-4
        ]
    );
}

#[test]
fn six_eight_eighths_group_in_threes() {
    let notes: Vec<_> = (0..6).map(|i| note(i * EIGHTH, EIGHTH, 1)).collect();
    assert_eq!(
        plan_text(&notes, meter(6, 8)),
        [
            "1:begin",
            "1:continue",
            "1:end", // dotted-quarter 1
            "1:begin",
            "1:continue",
            "1:end", // dotted-quarter 2
        ]
    );
}

#[test]
fn two_four_eighths_group_in_twos() {
    let notes: Vec<_> = (0..4).map(|i| note(i * EIGHTH, EIGHTH, 1)).collect();
    assert_eq!(
        plan_text(&notes, meter(2, 4)),
        ["1:begin", "1:end", "1:begin", "1:end"]
    );
}

#[test]
fn three_four_eighths_break_every_beat() {
    let notes: Vec<_> = (0..6).map(|i| note(i * EIGHTH, EIGHTH, 1)).collect();
    assert_eq!(
        plan_text(&notes, meter(3, 4)),
        ["1:begin", "1:end", "1:begin", "1:end", "1:begin", "1:end"]
    );
}

#[test]
fn four_four_sixteenths_break_every_beat_with_secondary() {
    let notes: Vec<_> = (0..16).map(|i| note(i * SIXTEENTH, SIXTEENTH, 2)).collect();
    let plan = plan_text(&notes, meter(4, 4));
    // Four groups of four; each group: begin/continue*2/end on BOTH beam levels.
    for group in 0..4 {
        let base = group * 4;
        assert_eq!(plan[base], "1:begin 2:begin", "group {group} note 0");
        assert_eq!(plan[base + 1], "1:continue 2:continue");
        assert_eq!(plan[base + 2], "1:continue 2:continue");
        assert_eq!(plan[base + 3], "1:end 2:end", "group {group} note 3");
    }
}

#[test]
fn secondary_beam_spans_only_the_sixteenths() {
    // In cut time the half-note beat holds [eighth, 16th, 16th, eighth] in one beam:
    // the primary (level 1) beam spans all four, the 16th (level 2) secondary beam
    // spans just the two sixteenths.
    let notes = [
        note(0, EIGHTH, 1),
        note(EIGHTH, SIXTEENTH, 2),
        note(EIGHTH + SIXTEENTH, SIXTEENTH, 2),
        note(2 * EIGHTH, EIGHTH, 1),
    ];
    let plan = plan_text(&notes, meter(2, 2));
    assert_eq!(plan[0], "1:begin");
    assert_eq!(plan[1], "1:continue 2:begin");
    assert_eq!(plan[2], "1:continue 2:end");
    assert_eq!(plan[3], "1:end");
}

#[test]
fn dotted_eighth_sixteenth_makes_a_hook() {
    // [dotted-eighth, 16th] in 2/4: primary beam over both; the lone 16th's second
    // beam is a backward hook.
    let notes = [note(0, 360, 1), note(360, SIXTEENTH, 2)];
    let plan = plan_text(&notes, meter(2, 4));
    assert_eq!(plan[0], "1:begin");
    assert_eq!(plan[1], "1:end 2:backward hook");
}

#[test]
fn triplet_eighths_beam_together() {
    let notes = [
        tuplet_note(0, 160, 1, 1),
        tuplet_note(160, 160, 1, 1),
        tuplet_note(320, 160, 1, 1),
    ];
    assert_eq!(
        plan_text(&notes, meter(4, 4)),
        ["1:begin", "1:continue", "1:end"]
    );
}

#[test]
fn triplet_at_beat_two_starts_new_beam() {
    // Two eighths (beat 1) then an eighth triplet starting at beat 2: the triplet is
    // its own beam via the tuplet-boundary rule.
    let notes = [
        note(0, EIGHTH, 1),
        note(EIGHTH, EIGHTH, 1),
        tuplet_note(2 * EIGHTH, 160, 1, 1),
        tuplet_note(2 * EIGHTH + 160, 160, 1, 1),
        tuplet_note(2 * EIGHTH + 320, 160, 1, 1),
    ];
    assert_eq!(
        plan_text(&notes, meter(2, 4)),
        ["1:begin", "1:end", "1:begin", "1:continue", "1:end"]
    );
}

#[test]
fn rest_breaks_the_beam() {
    // Two eighths, an eighth rest, then a lone eighth: only the first pair beams.
    let notes = [
        note(0, EIGHTH, 1),
        note(EIGHTH, EIGHTH, 1),
        rest(2 * EIGHTH, EIGHTH),
        note(3 * EIGHTH, EIGHTH, 1),
    ];
    assert_eq!(plan_text(&notes, meter(2, 4)), ["1:begin", "1:end", "", ""]);
}

#[test]
fn lone_eighth_is_not_beamed() {
    let notes = [note(0, EIGHTH, 1)];
    assert_eq!(plan_text(&notes, meter(4, 4)), [""]);
}

#[test]
fn quarter_notes_are_never_beamed() {
    let notes = [note(0, 480, 0), note(480, 480, 0)];
    assert_eq!(plan_text(&notes, meter(2, 4)), ["", ""]);
}

#[test]
fn fully_beamed_triplet_hides_bracket() {
    let notes = [
        tuplet_note(0, 160, 1, 1),
        tuplet_note(160, 160, 1, 1),
        tuplet_note(320, 160, 1, 1),
    ];
    let p = plan(&notes, meter(4, 4));
    assert!(!tuplet_shows_bracket(&notes, &p.groups, 1));
}

#[test]
fn triplet_with_a_rest_shows_bracket() {
    let notes = [
        tuplet_note(0, 160, 1, 1),
        BeamInput {
            is_rest: true,
            ..tuplet_note(160, 160, 1, 1)
        },
        tuplet_note(320, 160, 1, 1),
    ];
    let p = plan(&notes, meter(4, 4));
    assert!(tuplet_shows_bracket(&notes, &p.groups, 1));
}

#[test]
fn quarter_note_triplet_shows_bracket() {
    // Three quarter-note triplet members (beams = 0) → bracket shown.
    let notes = [
        tuplet_note(0, 320, 0, 1),
        tuplet_note(320, 320, 0, 1),
        tuplet_note(640, 320, 0, 1),
    ];
    let p = plan(&notes, meter(4, 4));
    assert!(tuplet_shows_bracket(&notes, &p.groups, 1));
}

#[test]
fn exotic_meter_falls_back_to_per_beat_breaks() {
    // 7/4 is not in the table → full break at every quarter beat.
    let notes: Vec<_> = (0..4).map(|i| note(i * EIGHTH, EIGHTH, 1)).collect();
    // beats at 0,480,960,...; eighths at 0,240,480,720 → break at 480 (beat 2).
    assert_eq!(
        plan_text(&notes, meter(7, 4)),
        ["1:begin", "1:end", "1:begin", "1:end"]
    );
}
