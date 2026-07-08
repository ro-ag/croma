//! Beam-grouping engine — a Rust port of MuseScore's automatic beaming.
//!
//! MuseScore drives beaming from a per-meter break table (`noteGroups[]` in
//! `groups.cpp`) plus a per-note `BeamMode`, then turns those modes into per-level
//! `<beam>` segments (`writeBeam` in its MusicXML exporter). This module reproduces
//! that pipeline over a flat list of [`BeamInput`] (one entry per time-advancing
//! event in a voice/measure) and returns, per input, the `<beam>` segments to emit.
//!
//! Ticks use MuseScore's convention: `DIVISION = 480` ticks per quarter note, so a
//! whole note is `1920` and a 1/32 note is `60`. All positions and durations are in
//! those units. See `docs/engraving-export.md`.

/// One time-advancing event (note, chord head, or rest) in beat order within a
/// single voice and measure. Chord *members* are not represented — a chord beams as
/// its head. Written flag count [`beams`](BeamInput::beams) comes from the note's
/// *written* (de-tupletted) type, so a triplet eighth carries `beams = 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BeamInput {
    /// Position within the measure, in ticks (whole = 1920).
    pub rtick: i64,
    /// Written duration, in ticks.
    pub dur: i64,
    /// Written beam/flag count: eighth = 1, 16th = 2, 32nd = 3, ... quarter+ = 0.
    pub beams: u8,
    pub is_rest: bool,
    /// Innermost tuplet pair id the event belongs to, for boundary handling.
    pub tuplet_id: Option<u32>,
}

/// A single `<beam number="N">text</beam>` to emit on a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BeamSegment {
    /// MusicXML beam level (1 = primary/eighth beam, 2 = 16th, ...).
    pub level: u8,
    pub text: BeamText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BeamText {
    Begin,
    Continue,
    End,
    ForwardHook,
    BackwardHook,
}

impl BeamText {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            BeamText::Begin => "begin",
            BeamText::Continue => "continue",
            BeamText::End => "end",
            BeamText::ForwardHook => "forward hook",
            BeamText::BackwardHook => "backward hook",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BeamMode {
    Auto,
    None,
    Begin,
    Begin16,
    Begin32,
    Mid,
}

/// The unreduced meter fraction (`6/8`, not `3/4`) — the grouping table is keyed on
/// the literal signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Meter {
    pub numerator: u32,
    pub denominator: u32,
}

const DIVISION: i64 = 480; // ticks per quarter
const D32: i64 = DIVISION / 8; // ticks per 1/32 note = 60

/// One entry of a meter's break table: a 1/32-note position and a packed action.
///
/// `action` is three nibbles `0xABC`: nibble 0 (low) = rule for an eighth landing
/// here, nibble 4 = for a 16th, nibble 8 = for a 32nd-and-shorter. Nibble value
/// `0` = continue, `1` = full break, `2` = break 16th-and-shorter sub-beams,
/// `3` = break 32nd-and-shorter.
#[derive(Debug, Clone, Copy)]
struct GroupNode {
    pos: i32, // in 1/32-note units
    action: u16,
}

