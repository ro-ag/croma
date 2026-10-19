//! Beam grouping and tuplet-bracket decisions for one voice of one measure.
//!
//! Implements `docs/engraving/rules.md` §1 (`plan`: the meter's beat grid §1.2,
//! the subdivision chain and beam spans §1.3, the grouping and output rules
//! B1–B12 §1.4, in the order of the decision procedure §1.5) and §2
//! (`tuplet_shows_bracket`). The Rust boundary is fixed by
//! `docs/engraving/contract.md`.
//!
//! Everything is computed arithmetically per note (rules.md §0.3): nothing is
//! allocated per beat or per bar, so a meter numerator of `u32::MAX` costs the
//! same as 4/4, and all tick arithmetic is `i64` with floor (Euclidean)
//! division so overfull measures and negative onsets stay well defined.

/// Ticks in a whole note; a quarter is 480 (rules.md §0.1).
const WHOLE: i64 = 1920;

/// The deepest beam level MusicXML can number: 1 (eighth) .. 8 (1024th)
/// (rules.md §0.2).
const MAX_LEVEL: u8 = 8;

/// Bound on the subdivision chain's length (rules.md §1.3, §0.3).
const MAX_CHAIN: usize = 64;

/// One event of a voice-measure as the beam planner sees it. A chord is a
/// single input; grace notes and chord members are not listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BeamInput {
    /// Onset within the measure, in ticks from the barline.
    pub rtick: i64,
    /// Written duration, in ticks.
    pub dur: i64,
    /// Flag count of the written note type (eighth = 1, 16th = 2, …; quarter
    /// or longer = 0).
    pub beams: u8,
    /// Whether the event is a rest.
    pub is_rest: bool,
    /// The innermost tuplet this event belongs to, if any.
    pub tuplet_id: Option<u32>,
}

/// One `<beam number="level">text</beam>` element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BeamSegment {
    /// 1 = primary (eighth) beam, 2 = 16th beam, 3 = 32nd, …
    pub level: u8,
    /// The MusicXML beam value at that level.
    pub text: BeamText,
}

/// The MusicXML `beam-value` vocabulary (rules.md §0.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BeamText {
    Begin,
    Continue,
    End,
    ForwardHook,
    BackwardHook,
}

impl BeamText {
    /// The `<beam>` element text for this value.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Begin => "begin",
            Self::Continue => "continue",
            Self::End => "end",
            Self::ForwardHook => "forward hook",
            Self::BackwardHook => "backward hook",
        }
    }
}

/// The literal time signature of the measure (6/8 stays 6/8; `C` arrives as
/// 4/4 and `C|` as 2/2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Meter {
    pub numerator: u32,
    pub denominator: u32,
}

/// The planner's answer for one voice-measure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BeamPlan {
    /// One entry per input, in input order; empty = no `<beam>` on that note.
    pub segments: Vec<Vec<BeamSegment>>,
    /// Each beam group as ascending input indices (2 or more members), in
    /// ascending order of first index.
    pub groups: Vec<Vec<usize>>,
}

/// How the meter divides a bar into beats (rules.md §1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grid {
    /// G0: quarter-note beats from the barline, for meters that do not fit the
    /// tick grid.
    Quarter,
    /// G1–G4: `n` units of `unit` ticks per bar of `bar` ticks.
    Metered {
        shape: Shape,
        n: i64,
        unit: i64,
        bar: i64,
    },
}

/// Which of the metered rules G1–G4 applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// G1: dotted beats of three units.
    Compound,
    /// G2: one beat per unit (quarter or longer).
    LongUnit,
    /// G3: short unit, numerator 1 to 4.
    Short,
    /// G4: short unit, irregular numerator (2s with a final 3 when odd).
    Irregular,
}

