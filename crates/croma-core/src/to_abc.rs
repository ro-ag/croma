//! Canonical `Score` -> ABC writer (the reverse of the MusicXML writer).
//!
//! Emits ABC that is a `croma fmt` fixed point and round-trips through
//! `parse_document` + `lower_score` with an identical structural projection.
use crate::model::TieRole;
use crate::{
    Accidental, AccidentalMark, BarlineKind, Pitch, Rational, RestVisibility, Score, TimedEventKind,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct AbcWriteOptions {}

/// Emit canonical ABC for `score`. Output is a `croma fmt` fixed point.
pub fn write_abc(score: &Score, _options: AbcWriteOptions) -> String {
    let mut out = String::new();
    let meta = &score.metadata;
    out.push_str(&format!("X:{}\n", meta.reference.text.trim()));
    if let Some(title) = &meta.title {
        out.push_str(&format!("T:{}\n", title.text.trim()));
    }
    let meter_display = meta
        .meter
        .as_ref()
        .map(|m| m.display.clone())
        .unwrap_or_else(|| "4/4".to_string());
    out.push_str(&format!("M:{meter_display}\n"));
    let unit = unit_length(score);
    out.push_str(&format!("L:{}/{}\n", unit.numerator, unit.denominator));
    if let Some(q) = tempo_field(score) {
        out.push_str(&format!("Q:{q}\n"));
    }
    let key_display = meta
        .key
        .as_ref()
        .map(|k| k.display.clone())
        .unwrap_or_else(|| "C".to_string());
    out.push_str(&format!("K:{key_display}\n"));
    out.push_str(&write_body(score, unit));
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Unit note length: ABC 2.1 default — measure duration < 3/4 → 1/16, else 1/8.
fn unit_length(score: &Score) -> Rational {
    let small = Rational::new(1, 16);
    let normal = Rational::new(1, 8);
    match score.metadata.meter.as_ref().and_then(|m| m.duration) {
        Some(dur) if (dur.numerator as u64) * 4 < (dur.denominator as u64) * 3 => small,
        _ => normal,
    }
}

fn tempo_field(score: &Score) -> Option<String> {
    let beat = score.metadata.tempo_model.as_ref()?.beat.as_ref()?;
    Some(format!(
        "{}/{}={}",
        beat.beat_numerator, beat.beat_denominator, beat.bpm
    ))
}

fn write_body(score: &Score, unit: Rational) -> String {
    let mut out = String::new();
    let Some(voice) = score.parts.first().and_then(|p| p.voices.first()) else {
        return out;
    };
    let (markers, scales) = tuplet_layout(&voice.events);
    let key_fifths = score.metadata.key.as_ref().map(|k| k.fifths).unwrap_or(0);
    // Per-measure accidental state, keyed by (step, octave), reset at each
    // barline — mirrors the parser so the writer only adds an explicit
    // accidental when the note's alter would not otherwise be reproduced.
    let mut measure_alters: std::collections::HashMap<(char, i8), i8> =
        std::collections::HashMap::new();
    for (idx, event) in voice.events.iter().enumerate() {
        // A tuplet marker `(p:q:r` opens before the first note/rest/chord of the
        // group.
        if let Some(marker) = &markers[idx] {
            out.push_str(marker);
        }
        let tuplet = scales[idx];
        match &event.kind {
            TimedEventKind::Note(note) => {
                let has_tie_stop = event
                    .attachments
                    .ties
                    .iter()
                    .any(|t| t.role == crate::model::TieRole::Stop);
                out.push_str(&event_prefix(&event.attachments));
                out.push_str(note_accidental(
                    &note.pitch,
                    note.written_accidental.as_ref(),
                    has_tie_stop,
                    key_fifths,
                    &mut measure_alters,
                ));
                out.push_str(&pitch_str(&note.pitch));
                out.push_str(&length_str(notated_duration(event.duration, tuplet), unit));
                out.push_str(&event_suffix(&event.attachments));
                out.push(' ');
            }
            TimedEventKind::Rest(rest) => {
                out.push_str(&event_prefix(&event.attachments));
                out.push(match rest.visibility {
                    RestVisibility::Visible => 'z',
                    RestVisibility::Invisible => 'x',
                });
                out.push_str(&length_str(notated_duration(event.duration, tuplet), unit));
                out.push_str(&event_suffix(&event.attachments));
                out.push(' ');
            }
            TimedEventKind::Chord(chord) => {
                // Slurs and ties can be recorded on the chord event, on
                // individual members, or (redundantly) on both. Merge them,
                // deduping by (pair_id, role), so the surrounding `(`, `)` and
                // `-` are emitted exactly once per distinct slur/tie.
                let mut merged = event.attachments.clone();
                for member in &chord.members {
                    for slur in &member.attachments.slurs {
                        if !merged
                            .slurs
                            .iter()
                            .any(|x| x.pair_id == slur.pair_id && x.role == slur.role)
                        {
                            merged.slurs.push(*slur);
                        }
                    }
                    for tie in &member.attachments.ties {
                        if !merged
                            .ties
                            .iter()
                            .any(|x| x.pair_id == tie.pair_id && x.role == tie.role)
                        {
                            merged.ties.push(*tie);
                        }
                    }
                }
                out.push_str(&event_prefix(&merged));
                // Per-member lengths; factor out a shared length to the outer
                // `[...]L` form when every member matches (e.g. `[CEG]2`), else
                // emit each member's own length (e.g. `[d3f]`).
                let lengths: Vec<String> = chord
                    .members
                    .iter()
                    .map(|m| length_str(notated_duration(m.duration, tuplet), unit))
                    .collect();
                let uniform = lengths.windows(2).all(|w| w[0] == w[1]);
                out.push('[');
                for (member, len) in chord.members.iter().zip(&lengths) {
                    let member_tie_stop = member
                        .attachments
                        .ties
                        .iter()
                        .any(|t| t.role == crate::model::TieRole::Stop);
                    out.push_str(note_accidental(
                        &member.pitch,
                        member.written_accidental.as_ref(),
                        member_tie_stop,
                        key_fifths,
                        &mut measure_alters,
                    ));
                    out.push_str(&pitch_str(&member.pitch));
                    if !uniform {
                        out.push_str(len);
                    }
                }
                out.push(']');
                if let Some(len) = lengths.first().filter(|_| uniform) {
                    out.push_str(len);
                }
                out.push_str(&event_suffix(&merged));
                out.push(' ');
            }
            TimedEventKind::Barline(b) => {
                measure_alters.clear();
                out.push_str(barline_str(b.kind));
                out.push(' ');
            }
            TimedEventKind::RepeatEnding(r) => {
                out.push_str(&ending_str(r));
                out.push(' ');
            }
            _ => {} // Spacer is out of scope
        }
    }
    format!("{}\n", out.trim_end())
}

/// Per-event tuplet open markers (`Some("(p:q:r")` at each group's first event).
type TupletMarkers = Vec<Option<String>>;
/// Per-event tuplet (actual, normal) ratio used to scale a notated length.
type TupletScales = Vec<Option<(u32, u32)>>;

/// Tuplet layout: for each event index, the open marker (`Some("(p:q:r")` at the
/// group's first event) and the ratio that scales that event's notated length.
///
/// Groups are keyed by `pair_id`; the span `r` is `Stop_index - Start_index + 1`,
/// which naturally folds in any rests *inside* the tuplet (they carry no
/// attachment). A tuplet LED by a rest has no `Start` event, so its true first
/// index is unknown — those are excluded by the harness, not emitted here.
fn tuplet_layout(events: &[crate::TimedEvent]) -> (TupletMarkers, TupletScales) {
    use crate::model::TupletRole;
    use std::collections::BTreeMap;
    // pair_id -> (actual, normal, start_index, stop_index, has_start)
    let mut groups: BTreeMap<u32, (u32, u32, usize, usize, bool)> = BTreeMap::new();
    for (i, event) in events.iter().enumerate() {
        for tuplet in &event.attachments.tuplets {
            let entry = groups.entry(tuplet.pair_id).or_insert((
                tuplet.actual_notes,
                tuplet.normal_notes,
                i,
                i,
                false,
            ));
            entry.0 = tuplet.actual_notes;
            entry.1 = tuplet.normal_notes;
            entry.2 = entry.2.min(i);
            entry.3 = entry.3.max(i);
            if tuplet.role == TupletRole::Start {
                entry.4 = true;
            }
        }
    }
    let mut markers = vec![None; events.len()];
    let mut scales = vec![None; events.len()];
    for (_pid, (actual, normal, start, stop, has_start)) in groups {
        // The span `r` is the event count from Start to Stop inclusive, which
        // also folds in any rests *inside* the tuplet (they carry no attachment).
        // A tuplet LED by a rest has no Start event, so its true first index is
        // unknown — such tunes are excluded by the harness, not emitted here.
        if !has_start {
            continue;
        }
        let span = stop - start + 1;
        markers[start] = Some(format!("({actual}:{normal}:{span}"));
        for slot in scales.iter_mut().take(stop + 1).skip(start) {
            *slot = Some((actual, normal));
        }
    }
    (markers, scales)
}

/// Attachments emitted BEFORE a note/rest head.
///
/// Order matters for binding: a grace group, slur `(`, or decoration that
/// precedes a grace group binds to the *grace* note, not the main note (ABC 2.1
/// §4.11/§4.20). So grace comes first, then the event's own slur-opens and
/// decorations, which therefore bind to the main note head:
/// `"Gm"{gf}(!trill!note`.
fn event_prefix(attachments: &crate::EventAttachments) -> String {
    use crate::model::SlurRole;
    let mut out = String::new();
    // Quoted strings: chord symbols (`"Gm"`) and annotations (`"^text"`). The
    // annotation `text` already carries its placement char, and chord-symbol
    // text never starts with one, so the parser re-distinguishes them; both
    // simply re-emit as `"<text>"`.
    for chord_symbol in &attachments.chord_symbols {
        out.push_str(&format!("\"{}\"", chord_symbol.text));
    }
    for annotation in &attachments.annotations {
        out.push_str(&format!("\"{}\"", annotation.text));
    }
    for grace in &attachments.grace_groups {
        // A slur recorded on the grace group binds to its first grace note, so
        // its `(` opens before the group (`({gf}` ...).
        for slur in &grace.slurs {
            if slur.role == SlurRole::Start {
                out.push('(');
            }
        }
        out.push_str(&grace_str(grace));
    }
    for slur in &attachments.slurs {
        if slur.role == SlurRole::Start {
            out.push('(');
        }
    }
    for deco in &attachments.decorations {
        out.push_str(&decoration_str(&deco.name));
    }
    out
}

/// Canonical ABC for a decoration. Shorthands and long forms both normalize to
/// the same canonical name on parse (`~`/`!roll!` -> "roll"), so the long
/// `!name!` form re-parses to an identical decoration; `.` (staccato) keeps its
/// shorthand, which is how the parser canonicalizes it.
fn decoration_str(name: &str) -> String {
    if name == "." {
        ".".to_string()
    } else {
        format!("!{name}!")
    }
}

/// Attachments emitted AFTER a note/rest (length suffix already written): the
/// tie marker, then one `)` per slur that stops on this event.
fn event_suffix(attachments: &crate::EventAttachments) -> String {
    use crate::model::SlurRole;
    let mut out = String::new();
    if attachments.ties.iter().any(|t| t.role == TieRole::Start) {
        out.push('-');
    }
    for slur in &attachments.slurs {
        if slur.role == SlurRole::Stop {
            out.push(')');
        }
    }
    out
}

/// Canonical ABC first/second-ending marker, e.g. `[1`, `[2`, `[1,3`, `[1-2`.
fn ending_str(model: &crate::model::RepeatEndingModel) -> String {
    use crate::model::RepeatEndingPartModel::{Range, Single};
    let parts: Vec<String> = model
        .endings
        .iter()
        .map(|p| match p {
            Single(n) => n.to_string(),
            Range { start, end } => format!("{start}-{end}"),
        })
        .collect();
    format!("[{}", parts.join(","))
}

/// Canonical ABC text for a barline kind.
fn barline_str(kind: BarlineKind) -> &'static str {
    match kind {
        BarlineKind::Regular => "|",
        BarlineKind::Double => "||",
        BarlineKind::Final => "|]",
        BarlineKind::RepeatStart => "|:",
        BarlineKind::RepeatEnd => ":|",
        BarlineKind::RepeatBoth => "::",
        // Out of slice-1 scope: emit a plain bar so output still parses, but
        // note this CHANGES the barline kind (e.g. Dotted -> Regular). Tunes
        // containing these kinds are excluded from the corpus proof by the
        // `_FORBIDDEN_BARLINE_RE` filter in `tools/prove_abc_roundtrip.py`; a
        // future slice that admits them must drop that exclusion and add real
        // emission here, or the round-trip silently regresses.
        BarlineKind::Initial
        | BarlineKind::Dotted
        | BarlineKind::Invisible
        | BarlineKind::Liberal => "|",
    }
}

/// ABC glyph for a concrete alter value.
fn alter_glyph(alter: i8) -> &'static str {
    match alter {
        -2 => "__",
        -1 => "_",
        0 => "=",
        1 => "^",
        2 => "^^",
        _ => "",
    }
}

