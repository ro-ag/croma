//! Beam-planner and tuplet-bracket tests: every worked example of
//! `docs/engraving/rules.md` §1.7 (E1–E24) and §2.4 (X1–X8), the edge cases of
//! §1.6 and §2.3, the grid (§1.2) and span (§1.3) tables, and a totality sweep
//! over extreme meters and ticks.
//!
//! Every plan built here also passes `assert_valid`, which checks the
//! contract's structural promises independently of the expected values: one
//! segment list per input, levels ascending from 1, well-formed MusicXML
//! begin…continue…end runs or single hooks per level, and `groups` equal to
//! the level-1 runs.

use super::*;
use BeamText::{BackwardHook as BH, Begin as B, Continue as C, End as E, ForwardHook as FH};

// ---- builders -------------------------------------------------------------

fn n(rtick: i64, dur: i64, beams: u8) -> BeamInput {
    BeamInput {
        rtick,
        dur,
        beams,
        is_rest: false,
        tuplet_id: None,
    }
}

fn rest(rtick: i64, dur: i64) -> BeamInput {
    BeamInput {
        rtick,
        dur,
        beams: 0,
        is_rest: true,
        tuplet_id: None,
    }
}

fn tup(id: u32, rtick: i64, dur: i64, beams: u8) -> BeamInput {
    BeamInput {
        tuplet_id: Some(id),
        ..n(rtick, dur, beams)
    }
}

fn m(numerator: u32, denominator: u32) -> Meter {
    Meter {
        numerator,
        denominator,
    }
}

/// `count` notes of `dur` ticks and `beams` flags, back to back from `start`.
fn run_of(start: i64, count: i64, dur: i64, beams: u8) -> Vec<BeamInput> {
    (0..count).map(|i| n(start + i * dur, dur, beams)).collect()
}

fn segs(spec: &[(u8, BeamText)]) -> Vec<BeamSegment> {
    spec.iter()
        .map(|&(level, text)| BeamSegment { level, text })
        .collect()
}

/// Plan and check the structural invariants before handing the plan back.
fn planned(notes: &[BeamInput], meter: Meter) -> BeamPlan {
    let result = plan(notes, meter);
    assert_valid(notes, &result);
    result
}

fn assert_segments(result: &BeamPlan, expected: &[&[(u8, BeamText)]]) {
    let expected: Vec<Vec<BeamSegment>> = expected.iter().map(|spec| segs(spec)).collect();
    assert_eq!(result.segments, expected);
}

/// The contract's structural promises, checked independently of any rule.
fn assert_valid(notes: &[BeamInput], result: &BeamPlan) {
    assert_eq!(
        result.segments.len(),
        notes.len(),
        "one segment list per input"
    );
    let mut grouped = vec![false; notes.len()];
    let mut previous_first = None;
    for group in &result.groups {
        assert!(group.len() >= 2, "groups have 2+ members: {group:?}");
        assert!(
            group.windows(2).all(|w| w[1] == w[0] + 1),
            "group members are adjacent inputs: {group:?}"
        );
        if let Some(first) = previous_first {
            assert!(group[0] > first, "groups ordered by first index");
        }
        previous_first = Some(group[0]);
        let last = group.len() - 1;
        for (pos, &index) in group.iter().enumerate() {
            assert!(!grouped[index], "input {index} in two groups");
            grouped[index] = true;
            assert!(!notes[index].is_rest, "a rest is never beamed");
            let level_one = result.segments[index].first().copied();
            let text = if pos == 0 {
                B
            } else if pos == last {
                E
            } else {
                C
            };
            assert_eq!(
                level_one,
                Some(BeamSegment { level: 1, text }),
                "level 1 of input {index} matches its group run"
            );
        }
        let top = group
            .iter()
            .map(|&i| result.segments[i].len())
            .max()
            .unwrap_or(0);
        for level in 2..=top {
            let mut open = false;
            for &index in group {
                let at_level = result.segments[index]
                    .iter()
                    .find(|s| usize::from(s.level) == level);
                match at_level.map(|s| s.text) {
                    Some(B) => {
                        assert!(!open, "begin inside an open run");
                        open = true;
                    }
                    Some(C) => assert!(open, "continue outside a run"),
                    Some(E) => {
                        assert!(open, "end outside a run");
                        open = false;
                    }
                    Some(FH | BH) => assert!(!open, "hook inside an open run"),
                    None => assert!(!open, "level {level} run interrupted at {index}"),
                }
            }
            assert!(!open, "level {level} run left open");
        }
    }
    for (index, list) in result.segments.iter().enumerate() {
        if !grouped[index] {
            assert!(list.is_empty(), "ungrouped input {index} has no beams");
        }
        for (pos, segment) in list.iter().enumerate() {
            assert_eq!(usize::from(segment.level), pos + 1, "levels ascend from 1");
        }
        assert!(list.len() <= 8, "at most 8 beam levels");
    }
}

