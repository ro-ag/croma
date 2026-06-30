//! Canonical `Score` -> ABC writer (the reverse of the MusicXML writer).
//!
//! Emits ABC that is a `croma fmt` fixed point and round-trips through
//! `parse_document` + `lower_score` with an identical structural projection.
use crate::model::{
    AlignedLyric, ClefChangeModel, EventAttachments, Fraction, HarmonyKindText, KeySignatureModel,
    Measure, MeterModel, SlurRole, TempoBeatRole, TempoModel, TieRole, TupletAttachment,
    TupletRole,
};
use crate::{Accidental, BarlineKind, Pitch, Rational, RestVisibility, Score, TimedEventKind};

#[derive(Debug, Clone, Copy, Default)]
pub struct AbcWriteOptions {}

/// Emit canonical ABC for `score`. Output is a `croma fmt` fixed point.
pub fn write_abc(score: &Score, _options: AbcWriteOptions) -> String {
    let mut out = String::new();
    let meta = &score.metadata;
    out.push_str(&format!("X:{}\n", meta.reference.text.trim()));
    // `%%score` staff-grouping directives; the last voice-bearing one wins on
    // re-parse, so re-emitting all in order is exact.
    for directive in &meta.directives {
        out.push_str(&format!("%%score {}\n", directive.value.text.trim()));
    }
    if let Some(title) = &meta.title {
        out.push_str(&format!("T:{}\n", title.text.trim()));
    }
    // `C:` composer field, after `T:` in canonical ABC order. The reader captures
    // `<creator type="composer">` into `composers`, and the ABC parser reads `C:`
    // back into `composers`, so emitting it makes structured composer metadata
    // survive a MusicXML -> ABC -> MusicXML round trip.
    for composer in &meta.composers {
        out.push_str(&format!("C:{}\n", composer.text.trim()));
    }
    // `M:` is optional in ABC; a tune without one must not gain a synthetic
    // meter.
    if let Some(meter) = &meta.meter {
        if let Some(instruction) = time_symbol_instruction(meter) {
            out.push_str(&format!("I:{instruction}\n"));
        }
        out.push_str(&format!("M:{}\n", meter.display));
    }
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
    // Post-tune lyrics (`W:`) round-trip to identical <credit> entries.
    for line in &meta.post_tune_lyrics {
        out.push_str(&format!("W:{}\n", line.text));
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Canonical display text for a mid-tune `Q:` tempo: optional quoted text,
/// then `n/d=bpm` when a numeric beat is present. Re-parses to the same
/// `TempoModel`, keeping the round-trip stable.
fn tempo_display(tempo: &crate::model::TempoModel) -> String {
    let mut out = String::new();
    if let Some(text) = &tempo.text {
        out.push('"');
        out.push_str(&abc_quoted_text(text));
        out.push('"');
    }
    if let Some(beat) = &tempo.beat {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&format!(
            "{}/{}={}",
            beat.beat_numerator, beat.beat_denominator, beat.bpm
        ));
    }
    out
}

fn sound_tempo_instruction(tempo: &TempoModel) -> Option<String> {
    let beat = tempo.beat?;
    let mut out = format!(
        "croma-sound-tempo bpm={} beat-n={} beat-d={}",
        beat.bpm, beat.beat_numerator, beat.beat_denominator
    );
    if let Some(text) = &tempo.text {
        if needs_hex_inline_carrier(text) {
            out.push_str(&format!(" text-hex={}", hex_utf8(text)));
        } else {
            out.push_str(&format!(" text=\"{}\"", abc_carrier_quoted(text)));
        }
    }
    Some(out)
}

fn tempo_instruction(tempo: &TempoModel) -> Option<String> {
    if tempo.text.is_none() && tempo.beat.is_none() {
        return None;
    }
    let role = match tempo.beat_role {
        TempoBeatRole::PrintedMetronome => "printed",
        TempoBeatRole::PlaybackSoundOnly => "sound",
    };
    let mut out = format!("croma-tempo role={role}");
    if let Some(text) = &tempo.text {
        if needs_hex_inline_carrier(text) {
            out.push_str(&format!(" text-hex={}", hex_utf8(text)));
        } else {
            out.push_str(&format!(" text=\"{}\"", abc_carrier_quoted(text)));
        }
    }
    if let Some(beat) = tempo.beat {
        out.push_str(&format!(
            " bpm={} beat-n={} beat-d={}",
            beat.bpm, beat.beat_numerator, beat.beat_denominator
        ));
    }
    Some(out)
}

fn tempo_needs_instruction(tempo: &TempoModel) -> bool {
    tempo.text.as_deref().is_some_and(needs_hex_inline_carrier)
}

fn harmony_text_instruction(kind_text: &HarmonyKindText) -> Option<String> {
    match kind_text {
        // ABC-native chords carry no provenance; the writer rebuilds `text=` from
        // the chord string, so no carrier is emitted.
        HarmonyKindText::AbcNative => None,
        HarmonyKindText::Textless => Some("croma-harmony-text textless=1".to_owned()),
        HarmonyKindText::Text(value) => Some(format!(
            "croma-harmony-text text=\"{}\"",
            abc_carrier_quoted(value)
        )),
    }
}

fn lyric_extend_instruction(verse: u32) -> String {
    format!("croma-lyric-extend verse={verse}")
}

fn lyric_duplicate_instruction(lyric: &AlignedLyric) -> String {
    let mut out = format!("croma-lyric-duplicate verse={}", lyric.verse);
    if needs_hex_inline_carrier(&lyric.text) {
        out.push_str(&format!(" text-hex={}", hex_utf8(&lyric.text)));
    } else {
        out.push_str(&format!(" text=\"{}\"", abc_carrier_quoted(&lyric.text)));
    }
    if lyric.same_note_extend {
        out.push_str(" extend=1");
    }
    out
}

fn needs_hex_inline_carrier(text: &str) -> bool {
    text.chars().any(|c| c == ']' || c == '%' || c.is_control())
}

fn hex_utf8(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(text.len() * 2);
    for byte in text.as_bytes() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn meter_restatement_instruction() -> &'static str {
    "croma-meter-restatement"
}

fn key_restatement_instruction() -> &'static str {
    "croma-key-restatement"
}

fn time_symbol_instruction(meter: &MeterModel) -> Option<String> {
    let symbol = meter.time_symbol.as_deref()?;
    if !matches!(symbol, "common" | "cut") {
        return None;
    }
    if (symbol == "common" && meter.display.trim() == "C")
        || (symbol == "cut" && meter.display.trim() == "C|")
    {
        return None;
    }
    Some(format!("croma-time-symbol symbol={symbol}"))
}

fn musicxml_forward_instruction() -> &'static str {
    "croma-musicxml-forward"
}

fn musicxml_sequence_backup_instruction(duration: Fraction) -> String {
    format!(
        "croma-musicxml-sequence-backup n={} d={}",
        duration.numerator, duration.denominator
    )
}

fn abc_tuplet_actual_supported(actual_notes: u32) -> bool {
    (2..=9).contains(&actual_notes)
}

fn musicxml_tuplet_instruction(tuplet: TupletAttachment) -> String {
    let role = match tuplet.role {
        TupletRole::Start => "start",
        TupletRole::Continue => "continue",
        TupletRole::Stop => "stop",
    };
    format!(
        "croma-musicxml-tuplet id={} actual={} normal={} role={}",
        tuplet.pair_id, tuplet.actual_notes, tuplet.normal_notes, role
    )
}

fn musicxml_after_grace_instruction() -> &'static str {
    "croma-after-grace"
}

fn clef_cursor_instruction(clef: &ClefChangeModel) -> Option<String> {
    let cursor_back = clef.musicxml_cursor_back?;
    let mut out = String::from("croma-clef-cursor");
    if needs_hex_inline_carrier(&clef.clef.text) {
        out.push_str(&format!(" clef-hex={}", hex_utf8(&clef.clef.text)));
    } else {
        out.push_str(&format!(
            " clef=\"{}\"",
            abc_carrier_quoted(&clef.clef.text)
        ));
    }
    out.push_str(&format!(
        " back-n={} back-d={}",
        cursor_back.numerator, cursor_back.denominator
    ));
    if let Some(pre_backup) = clef.musicxml_cursor_pre_backup {
        out.push_str(&format!(
            " pre-back-n={} pre-back-d={}",
            pre_backup.numerator, pre_backup.denominator
        ));
    }
    Some(out)
}

fn barline_style_instruction(kind: BarlineKind) -> Option<&'static str> {
    match kind {
        BarlineKind::Dashed => Some("croma-barline-style style=dashed"),
        _ => None,
    }
}

fn xvoice_slur_instruction(pair: u32, role: SlurRole) -> String {
    let role = match role {
        SlurRole::Start => "start",
        SlurRole::Stop => "stop",
    };
    format!("croma-xvoice-slur pair={pair} role={role}")
}

/// One end of a cross-voice slur to project onto a specific event: which
/// `attachments.slurs` entry to suppress (its `(`/`)` cannot span two `V:`
/// streams) and the shared `pair=`/`role` carrier to emit in its place.
#[derive(Clone, Copy)]
struct XvoiceSlurEmit {
    slur_index: usize,
    pair: u32,
    role: SlurRole,
}

type XvoiceSlurMap = std::collections::HashMap<(usize, usize), Vec<XvoiceSlurEmit>>;

/// Locate slur ends that ABC `(`/`)` cannot express because they do not pair
/// within their own `V:` stream — a slur reaching into or out of another voice.
///
/// Each voice's own slurs pair by the same LIFO stack the lowering uses, so a
/// voice whose slurs balance (the overwhelming majority, and every pure-ABC
/// score) yields nothing and the normal `(`/`)` projection is left untouched.
/// What stays unmatched is a *dangling* end: a stop popped on an empty stack or
/// a start never closed. Those are the ends ABC drops today — projecting only
/// per-note slur types, the round-trip just needs each dangling end to keep its
/// type, so each is re-paired (a dangling stop with a dangling start sharing the
/// source `<slur number>`/`pair_id`) and carried. Whichever start pairs with
/// whichever stop is immaterial to the per-note projection; pairing by document
/// order simply gives the carried ends matching `<slur number>`s.
///
/// The result is keyed by `(global voice index, event index)`.
fn detect_cross_voice_slurs(score: &Score) -> XvoiceSlurMap {
    struct Dangler {
        voice: usize,
        event: usize,
        slur_index: usize,
        pair_id: u32,
        measure: u32,
    }
    let mut dangling_starts: Vec<Dangler> = Vec::new();
    let mut dangling_stops: Vec<Dangler> = Vec::new();
    for (voice, voice_model) in score
        .parts
        .iter()
        .flat_map(|part| part.voices.iter())
        .enumerate()
    {
        let mut open: Vec<Dangler> = Vec::new();
        for (event, timed) in voice_model.events.iter().enumerate() {
            for (slur_index, slur) in timed.attachments.slurs.iter().enumerate() {
                let dangler = Dangler {
                    voice,
                    event,
                    slur_index,
                    pair_id: slur.pair_id,
                    measure: timed.measure.index,
                };
                match slur.role {
                    SlurRole::Start => open.push(dangler),
                    SlurRole::Stop => {
                        if open.pop().is_none() {
                            dangling_stops.push(dangler);
                        }
                    }
                }
            }
        }
        dangling_starts.append(&mut open);
    }
    let mut map = XvoiceSlurMap::new();
    if dangling_starts.is_empty() || dangling_stops.is_empty() {
        return map;
    }
    // Pair the dangling ends. Both ends of one source slur carry the same
    // `<slur number>` (`pair_id`), so group by it and pair a dangling stop with a
    // dangling start in document order — `(measure, voice, event)`, the order
    // MusicXML lays a measure out (voice by voice, each behind a `<backup>`).
    let mut starts: std::collections::BTreeMap<u32, Vec<Dangler>> =
        std::collections::BTreeMap::new();
    for dangler in dangling_starts {
        starts.entry(dangler.pair_id).or_default().push(dangler);
    }
    let mut stops: std::collections::BTreeMap<u32, Vec<Dangler>> =
        std::collections::BTreeMap::new();
    for dangler in dangling_stops {
        stops.entry(dangler.pair_id).or_default().push(dangler);
    }
    let order = |d: &Dangler| (d.measure, d.voice, d.event);
    let mut next_pair = 1u32;
    for (pair_id, mut group_starts) in starts {
        let Some(mut group_stops) = stops.remove(&pair_id) else {
            continue;
        };
        group_starts.sort_by_key(&order);
        group_stops.sort_by_key(&order);
        for (start, stop) in group_starts.into_iter().zip(group_stops) {
            let pair = next_pair;
            next_pair += 1;
            map.entry((start.voice, start.event))
                .or_default()
                .push(XvoiceSlurEmit {
                    slur_index: start.slur_index,
                    pair,
                    role: SlurRole::Start,
                });
            map.entry((stop.voice, stop.event))
                .or_default()
                .push(XvoiceSlurEmit {
                    slur_index: stop.slur_index,
                    pair,
                    role: SlurRole::Stop,
                });
        }
    }
    map
}