/// ABC glyph for an explicitly written accidental.
fn accidental_glyph(kind: Accidental) -> &'static str {
    match kind {
        Accidental::DoubleFlat => "__",
        Accidental::Flat => "_",
        Accidental::Natural => "=",
        Accidental::Sharp => "^",
        Accidental::DoubleSharp => "^^",
    }
}

/// The alter the key signature assigns to `step`, derived from the fifths count
/// (sharps F C G D A E B; flats B E A D G C F).
fn key_alter(step: char, fifths: i8) -> i8 {
    const SHARPS: [char; 7] = ['F', 'C', 'G', 'D', 'A', 'E', 'B'];
    const FLATS: [char; 7] = ['B', 'E', 'A', 'D', 'G', 'C', 'F'];
    let step = step.to_ascii_uppercase();
    if fifths > 0
        && SHARPS
            .iter()
            .take((fifths as usize).min(7))
            .any(|&c| c == step)
    {
        1
    } else if fifths < 0
        && FLATS
            .iter()
            .take(((-fifths) as usize).min(7))
            .any(|&c| c == step)
    {
        -1
    } else {
        0
    }
}

/// Accidental prefix for a note, updating the per-measure accidental state.
///
/// Emits the originally written accidental when present. Otherwise emits an
/// explicit accidental ONLY when the note's `alter` would not otherwise be
/// reproduced — i.e. it differs from what the key + measure-carry would yield —
/// which is a safety net for alters that come from sources the writer can't
/// re-express (e.g. an accidental carried by a parser-dropped cross-bar tie). A
/// tie carries the accidental for us, so no explicit glyph is emitted there.
/// State is keyed by (step, octave), matching the parser's per-octave carry.
fn note_accidental(
    pitch: &Pitch,
    written: Option<&AccidentalMark>,
    has_tie_stop: bool,
    key_fifths: i8,
    state: &mut std::collections::HashMap<(char, i8), i8>,
) -> &'static str {
    let key = (pitch.step.to_ascii_uppercase(), pitch.octave);
    let expected = state
        .get(&key)
        .copied()
        .unwrap_or_else(|| key_alter(pitch.step, key_fifths));
    if let Some(mark) = written {
        state.insert(key, pitch.alter);
        return accidental_glyph(mark.kind);
    }
    if pitch.alter != expected {
        state.insert(key, pitch.alter);
        if has_tie_stop {
            return "";
        }
        return alter_glyph(pitch.alter);
    }
    ""
}