/// Where an onset falls on the beat grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Place {
    /// Bar index `k` (floor division, so negative onsets land in bar -1).
    bar: i64,
    /// Beat start `bs` within the bar for G1–G4, or the beat index for G0;
    /// with `bar` it identifies the beat.
    beat: i64,
    /// Beat length `B`.
    len: i64,
    /// The beat's first division `d1` (2 or 3).
    d1: i64,
    /// `tl - bs`: the onset's distance from the beat start.
    offset: i64,
}

impl Place {
    fn same_beat(self, other: Self) -> bool {
        self.bar == other.bar && self.beat == other.beat
    }

    /// The level-`level` cell `floor((tl - bs) / S_L)` (rules.md §1.3).
    fn cell(self, level: u8) -> i64 {
        self.offset.div_euclid(span(self.len, self.d1, level))
    }
}

/// The per-input facts the planner needs (rules.md §1.5 step 3).
#[derive(Debug, Clone, Copy)]
struct Facts {
    beamable: bool,
    /// `min(beams, 8)` (rules.md §0.2).
    beams: u8,
    place: Place,
}

/// Classify the meter once (rules.md §1.2).
fn grid(meter: Meter) -> Grid {
    let n = i64::from(meter.numerator);
    let d = i64::from(meter.denominator);
    // rules.md G0: a zero meter or a denominator that does not divide the whole
    // note's ticks cannot be put on the grid.
    if n == 0 || d == 0 || WHOLE % d != 0 {
        return Grid::Quarter;
    }
    let unit = WHOLE / d;
    let shape = if n >= 6 && n % 3 == 0 {
        Shape::Compound
    } else if unit > 240 {
        Shape::LongUnit
    } else if n <= 4 {
        Shape::Short
    } else {
        Shape::Irregular
    };
    // n <= u32::MAX and unit <= 1920, so the bar length fits in i64 (≈ 8.2e12).
    Grid::Metered {
        shape,
        n,
        unit,
        bar: n * unit,
    }
}

/// Place onset `t` on the grid: bar index, beat, beat length, first division
/// and offset in the beat (rules.md §1.2 G0–G4). O(1), no per-beat tables.
fn locate(grid: Grid, t: i64) -> Place {
    let Grid::Metered {
        shape,
        n,
        unit,
        bar,
    } = grid
    else {
        // rules.md G0: beat index = floor(t / 480). The offset comes from the
        // Euclidean remainder rather than `t - index * 480`, which could
        // overflow at the ends of the i64 range.
        return Place {
            bar: 0,
            beat: t.div_euclid(480),
            len: 480,
            d1: 2,
            offset: t.rem_euclid(480),
        };
    };
    let bar_index = t.div_euclid(bar);
    // 0 <= local < bar, so every product below stays inside the bar.
    let local = t.rem_euclid(bar);
    let p = local / unit;
    let (len, d1, start) = match shape {
        // rules.md G1: dotted beats of three units.
        Shape::Compound => (3 * unit, 3, local - local % (3 * unit)),
        // rules.md G2: the unit is the beat.
        Shape::LongUnit => (unit, 2, p * unit),
        // rules.md G3: 1, 2 and 3 units make one whole-bar beat; 4 is 2+2.
        Shape::Short => {
            let (len, d1) = match n {
                1 => (unit, 2),
                3 => (3 * unit, 3),
                _ => (2 * unit, 2),
            };
            (len, d1, local - local % len)
        }
        // rules.md G4: pairs of units, and a final 3-unit beat when n is odd.
        // `n - 3` is used directly so a huge numerator costs nothing extra.
        Shape::Irregular => {
            if n % 2 == 0 || p < n - 3 {
                (2 * unit, 2, (p - p % 2) * unit)
            } else {
                (3 * unit, 3, (n - 3) * unit)
            }
        }
    };
    Place {
        bar: bar_index,
        beat: start,
        len,
        d1,
        offset: local - start,
    }
}