/// MuseScore's built-in `noteGroups[]` table (`groups.cpp`), verbatim.
fn table_nodes(meter: Meter) -> Option<Vec<GroupNode>> {
    macro_rules! g {
        ($($pos:expr => $act:expr),* $(,)?) => {
            vec![$(GroupNode { pos: $pos, action: $act }),*]
        };
    }
    let nodes = match (meter.numerator, meter.denominator) {
        (2, 2) => g![4=>0x200, 8=>0x110, 12=>0x200, 16=>0x111, 20=>0x200, 24=>0x110, 28=>0x200],
        (3, 2) => g![4=>0x200, 8=>0x110, 12=>0x200, 16=>0x111, 20=>0x200, 24=>0x110, 28=>0x200,
                     32=>0x111, 36=>0x200, 40=>0x110, 44=>0x200],
        (4, 2) => g![4=>0x200, 8=>0x110, 12=>0x200, 16=>0x111, 20=>0x200, 24=>0x110, 28=>0x200,
                     32=>0x111, 36=>0x200, 40=>0x110, 44=>0x200, 48=>0x111, 52=>0x200, 56=>0x110,
                     60=>0x200],
        (2, 4) => g![4=>0x200, 8=>0x111, 12=>0x200],
        (3, 4) => g![4=>0x200, 8=>0x111, 12=>0x200, 16=>0x111, 20=>0x200],
        (4, 4) => g![4=>0x200, 8=>0x110, 12=>0x200, 16=>0x111, 20=>0x200, 24=>0x110, 28=>0x200],
        (5, 4) => g![4=>0x200, 8=>0x110, 12=>0x200, 16=>0x110, 20=>0x200, 24=>0x111, 28=>0x200,
                     32=>0x110, 36=>0x200],
        (6, 4) => g![4=>0x200, 8=>0x110, 12=>0x200, 16=>0x110, 20=>0x200, 24=>0x111, 28=>0x200,
                     32=>0x110, 36=>0x200, 40=>0x110, 44=>0x200],
        (3, 8) => g![4=>0x200, 8=>0x200],
        (5, 8) => g![4=>0x200, 8=>0x200, 12=>0x111, 16=>0x200],
        (6, 8) => g![4=>0x200, 8=>0x200, 12=>0x111, 16=>0x200, 20=>0x200],
        (7, 8) => g![4=>0x200, 8=>0x200, 12=>0x111, 16=>0x200, 20=>0x111, 24=>0x200],
        (9, 8) => g![4=>0x200, 8=>0x200, 12=>0x111, 16=>0x200, 20=>0x200, 24=>0x111, 28=>0x200,
                     32=>0x200],
        (12, 8) => g![4=>0x200, 8=>0x200, 12=>0x111, 16=>0x200, 20=>0x200, 24=>0x111, 28=>0x200,
                      32=>0x200, 36=>0x111, 40=>0x200, 44=>0x200],
        _ => return None,
    };
    Some(nodes)
}

/// The grouping table for `meter`, falling back to MuseScore's `Groups::endings`
/// synthesis (a full break at every denominator beat) for meters not in the table.
fn group_nodes(meter: Meter) -> Vec<GroupNode> {
    if let Some(nodes) = table_nodes(meter) {
        return nodes;
    }
    // pos step, in 1/32 units, for one denominator beat
    let step = match meter.denominator {
        2 => 16,
        4 => 8,
        8 => 4,
        16 => 2,
        32 => 1,
        _ => return Vec::new(),
    };
    (1..meter.numerator as i32)
        .map(|i| GroupNode {
            pos: step * i,
            action: 0x111,
        })
        .collect()
}

/// The nibble shift for a note whose written flag count is `beams`
/// (eighth = shift 0, 16th = 4, 32nd-and-shorter = 8). `None` for quarter+.
fn beams_shift(beams: u8) -> Option<u32> {
    match beams {
        1 => Some(0),
        2 => Some(4),
        3.. => Some(8),
        0 => None,
    }
}

/// The nibble shift for a *tick length* (used for MuseScore's `bigBeatDuration`
/// "by position" lookup): eighth (240) = 0, 16th (120) = 4, 32nd-or-shorter = 8.
fn ticks_shift(ticks: i64) -> Option<u32> {
    match ticks {
        240 => Some(0),          // eighth
        120 => Some(4),          // 16th
        t if t <= 60 => Some(8), // 32nd or shorter
        _ => None,               // quarter+ or dotted → no break rule
    }
}

/// Look up the break mode for a note of the given `shift` (duration level) landing
/// exactly on `tick`. Positions between nodes → `Auto`.
fn beam_mode_at(nodes: &[GroupNode], tick: i64, shift: u32) -> BeamMode {
    for node in nodes {
        let node_tick = node.pos as i64 * D32;
        if node_tick < tick {
            continue;
        }
        if node_tick > tick {
            break;
        }
        let action = (node.action >> shift) & 0xf;
        return match action {
            1 => BeamMode::Begin,
            2 => BeamMode::Begin16,
            3 => BeamMode::Begin32,
            _ => BeamMode::Auto,
        };
    }
    BeamMode::Auto
}

/// MuseScore's `bigBeatDuration`: the coarsest power-of-two tick length (starting at
/// an eighth) that both is ≤ the local max note length and divides `rtick`.
fn big_beat_shift(rtick: i64, cur_dur: i64, prev_dur: i64) -> Option<u32> {
    let max_len = cur_dur.max(prev_dur);
    let mut smallest = 240i64; // eighth
    loop {
        if smallest <= max_len && rtick % smallest == 0 {
            break;
        }
        smallest /= 2;
        if smallest < D32 {
            smallest = cur_dur;
            break;
        }
    }
    ticks_shift(smallest)
}