/// Pitch letter plus octave marks (middle C = octave 4: `C`=4, `c`=5).
fn pitch_str(pitch: &Pitch) -> String {
    let letter = pitch.step.to_ascii_uppercase();
    if pitch.octave >= 5 {
        let mut s = letter.to_ascii_lowercase().to_string();
        s.push_str(&"'".repeat((pitch.octave - 5) as usize));
        s
    } else {
        let mut s = letter.to_string();
        s.push_str(&",".repeat((4 - pitch.octave) as usize));
        s
    }
}

/// Length suffix for `duration` relative to `unit`: `mult = duration / unit`.
fn length_str(duration: Rational, unit: Rational) -> String {
    let mult = Rational::new(
        duration.numerator.saturating_mul(unit.denominator),
        duration.denominator.saturating_mul(unit.numerator),
    );
    length_ratio_str(mult)
}

/// The NOTATED duration to render for an event: inside a tuplet, the stored
/// duration is the compressed (played) value, but ABC writes the pre-compression
/// notated length and the parser re-applies the ratio — so scale back up by
/// `actual/normal`. Outside a tuplet this is the duration unchanged.
fn notated_duration(duration: Rational, tuplet: Option<(u32, u32)>) -> Rational {
    match tuplet {
        Some((actual, normal)) => Rational::new(
            duration.numerator.saturating_mul(actual),
            duration.denominator.saturating_mul(normal),
        ),
        None => duration,
    }
}

