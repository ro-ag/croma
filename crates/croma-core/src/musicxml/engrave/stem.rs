//! Staff positions, stem direction, multi-voice rest placement, and slur/tie
//! placement.
//!
//! Implements `docs/engraving/rules.md` §3 (`abs_step` §3.1,
//! `middle_distance` §3.2, `note_type_has_stem` §3.3, `stem_up` §3.4), §4
//! (`rest_display`) and §5 (`slur_placement`, `tie_orientation`). The Rust
//! boundary is fixed by `docs/engraving/contract.md`; the staff a clef implies
//! always comes from [`clef_model`], never from parsing clef names here.

use crate::musicxml::attributes::clef_model;

/// Diatonic step of the treble staff's middle line, B4 (rules.md S2).
const TREBLE_MIDDLE: i32 = 41;

/// Step letters by diatonic index, C = 0 (rules.md §3.1, §4.2).
const STEP_LETTERS: [char; 7] = ['C', 'D', 'E', 'F', 'G', 'A', 'B'];

/// The diatonic staff position `7 * (octave + 1) + index` (middle C = 35),
/// case-insensitive in the step letter (rules.md §3.1).
pub(crate) fn abs_step(step: char, octave: i8) -> i32 {
    // rules.md §3.1 [choice]: an unknown letter counts as C, keeping the result
    // inside the octave the caller gave.
    let index = STEP_LETTERS
        .iter()
        .position(|&letter| letter == step.to_ascii_uppercase())
        .unwrap_or(0);
    // index < 7, and the i8 octave keeps the whole expression far inside i32.
    7 * (i32::from(octave) + 1) + index as i32
}

/// The diatonic step of the middle staff line for a clef's MusicXML `sign`
/// and `line` (rules.md S0–S4).
fn middle_step_for(sign: &str, line: &str) -> i32 {
    // rules.md S1/S4: the pitch each sign fixes and its standard line.
    let (reference, standard_line) = match sign {
        "G" => (39, 2),
        "F" => (31, 4),
        "C" => (35, 3),
        // rules.md S3: percussion reads as treble whatever its line; S4: an
        // unknown sign is treble.
        _ => return TREBLE_MIDDLE,
    };
    // rules.md S4 [choice]: a line other than "1".."5" uses the standard line.
    let line = match line {
        "1" => 1,
        "2" => 2,
        "3" => 3,
        "4" => 4,
        "5" => 5,
        _ => standard_line,
    };
    // rules.md S2: lines are two steps apart and line 3 is the middle.
    reference + 2 * (3 - line)
}

/// The middle line's diatonic step for an ABC clef name (`None` = treble).
/// rules.md S5: octave-transposing clefs are not adjusted.
fn middle_step(clef_text: Option<&str>) -> i32 {
    let model = clef_model(clef_text);
    middle_step_for(model.sign, model.line)
}

/// The signed diatonic distance from the staff's middle line to the note:
/// positive below the middle line, negative above (rules.md S6).
pub(crate) fn middle_distance(clef_text: Option<&str>, step: char, octave: i8) -> i32 {
    middle_step(clef_text) - abs_step(step, octave)
}

/// Whether a MusicXML note type carries a stem: half and shorter do; whole,
/// breve, long, maxima and anything unrecognised do not (rules.md N1–N3).
pub(crate) fn note_type_has_stem(note_type: &str) -> bool {
    // rules.md N3 [choice]: exact, case-sensitive match on the MusicXML values.
    matches!(
        note_type,
        "half"
            | "quarter"
            | "eighth"
            | "16th"
            | "32nd"
            | "64th"
            | "128th"
            | "256th"
            | "512th"
            | "1024th"
    )
}

/// The stem direction (`true` = up) of one note or chord (rules.md §3.4).
///
/// `own_distances` are its notes' middle distances; `beam_distances` those of
/// every note in its beam group (`None` when unbeamed). On a multi-voice staff
/// the voice slot decides, not the pitch.
pub(crate) fn stem_up(
    own_distances: &[i32],
    beam_distances: Option<&[i32]>,
    multivoice: bool,
    voice_slot: usize,
) -> bool {
    // rules.md D6 [choice]: even slots are upper parts (stems up), odd lower.
    if multivoice {
        return voice_slot.is_multiple_of(2);
    }
    // rules.md D5: a beamed note follows its whole group, so every member of
    // the group gets the same answer; an empty group list falls back to `own`.
    let distances = match beam_distances {
        Some(group) if !group.is_empty() => group,
        _ => own_distances,
    };
    // Widen to i64 so `-i32::MIN` cannot overflow (rules.md §0.3).
    let (Some(lowest), Some(highest)) = (
        distances.iter().map(|&d| i64::from(d)).max(),
        distances.iter().map(|&d| i64::from(d)).min(),
    ) else {
        // rules.md §3.4 step 3: nothing to judge → as if on the middle line.
        return false;
    };
    // rules.md D1/D3: the note farthest from the middle line decides.
    let below = lowest.max(0);
    let above = (-highest).max(0);
    if below != above {
        return below > above;
    }
    // rules.md D4: equidistant outer notes → majority side; a full balance (and
    // the middle line itself, D2) → down.
    let below_count = distances.iter().filter(|&&d| d > 0).count();
    let above_count = distances.iter().filter(|&&d| d < 0).count();
    below_count > above_count
}

/// The `<display-step>`/`<display-octave>` of a rest on a multi-voice staff:
/// two staff spaces above the middle line for even (upper) slots, two below
/// for odd (lower) slots (rules.md §4.2, R3, R4).
pub(crate) fn rest_display(clef_text: Option<&str>, voice_slot: usize) -> (char, i8) {
    let middle = middle_step(clef_text);
    // rules.md R3 [choice]: the top or bottom staff line, ±4 diatonic steps.
    let target = if voice_slot.is_multiple_of(2) {
        middle + 4
    } else {
        middle - 4
    };
    // rules.md §4.2 step 3: Euclidean split into octave and step index.
    let octave = target.div_euclid(7) - 1;
    let index = usize::try_from(target.rem_euclid(7)).unwrap_or(0);
    let letter = STEP_LETTERS.get(index).copied().unwrap_or('C');
    // Middle lines are 29..=43 for every clef model, so this never saturates.
    let octave = i8::try_from(octave).unwrap_or(if octave < 0 { i8::MIN } else { i8::MAX });
    (letter, octave)
}

/// The `placement` of a slur starting on a note (rules.md §5.1).
pub(crate) fn slur_placement(
    stem_up: Option<bool>,
    multivoice: bool,
    voice_slot: usize,
) -> &'static str {
    // rules.md L2: on a shared staff, each part's slurs sit on its outer side.
    if multivoice {
        return if voice_slot.is_multiple_of(2) {
            "above"
        } else {
            "below"
        };
    }
    match stem_up {
        // rules.md L1: opposite the stem, on the notehead side.
        Some(true) => "below",
        Some(false) => "above",
        // rules.md L3 [choice]: a stemless note is treated as stem-down.
        None => "above",
    }
}

/// The `orientation` of a tie (rules.md §5.2).
pub(crate) fn tie_orientation(stem_up: Option<bool>, multivoice: bool) -> &'static str {
    match (multivoice, stem_up) {
        // rules.md K2: on a shared staff, ties go on the stem side.
        (true, Some(false)) => "under",
        (true, _) => "over",
        // rules.md K1: in a single voice, ties curve away from the stem.
        (false, Some(true)) => "under",
        // rules.md K3 [choice]: stemless → over, as the stem-down default.
        (false, _) => "over",
    }
}

#[cfg(test)]
#[path = "stem_tests.rs"]
mod tests;