/// The inline carrier(s) for an event's cross-voice slur ends, emitted in place
/// of the suppressed `(`/`)`.
fn xvoice_slur_prefix(emits: &[XvoiceSlurEmit]) -> String {
    let mut out = String::new();
    for emit in emits {
        out.push_str(&format!(
            "[I:{}]",
            xvoice_slur_instruction(emit.pair, emit.role)
        ));
    }
    out
}

/// A copy of `attachments` with the cross-voice slur ends removed, so the normal
/// slur projection no longer emits a `(`/`)` for them.
fn strip_xvoice_slurs(
    attachments: &EventAttachments,
    emits: &[XvoiceSlurEmit],
) -> EventAttachments {
    let mut stripped = attachments.clone();
    let mut indexes: Vec<usize> = emits.iter().map(|emit| emit.slur_index).collect();
    indexes.sort_unstable_by(|a, b| b.cmp(a));
    for index in indexes {
        if index < stripped.slurs.len() {
            stripped.slurs.remove(index);
        }
    }
    stripped
}

fn ending_close_instruction(model: &crate::model::RepeatEndingCloseModel) -> Option<String> {
    let close_type = match model.close_type {
        crate::model::RepeatEndingCloseType::Stop => "stop",
        crate::model::RepeatEndingCloseType::Discontinue => "discontinue",
    };
    let location = match model.location {
        crate::model::RepeatEndingCloseLocation::Left => "left",
        crate::model::RepeatEndingCloseLocation::Right => "right",
    };
    let number = ending_number_value(&model.endings)?;
    Some(format!(
        "croma-ending-close type={close_type} location={location} number=\"{number}\""
    ))
}

fn ending_number_value(parts: &[crate::model::RepeatEndingPartModel]) -> Option<String> {
    use crate::model::RepeatEndingPartModel::{Range, Single, Text};
    let values = parts
        .iter()
        .map(|part| match part {
            Single(value) => Some(value.to_string()),
            Range { start, end } => Some(format!("{start}-{end}")),
            Text(_) => None,
        })
        .collect::<Option<Vec<_>>>()?;
    (!values.is_empty()).then(|| values.join(","))
}

fn initial_key_instruction(key: &KeySignatureModel) -> String {
    let mut out = format!("croma-initial-key fifths={}", key.fifths);
    if !key.explicit_accidentals.is_empty() {
        let accidentals = key
            .explicit_accidentals
            .iter()
            .map(|accidental| format!("{}:{}", accidental.step, accidental.accidental.alter()))
            .collect::<Vec<_>>()
            .join(",");
        out.push_str(&format!(" accidentals={accidentals}"));
    }
    out
}

fn initial_key_carrier(score: &Score, key: &KeySignatureModel) -> Option<String> {
    if score
        .metadata
        .key
        .as_ref()
        .is_some_and(|header| key.structurally_matches(header))
    {
        return None;
    }
    Some(initial_key_instruction(key))
}

fn initial_meter_instruction(meter: &MeterModel) -> String {
    let mut out = format!(
        "croma-initial-meter display=\"{}\"",
        abc_carrier_quoted(&meter.display)
    );
    if let Some(symbol) = meter.time_symbol.as_deref()
        && matches!(symbol, "common" | "cut")
    {
        out.push_str(&format!(" symbol={symbol}"));
    }
    out
}

fn initial_meter_carrier(score: &Score, meter: &MeterModel) -> Option<String> {
    if score
        .metadata
        .meter
        .as_ref()
        .is_some_and(|header| meter.structurally_matches(header))
    {
        return None;
    }
    Some(initial_meter_instruction(meter))
}

fn measure_number_instruction(display_number: &str) -> String {
    let mut out = "croma-measure-number".to_owned();
    if needs_hex_inline_carrier(display_number) {
        out.push_str(&format!(" n-hex={}", hex_utf8(display_number)));
    } else if display_number.chars().any(char::is_whitespace) {
        out.push_str(&format!(" n=\"{}\"", abc_carrier_quoted(display_number)));
    } else {
        out.push_str(&format!(" n={display_number}"));
    }
    out
}

fn measure_number_carrier(measure: &Measure) -> Option<String> {
    let display_number = measure.display_number.as_deref()?.trim();
    if display_number.is_empty() {
        return None;
    }
    let canonical_number = measure.id.index.saturating_add(1).to_string();
    (display_number != canonical_number).then(|| measure_number_instruction(display_number))
}

fn barline_starts_measure(kind: BarlineKind) -> bool {
    matches!(
        kind,
        BarlineKind::Regular | BarlineKind::Initial | BarlineKind::RepeatStart
    )
}

fn carrier_follows_leading_barline(kind: &TimedEventKind, joined: Option<&str>) -> bool {
    matches!(kind, TimedEventKind::Barline(barline) if joined.is_some() || barline_starts_measure(barline.kind))
}

fn abc_quoted_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
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
    let tempo = score.metadata.tempo_model.as_ref()?;
    if tempo.beat_role == TempoBeatRole::PlaybackSoundOnly || tempo_needs_instruction(tempo) {
        return None;
    }
    let display = tempo_display(tempo);
    (!display.is_empty()).then_some(display)
}

fn initial_tempo_instruction(score: &Score) -> Option<String> {
    let tempo = score.metadata.tempo_model.as_ref()?;
    if tempo.beat_role == TempoBeatRole::PlaybackSoundOnly || !tempo_needs_instruction(tempo) {
        return None;
    }
    tempo_instruction(tempo)
}

fn write_body(score: &Score, unit: Rational) -> String {
    // Single default voice: no `V:` header line (the dominant corpus shape).
    let single = score.parts.len() == 1
        && score.parts[0].voices.len() == 1
        && score.parts[0].voices[0].id.value == "1"
        && score.parts[0].voices[0].properties == crate::model::VoicePropertiesModel::default();
    let xvoice = detect_cross_voice_slurs(score);
    let mut body = String::new();
    let mut voice_global_index = 0usize;
    for (part_index, part) in score.parts.iter().enumerate() {
        for (voice_index, voice) in part.voices.iter().enumerate() {
            if !single {
                body.push_str(&voice_header_line(voice));
            }
            if voice_index == 0 {
                body.push_str(&musicxml_instrument_directive_lines(part));
            }
            body.push_str(&midi_directive_lines(voice));
            if part_index == 0
                && voice_index == 0
                && let Some(instruction) = initial_tempo_instruction(score)
            {
                body.push_str(&format!("[I:{instruction}] "));
            }
            if let Some(key) = &voice.initial_key
                && let Some(instruction) = initial_key_carrier(score, key)
            {
                body.push_str(&format!("[I:{instruction}] "));
            }
            if let Some(meter) = &voice.initial_meter
                && let Some(instruction) = initial_meter_carrier(score, meter)
            {
                body.push_str(&format!("[I:{instruction}] "));
            }
            body.push_str(&write_voice(voice, unit, voice_global_index, &xvoice));
            voice_global_index += 1;
        }
    }
    body
}

/// The `V:` header line for a voice: id plus its retained properties in a
/// fixed canonical order, each echoing the model field under its parser key.
fn voice_header_line(voice: &crate::model::Voice) -> String {
    use crate::model::StemDirectionModel;
    let p = &voice.properties;
    let mut s = format!("V:{}", voice.id.value);
    for (key, value) in [
        ("name", &p.name),
        ("nm", &p.nm),
        ("subname", &p.subname),
        ("snm", &p.snm),
    ] {
        if let Some(text) = value {
            s.push_str(&format!(" {key}=\"{}\"", abc_carrier_quoted(&text.text)));
        }
    }
    if let Some(clef) = &p.clef {
        s.push_str(&format!(" clef={}", clef.text));
    }
    if let Some(stem) = &p.stem {
        s.push_str(match stem {
            StemDirectionModel::Up => " stem=up",
            StemDirectionModel::Down => " stem=down",
        });
    }
    if let Some(octave) = &p.octave {
        s.push_str(&format!(" octave={}", octave.text));
    }
    if let Some(transpose) = &p.transpose {
        s.push_str(&format!(" transpose={}", transpose.text));
    }
    if let Some(middle) = &p.middle {
        s.push_str(&format!(" middle={}", middle.text));
    }
    s.push('\n');
    s
}

/// Re-emit a voice's score-meaningful `%%MIDI` directives as line-start tune
/// directives, inverting the forward translation in `lower`. Placed directly
/// after the voice's `V:` switch so the re-parse scopes them to this voice.
/// Each line is the canonical spelling the forward parser reads back into the
/// same [`MidiInstrumentModel`]:
/// - `program <channel> <prog>` when both are set, else `program <prog>`,
///   else a standalone `channel <n>`;
/// - `control 7 <vol>` (CC7 volume) / `control 10 <pan>` (CC10 pan);
/// - `midi-unpitched <n>` for MusicXML-origin unpitched percussion maps;
/// - `transpose <n>` for `%%MIDI transpose`.
///
/// Program/channel values are written in the same conventions the parser reads
/// (0-based GM program, 1-16 channel), so the round-trip is value-for-value.
fn midi_directive_lines(voice: &crate::model::Voice) -> String {
    let mut s = String::new();
    if let Some(midi) = &voice.midi_instrument {
        match (midi.channel, midi.program) {
            (Some(channel), Some(program)) => {
                s.push_str(&format!("%%MIDI program {channel} {program}\n"));
            }
            (None, Some(program)) => s.push_str(&format!("%%MIDI program {program}\n")),
            (Some(channel), None) => s.push_str(&format!("%%MIDI channel {channel}\n")),
            (None, None) => {}
        }
        if let Some(volume) = midi.volume_cc {
            s.push_str(&format!("%%MIDI control 7 {volume}\n"));
        }
        if let Some(pan) = midi.pan_cc {
            s.push_str(&format!("%%MIDI control 10 {pan}\n"));
        }
        if let Some(unpitched) = midi.midi_unpitched {
            s.push_str(&format!("%%MIDI midi-unpitched {unpitched}\n"));
        }
    }
    if let Some(transpose) = voice.midi_transpose {
        s.push_str(&format!("%%MIDI transpose {transpose}\n"));
    }
    s
}

fn musicxml_instrument_directive_lines(part: &crate::model::Part) -> String {
    let mut s = String::new();
    for instrument in &part.instruments {
        if instrument.id.trim().is_empty() {
            continue;
        }
        s.push_str("%%croma-musicxml-instrument");
        s.push_str(&format!(
            " id=\"{}\"",
            abc_carrier_quoted(instrument.id.as_str())
        ));
        if let Some(name) = &instrument.name {
            s.push_str(&format!(
                " name=\"{}\"",
                abc_carrier_quoted(name.text.as_str())
            ));
        }
        if let Some(midi) = &instrument.midi {
            if let Some(channel) = midi.channel {
                s.push_str(&format!(" channel={channel}"));
            }
            if let Some(program) = midi.program {
                s.push_str(&format!(" program={program}"));
            }
            if let Some(volume) = midi.volume_cc {
                s.push_str(&format!(" volume-cc={volume}"));
            }
            if let Some(pan) = midi.pan_cc {
                s.push_str(&format!(" pan-cc={pan}"));
            }
            if let Some(unpitched) = midi.midi_unpitched {
                s.push_str(&format!(" midi-unpitched={unpitched}"));
            }
        }
        s.push('\n');
    }
    s
}