/// The level-`level` span `S_L`: the largest element of the beat's
/// subdivision chain that is `<= 2880 >> level`, else the chain's smallest
/// element (rules.md §1.3). The chain is walked, never stored.
fn span(beat_len: i64, d1: i64, level: u8) -> i64 {
    let limit = 2880_i64 >> level.min(MAX_LEVEL);
    // rules.md §1.3: c0 = B; c1 = B / d1 when it divides; then halve while
    // the previous element is even. The chain is strictly decreasing, so the
    // first element under the limit is the largest one.
    let mut element = beat_len.max(1);
    if element <= limit {
        return element;
    }
    let mut next = (d1 > 0 && element % d1 == 0).then(|| element / d1);
    for _ in 1..MAX_CHAIN {
        let Some(candidate) = next.filter(|&candidate| candidate >= 1) else {
            break;
        };
        element = candidate;
        if element <= limit {
            return element;
        }
        next = (element % 2 == 0).then_some(element / 2);
    }
    element
}

/// Plan the beams of one voice-measure (rules.md §1.5).
pub(crate) fn plan(notes: &[BeamInput], meter: Meter) -> BeamPlan {
    let mut segments: Vec<Vec<BeamSegment>> = vec![Vec::new(); notes.len()];
    if notes.is_empty() {
        return BeamPlan {
            segments,
            groups: Vec::new(),
        };
    }
    let grid = grid(meter);
    let facts: Vec<Facts> = notes
        .iter()
        .map(|note| {
            let beams = note.beams.min(MAX_LEVEL);
            Facts {
                // rules.md B1: a rest or a flagless value is never beamed.
                beamable: !note.is_rest && beams >= 1,
                beams,
                place: locate(grid, note.rtick),
            }
        })
        .collect();

    let provisional = provisional_groups(notes, &facts);
    let merged = merge_eighth_beats(provisional, notes, &facts, grid);
    // rules.md B7: a lone note keeps its flag.
    let groups: Vec<Vec<usize>> = merged.into_iter().filter(|g| g.len() >= 2).collect();

    for group in &groups {
        emit_group(group, notes, &facts, &mut segments);
    }
    BeamPlan { segments, groups }
}

/// Whether adjacent beamable inputs `a` then `b` may share a beam (rules.md
/// B3–B5).
fn joins(a: (&BeamInput, &Facts), b: (&BeamInput, &Facts)) -> bool {
    // rules.md B3: an out-of-order onset breaks the beam.
    if b.0.rtick < a.0.rtick {
        return false;
    }
    match (a.0.tuplet_id, b.0.tuplet_id) {
        // rules.md B4: same beat and same level-1 cell, by onsets only.
        (None, None) => a.1.place.same_beat(b.1.place) && a.1.place.cell(1) == b.1.place.cell(1),
        // rules.md B5: a tuplet beams as a unit, across beat boundaries.
        (Some(x), Some(y)) => x == y,
        // rules.md B5 [choice]: a tuplet never merges with its neighbours.
        _ => false,
    }
}

