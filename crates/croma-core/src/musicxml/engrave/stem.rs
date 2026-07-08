//! Stem direction, multi-voice rest placement, and slur/tie orientation — a port of
//! MuseScore's engraving rules. All work in a diatonic staff-step coordinate: a
//! note's signed distance from the staff's middle line drives auto stem direction,
//! and voice parity overrides it when a staff carries multiple voices.

/// Diatonic step index of a pitch letter (C = 0 … B = 6).
fn step_index(step: char) -> i32 {
    match step.to_ascii_uppercase() {
        'C' => 0,
        'D' => 1,
        'E' => 2,
        'F' => 3,
        'G' => 4,
        'A' => 5,
        'B' => 6,
        _ => 0,
    }
}

/// Absolute diatonic step of a pitch: `7 * (octave + 1) + step_index`. Middle C
/// (C4) = 35, matching MuseScore's `absStep`.
pub(crate) fn abs_step(step: char, octave: i8) -> i32 {
    7 * (i32::from(octave) + 1) + step_index(step)
}

/// Absolute diatonic step of the pitch sitting on a clef's middle staff line:
/// treble = B4, bass = D3, alto = C4, tenor = A3. Unknown clefs default to treble.
fn middle_abs_step(clef_text: Option<&str>) -> i32 {
    match clef_family(clef_text) {
        ClefFamily::Treble => abs_step('B', 4),
        ClefFamily::Bass => abs_step('D', 3),
        ClefFamily::Alto => abs_step('C', 4),
        ClefFamily::Tenor => abs_step('A', 3),
    }
}

/// Absolute diatonic step of a clef's top staff line (MuseScore `pitchOffset`):
/// treble = F5, bass = A3, alto = G4, tenor = F4. Used for rest placement.
fn top_line_abs_step(clef_text: Option<&str>) -> i32 {
    match clef_family(clef_text) {
        ClefFamily::Treble => abs_step('F', 5),
        ClefFamily::Bass => abs_step('A', 3),
        ClefFamily::Alto => abs_step('G', 4),
        ClefFamily::Tenor => abs_step('F', 4),
    }
}

enum ClefFamily {
    Treble,
    Bass,
    Alto,
    Tenor,
}

fn clef_family(clef_text: Option<&str>) -> ClefFamily {
    let text = clef_text.unwrap_or("treble").trim().to_ascii_lowercase();
    if text.starts_with("bass") || text.starts_with('f') {
        ClefFamily::Bass
    } else if text.starts_with("tenor") {
        ClefFamily::Tenor
    } else if text.starts_with("alto") || text.starts_with("c") {
        ClefFamily::Alto
    } else {
        ClefFamily::Treble
    }
}

/// Signed distance of a pitch from the staff middle line, in diatonic steps
/// (`> 0` = below the middle line → stem tends up). Equals MuseScore's
/// `noteLine - middleLine`.
pub(crate) fn middle_distance(clef_text: Option<&str>, step: char, octave: i8) -> i32 {
    middle_abs_step(clef_text) - abs_step(step, octave)
}

/// MuseScore `computeAutoStemDirection`: pair the outermost notes inward; the first
/// non-zero sum decides (`> 0` → up). All-balanced (e.g. a single note on the middle
/// line) → down.
pub(crate) fn auto_stem_up(distances: &[i32]) -> bool {
    let mut sorted = distances.to_vec();
    sorted.sort_unstable();
    let mut left = 0usize;
    let mut right = sorted.len().saturating_sub(1);
    while left <= right {
        let sum = sorted[left] + sorted[right];
        if sum != 0 {
            return sum > 0;
        }
        if left == right {
            break;
        }
        left += 1;
        right -= 1;
    }
    false
}

/// Resolve a note/chord's stem direction. Multi-voice staves force voice parity
/// (slot 0/2 up, 1/3 down); otherwise a beamed unit takes the whole beam's auto
/// direction and a lone unit takes its own.
pub(crate) fn stem_up(
    own_distances: &[i32],
    beam_distances: Option<&[i32]>,
    multivoice: bool,
    voice_slot: usize,
) -> bool {
    if multivoice {
        return voice_slot.is_multiple_of(2);
    }
    match beam_distances {
        Some(distances) => auto_stem_up(distances),
        None => auto_stem_up(own_distances),
    }
}

/// Whether a written note type carries a stem at all (whole notes and longer do not).
pub(crate) fn note_type_has_stem(note_type: &str) -> bool {
    !matches!(note_type, "whole" | "breve" | "long" | "maxima")
}

/// Display step + octave for a multi-voice rest, offset off the middle line so it
/// clears the other voice: slot 0/2 one space above, slot 1/3 one space below.
pub(crate) fn rest_display(clef_text: Option<&str>, voice_slot: usize) -> (char, i8) {
    const NATURAL_LINE: i32 = 2; // middle line, in whole-space units
    let voice_offset = if voice_slot.is_multiple_of(2) { -1 } else { 1 };
    // 2 diatonic steps per space, measured down from the top line; then MuseScore's
    // -7 octave correction.
    let mut po = top_line_abs_step(clef_text) - 2 * (NATURAL_LINE + voice_offset) - 7;
    po = po.clamp(0, 69);
    let octave = (po / 7) as i8;
    let step = ['C', 'D', 'E', 'F', 'G', 'A', 'B'][(po % 7) as usize];
    (step, octave)
}

/// MusicXML `placement` for a slur attached at a note with the given stem direction:
/// multi-voice puts it on the stem side (slot 0 above, else below); otherwise it goes
/// opposite the stem (the notehead side).
pub(crate) fn slur_placement(
    stem_up: Option<bool>,
    multivoice: bool,
    voice_slot: usize,
) -> &'static str {
    if multivoice {
        return if voice_slot.is_multiple_of(2) {
            "above"
        } else {
            "below"
        };
    }
    match stem_up {
        Some(true) => "below",
        Some(false) | None => "above",
    }
}

/// MusicXML `orientation` for a tie at a note: multi-voice puts it on the stem side,
/// otherwise opposite the stem.
pub(crate) fn tie_orientation(stem_up: Option<bool>, multivoice: bool) -> &'static str {
    if multivoice {
        return match stem_up {
            Some(false) => "under",
            _ => "over",
        };
    }
    match stem_up {
        Some(true) => "under",
        Some(false) | None => "over",
    }
}

#[cfg(test)]
#[path = "stem_tests.rs"]
mod tests;