/// Quote a string for a `name="..."`-style ABC carrier value: neutralise any
/// control character (a foreign `<part-name>`/instrument name may carry a display
/// line-break, e.g. "S\nA", which would split the single-line `V:`/`%%` header and
/// corrupt every following note) to a space, then escape `\` and `"`. ABC header
/// names are single-line, so a multi-line name's break is inexpressible and
/// normalised to a space rather than preserved.
fn abc_carrier_quoted(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

/// Replicates the parser's written->stored octave shift for a voice's
/// `clef=` (`±8`/`±15`), `octave=` and `middle=` modifiers. Stored pitches
/// already carry the shift, so the writer SUBTRACTS it to recover the written
/// octave (the re-parse re-applies the echoed modifiers).
///
/// MUST stay value-for-value identical to `lower::voice::voice_octave_shift`
/// (same clamps: `octave=` to ±9, total to ±12) or every `octave=`/`clef±`
/// voice breaks round-trip.
fn voice_octave_shift(properties: &crate::model::VoicePropertiesModel) -> i8 {
    let mut shift: i32 = 0;
    if let Some(clef) = properties.clef.as_ref() {
        let clef = clef.text.as_str();
        if clef.contains("-15") {
            shift -= 2;
        } else if clef.contains("+15") {
            shift += 2;
        } else if clef.contains("-8") {
            shift -= 1;
        } else if clef.contains("+8") {
            shift += 1;
        }
    }
    if let Some(octave) = properties.octave.as_ref()
        && let Ok(value) = octave.text.trim().parse::<i64>()
    {
        shift += value.clamp(-9, 9) as i32;
    }
    if let Some(middle) = properties.middle.as_ref() {
        shift += i32::from(crate::lower::voice::middle_octave_shift(
            middle.text.as_str(),
        ));
    }
    shift.clamp(-12, 12) as i8
}

/// A pitch moved back to its written octave for emission. Saturating, like
/// the lowering-side addition, so a boundary-saturated stored octave cannot
/// overflow back out of i8.
fn shifted(pitch: &Pitch, shift: i8) -> Pitch {
    Pitch {
        octave: pitch.octave.saturating_sub(shift),
        ..*pitch
    }
}

fn write_voice(
    voice: &crate::model::Voice,
    unit: Rational,
    voice_global_index: usize,
    xvoice: &XvoiceSlurMap,
) -> String {
    let mut out = String::new();
    let shift = voice_octave_shift(&voice.properties);
    // Overlay segments (`&`) grouped by the measure they belong to; spliced
    // before that measure's closing barline, in segment order.
    let mut overlays: std::collections::BTreeMap<u32, Vec<&crate::model::OverlaySegment>> =
        std::collections::BTreeMap::new();
    for measure in &voice.measures {
        for segment in &measure.overlays {
            overlays
                .entry(segment.measure_index)
                .or_default()
                .push(segment);
        }
    }
    let (markers, scales) = tuplet_layout(&voice.events);
    let measure_number_carriers = voice
        .measures
        .iter()
        .filter_map(|measure| {
            measure_number_carrier(measure).map(|carrier| (measure.id.index, carrier))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    // Adjacent barline pairs that lowering splits out of ONE source token must
    // be re-joined on emission: `||:` -> [Double, RepeatStart] and `[|:` ->
    // [Initial, RepeatStart]. Both halves of a split token share the SAME
    // source span — that distinguishes them from a real two-token `|| |:` pair
    // (different spans), which must stay split or its phantom empty measure
    // would collapse. Emitting a split pair as two spaced tokens creates a
    // phantom empty measure when the pair leads the tune.
    let mut joined: Vec<Option<&'static str>> = vec![None; voice.events.len()];
    let mut skip = vec![false; voice.events.len()];
    for i in 0..voice.events.len().saturating_sub(1) {
        if skip[i] {
            continue;
        }
        let (TimedEventKind::Barline(a), TimedEventKind::Barline(b)) =
            (&voice.events[i].kind, &voice.events[i + 1].kind)
        else {
            continue;
        };
        if a.span != b.span {
            continue;
        }
        match (a.kind, b.kind) {
            (BarlineKind::Double, BarlineKind::RepeatStart) => {
                joined[i] = Some("||:");
                skip[i + 1] = true;
            }
            (BarlineKind::Initial, BarlineKind::RepeatStart) => {
                joined[i] = Some("[|:");
                skip[i + 1] = true;
            }
            _ => {}
        }
    }
    let mut current_measure: Option<u32> = None;
    let mut measure_event_seen = false;
    for (idx, event) in voice.events.iter().enumerate() {
        if skip[idx] {
            continue;
        }
        let mut measure_carrier_after_barline = None;
        // Measure transition without a closing barline: flush that measure's
        // overlays before anything from the new measure is emitted.
        let m = event.measure.index;
        if current_measure != Some(m) {
            if let Some(prev) = current_measure
                && let Some(segments) = overlays.remove(&prev)
            {
                for segment in segments {
                    out.push_str(&overlay_str(segment, unit, shift));
                }
            }
            let missing_start = current_measure.map_or(0, |prev| prev.saturating_add(1));
            write_sparse_voice_measure_gaps(&mut out, missing_start, m);
            current_measure = Some(m);
            measure_event_seen = false;
            if let Some(carrier) = measure_number_carriers.get(&m) {
                if carrier_follows_leading_barline(&event.kind, joined[idx]) {
                    measure_carrier_after_barline = Some(carrier.as_str());
                } else {
                    out.push_str(&format!("[I:{carrier}] "));
                }
            }
        }
        // A barline that is not the very first event of its measure closes it
        // (even a barline-only measure: `| & ... |`); flush overlays before it.
        if measure_event_seen
            && matches!(event.kind, TimedEventKind::Barline(_))
            && let Some(segments) = overlays.remove(&m)
        {
            for segment in segments {
                out.push_str(&overlay_str(segment, unit, shift));
            }
        }
        measure_event_seen = true;
        // Tuplet markers `(p:q:r` open before the first note/rest/chord of the
        // group. Nested groups can contribute more than one marker here.
        out.push_str(&markers[idx]);
        let tuplet = scales[idx];
        // Cross-voice slur ends (rare): emit each as an inline `[I:croma-xvoice-
        // slur ...]` carrier and render the event WITHOUT the `(`/`)` that cannot
        // span two `V:` streams. For every other event this is a no-op borrow.
        let xvoice_emits: &[XvoiceSlurEmit] = xvoice
            .get(&(voice_global_index, idx))
            .map_or(&[], Vec::as_slice);
        out.push_str(&xvoice_slur_prefix(xvoice_emits));
        let stripped_attachments;
        let attachments: &EventAttachments = if xvoice_emits.is_empty() {
            &event.attachments
        } else {
            stripped_attachments = strip_xvoice_slurs(&event.attachments, xvoice_emits);
            &stripped_attachments
        };
        match &event.kind {
            TimedEventKind::Note(note) => {
                let written = shifted(&note.pitch, shift);
                out.push_str(&event_prefix(attachments));
                out.push_str(note_accidental(
                    note.written_accidental.as_ref().map(|m| m.kind),
                ));
                out.push_str(&pitch_str(&written));
                out.push_str(&length_str(notated_duration(event.duration, tuplet), unit));
                out.push_str(&event_suffix(attachments));
                out.push(' ');
            }
            TimedEventKind::Rest(rest) => {
                out.push_str(&event_prefix(attachments));
                out.push(match rest.visibility {
                    RestVisibility::Visible => 'z',
                    RestVisibility::Invisible => 'x',
                });
                out.push_str(&length_str(notated_duration(event.duration, tuplet), unit));
                out.push_str(&event_suffix(attachments));
                out.push(' ');
            }
            TimedEventKind::Chord(chord) => {
                // Slurs can be recorded on the chord event, on individual
                // members, or (redundantly) on both. Merge them, deduping by
                // (pair_id, role), so `(`/`)` are emitted exactly once per
                // distinct slur. Ties are NOT merged: a tie binds a specific
                // member (`[dg-]` ties only g), so each is emitted inline after
                // its member. A whole-chord tie (`[CE]2-`) also records ties on
                // every member, so inline emission reproduces its sound exactly.
                let mut merged = attachments.clone();
                merged.ties.clear();
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
                for (member_index, (member, len)) in chord.members.iter().zip(&lengths).enumerate()
                {
                    let written = shifted(&member.pitch, shift);
                    out.push_str(&chord_member_prefix(
                        &member.attachments,
                        (member_index == 0).then_some(&merged),
                    ));
                    out.push_str(note_accidental(
                        member.written_accidental.as_ref().map(|m| m.kind),
                    ));
                    out.push_str(&pitch_str(&written));
                    if !uniform {
                        out.push_str(len);
                    }
                    if member
                        .attachments
                        .ties
                        .iter()
                        .any(|t| t.role == TieRole::Start)
                    {
                        out.push('-');
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
                if let Some(instruction) = barline_style_instruction(b.kind) {
                    out.push_str(&format!("[I:{instruction}] "));
                }
                out.push_str(joined[idx].unwrap_or_else(|| barline_str(b.kind)));
                out.push(' ');
                if let Some(carrier) = measure_carrier_after_barline {
                    out.push_str(&format!("[I:{carrier}] "));
                }
            }
            TimedEventKind::RepeatEnding(r) => {
                out.push_str(&ending_str(r));
                out.push(' ');
            }
            TimedEventKind::RepeatEndingClose(close) => {
                if let Some(instruction) = ending_close_instruction(close) {
                    out.push_str(&format!("[I:{instruction}] "));
                }
            }
            TimedEventKind::Spacer => {
                out.push_str("y ");
            }
            // Mid-tune changes re-emit inline; `display` is the verbatim
            // source text (modes, C/C|, exp-accidental lists, clef tokens).
            // The parser re-applies them at this position, reproducing the
            // baked-in alters / meter state downstream.
            TimedEventKind::KeyChange(key) => {
                if key.preserve_restatement {
                    out.push_str(&format!("[I:{}] ", key_restatement_instruction()));
                }
                out.push_str(&format!("[K:{}] ", key.display));
            }
            TimedEventKind::MeterChange(meter) => {
                if let Some(instruction) = time_symbol_instruction(meter) {
                    out.push_str(&format!("[I:{instruction}] "));
                }
                if meter.preserve_restatement {
                    out.push_str(&format!("[I:{}] ", meter_restatement_instruction()));
                }
                out.push_str(&format!("[M:{}] ", meter.display));
            }
            TimedEventKind::ClefChange(clef) => {
                if let Some(instruction) = clef_cursor_instruction(clef) {
                    out.push_str(&format!("[I:{instruction}] "));
                }
            }
            TimedEventKind::TempoChange(tempo) => {
                if tempo.beat_role == TempoBeatRole::PlaybackSoundOnly
                    && let Some(instruction) = sound_tempo_instruction(tempo)
                {
                    out.push_str(&format!("[I:{instruction}] "));
                } else if tempo_needs_instruction(tempo)
                    && let Some(instruction) = tempo_instruction(tempo)
                {
                    out.push_str(&format!("[I:{instruction}] "));
                } else {
                    out.push_str(&format!("[Q:{}] ", tempo_display(tempo)));
                }
            }
            TimedEventKind::SectionLabel(label) => {
                // A foreign <rehearsal> may carry control characters (e.g. a
                // trailing newline "pre chorus\n"). Both `[P:...]` and whole-line
                // `P:...` fields are single-line, so a raw newline would split the
                // line and corrupt the following music. Section labels are
                // single-line in ABC, so normalise any control char to a space and
                // trim — the break is inexpressible, not preserved.
                let label = label
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect::<String>();
                let label = label.trim();
                // If the label contains `[` or `]` it cannot be safely emitted
                // as an inline field `[P:{label}]` — the parser reads up to the
                // first `]`, truncating the label.  Emit as a whole-line body
                // field instead: trim any trailing space from the preceding
                // music content, start the body field on a new line (or at the
                // beginning if there is no preceding content), then resume on
                // yet another new line for the music that follows.
                // Whole-line body fields allow `]` in the value (ABC 2.1 §4.3).
                if label.contains('[') || label.contains(']') {
                    let trimmed = out.trim_end_matches(' ').to_owned();
                    out.clear();
                    if !trimmed.is_empty() {
                        out.push_str(&trimmed);
                        out.push('\n');
                    }
                    out.push_str(&format!("P:{label}\n"));
                } else {
                    out.push_str(&format!("[P:{label}] "));
                }
            }
        }
    }
    // Flush overlays of the final measure (and any not reached via barlines).
    for (_index, segments) in std::mem::take(&mut overlays) {
        for segment in segments {
            out.push_str(&overlay_str(segment, unit, shift));
        }
    }
    let mut body = format!("{}\n", out.trim_end());
    // Verse lines first, then symbol lines: each group must be internally
    // adjacent (adjacency drives verse/layer numbering on re-parse), and both
    // align over the same single block of notes emitted above.
    let align_events = lyric_align_events(voice);
    for line in lyric_lines(&align_events) {
        body.push_str(&line);
        body.push('\n');
    }
    for line in symbol_lines(&align_events) {
        body.push_str(&line);
        body.push('\n');
    }
    body
}

/// Preserve sparse MusicXML-origin voices that first appear after measure 1 (or
/// skip interior measures). ABC voice lines start at their first token, so a
/// missing measure index must be held with an inert spacer measure; `y` anchors
/// the measure on re-parse but emits no MusicXML note/rest.
fn write_sparse_voice_measure_gaps(out: &mut String, start: u32, end: u32) {
    for _ in start..end {
        out.push_str("y | ");
    }
}

/// Per-event tuplet open markers (`"(p:q:r"` at each group's first event).
type TupletMarkers = Vec<String>;
/// Per-event product tuplet (actual, normal) ratio used to scale a notated length.
type TupletScales = Vec<Option<TupletScale>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TupletScale {
    Ratio(u32, u32),
    Overflow,
}

/// Tuplet layout: for each event index, the open markers (`"(p:q:r"` at the
/// groups' first event) and the ratio product that scales that event's notated
/// length.
///
/// Groups are keyed by `pair_id`; the span `r` runs from the first to the last
/// attached event inclusive. Rests carry tuplet attachments like notes do, so a
/// rest-led tuplet's first index is its `Start` rest. A pair seen here always
/// includes its `Start`: tuplets opened inside an overlay are discarded at the
/// barline (`finish_open_tuplets_at_boundary`) and never reach voice events.
fn tuplet_layout(events: &[crate::TimedEvent]) -> (TupletMarkers, TupletScales) {
    use std::collections::BTreeMap;
    // pair_id -> (actual, normal, start_index, stop_index)
    let mut groups: BTreeMap<u32, (u32, u32, usize, usize)> = BTreeMap::new();
    for (i, event) in events.iter().enumerate() {
        for tuplet in &event.attachments.tuplets {
            let entry = groups.entry(tuplet.pair_id).or_insert((
                tuplet.actual_notes,
                tuplet.normal_notes,
                i,
                i,
            ));
            entry.0 = tuplet.actual_notes;
            entry.1 = tuplet.normal_notes;
            entry.2 = entry.2.min(i);
            entry.3 = entry.3.max(i);
        }
    }
    let mut markers = vec![String::new(); events.len()];
    let mut scales = vec![None; events.len()];
    for (_pid, (actual, normal, start, stop)) in groups {
        if !abc_tuplet_actual_supported(actual) {
            continue;
        }
        // `r` counts the notes/rests/chords the group covers; an interior
        // spacer consumes no tuplet slot on re-parse and must not inflate it.
        let span = events[start..=stop]
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    TimedEventKind::Note(_) | TimedEventKind::Rest(_) | TimedEventKind::Chord(_)
                )
            })
            .count();
        markers[start].push_str(&format!("({actual}:{normal}:{span}"));
        for slot in scales.iter_mut().take(stop + 1).skip(start) {
            multiply_tuplet_scale(slot, actual, normal);
        }
    }
    (markers, scales)
}

fn multiply_tuplet_scale(slot: &mut Option<TupletScale>, actual: u32, normal: u32) {
    match slot {
        Some(TupletScale::Ratio(active_actual, active_normal)) => {
            *slot = checked_ratio_product(*active_actual, *active_normal, actual, normal)
                .map(|(actual, normal)| TupletScale::Ratio(actual, normal))
                .or(Some(TupletScale::Overflow));
        }
        Some(TupletScale::Overflow) => {}
        None => *slot = Some(TupletScale::Ratio(actual, normal)),
    }
}

fn checked_ratio_product(
    actual: u32,
    normal: u32,
    factor_actual: u32,
    factor_normal: u32,
) -> Option<(u32, u32)> {
    let actual = u64::from(actual) * u64::from(factor_actual);
    let normal = u64::from(normal) * u64::from(factor_normal);
    ratio_to_u32(actual, normal)
}

fn ratio_to_u32(numerator: u64, denominator: u64) -> Option<(u32, u32)> {
    if numerator <= u64::from(u32::MAX) && denominator <= u64::from(u32::MAX) {
        return Some((numerator as u32, denominator as u32));
    }
    let gcd = gcd_u64(numerator, denominator);
    let numerator = numerator / gcd;
    let denominator = denominator / gcd;
    (numerator <= u64::from(u32::MAX) && denominator <= u64::from(u32::MAX))
        .then_some((numerator as u32, denominator as u32))
}

fn gcd_u64(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}

/// Attachments emitted BEFORE a note/rest head.
///
/// Order matters for binding: a grace group, slur `(`, or decoration that
/// precedes a grace group binds to the *grace* note, not the main note (ABC 2.1
/// §4.11/§4.20). So grace comes first, then the event's own slur-opens, quoted
/// text, and decorations, which therefore bind to the main note head:
/// `{gf}("Gm"!trill!note`.
fn event_prefix(attachments: &crate::EventAttachments) -> String {
    use crate::model::SlurRole;
    let mut out = String::new();
    // Grace groups FIRST: the canonical order is `({gf}("F"!deco!note`. The
    // parser now also accepts quoted text before a grace group (`"F"{AB}c`
    // binds "F" to `c`), but this emission order stays canonical.
    for grace in &attachments.grace_groups {
        // A slur recorded on the grace group binds to its first grace note, so
        // its `(` opens before the group (`({gf}` ...).
        for slur in &grace.slurs {
            if slur.role == SlurRole::Start {
                out.push('(');
            }
        }
        // Grace pitches are stored UNSHIFTED by lowering (the voice octave
        // shift applies only to main notes and chord members), so they emit
        // as stored — shifting them here would drift one octave per
        // round-trip in `clef=±8`/`octave=`/`middle=` voices.
        out.push_str(&grace_str(grace));
    }
    if let Some(instrument) = &attachments.instrument {
        out.push_str(&format!(
            "[I:croma-note-instrument id=\"{}\"]",
            abc_carrier_quoted(instrument.id.as_str())
        ));
    }
    if attachments.musicxml_forward {
        out.push_str(&format!("[I:{}]", musicxml_forward_instruction()));
    }
    if let Some(duration) = attachments.musicxml_sequence_backup {
        out.push_str(&format!(
            "[I:{}]",
            musicxml_sequence_backup_instruction(duration)
        ));
    }
    let mut musicxml_tuplets: Vec<_> = attachments
        .tuplets
        .iter()
        .copied()
        .filter(|tuplet| !abc_tuplet_actual_supported(tuplet.actual_notes))
        .collect();
    musicxml_tuplets.sort_by_key(|tuplet| {
        let role_order = match tuplet.role {
            TupletRole::Start => 0u8,
            TupletRole::Continue => 1,
            TupletRole::Stop => 2,
        };
        (tuplet.pair_id, role_order)
    });
    for tuplet in musicxml_tuplets {
        out.push_str(&format!("[I:{}]", musicxml_tuplet_instruction(tuplet)));
    }
    let mut primary_lyric_verses = Vec::new();
    for lyric in &attachments.lyrics {
        if lyric.control != crate::model::LyricControl::Syllable {
            continue;
        }
        if primary_lyric_verses.contains(&lyric.verse) {
            out.push_str(&format!("[I:{}]", lyric_duplicate_instruction(lyric)));
        } else {
            primary_lyric_verses.push(lyric.verse);
            if lyric.same_note_extend {
                out.push_str(&format!("[I:{}]", lyric_extend_instruction(lyric.verse)));
            }
        }
    }
    // Event slur-opens next, then quoted strings — both `"G7"(DE)` and
    // `("G7"DE)` now parse with the chord symbol bound to `D`; the slur-first
    // order stays canonical.
    for slur in &attachments.slurs {
        if slur.role == SlurRole::Start {
            out.push('(');
        }
    }
    // Quoted strings: chord symbols (`"Gm"`) and annotations (`"^text"`). The
    // annotation `text` already carries its placement char, and chord-symbol
    // text never starts with one, so the parser re-distinguishes them; both
    // simply re-emit as `"<text>"`.
    for chord_symbol in &attachments.chord_symbols {
        if let Some(instruction) = harmony_text_instruction(&chord_symbol.musicxml_harmony_text) {
            out.push_str(&format!("[I:{instruction}]"));
        }
        out.push_str(&quoted_str(&chord_symbol.text));
    }
    // `s:`-aligned chord symbols re-emit INLINE: the exporter routes both
    // through the same <harmony> path (inline ones first, aligned ones after),
    // so inlining them here — after the event's own inline chord symbols —
    // reproduces the MusicXML byte-for-byte. The other aligned kinds
    // (Decoration/Annotation/Raw) render differently inline and are re-emitted
    // as `s:` lines instead (see `symbol_lines`).
    for symbol in &attachments.symbols {
        if symbol.kind == crate::model::AlignedSymbolKind::ChordSymbol {
            out.push_str(&quoted_str(&symbol.text));
        }
    }
    for annotation in &attachments.annotations {
        out.push_str(&quoted_str(&annotation.text));
    }
    for deco in &attachments.decorations {
        out.push_str(&decoration_str(&deco.name));
    }
    out
}

/// A quoted ABC string. The parser stores quoted text in source-escaped form
/// (an interior quote is kept as `\"`), so the text re-emits verbatim —
/// escaping again would corrupt it.
fn quoted_str(text: &str) -> String {
    format!("\"{text}\"")
}

/// Reconstructed `w:` lyric lines.
///
/// The writer emits the whole tune as ONE music line, so each verse is a single
/// `w:` line over one alignment block covering every lyric-bearing event
/// (single notes and chords; rests/spacers/barlines consume no position).
/// Verse numbers come from line ADJACENCY on re-parse, so verse `k` is line `k`
/// and gap verses are held with a placeholder all-skip line. Per event: a
/// Syllable token (plus `-` for each Hyphen attachment, which the parser stores
/// on the same note), `_` for an Extender, `*` for no lyric. Lines carrying
/// syllables are padded with `*` to the full block length so re-parsing is
/// warning-free (an under-filled syllable line warns).
#[derive(Debug, Clone, Copy)]
struct LyricAlignEvent<'a> {
    source_start: usize,
    attachments: &'a EventAttachments,
}

fn lyric_align_events(voice: &crate::model::Voice) -> Vec<LyricAlignEvent<'_>> {
    let mut alignable = Vec::new();
    for event in &voice.events {
        if matches!(
            event.kind,
            TimedEventKind::Note(_) | TimedEventKind::Chord(_)
        ) {
            alignable.push(LyricAlignEvent {
                source_start: event.source.start,
                attachments: &event.attachments,
            });
        }
    }
    for measure in &voice.measures {
        for segment in &measure.overlays {
            for event in &segment.events {
                if event.alignable {
                    alignable.push(LyricAlignEvent {
                        source_start: event.span.start,
                        attachments: &event.attachments,
                    });
                }
            }
        }
    }
    alignable.sort_by_key(|event| event.source_start);
    alignable
}