/// MuseScore `Groups::baseBeamMode` — table lookup by duration and position with the
/// tuplet-boundary and hole corrections.
fn base_beam_mode(nodes: &[GroupNode], notes: &[BeamInput], i: usize) -> BeamMode {
    let cur = notes[i];
    let prev = i.checked_sub(1).map(|p| notes[p]);
    let tick = cur.rtick;

    let by_type = beams_shift(cur.beams)
        .map(|shift| beam_mode_at(nodes, tick, shift))
        .unwrap_or(BeamMode::Auto);
    let by_pos = big_beat_shift(tick, cur.dur, prev.map_or(0, |p| p.dur))
        .map(|shift| beam_mode_at(nodes, tick, shift))
        .unwrap_or(BeamMode::Auto);
    let mut val = if by_type == BeamMode::Auto {
        by_pos
    } else {
        by_type
    };

    if val == BeamMode::Auto
        && tick != 0
        && let Some(prev) = prev
    {
        // One (not both) endpoints in a tuplet, same written duration: treat as one
        // level shorter, nudging a break at the tuplet seam.
        if cur.tuplet_id != prev.tuplet_id && cur.beams == prev.beams && cur.beams >= 1 {
            val = beam_mode_at(nodes, tick, 4);
        }
        // A gap before this note forces a new beam.
        if prev.tuplet_id.is_none() && prev.rtick + prev.dur < cur.rtick {
            val = BeamMode::Begin;
        }
    }
    val
}

/// MuseScore `Groups::actualBeamMode` — final mode after the rest/quarter guards, the
/// barline-start rule, the x/4 beat-subdivision adaptivity, and `Auto → Mid`.
fn actual_beam_mode(
    nodes: &[GroupNode],
    notes: &[BeamInput],
    i: usize,
    meter: Meter,
    beat_min: &[i64],
) -> BeamMode {
    let cur = notes[i];
    // Rests and quarter-or-longer chords are never auto-beamed.
    if cur.is_rest || cur.beams == 0 {
        return BeamMode::None;
    }
    let mut bm = base_beam_mode(nodes, notes, i);
    if bm == BeamMode::Auto {
        if cur.rtick == 0 {
            return BeamMode::Begin;
        }
        // x/4 meters: on a quarter beat, re-evaluate using the shortest note in this
        // and the previous beat, so a beat of 16ths splits the eighth beam at the beat.
        if meter.denominator == 4 && cur.rtick % DIVISION == 0 {
            let beat = (cur.rtick / DIVISION) as usize;
            let cur_min = beat_min.get(beat).copied().unwrap_or(i64::MAX);
            let prev_min = beat.checked_sub(1).and_then(|b| beat_min.get(b)).copied();
            let min_dur = prev_min.map_or(cur_min, |p| p.min(cur_min));
            if min_dur != i64::MAX && min_dur < cur.dur {
                let mut probe = notes.to_vec();
                probe[i].dur = min_dur;
                probe[i].beams = beams_for_ticks(min_dur);
                bm = base_beam_mode(nodes, &probe, i);
            }
        }
    }
    match bm {
        BeamMode::Auto => BeamMode::Mid,
        other => other,
    }
}

/// Flag count of a power-of-two tick length (for the beat-subdivision probe).
fn beams_for_ticks(ticks: i64) -> u8 {
    match ticks {
        t if t >= DIVISION => 0, // quarter+
        120 => 2,
        60 => 3,
        30 => 4,
        15 => 5,
        _ => 1, // eighth (240) and anything else
    }
}

/// Per-quarter-beat minimum written duration in ticks (for the x/4 adaptivity).
fn beat_minimums(notes: &[BeamInput], meter: Meter) -> Vec<i64> {
    if meter.denominator != 4 {
        return Vec::new();
    }
    let beats = meter.numerator as usize + 1;
    let mut mins = vec![i64::MAX; beats + 1];
    for note in notes {
        if note.is_rest {
            continue;
        }
        let beat = (note.rtick / DIVISION) as usize;
        if beat < mins.len() {
            mins[beat] = mins[beat].min(note.dur);
        }
    }
    mins
}

/// The full beam plan for one voice/measure: the per-note `<beam>` segments plus the
/// beam groups (each a list of indices into `notes`) that produced them. The groups
/// feed the tuplet-bracket default (a tuplet that is exactly one beam shows no bracket).
pub(crate) struct BeamPlan {
    pub segments: Vec<Vec<BeamSegment>>,
    pub groups: Vec<Vec<usize>>,
}