// ---- §0.4 / contract ------------------------------------------------------

#[test]
fn beam_text_spells_the_musicxml_beam_values() {
    // rules.md §0.4: exactly the five MusicXML beam-value strings.
    assert_eq!(B.as_str(), "begin");
    assert_eq!(C.as_str(), "continue");
    assert_eq!(E.as_str(), "end");
    assert_eq!(FH.as_str(), "forward hook");
    assert_eq!(BH.as_str(), "backward hook");
}

// ---- §1.2 grid ------------------------------------------------------------

/// (beat start, beat length, d1) of an onset in local ticks.
fn beat_at(meter: Meter, t: i64) -> (i64, i64, i64) {
    let place = locate(grid(meter), t);
    (place.beat, place.len, place.d1)
}

#[test]
fn g0_fallback_meters_use_quarter_beats() {
    // rules.md G0: 0/4, 4/0, 3/7 and 4/256 cannot be put on the tick grid.
    for meter in [m(0, 4), m(4, 0), m(0, 0), m(3, 7), m(4, 256)] {
        assert_eq!(grid(meter), Grid::Quarter, "{meter:?}");
    }
    let place = locate(Grid::Quarter, 1000);
    assert_eq!(
        (place.bar, place.beat, place.len, place.d1, place.offset),
        (0, 2, 480, 2, 40)
    );
}

#[test]
fn g1_compound_meters_have_dotted_beats() {
    // rules.md G1: 6/8, 12/8, 6/4, 6/16 beat in threes of the unit.
    assert_eq!(beat_at(m(6, 8), 960), (720, 720, 3));
    assert_eq!(beat_at(m(12, 8), 1500), (1440, 720, 3));
    assert_eq!(beat_at(m(6, 4), 1500), (1440, 1440, 3));
    assert_eq!(beat_at(m(6, 16), 400), (360, 360, 3));
}

#[test]
fn g2_long_unit_meters_beat_per_unit() {
    // rules.md G2: 4/4, 3/2, 5/4 (per-unit, [choice]) and 2/1.
    assert_eq!(beat_at(m(4, 4), 1000), (960, 480, 2));
    assert_eq!(beat_at(m(3, 2), 1000), (960, 960, 2));
    assert_eq!(beat_at(m(5, 4), 1920), (1920, 480, 2));
    assert_eq!(beat_at(m(2, 1), 2000), (1920, 1920, 2));
}

#[test]
fn g3_short_unit_meters_up_to_four() {
    // rules.md G3: 1/8 one unit, 2/8 and 3/8 the whole bar, 4/8 in 2+2.
    assert_eq!(beat_at(m(1, 8), 100), (0, 240, 2));
    assert_eq!(beat_at(m(2, 8), 300), (0, 480, 2));
    assert_eq!(beat_at(m(3, 8), 600), (0, 720, 3));
    assert_eq!(beat_at(m(3, 16), 300), (0, 360, 3));
    assert_eq!(beat_at(m(4, 8), 500), (480, 480, 2));
}

#[test]
fn g4_irregular_meters_pair_units_with_a_final_three() {
    // rules.md G4 [choice]: 7/8 = 2+2+3, 8/8 = 2+2+2+2.
    let seven: Vec<_> = (0..7).map(|p| beat_at(m(7, 8), p * 240)).collect();
    assert_eq!(
        seven,
        [
            (0, 480, 2),
            (0, 480, 2),
            (480, 480, 2),
            (480, 480, 2),
            (960, 720, 3),
            (960, 720, 3),
            (960, 720, 3),
        ]
    );
    let eight: Vec<_> = (0..8).map(|p| beat_at(m(8, 8), p * 240).0).collect();
    assert_eq!(eight, [0, 0, 480, 480, 960, 960, 1440, 1440]);
}

#[test]
fn non_power_of_two_denominators_that_divide_1920_use_the_grid() {
    // rules.md §1.6: D = 3 (unit 640) and D = 6 (unit 320) are judged by unit.
    assert_eq!(beat_at(m(2, 3), 700), (640, 640, 2));
    assert_eq!(beat_at(m(3, 6), 700), (640, 320, 2));
}

// ---- §1.3 spans -----------------------------------------------------------