fn lyric_lines(events: &[LyricAlignEvent<'_>]) -> Vec<String> {
    use crate::model::LyricControl;
    let total = events.len();
    let mut verses: std::collections::BTreeMap<u32, Vec<Option<String>>> =
        std::collections::BTreeMap::new();
    let mut max_verse = 0u32;
    for (pos, event) in events.iter().enumerate() {
        // Chord lyrics are duplicated onto the first member; read the
        // event-level copy only.
        let mut syllable_verses = std::collections::BTreeSet::new();
        for lyric in &event.attachments.lyrics {
            max_verse = max_verse.max(lyric.verse);
            let slot = &mut verses
                .entry(lyric.verse)
                .or_insert_with(|| vec![None; total])[pos];
            match lyric.control {
                LyricControl::Syllable => {
                    if syllable_verses.insert(lyric.verse) {
                        slot.get_or_insert_with(String::new)
                            .push_str(&lyric_escape(&lyric.text));
                    }
                }
                LyricControl::Hyphen => {
                    // At most one written hyphen per slot: extra Hyphen
                    // attachments are XML-invisible, and a second `-` in the
                    // token would re-parse as a Skip and shift every later
                    // syllable one note right.
                    let token = slot.get_or_insert_with(String::new);
                    if !token.ends_with('-') {
                        token.push('-');
                    }
                }
                LyricControl::Extender => *slot = Some("_".to_string()),
                // Skip is never stored on a note (a `*` advances without
                // attaching); defensive no-op.
                LyricControl::Skip => {}
            }
        }
    }
    if verses.is_empty() {
        return Vec::new();
    }
    (1..=max_verse)
        .map(|v| match verses.get(&v) {
            Some(tokens) => {
                let body: Vec<String> = tokens
                    .iter()
                    .map(|t| match t {
                        // An orphan-Hyphen slot (hyphens with no syllable) is
                        // XML-invisible and unencodable; a bare `--` would even
                        // re-parse as TWO skips and shift every later syllable.
                        // One `*` keeps the position and drops the orphan.
                        Some(tok) if tok.chars().all(|c| c == '-') => "*".to_string(),
                        Some(tok) => tok.clone(),
                        None => "*".to_string(),
                    })
                    .collect();
                format!("w:{}", body.join(" "))
            }
            // A verse with no lyrics anywhere: hold its number with one skip.
            None => "w:*".to_string(),
        })
        .collect()
}

/// Escape a stored lyric syllable back to `w:` token text: a stored space was
/// written `~`, and the token metacharacters are backslash-escaped.
fn lyric_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            // A `+:` continuation line stores its join as a newline inside the
            // syllable; a raw newline would re-parse as music. Only ASCII
            // whitespace/control folds into the `~` space form — the parser's
            // token separators are space/tab only, so Unicode whitespace (e.g.
            // U+00A0 in mojibake lyrics) is opaque text and passes through.
            c if c.is_ascii_whitespace() || c.is_ascii_control() => out.push('~'),
            '\\' | '-' | '*' | '_' | '|' | '~' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Reconstructed `s:` symbol lines for the aligned symbols that cannot be
/// inlined (Decoration/Annotation/Raw — inline forms render different MusicXML,
/// e.g. `<notations>` instead of `<direction><words>`; ChordSymbol is inlined
/// in `event_prefix` instead).
///
/// The writer emits the whole tune as ONE music line, so there is a single
/// alignment block: every single note and chord consumes one `s:` position
/// (rests/spacers/barlines do not). One `s:` line is emitted per layer, all
/// adjacent (adjacency is what makes the parser number layers 1, 2, ...), with
/// `*` padding for empty positions and trailing `*` runs trimmed.
fn symbol_lines(events: &[LyricAlignEvent<'_>]) -> Vec<String> {
    use crate::model::AlignedSymbolKind;
    // (layer -> tokens per alignable position)
    let mut layers: std::collections::BTreeMap<u32, Vec<Option<String>>> =
        std::collections::BTreeMap::new();
    let mut position = 0usize;
    let total = events.len();
    for event in events {
        // Chord-aligned symbols are duplicated onto the first member; read the
        // event-level copy only.
        for symbol in &event.attachments.symbols {
            let token = match symbol.kind {
                AlignedSymbolKind::ChordSymbol => continue, // inlined
                AlignedSymbolKind::Decoration => format!("!{}!", symbol.text),
                AlignedSymbolKind::Annotation => quoted_str(&symbol.text),
                AlignedSymbolKind::Raw => symbol.text.clone(),
            };
            layers
                .entry(symbol.layer)
                .or_insert_with(|| vec![None; total])[position] = Some(token);
        }
        position += 1;
    }
    layers
        .into_values()
        .map(|tokens| {
            let last = tokens
                .iter()
                .rposition(Option::is_some)
                .map_or(0, |i| i + 1);
            let body: Vec<String> = tokens[..last]
                .iter()
                .map(|t| t.clone().unwrap_or_else(|| "*".to_string()))
                .collect();
            format!("s:{}", body.join(" "))
        })
        .filter(|line| line != "s:")
        .collect()
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

/// Decorations that belong to a specific chord member emit inside `[...]`.
/// The first member often duplicates the event-level attachment used by whole
/// chord decorations, so skip names already emitted by the chord prefix.
fn chord_member_prefix(
    attachments: &crate::EventAttachments,
    event_attachments: Option<&crate::EventAttachments>,
) -> String {
    let mut out = String::new();
    for decoration in &attachments.decorations {
        if event_attachments.is_some_and(|event_attachments| {
            event_attachments
                .decorations
                .iter()
                .any(|event_decoration| event_decoration.name == decoration.name)
        }) {
            continue;
        }
        out.push_str(&decoration_str(&decoration.name));
    }
    out
}

/// Attachments emitted AFTER a note/rest (length suffix already written): the
/// tie marker, one `)` per slur stop, then any after-grace/trill termination.
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
    for grace in &attachments.after_grace_groups {
        for slur in &grace.slurs {
            if slur.role == SlurRole::Start {
                out.push('(');
            }
        }
        out.push_str(&format!("[I:{}]", musicxml_after_grace_instruction()));
        out.push_str(&grace_str(grace));
    }
    out
}

/// Canonical ABC first/second-ending marker, e.g. `[1`, `[2`, `[1,3`, `[1-2`.
fn ending_str(model: &crate::model::RepeatEndingModel) -> String {
    use crate::model::RepeatEndingPartModel::{Range, Single, Text};
    let parts: Vec<String> = model
        .endings
        .iter()
        .map(|p| match p {
            Single(n) => n.to_string(),
            Range { start, end } => format!("{start}-{end}"),
            Text(text) => format!("\"{}\"", escape_abc_quotes(text)),
        })
        .collect();
    format!("[{}", parts.join(","))
}

fn escape_abc_quotes(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
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
        BarlineKind::Dotted => ".|",
        BarlineKind::Dashed => "|",
        BarlineKind::Invisible => "[|]",
        // Initial emits as a plain bar: it never renders a <barline> element
        // and segments measures exactly like Regular, so `|` is structurally
        // faithful. The Initial+RepeatStart split pair is re-joined as `[|:`
        // above.
        BarlineKind::Initial => "|",
        // Liberal must re-parse as Liberal: an empty measure between `|` and a
        // Liberal is preserved, while between two plain `|` it is coalesced
        // away — so substituting `|` can drop a measure. The original liberal
        // spelling is not stored; `|||` is the most innocuous spelling that
        // classifies as Liberal.
        BarlineKind::Liberal => "|||",
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

/// Accidental prefix for a note: the originally written accidental's glyph,
/// or nothing. Every other alter (key signature, measure carry, tie carry
/// across barlines) is reproduced by the parser's own accidental propagation
/// on re-parse, so no synthesized glyph is ever needed.
fn note_accidental(written: Option<Accidental>) -> &'static str {
    written.map(accidental_glyph).unwrap_or("")
}

/// Render one `&` overlay segment: `& ` plus its events, grouping consecutive
/// same-source chord members into `[...]` (mirroring the semantic lowering).
fn overlay_str(segment: &crate::model::OverlaySegment, unit: Rational, shift: i8) -> String {
    use crate::model::TimelineEventKind;
    let mut out = String::from("& ");
    let events = &segment.events;
    let (ov_markers, ov_scales) = overlay_tuplet_layout(events);
    let mut i = 0;
    while i < events.len() {
        let event = &events[i];
        out.push_str(&ov_markers[i]);
        let tuplet = ov_scales[i];
        match &event.kind {
            TimelineEventKind::Note { .. } => {
                let mut end = i + 1;
                while end < events.len() && overlay_same_chord(event, &events[end]) {
                    end += 1;
                }
                let group = &events[i..end];
                let chord = group.len() > 1;
                let mut lead = event.attachments.clone();
                if chord {
                    lead.ties.clear();
                }
                out.push_str(&event_prefix(&lead));
                let lengths: Vec<String> = group
                    .iter()
                    .map(|e| length_str(notated_duration(e.duration, tuplet), unit))
                    .collect();
                let uniform = lengths.windows(2).all(|w| w[0] == w[1]);
                if chord {
                    out.push('[');
                }
                for (e, len) in group.iter().zip(&lengths) {
                    let TimelineEventKind::Note {
                        step,
                        octave,
                        accidental,
                        ..
                    } = &e.kind
                    else {
                        continue;
                    };
                    out.push_str(note_accidental(*accidental));
                    out.push_str(&pitch_letter_str(*step, *octave - shift));
                    if !chord || !uniform {
                        out.push_str(len);
                    }
                    if chord && e.attachments.ties.iter().any(|t| t.role == TieRole::Start) {
                        out.push('-');
                    }
                }
                if chord {
                    out.push(']');
                    if let Some(len) = lengths.first().filter(|_| uniform) {
                        out.push_str(len);
                    }
                }
                out.push_str(&event_suffix(&lead));
                out.push(' ');
                i = end;
                continue;
            }
            TimelineEventKind::Rest { visibility, .. } => {
                out.push_str(&event_prefix(&event.attachments));
                out.push(match visibility {
                    RestVisibility::Visible => 'z',
                    RestVisibility::Invisible => 'x',
                });
                out.push_str(&length_str(notated_duration(event.duration, tuplet), unit));
                out.push_str(&event_suffix(&event.attachments));
                out.push(' ');
            }
            TimelineEventKind::Spacer => out.push_str("y "),
            TimelineEventKind::Barline { kind } => {
                out.push_str(barline_str(*kind));
                out.push(' ');
            }
            TimelineEventKind::VariantEnding { .. }
            | TimelineEventKind::VariantEndingClose { .. }
            | TimelineEventKind::KeyChange(_)
            | TimelineEventKind::MeterChange(_)
            | TimelineEventKind::ClefChange(_)
            | TimelineEventKind::TempoChange(_)
            | TimelineEventKind::SectionLabel(_) => {}
        }
        i += 1;
    }
    out
}

/// Tuplet layout over an overlay segment's events — the same pair-id
/// reconstruction as `tuplet_layout`, over `VoiceTimedEvent`s, plus a
/// Start-presence guard for pairs that straddle in from the main voice.
fn overlay_tuplet_layout(
    events: &[crate::model::VoiceTimedEvent],
) -> (TupletMarkers, TupletScales) {
    use crate::model::{TimelineEventKind, TupletRole};
    use std::collections::BTreeMap;
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
    let mut markers = vec![String::new(); events.len()];
    let mut scales = vec![None; events.len()];
    for (_pid, (actual, normal, start, stop, has_start)) in groups {
        // Unlike `tuplet_layout`, a pair here can lack its Start: a tuplet
        // straddling an `&` (`C (3DE & FGA z |`) keeps Start/Continue in the
        // main voice events and leaves a Stop-only pair in the overlay
        // segment. Emitting a marker at that Stop would open a bogus tuplet,
        // so such pairs are skipped (the events still carry their already
        // tuplet-scaled durations, which round-trip exactly).
        if !has_start || !abc_tuplet_actual_supported(actual) {
            continue;
        }
        let span = events[start..=stop]
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    TimelineEventKind::Note { .. } | TimelineEventKind::Rest { .. }
                )
            })
            .count();
        markers[start].push_str(&format!("({actual}:{normal}:{span}"));
        for slot in scales.iter_mut().take(stop + 1).skip(start) {
            multiply_tuplet_scale(slot, actual, normal);
        }
    }
    (markers, scales)
}