/// Plan the `<beam>` segments and beam grouping for one voice/measure.
pub(crate) fn plan(notes: &[BeamInput], meter: Meter) -> BeamPlan {
    let mut segments = vec![Vec::new(); notes.len()];
    if notes.len() < 2 {
        return BeamPlan {
            segments,
            groups: Vec::new(),
        };
    }
    let nodes = group_nodes(meter);
    let beat_min = beat_minimums(notes, meter);
    let modes: Vec<BeamMode> = (0..notes.len())
        .map(|i| actual_beam_mode(&nodes, notes, i, meter, &beat_min))
        .collect();

    // Group consecutive non-`None` events into beams, splitting at `Begin`.
    let mut current: Vec<usize> = Vec::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let flush = |current: &mut Vec<usize>, groups: &mut Vec<Vec<usize>>| {
        if current.len() >= 2 {
            groups.push(std::mem::take(current));
        } else {
            current.clear();
        }
    };
    for (i, &mode) in modes.iter().enumerate() {
        match mode {
            BeamMode::None => flush(&mut current, &mut groups),
            BeamMode::Begin => {
                flush(&mut current, &mut groups);
                current.push(i);
            }
            _ => current.push(i),
        }
    }
    flush(&mut current, &mut groups);

    for group in &groups {
        emit_beam(group, notes, &modes, &mut segments);
    }
    BeamPlan { segments, groups }
}

/// Whether a tuplet should show a bracket under MuseScore's `AUTO_BRACKET` default:
/// the bracket is **hidden** (number only) exactly when the tuplet's members are all
/// beamed chords of uniform beam count with no rests, forming a single beam group the
/// tuplet exactly spans; otherwise the bracket is **shown**.
pub(crate) fn tuplet_shows_bracket(
    notes: &[BeamInput],
    groups: &[Vec<usize>],
    pair_id: u32,
) -> bool {
    let members: Vec<usize> = (0..notes.len())
        .filter(|&i| notes[i].tuplet_id == Some(pair_id))
        .collect();
    if members.len() < 2 {
        return false; // degenerate (single element) → no bracket
    }
    let first_beams = notes[members[0]].beams;
    for &i in &members {
        if notes[i].is_rest || notes[i].beams == 0 || notes[i].beams != first_beams {
            return true;
        }
    }
    // Number-only only if the members are exactly one beam group.
    let member_set: std::collections::BTreeSet<usize> = members.iter().copied().collect();
    let spans_one_beam = groups.iter().any(|group| {
        group
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            == member_set
    });
    !spans_one_beam
}

/// Emit per-level `<beam>` segments for one beam group (MuseScore `writeBeam`).
fn emit_beam(
    beam: &[usize],
    notes: &[BeamInput],
    modes: &[BeamMode],
    out: &mut [Vec<BeamSegment>],
) {
    for (pos, &i) in beam.iter().enumerate() {
        let blc = notes[i].beams as i32;
        let blp = if pos > 0 {
            notes[beam[pos - 1]].beams as i32
        } else {
            -1
        };
        let bln = if pos + 1 < beam.len() {
            notes[beam[pos + 1]].beams as i32
        } else {
            -1
        };
        let bmc = modes[i];
        let bmn = beam
            .get(pos + 1)
            .map(|&n| modes[n])
            .unwrap_or(BeamMode::Auto);

        for level in 1..=blc {
            let secondary_begin =
                (bmc == BeamMode::Begin16 && level > 1) || (bmc == BeamMode::Begin32 && level > 2);
            let secondary_end =
                (bmn == BeamMode::Begin16 && level > 1) || (bmn == BeamMode::Begin32 && level > 2);
            let text = if (blp < level && bln >= level) || secondary_begin {
                BeamText::Begin
            } else if blp < level && bln < level {
                if bln > 0 {
                    BeamText::ForwardHook
                } else if blp > 0 {
                    BeamText::BackwardHook
                } else {
                    continue;
                }
            } else if (blp >= level && bln < level) || secondary_end {
                BeamText::End
            } else {
                BeamText::Continue
            };
            out[i].push(BeamSegment {
                level: level as u8,
                text,
            });
        }
    }
}

#[cfg(test)]
#[path = "beam_tests.rs"]
mod tests;