#[test]
fn span_limits_pick_the_listed_beam_spans() {
    // rules.md §1.3 bullet list: (beat, d1) → S_1, S_2, S_3.
    let spans = |len, d1| (span(len, d1, 1), span(len, d1, 2), span(len, d1, 3));
    assert_eq!(spans(480, 2), (480, 480, 240)); // 4/4, 2/4, 3/4
    assert_eq!(spans(960, 2), (960, 480, 240)); // 2/2, 3/2, 4/2
    assert_eq!(spans(720, 3), (720, 720, 240)); // 6/8, 9/8, 12/8, 3/8
    assert_eq!(spans(360, 3), (360, 360, 360)); // 6/16, 9/16, 12/16
    assert_eq!(spans(1440, 3), (1440, 480, 240)); // 6/4
    assert_eq!(span(1920, 2, 1), 960); // 2/1
}

#[test]
fn span_falls_back_to_the_chains_smallest_element() {
    // rules.md §1.3: chains 720,240,…,15 and 480,…,15 have nothing <= 11 at
    // level 8, so the smallest element (15) is used.
    assert_eq!(span(720, 3, 8), 15);
    assert_eq!(span(480, 2, 8), 15);
    // An odd beat with d1 = 3 still divides once (45 → 15), then stops.
    assert_eq!(span(45, 3, 8), 15);
    // An odd beat with d1 = 2 has no c1: the chain is just [B].
    assert_eq!(span(15, 2, 8), 15);
}

// ---- §1.7 worked examples -------------------------------------------------

#[test]
fn e1_four_four_eight_eighths_beam_in_half_bars() {
    // rules.md E1 (B6 half-bar merge).
    let result = planned(&run_of(0, 8, 240, 1), m(4, 4));
    assert_eq!(result.groups, [vec![0, 1, 2, 3], vec![4, 5, 6, 7]]);
    assert_segments(
        &result,
        &[
            &[(1, B)],
            &[(1, C)],
            &[(1, C)],
            &[(1, E)],
            &[(1, B)],
            &[(1, C)],
            &[(1, C)],
            &[(1, E)],
        ],
    );
}

#[test]
fn e2_four_four_never_beams_across_the_middle() {
    // rules.md E2: beats 2 and 3 lie in different half-bars.
    let notes = [
        n(0, 480, 0),
        n(480, 240, 1),
        n(720, 240, 1),
        n(960, 240, 1),
        n(1200, 240, 1),
        n(1440, 480, 0),
    ];
    let result = planned(&notes, m(4, 4));
    assert_eq!(result.groups, [vec![1, 2], vec![3, 4]]);
    assert_segments(
        &result,
        &[&[], &[(1, B)], &[(1, E)], &[(1, B)], &[(1, E)], &[]],
    );
}

#[test]
fn e3_four_four_half_bar_plus_beat() {
    // rules.md E3: the first half-bar merges; beat 3's pair has no partner.
    let mut notes = run_of(0, 6, 240, 1);
    notes.push(n(1440, 480, 0));
    let result = planned(&notes, m(4, 4));
    assert_eq!(result.groups, [vec![0, 1, 2, 3], vec![4, 5]]);
    assert_segments(
        &result,
        &[
            &[(1, B)],
            &[(1, C)],
            &[(1, C)],
            &[(1, E)],
            &[(1, B)],
            &[(1, E)],
            &[],
        ],
    );
}

#[test]
fn e4_three_four_six_eighths_beam_as_one_bar() {
    // rules.md E4 (B6, full-bar merge in 3/4).
    let result = planned(&run_of(0, 6, 240, 1), m(3, 4));
    assert_eq!(result.groups, [vec![0, 1, 2, 3, 4, 5]]);
    assert_segments(
        &result,
        &[
            &[(1, B)],
            &[(1, C)],
            &[(1, C)],
            &[(1, C)],
            &[(1, C)],
            &[(1, E)],
        ],
    );
}

#[test]
fn e5_three_four_partial_bar_stays_per_beat() {
    // rules.md E5 [choice]: four eighths and a quarter do not merge.
    let mut notes = run_of(0, 4, 240, 1);
    notes.push(n(960, 480, 0));
    let result = planned(&notes, m(3, 4));
    assert_eq!(result.groups, [vec![0, 1], vec![2, 3]]);
}

#[test]
fn e6_two_four_sixteenths_block_the_merge() {
    // rules.md E6: beat 0 is not a complete eighth beat.
    let mut notes = run_of(0, 4, 120, 2);
    notes.extend([n(480, 240, 1), n(720, 240, 1)]);
    let result = planned(&notes, m(2, 4));
    assert_eq!(result.groups, [vec![0, 1, 2, 3], vec![4, 5]]);
    assert_segments(
        &result,
        &[
            &[(1, B), (2, B)],
            &[(1, C), (2, C)],
            &[(1, C), (2, C)],
            &[(1, E), (2, E)],
            &[(1, B)],
            &[(1, E)],
        ],
    );
}