/// True when `next` continues the chord group opened at `first` (same source
/// token, same onset, flagged as a chord member).
fn overlay_same_chord(
    first: &crate::model::VoiceTimedEvent,
    next: &crate::model::VoiceTimedEvent,
) -> bool {
    use crate::model::TimelineEventKind;
    first.source_order == next.source_order
        && first.onset == next.onset
        && matches!(next.kind, TimelineEventKind::Note { chord: true, .. })
}

/// Pitch letter plus octave marks (middle C = octave 4: `C`=4, `c`=5).
fn pitch_str(pitch: &Pitch) -> String {
    pitch_letter_str(pitch.step, pitch.octave)
}

/// Pitch letter plus octave marks for a bare step/octave pair (middle C =
/// octave 4: `C`=4, `c`=5).
fn pitch_letter_str(step: char, octave: i8) -> String {
    let letter = step.to_ascii_uppercase();
    // Widen before the repeat-count subtraction: stored octaves reach the i8
    // bounds (lowering clamps long octave-mark runs to ±128), so `4 - octave`
    // overflows i8 for octaves below -123 (debug panic / release capacity
    // overflow in `croma dump abc`).
    let octave = i32::from(octave);
    if octave >= 5 {
        let mut s = letter.to_ascii_lowercase().to_string();
        s.push_str(&"'".repeat((octave - 5) as usize));
        s
    } else {
        let mut s = letter.to_string();
        s.push_str(&",".repeat((4 - octave) as usize));
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
fn notated_duration(duration: Rational, tuplet: Option<TupletScale>) -> Rational {
    match tuplet {
        Some(TupletScale::Ratio(actual, normal)) => {
            scaled_rational(duration, actual, normal).unwrap_or(duration)
        }
        Some(TupletScale::Overflow) => duration,
        None => duration,
    }
}

fn scaled_rational(duration: Rational, actual: u32, normal: u32) -> Option<Rational> {
    let numerator = u64::from(duration.numerator) * u64::from(actual);
    let denominator = u64::from(duration.denominator) * u64::from(normal);
    ratio_to_u32(numerator, denominator)
        .map(|(numerator, denominator)| Rational::new(numerator, denominator))
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
    use crate::model::{GraceEventKind, SlurRole};
    let mut out = String::from("{");
    let mut trailing_slur_stops = String::new();
    if group.slash.is_some() {
        out.push('/');
    }
    for grace in &group.events {
        for slur in &grace.slurs {
            if slur.role == SlurRole::Start {
                out.push('(');
            }
        }
        match &grace.kind {
            GraceEventKind::Note(note) => {
                for deco in &note.decorations {
                    out.push_str(&decoration_str(&deco.name));
                }
                out.push_str(note_accidental(
                    note.written_accidental.as_ref().map(|m| m.kind),
                ));
                out.push_str(&pitch_str(&note.pitch));
                out.push_str(&length_ratio_str(note.length_multiplier));
            }
            GraceEventKind::Rest(_) => out.push('z'),
            GraceEventKind::Chord(members) => {
                out.push('[');
                for note in members {
                    for deco in &note.decorations {
                        out.push_str(&decoration_str(&deco.name));
                    }
                    out.push_str(note_accidental(
                        note.written_accidental.as_ref().map(|m| m.kind),
                    ));
                    out.push_str(&pitch_str(&note.pitch));
                    out.push_str(&length_ratio_str(note.length_multiplier));
                }
                out.push(']');
            }
        }
        for slur in &grace.slurs {
            if slur.role == SlurRole::Stop {
                if slur.span.start >= group.span.end {
                    trailing_slur_stops.push(')');
                } else {
                    out.push(')');
                }
            }
        }
    }
    out.push('}');
    out.push_str(&trailing_slur_stops);
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

    /// `(program, channel, volume_cc, pan_cc)` — a [`MidiInstrumentModel`]
    /// stripped of its source span, which legitimately differs after re-parsing.
    type MidiFields = (Option<u8>, Option<u8>, Option<u8>, Option<u8>);

    /// Score-meaningful MIDI fields per voice, comparable across a write ->
    /// re-parse cycle (span excluded).
    fn midi_seq(score: &crate::Score) -> Vec<(Option<MidiFields>, Option<i16>)> {
        let mut v = Vec::new();
        for p in &score.parts {
            for voice in &p.voices {
                let instrument = voice
                    .midi_instrument
                    .map(|m| (m.program, m.channel, m.volume_cc, m.pan_cc));
                v.push((instrument, voice.midi_transpose));
            }
        }
        v
    }

    #[test]
    fn midi_instrument_directives_roundtrip() {
        // `%%MIDI program`/`channel`/`control` CC7-10/`transpose` carry
        // score-meaningful sound metadata (forward-translated to MusicXML
        // `<midi-instrument>`). The writer must re-emit them per voice so a Score
        // that carries them — e.g. one built by the MusicXML reader — survives a
        // write -> re-parse cycle instead of collapsing every voice to channel 1.
        let src = concat!(
            "X:1\nL:1/8\nK:C\n",
            "V:1\n%%MIDI program 1 52\n%%MIDI control 7 80\n%%MIDI control 10 64\n",
            "%%MIDI transpose -12\nCDEF\n",
            "V:2\n%%MIDI program 2 0\nGABc\n",
        );
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        assert!(
            abc.contains("%%MIDI program 1 52"),
            "writer dropped the MIDI instrument: {abc:?}"
        );
        let s2 = score_of(&abc);
        assert_eq!(midi_seq(&s1), midi_seq(&s2), "MIDI round-trip via {abc:?}");
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

    #[test]
    fn extreme_octave_marks_roundtrip_without_panicking() {
        // Lowering clamps the octave-mark sum into i8 range, so a 130-comma
        // run stores octave -126 and 200 up-marks store octave 127. The
        // writer's down-mark repeat count `4 - octave` used to overflow i8
        // (debug panic / release capacity overflow); it must widen instead.
        let commas = ",".repeat(130);
        let quotes = "'".repeat(200);
        let src = format!("X:1\nL:1/4\nK:C\nC{commas} c{quotes} D E |\n");
        let (a, b) = roundtrip_pitches(&src);
        assert_eq!(a, b, "pitch round-trip failed for extreme octave marks");
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

    /// `(is_rest, role)` per tuplet attachment, in event order — ratios alone
    /// can match even when a role landed on the wrong event (or nowhere).
    fn tuplet_roles(score: &crate::Score) -> Vec<(bool, crate::model::TupletRole)> {
        let mut v = Vec::new();
        for p in &score.parts {
            for voice in &p.voices {
                for e in &voice.events {
                    let is_rest = matches!(e.kind, crate::TimedEventKind::Rest(_));
                    for t in &e.attachments.tuplets {
                        v.push((is_rest, t.role));
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
            assert_eq!(tuplet_roles(&s1), tuplet_roles(&s2), "roles for {src:?}");
            assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
        }
    }

    #[test]
    fn nested_tuplets_roundtrip_preserves_outer_and_inner_groups() {
        let src = "X:1\nM:C\nL:1/4\nK:none\n(7:8:8(3A/A/A/ A/A/A/A/A/|\n";
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());

        assert!(abc.contains("(7:8:8"), "{abc}");
        assert!(abc.contains("(3:2:3"), "{abc}");
        assert!(!abc.contains("A8/7"), "{abc}");
        let s2 = score_of(&abc);
        assert_eq!(tuplet_ratios(&s1), tuplet_ratios(&s2), "{abc}");
        assert_eq!(tuplet_roles(&s1), tuplet_roles(&s2), "{abc}");
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2), "{abc}");
    }

    #[test]
    fn overflowing_nested_tuplet_product_does_not_saturate_notated_length() {
        let src = concat!(
            "X:1\n",
            "M:C\n",
            "L:1/8\n",
            "K:C\n",
            "(9:8:1(9:8:1(9:8:1(9:8:1(9:8:1(9:8:1",
            "(9:8:1(9:8:1(9:8:1(9:8:1(9:8:1A|\n",
        );
        let abc = write_abc(&score_of(src), AbcWriteOptions::default());

        assert_eq!(abc.matches("(9:8:1").count(), 11, "{abc}");
        assert!(
            !abc.contains("A8"),
            "overflowing product must not be baked into a saturated length: {abc}"
        );
    }

    #[test]
    fn rest_tuplets_roundtrip() {
        // Rests carry tuplet roles like notes do, so a rest-led (or rest-closed,
        // or all-rest) tuplet keeps its marker and span across a round-trip.
        for src in [
            "X:1\nM:4/4\nL:1/8\nK:C\n(3zBA F2 |\n",
            "X:1\nM:4/4\nL:1/8\nK:C\n(3BAz F2 |\n",
            "X:1\nM:4/4\nL:1/8\nK:C\n(3zzz F2 |\n",
            "X:1\nM:4/4\nL:1/8\nK:C\n(3:2:3z B A F2 |\n",
            "X:1\nM:4/4\nL:1/8\nK:C\n(3z>BA F2 |\n",
        ] {
            let s1 = score_of(src);
            let abc = write_abc(&s1, AbcWriteOptions::default());
            let s2 = score_of(&abc);
            assert_eq!(
                tuplet_ratios(&s1),
                tuplet_ratios(&s2),
                "tuplets for {src:?} -> {abc:?}"
            );
            assert_eq!(
                tuplet_roles(&s1),
                tuplet_roles(&s2),
                "roles for {src:?} -> {abc:?}"
            );
            assert_eq!(
                pitch_seq(&s1),
                pitch_seq(&s2),
                "pitches for {src:?} -> {abc:?}"
            );
            let durations = |s: &crate::Score| {
                s.parts[0].voices[0]
                    .events
                    .iter()
                    .filter_map(|e| match e.kind {
                        crate::TimedEventKind::Note(_) | crate::TimedEventKind::Rest(_) => {
                            Some(e.duration)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                durations(&s1),
                durations(&s2),
                "durations for {src:?} -> {abc:?}"
            );
        }
        // `(3BAz` previously dropped the Stop (it sat on the rest) and emitted a
        // wrong span of 2; the rest must count, giving `(3:2:3`.
        let s1 = score_of("X:1\nM:4/4\nL:1/8\nK:C\n(3BAz F2 |\n");
        let abc = write_abc(&s1, AbcWriteOptions::default());
        assert!(abc.contains("(3:2:3"), "expected (3:2:3 marker in {abc:?}");
    }

    fn grace_pitches(score: &crate::Score) -> Vec<(char, i8, i8, bool, bool)> {
        let mut v = Vec::new();
        for p in &score.parts {
            for voice in &p.voices {
                for e in &voice.events {
                    for (after_grace, g) in e
                        .attachments
                        .grace_groups
                        .iter()
                        .map(|group| (false, group))
                        .chain(
                            e.attachments
                                .after_grace_groups
                                .iter()
                                .map(|group| (true, group)),
                        )
                    {
                        let slash = g.slash.is_some();
                        for ge in &g.events {
                            if let crate::model::GraceEventKind::Note(n) = &ge.kind {
                                v.push((
                                    n.pitch.step,
                                    n.pitch.alter,
                                    n.pitch.octave,
                                    slash,
                                    after_grace,
                                ));
                            }
                        }
                    }
                }
            }
        }
        v
    }

    fn grace_slur_roles(score: &crate::Score) -> Vec<(bool, crate::model::SlurRole)> {
        let mut v = Vec::new();
        for p in &score.parts {
            for voice in &p.voices {
                for e in &voice.events {
                    for (after_grace, g) in e
                        .attachments
                        .grace_groups
                        .iter()
                        .map(|group| (false, group))
                        .chain(
                            e.attachments
                                .after_grace_groups
                                .iter()
                                .map(|group| (true, group)),
                        )
                    {
                        v.extend(g.slurs.iter().map(|slur| (after_grace, slur.role)));
                        for ge in &g.events {
                            v.extend(ge.slurs.iter().map(|slur| (after_grace, slur.role)));
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
    fn grace_internal_slurs_roundtrip() {
        let src = "X:1\nL:1/8\nK:C\n{(fg)}a2 {(ef)}g2|]\n";
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);

        assert!(
            abc.contains("{(fg)}a2 {(ef)}g2"),
            "grace-internal slurs missing from ABC: {abc:?}"
        );
        assert_eq!(grace_pitches(&s1), grace_pitches(&s2));
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
    }

    #[test]
    fn bare_grace_slur_roundtrips() {
        let src = "X:1\nL:1/8\nK:C\n({Bc})D|\n";
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);

        assert!(
            abc.contains("({Bc})D"),
            "bare grace slur missing from ABC: {abc:?}"
        );
        assert_eq!(grace_pitches(&s1), grace_pitches(&s2));
        assert_eq!(grace_slur_roles(&s1), grace_slur_roles(&s2));
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
    }

    #[test]
    fn bare_grace_slur_before_barline_roundtrips() {
        let src = "X:1\nL:1/8\nK:C\nA2({Bc})|\n";
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);

        assert!(
            abc.contains("A2([I:croma-after-grace]{Bc})"),
            "bare grace before barline missing from ABC: {abc:?}"
        );
        assert_eq!(grace_pitches(&s1), grace_pitches(&s2));
        assert_eq!(grace_slur_roles(&s1), grace_slur_roles(&s2));
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
    }

    #[test]
    fn after_grace_carrier_roundtrips_on_chords() {
        let src = "X:1\nL:1/8\nK:C\n[CEG]8[I:croma-after-grace]{d} | D8 |\n";
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);

        assert!(
            abc.contains("[CEG]8[I:croma-after-grace]{d}"),
            "chord after-grace carrier missing from ABC: {abc:?}"
        );
        assert_eq!(
            grace_pitches(&s1),
            grace_pitches(&s2),
            "grace for {src:?} -> {abc:?}"
        );
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
    }

    #[test]
    fn trailing_trill_grace_notes_roundtrip_after_principal_note() {
        let src = "X:1\nT:Trailing Grace\nM:4/4\nL:1/8\nK:C\nTe6{de}|d2f f2f|\n";
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);

        assert!(
            abc.contains("!trill!e6[I:croma-after-grace]{de}"),
            "after-grace suffix missing from ABC: {abc:?}"
        );
        assert_eq!(
            grace_pitches(&s1),
            grace_pitches(&s2),
            "grace for {src:?} -> {abc:?}"
        );
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
    }

    #[test]
    fn chord_symbol_after_slur_open_survives() {
        // The writer emits the slur-open first (`("G7"DE)`), the canonical
        // order; the parser keeps the chord symbol in either order.
        let src = "X:1\nL:1/8\nK:C\n(\"G7\"DE) F |\n";
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(
            text_attachments(&s1),
            text_attachments(&s2),
            "chord symbol lost: {abc:?}"
        );
        assert!(!text_attachments(&s1).is_empty());
    }

    #[test]
    fn chord_symbol_after_grace_survives() {
        // The writer emits the grace group first (`{AB}"F"c`), the canonical
        // order; the parser keeps the chord symbol in either order.
        let src = "X:1\nL:1/8\nK:C\n{AB}\"F\"c2 d2 |\n";
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(
            text_attachments(&s1),
            text_attachments(&s2),
            "chord symbol lost: {abc:?}"
        );
        assert!(!text_attachments(&s1).is_empty());
    }

    fn member_tie_starts(score: &crate::Score) -> Vec<Vec<bool>> {
        let mut v = Vec::new();
        for p in &score.parts {
            for voice in &p.voices {
                for e in &voice.events {
                    if let crate::TimedEventKind::Chord(c) = &e.kind {
                        v.push(
                            c.members
                                .iter()
                                .map(|m| {
                                    m.attachments.ties.iter().any(|t| t.role == TieRole::Start)
                                })
                                .collect(),
                        );
                    }
                }
            }
        }
        v
    }

    #[test]
    fn chord_member_ties_stay_per_member() {
        // `[dg-]` ties only g; the writer must not promote it to a whole-chord
        // tie (`[dg]-`), which would also tie d.
        for src in [
            "X:1\nL:1/8\nK:C\n[dg-]2 [dg]2 |\n",
            "X:1\nL:1/8\nK:C\n[C-E]2 [CE]2 |\n",
            "X:1\nL:1/8\nK:C\n[CE]2- [CE]2 |\n",
        ] {
            let s1 = score_of(src);
            let abc = write_abc(&s1, AbcWriteOptions::default());
            let s2 = score_of(&abc);
            assert_eq!(
                member_tie_starts(&s1),
                member_tie_starts(&s2),
                "member ties for {src:?} -> {abc:?}"
            );
        }
    }

    fn harmony_texts(score: &crate::Score) -> Vec<String> {
        use crate::model::AlignedSymbolKind;
        let mut v = Vec::new();
        for p in &score.parts {
            for voice in &p.voices {
                for e in &voice.events {
                    for c in &e.attachments.chord_symbols {
                        v.push(c.text.clone());
                    }
                    for s in &e.attachments.symbols {
                        if s.kind == AlignedSymbolKind::ChordSymbol {
                            v.push(s.text.clone());
                        }
                    }
                }
            }
        }
        v
    }

    #[test]
    fn aligned_chord_symbols_inline_roundtrip() {
        // `s:`-aligned chord symbols re-emit inline; the <harmony> sequence
        // (inline + aligned, in exporter order) must survive the round-trip.
        let src = "X:1\nL:1/4\nK:C\nC \"D7\"D [EG] F |\ns:\"Gm\" * \"C\"\n";
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(
            harmony_texts(&s1),
            harmony_texts(&s2),
            "harmony for {abc:?}"
        );
        assert_eq!(harmony_texts(&s1).len(), 3);
        assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
    }

    fn symbol_tokens(score: &crate::Score) -> Vec<(String, String)> {
        let mut v = Vec::new();
        for p in &score.parts {
            for voice in &p.voices {
                for e in &voice.events {
                    for s in &e.attachments.symbols {
                        if s.kind != crate::model::AlignedSymbolKind::ChordSymbol {
                            v.push((format!("{:?}", s.kind), s.text.clone()));
                        }
                    }
                }
                for measure in &voice.measures {
                    for segment in &measure.overlays {
                        for e in &segment.events {
                            for s in &e.attachments.symbols {
                                if s.kind != crate::model::AlignedSymbolKind::ChordSymbol {
                                    v.push((format!("{:?}", s.kind), s.text.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
        v
    }

    #[test]
    fn aligned_symbols_reemit_as_symbol_lines() {
        // Decoration/Annotation/Raw aligned symbols re-emit as s: lines (their
        // inline forms render different MusicXML), preserving kind + text +
        // note alignment; layered (adjacent) s: lines round-trip too.
        for src in [
            "X:1\nL:1/4\nK:C\nCDEF|\ns:!trill! * \"^slow\" foo\n",
            "X:1\nL:1/4\nK:C\nCD z EF|\ns:!trill! * !fermata!\ns:* +mordent+\n",
            "X:1\nL:1/4\nK:C\nC & E F|\ns:* \"^overlay\" !trill!\n",
            "X:1\nL:1/4\nK:C\nC & E & G|\ns:* \"^first\" \"^second\"\n",
            "X:1\nL:1/4\nK:C\nC D & z E|\ns:* * \"^after-rest\"\n",
            "X:1\nL:1/4\nK:C\nC & [EG] F|\ns:* \"^chord\" !trill!\n",
        ] {
            let s1 = score_of(src);
            let abc = write_abc(&s1, AbcWriteOptions::default());
            let s2 = score_of(&abc);
            assert_eq!(
                symbol_tokens(&s1),
                symbol_tokens(&s2),
                "aligned symbols for {src:?} -> {abc:?}"
            );
            assert!(!symbol_tokens(&s1).is_empty());
            assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
        }
    }

    fn measure_count(score: &crate::Score) -> u32 {
        score.parts[0].voices[0]
            .events
            .iter()
            .map(|e| e.measure.index)
            .max()
            .map_or(0, |m| m + 1)
    }

    #[test]
    fn exotic_barlines_roundtrip() {
        // Dotted and Invisible keep their kind; Initial and Liberal normalize
        // to Regular (structurally identical — neither renders a <barline>).
        for (src, same_kinds) in [
            ("X:1\nL:1/4\nK:C\nCD .| EF |\n", true),
            ("X:1\nL:1/4\nK:C\nCD [|] EF |\n", true),
            ("X:1\nL:1/4\nK:C\n[| CD EF |\n", false),
            ("X:1\nL:1/4\nK:C\nCD ||| EF |\n", false),
        ] {
            let s1 = score_of(src);
            let abc = write_abc(&s1, AbcWriteOptions::default());
            let s2 = score_of(&abc);
            assert_eq!(pitch_seq(&s1), pitch_seq(&s2), "{abc:?}");
            assert_eq!(measure_count(&s1), measure_count(&s2), "{abc:?}");
            if same_kinds {
                assert_eq!(barline_kinds(&s1), barline_kinds(&s2), "{abc:?}");
            }
        }
    }

    #[test]
    fn split_token_barline_pairs_rejoin() {
        // `||:` lowers to [Double, RepeatStart] sharing one span; emitting the
        // pair as two spaced tokens would phantom an empty leading measure.
        for src in [
            "X:1\nL:1/4\nK:C\n||: CDEF :|\n",
            "X:1\nL:1/4\nK:C\n[|: CDEF :|\n",
            "X:1\nL:1/4\nK:C\nCDEF ||: GABc :|\n",
            // `|]:` lowers to [Final, RepeatStart] sharing one span (a final bar
            // fused with a forward repeat); it must rejoin to `|]:`, not split
            // into a spaced `|] |:` that phantoms an empty measure.
            "X:1\nL:1/4\nK:C\nCDEF |]: GABc :|\n",
        ] {
            let s1 = score_of(src);
            let abc = write_abc(&s1, AbcWriteOptions::default());
            let s2 = score_of(&abc);
            assert_eq!(measure_count(&s1), measure_count(&s2), "{src:?} -> {abc:?}");
            assert_eq!(barline_kinds(&s1), barline_kinds(&s2), "{src:?} -> {abc:?}");
        }
        // A real two-token `|| |:` pair mid-tune has distinct spans and must
        // NOT be rejoined (its phantom measure is real).
        let src = "X:1\nL:1/4\nK:C\nCDEF || |: GABc :|\n";
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(measure_count(&s1), measure_count(&s2), "{abc:?}");
    }

    #[test]
    fn spacers_roundtrip() {
        // `y` spacers are zero-duration layout events; dropping them collapses
        // spacer-only measures, so they are emitted. Also inside a tuplet the
        // explicit `(p:q:r` span counts events, keeping the group intact.
        for src in [
            "X:1\nL:1/4\nK:C\nCD y EF | GA y2 BC |\n",
            "X:1\nM:4/4\nL:1/8\nK:C\n(3CDE y F2 |\n",
        ] {
            let s1 = score_of(src);
            let abc = write_abc(&s1, AbcWriteOptions::default());
            let s2 = score_of(&abc);
            assert_eq!(pitch_seq(&s1), pitch_seq(&s2), "{src:?} -> {abc:?}");
            assert_eq!(measure_count(&s1), measure_count(&s2), "{src:?} -> {abc:?}");
            let spacers = |s: &crate::Score| {
                s.parts[0].voices[0]
                    .events
                    .iter()
                    .filter(|e| matches!(e.kind, crate::TimedEventKind::Spacer))
                    .count()
            };
            assert_eq!(spacers(&s1), spacers(&s2), "{src:?} -> {abc:?}");
        }
    }

    fn lyric_tokens(score: &crate::Score) -> Vec<(u32, String, String)> {
        let mut v = Vec::new();
        for p in &score.parts {
            for voice in &p.voices {
                for e in &voice.events {
                    for l in &e.attachments.lyrics {
                        v.push((l.verse, format!("{:?}", l.control), l.text.clone()));
                    }
                }
                for measure in &voice.measures {
                    for segment in &measure.overlays {
                        for e in &segment.events {
                            for l in &e.attachments.lyrics {
                                v.push((l.verse, format!("{:?}", l.control), l.text.clone()));
                            }
                        }
                    }
                }
            }
        }
        v
    }

    #[test]
    fn lyrics_roundtrip() {
        for src in [
            // syllables, hyphen, extender, skip, '~' space, across barlines
            "X:1\nL:1/4\nK:C\nCDEF|GAB c|\nw:doe-ray me_ fa * la~la ti\n",
            // multi-verse adjacency
            "X:1\nL:1/4\nK:C\nCD EF|\nw:one two three four\nw:uno dos tres cuatro\n",
            // verse gap (1 and 3) + chord consumes one position + rest skipped
            "X:1\nL:1/4\nK:C\nC z [EG] F|\nw:a b c\nw:*\nw:x y z\n",
            // escaped metacharacters in syllables
            "X:1\nL:1/4\nK:C\nCD|\nw:a\\-b c\\*d\n",
        ] {
            let s1 = score_of(src);
            let abc = write_abc(&s1, AbcWriteOptions::default());
            let s2 = score_of(&abc);
            assert_eq!(
                lyric_tokens(&s1),
                lyric_tokens(&s2),
                "lyrics for {src:?} -> {abc:?}"
            );
            assert!(
                !lyric_tokens(&s1).is_empty(),
                "fixture has no lyrics: {src:?}"
            );
            assert_eq!(pitch_seq(&s1), pitch_seq(&s2));
        }
    }

    #[test]
    fn overlay_lyrics_roundtrip_in_source_order() {
        for src in [
            "X:1\nM:C\nL:1/8\nK:G\nD2D2 D2D>D&x6C>D|E2FE D2D2|\nw:And tis down by the green-wood side-oh!\n",
            "X:1\nM:C|\nL:1/4\nK:C\nV:1\nd\"^Duo:p\"3/4d/4de&dG3/4G/4Ge|\nw:the king of\n",
        ] {
            let s1 = score_of(src);
            let abc = write_abc(&s1, AbcWriteOptions::default());
            let s2 = score_of(&abc);
            assert_eq!(
                lyric_tokens(&s1),
                lyric_tokens(&s2),
                "lyrics for {src:?} -> {abc:?}"
            );
            assert_eq!(overlay_pitches(&s1), overlay_pitches(&s2));
        }
    }

    fn voice_shape(score: &crate::Score) -> Vec<(String, Vec<Option<String>>, usize)> {
        score
            .parts
            .iter()
            .flat_map(|p| {
                p.voices.iter().map(|v| {
                    let p = &v.properties;
                    let texts = vec![
                        p.name.as_ref().map(|t| t.text.clone()),
                        p.nm.as_ref().map(|t| t.text.clone()),
                        p.subname.as_ref().map(|t| t.text.clone()),
                        p.snm.as_ref().map(|t| t.text.clone()),
                        p.clef.as_ref().map(|t| t.text.clone()),
                        p.stem.map(|s| format!("{s:?}")),
                        p.octave.as_ref().map(|t| t.text.clone()),
                        p.transpose.as_ref().map(|t| t.text.clone()),
                        p.middle.as_ref().map(|t| t.text.clone()),
                    ];
                    (v.id.value.clone(), texts, v.events.len())
                })
            })
            .collect()
    }

    #[test]
    fn multi_voice_roundtrip() {
        for src in [
            "X:1\nL:1/4\nK:C\nV:1\nCDEF|\nV:2\nE,F,G,A,|\n",
            "X:1\nL:1/4\nK:C\nV:T name=\"Tenor\" clef=treble-8\nCDEF|\nV:B clef=bass\nC,D,E,F,|\n",
            "X:1\nL:1/4\nK:C\nV:1 octave=-1\nCDEF|\nV:2 octave=1\nGABc|\n",
            // Oversized modifiers clamp (octave=99999 -> +9, treble+15
            // octave=125 -> +11) and must clamp IDENTICALLY in lowering and
            // in the writer's mirrored shift, or pitches drift per pass.
            "X:1\nL:1/4\nK:C\nV:1 octave=99999\nCDEF|\nV:2 clef=treble+15 octave=125\nGABc|\n",
        ] {
            let s1 = score_of(src);
            let abc = write_abc(&s1, AbcWriteOptions::default());
            let s2 = score_of(&abc);
            assert_eq!(pitch_seq(&s1), pitch_seq(&s2), "{src:?} -> {abc:?}");
            assert_eq!(voice_shape(&s1), voice_shape(&s2), "{src:?} -> {abc:?}");
        }
    }

    fn overlay_pitches(score: &crate::Score) -> Vec<(u32, char, i8)> {
        let mut v = Vec::new();
        for p in &score.parts {
            for voice in &p.voices {
                for m in &voice.measures {
                    for seg in &m.overlays {
                        for e in &seg.events {
                            if let crate::model::TimelineEventKind::Note { step, octave, .. } =
                                e.kind
                            {
                                v.push((seg.measure_index, step, octave));
                            }
                        }
                    }
                }
            }
        }
        v
    }

    #[test]
    fn overlays_roundtrip() {
        for src in [
            "X:1\nL:1/4\nK:C\nC2 E2 & G,2 B,2 |\n",
            "X:1\nL:1/4\nK:C\nCDEF | G2 A2 & B,2 C2 | E4 |\n",
            "X:1\nL:1/4\nK:C\nC2 E2 & G,2 B,2 & E,2 F,2 |\n",
        ] {
            let s1 = score_of(src);
            let abc = write_abc(&s1, AbcWriteOptions::default());
            let s2 = score_of(&abc);
            assert_eq!(pitch_seq(&s1), pitch_seq(&s2), "{src:?} -> {abc:?}");
            assert_eq!(
                overlay_pitches(&s1),
                overlay_pitches(&s2),
                "{src:?} -> {abc:?}"
            );
        }
    }

    #[test]
    fn review_regressions_roundtrip() {
        // Repros from the adversarial review: each must round-trip with an
        // identical structural shape AND be write-idempotent.
        for src in [
            // C1: grace pitches are stored unshifted in octave-shifted voices
            "X:1\nL:1/4\nK:C\nV:1 clef=treble-8\n{fg}A B C D|\n",
            "X:1\nL:1/4\nK:C\nV:1 middle=d\n{fg}A B C D|\n",
            // I1: `+:` continuation joins with a newline inside the syllable
            "X:1\nL:1/4\nK:C\nCDEF|\nw:a b\n+:c d\n",
            // I2a: spacer inside a tuplet consumes no tuplet slot
            "X:1\nM:4/4\nL:1/8\nK:C\nC (3D y E F|\n",
            // I2b: tuplet inside an overlay segment
            "X:1\nL:1/4\nK:C\nC2 E2 & (3G,B,D G,2|\n",
            // I3: overlay in a measure with no primary-voice content
            "X:1\nL:1/4\nK:C\n| & CDEF|\n",
            // I4: backslash in a quoted voice property
            "X:1\nL:1/4\nK:C\nV:1 name=\"a\\\\b\"\nCDEF|\n",
        ] {
            let s1 = score_of(src);
            let abc1 = write_abc(&s1, AbcWriteOptions::default());
            let s2 = score_of(&abc1);
            let abc2 = write_abc(&s2, AbcWriteOptions::default());
            assert_eq!(abc1, abc2, "not idempotent for {src:?}");
            assert_eq!(pitch_seq(&s1), pitch_seq(&s2), "{src:?} -> {abc1:?}");
            assert_eq!(
                measure_count(&s1),
                measure_count(&s2),
                "{src:?} -> {abc1:?}"
            );
            assert_eq!(
                grace_pitches(&s1),
                grace_pitches(&s2),
                "{src:?} -> {abc1:?}"
            );
            assert_eq!(
                tuplet_ratios(&s1),
                tuplet_ratios(&s2),
                "{src:?} -> {abc1:?}"
            );
            assert_eq!(
                overlay_pitches(&s1),
                overlay_pitches(&s2),
                "{src:?} -> {abc1:?}"
            );
            assert_eq!(voice_shape(&s1), voice_shape(&s2), "{src:?} -> {abc1:?}");
        }
    }

    fn change_events(score: &crate::Score) -> Vec<(usize, String)> {
        let mut v = Vec::new();
        for p in &score.parts {
            for voice in &p.voices {
                for (i, e) in voice.events.iter().enumerate() {
                    match &e.kind {
                        crate::TimedEventKind::KeyChange(k) => {
                            v.push((i, format!("K:{}", k.display)));
                        }
                        crate::TimedEventKind::MeterChange(m) => {
                            v.push((i, format!("M:{}", m.display)));
                        }
                        _ => {}
                    }
                }
            }
        }
        v
    }

    #[test]
    fn mid_tune_key_meter_changes_roundtrip() {
        for src in [
            // inline at a barline + meter change
            "X:1\nL:1/4\nK:C\nCDEF|[K:F]GAB_B|[M:3/4]ABc|\n",
            // mid-measure key change
            "X:1\nL:1/4\nK:C\n^FGA[K:D]F|FGAF|\n",
            // standalone body lines
            "X:1\nL:1/4\nK:C\nCDEF|\nK:D\nFGAF|\n",
            // mode + cut-time displays echo verbatim
            "X:1\nL:1/4\nK:C\nCDEF|[K:Ador]ABcd|[M:C|]ABcd|\n",
            // no-op restatement of the header key (49 corpus files do this)
            "X:1\nL:1/4\nK:C\nCDEF|[K:C]GABc|\n",
            // same-measure ledger interaction: natural F before [K:D] holds
            "X:1\nL:1/4\nK:C\n=FGA[K:D]F|FGAF|\n",
            // multi-voice broadcast: standalone K:D between voice bodies
            "X:1\nL:1/4\nK:C\nV:1\nCDEF|\nK:D\nV:1\nFGAF|\nV:2\nCDEF|\nFGAF|\n",
        ] {
            let s1 = score_of(src);
            let abc1 = write_abc(&s1, AbcWriteOptions::default());
            let s2 = score_of(&abc1);
            let abc2 = write_abc(&s2, AbcWriteOptions::default());
            assert_eq!(abc1, abc2, "not idempotent for {src:?}");
            assert_eq!(pitch_seq(&s1), pitch_seq(&s2), "{src:?} -> {abc1:?}");
            assert_eq!(
                change_events(&s1),
                change_events(&s2),
                "{src:?} -> {abc1:?}"
            );
            assert_eq!(
                measure_count(&s1),
                measure_count(&s2),
                "{src:?} -> {abc1:?}"
            );
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

    #[test]
    fn section_label_with_brackets_emits_whole_line_not_inline() {
        // A `P:` label whose text contains `[` or `]` cannot be safely emitted
        // as an inline field `[P:{label}]` — the parser reads up to the first `]`,
        // truncating the label.  The fix is to emit it as a whole-line body field
        // `P:{label}` (on its own line) which allows `]` in the value.
        // This is the tune_013528 case: P:Avokatrilli   [Am]
        let src = "X:1\nT:T\nM:2/4\nL:1/8\nK:Am\nP:Avokatrilli   [Am]\nAB cd |\n";
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        // Must NOT contain the broken inline form.
        assert!(
            !abc.contains("[P:Avokatrilli"),
            "label containing `[` must not be emitted as inline `[P:...]`; got:\n{abc}"
        );
        // Must contain a whole-line `P:` with the full label (including `[Am]`).
        assert!(
            abc.contains("\nP:Avokatrilli   [Am]\n"),
            "label containing `[` must be emitted as whole-line `P:...[Am]`; got:\n{abc}"
        );
        // Re-parsing must recover the identical SectionLabel event.
        let s2 = score_of(&abc);
        let labels1: Vec<_> = s1
            .parts
            .iter()
            .flat_map(|p| p.voices.iter())
            .flat_map(|v| v.events.iter())
            .filter_map(|e| {
                if let crate::TimedEventKind::SectionLabel(l) = &e.kind {
                    Some(l.clone())
                } else {
                    None
                }
            })
            .collect();
        let labels2: Vec<_> = s2
            .parts
            .iter()
            .flat_map(|p| p.voices.iter())
            .flat_map(|v| v.events.iter())
            .filter_map(|e| {
                if let crate::TimedEventKind::SectionLabel(l) = &e.kind {
                    Some(l.clone())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            labels1, labels2,
            "section label must survive the ABC → write_abc → re-parse round-trip"
        );
    }
}