/// ABC length suffix for an already-reduced multiplier of the unit length.
fn length_ratio_str(mult: Rational) -> String {
    match (mult.numerator, mult.denominator) {
        (1, 1) => String::new(),
        (n, 1) => n.to_string(),
        (1, 2) => "/".to_string(),
        (1, d) => format!("/{d}"),
        (n, d) => format!("{n}/{d}"),
    }
}

/// ABC grace group: `{...}` (or `{/...}` for an acciaccatura/slashed group).
/// Grace-note lengths are relative to the grace base unit via `length_multiplier`.
fn grace_str(group: &crate::model::GraceGroupAttachment) -> String {
    use crate::model::GraceEventKind;
    let mut out = String::from("{");
    if group.slash.is_some() {
        out.push('/');
    }
    for grace in &group.events {
        match &grace.kind {
            GraceEventKind::Note(note) => {
                out.push_str(
                    note.written_accidental
                        .as_ref()
                        .map(|m| accidental_glyph(m.kind))
                        .unwrap_or(""),
                );
                out.push_str(&pitch_str(&note.pitch));
                out.push_str(&length_ratio_str(note.length_multiplier));
            }
            GraceEventKind::Rest(_) => out.push('z'),
            GraceEventKind::Chord(members) => {
                out.push('[');
                for note in members {
                    out.push_str(
                        note.written_accidental
                            .as_ref()
                            .map(|m| accidental_glyph(m.kind))
                            .unwrap_or(""),
                    );
                    out.push_str(&pitch_str(&note.pitch));
                    out.push_str(&length_ratio_str(note.length_multiplier));
                }
                out.push(']');
            }
        }
    }
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
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
            assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
        }
    }

    fn grace_pitches(score: &crate::Score) -> Vec<(char, i8, i8, bool)> {
        let mut v = Vec::new();
        for p in &score.parts {
            for voice in &p.voices {
                for e in &voice.events {
                    for g in &e.attachments.grace_groups {
                        let slash = g.slash.is_some();
                        for ge in &g.events {
                            if let crate::model::GraceEventKind::Note(n) = &ge.kind {
                                v.push((n.pitch.step, n.pitch.alter, n.pitch.octave, slash));
                            }
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
}