#[test]
fn e7_dotted_eighth_sixteenth_hooks_backward() {
    // rules.md E7 (B10 rule 2: last member → backward hook).
    let result = planned(&[n(0, 360, 1), n(360, 120, 2)], m(4, 4));
    assert_eq!(result.groups, [vec![0, 1]]);
    assert_segments(&result, &[&[(1, B)], &[(1, E), (2, BH)]]);
}

#[test]
fn e8_sixteenth_dotted_eighth_hooks_forward() {
    // rules.md E8 (B10 rule 1: first member → forward hook).
    let result = planned(&[n(0, 120, 2), n(120, 360, 1)], m(4, 4));
    assert_segments(&result, &[&[(1, B), (2, FH)], &[(1, E)]]);
}

#[test]
fn e9_six_eight_eighths_beam_in_threes() {
    // rules.md E9 (G1, P3).
    let result = planned(&run_of(0, 6, 240, 1), m(6, 8));
    assert_eq!(result.groups, [vec![0, 1, 2], vec![3, 4, 5]]);
    assert_segments(
        &result,
        &[
            &[(1, B)],
            &[(1, C)],
            &[(1, E)],
            &[(1, B)],
            &[(1, C)],
            &[(1, E)],
        ],
    );
}

#[test]
fn e10_six_eight_interior_hook_completes_a_subdivision() {
    // rules.md E10: pos 360, 360 % 240 != 0 → backward hook (B10 rule 3).
    let notes = [
        n(0, 360, 1),
        n(360, 120, 2),
        n(480, 240, 1),
        n(720, 240, 1),
        n(960, 240, 1),
        n(1200, 240, 1),
    ];
    let result = planned(&notes, m(6, 8));
    assert_eq!(result.groups, [vec![0, 1, 2], vec![3, 4, 5]]);
    assert_segments(
        &result,
        &[
            &[(1, B)],
            &[(1, C), (2, BH)],
            &[(1, E)],
            &[(1, B)],
            &[(1, C)],
            &[(1, E)],
        ],
    );
}

#[test]
fn e11_six_eight_interior_hook_starts_a_subdivision() {
    // rules.md E11: pos 240, 240 % 240 == 0 → forward hook (B10 rule 3).
    let result = planned(&[n(0, 240, 1), n(240, 120, 2), n(360, 360, 1)], m(6, 8));
    assert_segments(&result, &[&[(1, B)], &[(1, C), (2, FH)], &[(1, E)]]);
}

#[test]
fn e12_six_eight_sixteenths_under_one_double_beam() {
    // rules.md E12: S_2 = 720, so the double beam runs through the beat.
    let mut notes = run_of(0, 6, 120, 2);
    notes.push(n(720, 720, 0));
    let result = planned(&notes, m(6, 8));
    assert_eq!(result.groups, [vec![0, 1, 2, 3, 4, 5]]);
    assert_segments(
        &result,
        &[
            &[(1, B), (2, B)],
            &[(1, C), (2, C)],
            &[(1, C), (2, C)],
            &[(1, C), (2, C)],
            &[(1, C), (2, C)],
            &[(1, E), (2, E)],
            &[],
        ],
    );
}

#[test]
fn e13_four_four_thirty_seconds_break_at_the_half_beat() {
    // rules.md E13: S_3 = 240 splits level 3 between onsets 180 and 240.
    let mut notes = run_of(0, 8, 60, 3);
    notes.extend([n(480, 960, 0), n(1440, 480, 0)]);
    let result = planned(&notes, m(4, 4));
    assert_eq!(result.groups, [vec![0, 1, 2, 3, 4, 5, 6, 7]]);
    assert_segments(
        &result,
        &[
            &[(1, B), (2, B), (3, B)],
            &[(1, C), (2, C), (3, C)],
            &[(1, C), (2, C), (3, C)],
            &[(1, C), (2, C), (3, E)],
            &[(1, C), (2, C), (3, B)],
            &[(1, C), (2, C), (3, C)],
            &[(1, C), (2, C), (3, C)],
            &[(1, E), (2, E), (3, E)],
            &[],
            &[],
        ],
    );
}

#[test]
fn e14_three_levels_with_a_hook() {
    // rules.md E14: levels 2 run n1–n2; level 3 is a lone backward hook.
    let result = planned(&[n(0, 240, 1), n(240, 180, 2), n(420, 60, 3)], m(4, 4));
    assert_segments(
        &result,
        &[&[(1, B)], &[(1, C), (2, B)], &[(1, E), (2, E), (3, BH)]],
    );
}