/// Scan left to right building provisional groups, including one-member ones
/// (rules.md §1.5 step 4). Rests and flagless values end the current group
/// and join none (B1, B2).
fn provisional_groups(notes: &[BeamInput], facts: &[Facts]) -> Vec<Vec<usize>> {
    let mut groups = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut previous: Option<(&BeamInput, &Facts)> = None;
    for (index, (note, fact)) in notes.iter().zip(facts).enumerate() {
        if !fact.beamable {
            if !current.is_empty() {
                groups.push(std::mem::take(&mut current));
            }
            previous = None;
            continue;
        }
        let continues = previous.is_some_and(|prev| joins(prev, (note, fact)));
        if !continues && !current.is_empty() {
            groups.push(std::mem::take(&mut current));
        }
        current.push(index);
        previous = Some((note, fact));
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

/// If `group` is a complete eighth beat (rules.md B6), its `(bar, beat
/// index)`; the beat index is `bs / unit`.
fn complete_eighth_beat(
    group: &[usize],
    notes: &[BeamInput],
    facts: &[Facts],
    unit: i64,
) -> Option<(i64, i64)> {
    let [first, second] = group else {
        return None;
    };
    let (a_note, a) = (notes.get(*first)?, facts.get(*first)?);
    let (b_note, b) = (notes.get(*second)?, facts.get(*second)?);
    let pure_eighth = |note: &BeamInput, fact: &Facts| {
        note.tuplet_id.is_none() && fact.beams == 1 && note.dur == 240
    };
    let on_the_beat = a.place.same_beat(b.place) && a.place.offset == 0 && b.place.offset == 240;
    (pure_eighth(a_note, a) && pure_eighth(b_note, b) && on_the_beat)
        .then(|| (a.place.bar, a.place.beat / unit))
}

/// rules.md B6: in 2/4 and 3/4 a whole bar, and in 4/4 each half-bar, of
/// complete eighth beats is beamed as one group. Any other grid is returned
/// unchanged.
fn merge_eighth_beats(
    groups: Vec<Vec<usize>>,
    notes: &[BeamInput],
    facts: &[Facts],
    grid: Grid,
) -> Vec<Vec<usize>> {
    let Grid::Metered {
        shape: Shape::LongUnit,
        n: n @ 2..=4,
        unit: 480,
        ..
    } = grid
    else {
        return groups;
    };
    // rules.md B6: 2/4 → {0,1}, 3/4 → {0,1,2}, 4/4 → {0,1} and {2,3}.
    let span_len: usize = if n == 3 { 3 } else { 2 };
    let beats: Vec<Option<(i64, i64)>> = groups
        .iter()
        .map(|group| complete_eighth_beat(group, notes, facts, 480))
        .collect();

    let mut out = Vec::with_capacity(groups.len());
    let mut slots: Vec<Option<Vec<usize>>> = groups.into_iter().map(Some).collect();
    let mut at = 0;
    while at < slots.len() {
        if merges_from(at, span_len, &beats, &slots) {
            let mut merged = Vec::new();
            for slot in slots.iter_mut().skip(at).take(span_len) {
                merged.extend(slot.take().unwrap_or_default());
            }
            out.push(merged);
            at += span_len;
        } else {
            if let Some(group) = slots.get_mut(at).and_then(Option::take) {
                out.push(group);
            }
            at += 1;
        }
    }
    out
}

/// Whether the `span_len` provisional groups starting at `at` form a complete
/// merge span: each a complete eighth beat of the same bar, beats
/// `b, b+1, …` with `b` a span start, and consecutive in input order.
fn merges_from(
    at: usize,
    span_len: usize,
    beats: &[Option<(i64, i64)>],
    slots: &[Option<Vec<usize>>],
) -> bool {
    let Some(Some((bar, first_beat))) = beats.get(at).copied() else {
        return false;
    };
    // span_len is 2 or 3, so the cast is exact.
    if first_beat.rem_euclid(span_len as i64) != 0 {
        return false;
    }
    let mut previous_last: Option<usize> = None;
    for (expected, index) in (first_beat..).zip(at..at + span_len) {
        if beats.get(index).copied().flatten() != Some((bar, expected)) {
            return false;
        }
        let Some(Some(group)) = slots.get(index) else {
            return false;
        };
        if let Some(last) = previous_last
            && group.first().copied() != last.checked_add(1)
        {
            return false;
        }
        previous_last = group.last().copied();
    }
    true
}

/// Emit every level's `<beam>` segments for one group (rules.md B8–B11).
fn emit_group(
    group: &[usize],
    notes: &[BeamInput],
    facts: &[Facts],
    segments: &mut [Vec<BeamSegment>],
) {
    let Some(last) = group.len().checked_sub(1) else {
        return;
    };
    let beams_of = |index: usize| facts.get(index).map_or(0, |fact| fact.beams);
    let mut push = |index: usize, level: u8, text: BeamText| {
        if let Some(list) = segments.get_mut(index) {
            list.push(BeamSegment { level, text });
        }
    };

    // rules.md B8: level 1 runs unbroken from the first member to the last.
    for (pos, &index) in group.iter().enumerate() {
        let text = if pos == 0 {
            BeamText::Begin
        } else if pos == last {
            BeamText::End
        } else {
            BeamText::Continue
        };
        push(index, 1, text);
    }

    // A group is a tuplet group when its members carry a tuplet id; B5 makes
    // every member share it.
    let tuplet_group = group
        .first()
        .and_then(|&i| notes.get(i))
        .is_some_and(|note| note.tuplet_id.is_some());
    // rules.md B10: in a tuplet group a member's position is the sum of the
    // written durations before it (tuplet onsets are scaled).
    let mut written_pos = Vec::with_capacity(group.len());
    let mut running = 0_i64;
    for &index in group {
        written_pos.push(running);
        running = running.saturating_add(notes.get(index).map_or(0, |note| note.dur));
    }
    let top = group.iter().map(|&i| beams_of(i)).max().unwrap_or(0);

    for level in 2..=top {
        // rules.md B9: consecutive members are connected at this level when
        // both reach it and (outside tuplets) share the beat and level-L cell.
        let connected = |a: usize, b: usize| {
            if beams_of(a) < level || beams_of(b) < level {
                return false;
            }
            if tuplet_group {
                return true;
            }
            match (facts.get(a), facts.get(b)) {
                (Some(x), Some(y)) => {
                    x.place.same_beat(y.place) && x.place.cell(level) == y.place.cell(level)
                }
                _ => false,
            }
        };
        let mut pos = 0;
        while let Some(&index) = group.get(pos) {
            if beams_of(index) < level {
                pos += 1;
                continue;
            }
            let start = pos;
            while let (Some(&a), Some(&b)) = (group.get(pos), group.get(pos + 1)) {
                if !connected(a, b) {
                    break;
                }
                pos += 1;
            }
            if pos > start {
                for (run_pos, &member) in group.iter().enumerate().take(pos + 1).skip(start) {
                    let text = if run_pos == start {
                        BeamText::Begin
                    } else if run_pos == pos {
                        BeamText::End
                    } else {
                        BeamText::Continue
                    };
                    push(member, level, text);
                }
            } else {
                let position = if tuplet_group {
                    written_pos.get(pos).copied().unwrap_or(0)
                } else {
                    facts.get(index).map_or(0, |fact| fact.place.offset)
                };
                push(index, level, hook(pos, last, level, position));
            }
            pos += 1;
        }
    }
}

/// The partial-beam direction for a lone member at `pos` of a group whose last
/// position is `last` (rules.md B10, P7).
fn hook(pos: usize, last: usize, level: u8, position: i64) -> BeamText {
    if pos == 0 {
        return BeamText::ForwardHook;
    }
    if pos == last {
        return BeamText::BackwardHook;
    }
    // rules.md B10: v is the value of a note with one beam fewer. A note on a
    // subdivision start belongs with what follows; otherwise with what precedes.
    let v = WHOLE >> (level.min(MAX_LEVEL) + 1);
    if v > 0 && position.rem_euclid(v) == 0 {
        BeamText::ForwardHook
    } else {
        BeamText::BackwardHook
    }
}

/// Whether tuplet `pair_id` prints its bracket (`true`) or only its number
/// (`false`), given the beam groups `plan` returned (rules.md §2.2).
pub(crate) fn tuplet_shows_bracket(
    notes: &[BeamInput],
    groups: &[Vec<usize>],
    pair_id: u32,
) -> bool {
    // rules.md T1: the members, ascending.
    let members: Vec<usize> = notes
        .iter()
        .enumerate()
        .filter(|(_, note)| note.tuplet_id == Some(pair_id))
        .map(|(index, _)| index)
        .collect();
    // rules.md §2.2 step 2 [choice]: no members → bracket (harmless default).
    if members.is_empty() {
        return true;
    }
    // rules.md T2/§2.2 step 3: only a beam covering exactly the tuplet shows its
    // extent; anything else (T3) needs the bracket.
    !groups.contains(&members)
}

#[cfg(test)]
#[path = "beam_tests.rs"]
mod tests;