#[test]
fn e15_a_rest_interrupts_the_beam() {
    // rules.md E15 (B2 [choice]).
    let notes = [n(0, 120, 2), rest(120, 120), n(240, 120, 2), n(360, 120, 2)];
    let result = planned(&notes, m(2, 4));
    assert_eq!(result.groups, [vec![2, 3]]);
    assert_segments(&result, &[&[], &[], &[(1, B), (2, B)], &[(1, E), (2, E)]]);
}

#[test]
fn e16_a_triplet_is_its_own_group() {
    // rules.md E16 (B5, B6): the tuplet blocks the 2/4 merge.
    let notes = [
        tup(1, 0, 240, 1),
        tup(1, 160, 240, 1),
        tup(1, 320, 240, 1),
        n(480, 240, 1),
        n(720, 240, 1),
    ];
    let result = planned(&notes, m(2, 4));
    assert_eq!(result.groups, [vec![0, 1, 2], vec![3, 4]]);
}

#[test]
fn e17_three_eight_sixteenths_beam_as_one_bar() {
    // rules.md E17 (G3, N = 3).
    let result = planned(&run_of(0, 6, 120, 2), m(3, 8));
    assert_eq!(result.groups, [vec![0, 1, 2, 3, 4, 5]]);
    assert_segments(
        &result,
        &[
            &[(1, B), (2, B)],
            &[(1, C), (2, C)],
            &[(1, C), (2, C)],
            &[(1, C), (2, C)],
            &[(1, C), (2, C)],
            &[(1, E), (2, E)],
        ],
    );
}

#[test]
fn e18_five_and_seven_eight_group_as_twos_then_three() {
    // rules.md E18 (G4 [choice]).
    assert_eq!(
        planned(&run_of(0, 5, 240, 1), m(5, 8)).groups,
        [vec![0, 1], vec![2, 3, 4]]
    );
    assert_eq!(
        planned(&run_of(0, 7, 240, 1), m(7, 8)).groups,
        [vec![0, 1], vec![2, 3], vec![4, 5, 6]]
    );
}

#[test]
fn e19_nine_sixteen_sixteenths_beam_in_threes() {
    // rules.md E19 (G1 with a 16th unit).
    let result = planned(&run_of(0, 9, 120, 2), m(9, 16));
    assert_eq!(result.groups, [vec![0, 1, 2], vec![3, 4, 5], vec![6, 7, 8]]);
    let triple: [&[(u8, BeamText)]; 3] = [&[(1, B), (2, B)], &[(1, C), (2, C)], &[(1, E), (2, E)]];
    let expected: Vec<&[(u8, BeamText)]> = triple.iter().copied().cycle().take(9).collect();
    assert_segments(&result, &expected);
}

#[test]
fn e20_two_two_sixteenths_break_at_each_quarter() {
    // rules.md E20: S_1 = 960, S_2 = 480.
    let mut notes = run_of(0, 8, 120, 2);
    notes.push(n(960, 960, 0));
    let result = planned(&notes, m(2, 2));
    assert_eq!(result.groups, [vec![0, 1, 2, 3, 4, 5, 6, 7]]);
    assert_segments(
        &result,
        &[
            &[(1, B), (2, B)],
            &[(1, C), (2, C)],
            &[(1, C), (2, C)],
            &[(1, C), (2, E)],
            &[(1, C), (2, B)],
            &[(1, C), (2, C)],
            &[(1, C), (2, C)],
            &[(1, E), (2, E)],
            &[],
        ],
    );
}

#[test]
fn e21_huge_odd_numerator_puts_the_last_three_units_together() {
    // rules.md E21: N = 4294967295 over 8, final 3-unit beat and an even pair.
    let meter = m(u32::MAX, 8);
    let last_three = [
        n(1_030_792_150_080, 240, 1),
        n(1_030_792_150_320, 240, 1),
        n(1_030_792_150_560, 240, 1),
    ];
    assert_eq!(planned(&last_three, meter).groups, [vec![0, 1, 2]]);
    let units = i64::from(u32::MAX);
    let pair = [n((units - 5) * 240, 240, 1), n((units - 4) * 240, 240, 1)];
    assert_eq!(planned(&pair, meter).groups, [vec![0, 1]]);
}

#[test]
fn e22_fallback_meter_beams_per_quarter() {
    // rules.md E22 (G0 for 3/7).
    assert_eq!(
        planned(&run_of(0, 4, 240, 1), m(3, 7)).groups,
        [vec![0, 1], vec![2, 3]]
    );
}

#[test]
fn e23_overfull_measure_repeats_the_bar_grid() {
    // rules.md E23: bar 0 merges; bar 1 (k = 1) has an incomplete span.
    let result = planned(&run_of(0, 6, 240, 1), m(2, 4));
    assert_eq!(result.groups, [vec![0, 1, 2, 3], vec![4, 5]]);
}

#[test]
fn e24_empty_single_and_rest_inputs() {
    // rules.md E24 / §1.6: empty plan, a lone eighth, two eighth rests.
    let empty = planned(&[], m(4, 4));
    assert!(empty.segments.is_empty() && empty.groups.is_empty());
    let single = planned(&[n(0, 240, 1)], m(4, 4));
    assert_eq!(single.segments, [Vec::<BeamSegment>::new()]);
    assert!(single.groups.is_empty());
    let rests = planned(&[rest(0, 240), rest(240, 240)], m(4, 4));
    assert_eq!(rests.segments, [Vec::new(), Vec::new()]);
    assert!(rests.groups.is_empty());
}

// ---- §1.4 / §1.6 edge cases -----------------------------------------------

#[test]
fn quarters_and_longer_are_never_beamed() {
    // rules.md B1 / P5 / §1.6: all quarter-or-longer → no groups.
    let notes = [n(0, 480, 0), n(480, 480, 0), n(960, 960, 0)];
    let result = planned(&notes, m(4, 4));
    assert!(result.groups.is_empty());
}

#[test]
fn eighths_in_different_beats_stay_flagged() {
    // rules.md §1.6: 4/4 eighths at 240 and 480 lie in different beats.
    let result = planned(&[n(240, 240, 1), n(480, 240, 1)], m(4, 4));
    assert!(result.groups.is_empty());
    assert_segments(&result, &[&[], &[]]);
}

#[test]
fn out_of_order_onsets_break_the_beam() {
    // rules.md B3: rtick[i] < rtick[i-1] breaks between them.
    let result = planned(&[n(0, 120, 2), n(240, 120, 2), n(120, 120, 2)], m(4, 4));
    assert_eq!(result.groups, [vec![0, 1]]);
}

#[test]
fn negative_onsets_land_in_bar_minus_one() {
    // rules.md §1.6: floor division puts -480 and -240 in beat 3 of bar -1,
    // and the barline at 0 still separates bar -1 from bar 0.
    assert_eq!(
        planned(&[n(-480, 240, 1), n(-240, 240, 1)], m(4, 4)).groups,
        [vec![0, 1]]
    );
    assert!(
        planned(&[n(-240, 240, 1), n(0, 240, 1)], m(4, 4))
            .groups
            .is_empty()
    );
}

#[test]
fn notes_in_different_bars_never_join() {
    // rules.md §1.6: overfull 6/8, the pair straddling the barline splits.
    assert!(
        planned(&[n(1200, 240, 1), n(1440, 240, 1)], m(6, 8))
            .groups
            .is_empty()
    );
}

#[test]
fn non_positive_durations_join_by_onset_but_never_merge() {
    // rules.md §1.6: dur <= 0 only fails the complete-eighth-beat test (B6).
    let notes = [n(0, 0, 1), n(240, 0, 1), n(480, -5, 1), n(720, 0, 1)];
    assert_eq!(planned(&notes, m(2, 4)).groups, [vec![0, 1], vec![2, 3]]);
}

#[test]
fn beam_counts_above_eight_clamp_to_eight() {
    // rules.md §0.2 [choice]: MusicXML numbers beams 1..8 only.
    let result = planned(&[n(0, 8, 12), n(8, 8, 255)], m(4, 4));
    assert_eq!(result.groups, [vec![0, 1]]);
    let levels: Vec<u8> = result.segments[0].iter().map(|s| s.level).collect();
    assert_eq!(levels, (1..=8).collect::<Vec<u8>>());
}

#[test]
fn half_bar_merge_also_covers_beats_three_and_four() {
    // rules.md B6: 4/4 beats {2, 3} merge when both are complete eighth beats.
    let mut notes = vec![n(0, 960, 0)];
    notes.extend(run_of(960, 4, 240, 1));
    assert_eq!(planned(&notes, m(4, 4)).groups, [vec![1, 2, 3, 4]]);
}

#[test]
fn merge_requires_pure_eighths() {
    // rules.md B6 [choice]: a dotted pair in a beat keeps per-beat beaming.
    let notes = [n(0, 360, 1), n(360, 120, 2), n(480, 240, 1), n(720, 240, 1)];
    assert_eq!(planned(&notes, m(2, 4)).groups, [vec![0, 1], vec![2, 3]]);
}

#[test]
fn merge_requires_every_beat_of_the_span() {
    // rules.md B6: a rest leaving beat 1 with one eighth cancels the merge.
    let notes = [n(0, 240, 1), n(240, 240, 1), rest(480, 240), n(720, 240, 1)];
    assert_eq!(planned(&notes, m(2, 4)).groups, [vec![0, 1]]);
}

#[test]
fn merge_applies_only_to_quarter_beat_meters_up_to_four() {
    // rules.md B6: 2/2 and 5/4 do not merge eighth beats.
    assert_eq!(
        planned(&run_of(0, 4, 240, 1), m(5, 4)).groups,
        [vec![0, 1], vec![2, 3]]
    );
    // 2/2: S_1 = 960 already puts four eighths under one beam per half.
    assert_eq!(
        planned(&run_of(0, 8, 240, 1), m(2, 2)).groups,
        [vec![0, 1, 2, 3], vec![4, 5, 6, 7]]
    );
}

#[test]
fn tuplets_beam_across_beats_but_not_with_neighbours() {
    // rules.md B5: same id joins across a beat boundary; another id or a plain
    // note never joins.
    let notes = [
        tup(3, 320, 160, 1),
        tup(3, 480, 160, 1),
        tup(3, 640, 160, 1),
        tup(4, 800, 80, 1),
        tup(4, 880, 80, 1),
        n(960, 240, 1),
        n(1200, 240, 1),
    ];
    let result = planned(&notes, m(4, 4));
    assert_eq!(result.groups, [vec![0, 1, 2], vec![3, 4], vec![5, 6]]);
}

#[test]
fn tuplet_secondary_beams_are_unbroken() {
    // rules.md B9 [choice]: no subdivision breaks inside a tuplet, even where
    // the meter grid would break level 3 (S_3 = 240 in 4/4).
    let notes: Vec<_> = (0..6).map(|i| tup(5, i * 80, 60, 3)).collect();
    let result = planned(&notes, m(4, 4));
    assert_eq!(result.groups, [vec![0, 1, 2, 3, 4, 5]]);
    assert_eq!(result.segments[3], segs(&[(1, C), (2, C), (3, C)]));
}

#[test]
fn tuplet_interior_hooks_use_written_durations() {
    // rules.md B10 rule 3, tuplet case: pos = sum of written durs before the
    // note, so 240 → forward and 360 → backward.
    let forward = planned(
        &[tup(1, 0, 240, 1), tup(1, 160, 120, 2), tup(1, 240, 240, 1)],
        m(4, 4),
    );
    assert_segments(&forward, &[&[(1, B)], &[(1, C), (2, FH)], &[(1, E)]]);
    let backward = planned(
        &[tup(1, 0, 360, 1), tup(1, 240, 120, 2), tup(1, 320, 240, 1)],
        m(4, 4),
    );
    assert_segments(&backward, &[&[(1, B)], &[(1, C), (2, BH)], &[(1, E)]]);
}

// ---- totality -------------------------------------------------------------

#[test]
fn plan_is_total_over_extreme_meters_and_ticks() {
    // rules.md §0.3 / §1.6: no panics and no per-beat or per-bar allocation;
    // a numerator of u32::MAX must cost the same as 4/4 (anything proportional
    // to it would exhaust memory or time here).
    let meters = [
        m(u32::MAX, 4),
        m(268_435_457, 4),
        m(u32::MAX, 8),
        m(u32::MAX, 1),
        m(u32::MAX, u32::MAX),
        m(0, 4),
        m(4, 0),
        m(0, 0),
        m(1, 1920),
        m(3, 128),
        m(4, 256),
        m(3, 7),
        m(1, 1),
        m(6, 1),
    ];
    let edge_ticks = [
        i64::MIN,
        i64::MIN + 1,
        -1_000_000_007,
        -1,
        0,
        1,
        1_000_000_007,
        i64::MAX - 1,
        i64::MAX,
    ];
    let inputs: Vec<Vec<BeamInput>> = vec![
        Vec::new(),
        vec![n(0, 240, 1)],
        vec![rest(0, 480), rest(480, 480), rest(960, 960)],
        edge_ticks.iter().map(|&t| n(t, 240, 2)).collect(),
        edge_ticks.iter().map(|&t| n(t, i64::MAX, 3)).collect(),
        edge_ticks
            .iter()
            .map(|&t| tup(1, t, i64::MAX, 255))
            .collect(),
        vec![
            tup(9, 0, i64::MAX, 2),
            tup(9, 1, i64::MAX, 1),
            tup(9, 2, i64::MIN, 2),
        ],
        vec![
            n(i64::MAX, i64::MIN, 4),
            n(i64::MAX, i64::MIN, 4),
            n(i64::MAX, 0, 1),
        ],
        vec![
            n(i64::MIN, 240, 1),
            n(i64::MIN, 240, 1),
            n(i64::MIN + 240, 120, 2),
        ],
        run_of(0, 64, 30, 6),
    ];
    for meter in meters {
        for notes in &inputs {
            planned(notes, meter);
        }
    }
}

// ---- §2 tuplet_shows_bracket ----------------------------------------------

#[test]
fn x1_beamed_triplet_prints_only_its_number() {
    // rules.md X1 (T2): E16's tuplet is exactly one beam group.
    let notes = [
        tup(1, 0, 240, 1),
        tup(1, 160, 240, 1),
        tup(1, 320, 240, 1),
        n(480, 240, 1),
        n(720, 240, 1),
    ];
    let result = planned(&notes, m(2, 4));
    assert!(!tuplet_shows_bracket(&notes, &result.groups, 1));
}

#[test]
fn x2_quarter_note_triplet_needs_a_bracket() {
    // rules.md X2 (T3): written quarters are never beamed.
    let notes = [tup(1, 0, 480, 0), tup(1, 320, 480, 0), tup(1, 640, 480, 0)];
    let result = planned(&notes, m(4, 4));
    assert!(result.groups.is_empty());
    assert!(tuplet_shows_bracket(&notes, &result.groups, 1));
}

#[test]
fn x3_triplet_with_a_middle_rest_needs_a_bracket() {
    // rules.md X3 (T3).
    let mut middle = tup(1, 160, 240, 0);
    middle.is_rest = true;
    let notes = [tup(1, 0, 240, 1), middle, tup(1, 320, 240, 1)];
    let result = planned(&notes, m(4, 4));
    assert!(result.groups.is_empty());
    assert!(tuplet_shows_bracket(&notes, &result.groups, 1));
}

#[test]
fn x4_triplet_ending_in_a_rest_needs_a_bracket() {
    // rules.md X4 (T3, §2.2 equality): group {0,1} != members {0,1,2}.
    let mut last = tup(1, 320, 240, 0);
    last.is_rest = true;
    let notes = [tup(1, 0, 240, 1), tup(1, 160, 240, 1), last];
    let result = planned(&notes, m(4, 4));
    assert_eq!(result.groups, [vec![0, 1]]);
    assert!(tuplet_shows_bracket(&notes, &result.groups, 1));
}

#[test]
fn x5_adjacent_triplets_each_print_only_their_number() {
    // rules.md X5: two tuplet ids form two groups (B5).
    let notes = [
        tup(1, 0, 240, 1),
        tup(1, 160, 240, 1),
        tup(1, 320, 240, 1),
        tup(2, 480, 240, 1),
        tup(2, 640, 240, 1),
        tup(2, 800, 240, 1),
    ];
    let result = planned(&notes, m(4, 4));
    assert_eq!(result.groups, [vec![0, 1, 2], vec![3, 4, 5]]);
    assert!(!tuplet_shows_bracket(&notes, &result.groups, 1));
    assert!(!tuplet_shows_bracket(&notes, &result.groups, 2));
}

#[test]
fn x6_sixteenth_sextuplet_prints_only_its_number() {
    // rules.md X6.
    let notes: Vec<_> = (0..6).map(|i| tup(7, i * 80, 120, 2)).collect();
    let result = planned(&notes, m(4, 4));
    assert_eq!(result.groups, [vec![0, 1, 2, 3, 4, 5]]);
    assert!(!tuplet_shows_bracket(&notes, &result.groups, 7));
}

#[test]
fn x7_absent_tuplet_defaults_to_a_bracket() {
    // rules.md X7 / §2.2 step 2 [choice].
    let notes = [tup(1, 0, 240, 1), tup(1, 160, 240, 1)];
    assert!(tuplet_shows_bracket(&notes, &[vec![0, 1]], 9));
}

#[test]
fn x8_a_group_wider_than_the_tuplet_needs_a_bracket() {
    // rules.md X8: equality, not containment.
    let notes = [
        tup(1, 0, 240, 1),
        tup(1, 160, 240, 1),
        tup(1, 320, 240, 1),
        n(480, 240, 1),
    ];
    assert!(tuplet_shows_bracket(&notes, &[vec![0, 1, 2, 3]], 1));
}

#[test]
fn bracket_edge_cases_default_to_true() {
    // rules.md §2.3: empty notes or groups, and a one-member tuplet.
    assert!(tuplet_shows_bracket(&[], &[], 1));
    let notes = [tup(1, 0, 240, 1), tup(1, 160, 240, 1)];
    assert!(tuplet_shows_bracket(&notes, &[], 1));
    let lone = [tup(4, 0, 240, 1), n(240, 240, 1)];
    let result = planned(&lone, m(4, 4));
    assert!(tuplet_shows_bracket(&lone, &result.groups, 4));
}
