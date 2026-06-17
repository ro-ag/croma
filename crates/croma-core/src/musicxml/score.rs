use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::model::{
    BarlineKind, Fraction, Measure, MeasureBarline, MeasureId, MidiInstrumentModel, Part, Score,
    TimedEventKind, TimelineEventKind,
};

use super::{
    BarlineLocation, EndingType, MeasureSequence, MusicXmlWriter, SequenceEvent,
    barline::EndingDisplay,
};

impl<'score> MusicXmlWriter<'score> {
    /// `W:` post-tune words are text printed after the tune (ABC 2.1), not music
    /// aligned to notes. MusicXML represents such page-level text with
    /// score-header `<credit>` elements rather than in-measure directions.
    pub(crate) fn write_credits(&mut self) {
        for line in &self.score.metadata.post_tune_lyrics {
            if line.text.trim().is_empty() {
                continue;
            }
            self.xml.start("credit", &[("page", "1")]);
            self.xml.text_element("credit-words", &line.text);
            self.xml.end("credit");
        }
    }

    pub(crate) fn write_metadata(&mut self) {
        if let Some(title) = &self.score.metadata.title {
            self.xml.start("work", &[]);
            self.xml.text_element("work-title", &title.text);
            self.xml.end("work");
        }

        if !self.score.metadata.composers.is_empty() {
            self.xml.start("identification", &[]);
            for composer in &self.score.metadata.composers {
                self.xml
                    .text_element_attrs("creator", &[("type", "composer")], &composer.text);
            }
            self.xml.end("identification");
        }
    }

    pub(crate) fn write_part_list(&mut self) {
        self.xml.start("part-list", &[]);
        for (index, part) in self.score.parts.iter().enumerate() {
            let id = part_xml_id(part, index);
            self.xml.start("score-part", &[("id", id.as_str())]);
            self.xml
                .text_element("part-name", part_name(part, self.score).as_str());
            self.write_part_instruments(part, &id);
            self.xml.end("score-part");
        }
        self.xml.end("part-list");
    }

    /// Emit `<score-instrument>` / `<midi-instrument>` for each voice in this
    /// part that carries score-translatable `%%MIDI` sound metadata (program,
    /// channel, CC7 volume, or CC10 pan). The instrument name is the General MIDI
    /// name when a program is present, otherwise the part name (never abc2xml's
    /// literal "no name" filler). abc2midi/GM programs are 0-based; the MusicXML
    /// `<midi-program>` is 1-based (GM+1). `<volume>` is `cc / 1.27` and `<pan>`
    /// is `cc / 127 * 180 - 90`, matching abc2xml.
    fn write_part_instruments(&mut self, part: &Part, part_id: &str) {
        let fallback_name = part_name(part, self.score);
        let instruments: Vec<(String, String, MidiInstrumentModel)> = part
            .voices
            .iter()
            .filter_map(|voice| voice.midi_instrument)
            .filter(MidiInstrumentModel::has_content)
            .enumerate()
            .map(|(seq, midi)| {
                let name = midi.program.map_or_else(
                    || fallback_name.clone(),
                    |program| gm_program_name(program).to_owned(),
                );
                (format!("{part_id}-I{}", seq + 1), name, midi)
            })
            .collect();
        if instruments.is_empty() {
            return;
        }
        // MusicXML orders all <score-instrument> before all <midi-instrument>
        // within a <score-part>.
        for (instrument_id, name, _) in &instruments {
            self.xml
                .start("score-instrument", &[("id", instrument_id.as_str())]);
            self.xml.text_element("instrument-name", name);
            self.xml.end("score-instrument");
        }
        for (instrument_id, _, midi) in &instruments {
            self.xml
                .start("midi-instrument", &[("id", instrument_id.as_str())]);
            if let Some(channel) = midi.channel {
                self.xml.text_element("midi-channel", &channel.to_string());
            }
            if let Some(program) = midi.program {
                self.xml
                    .text_element("midi-program", &(u16::from(program) + 1).to_string());
            }
            if let Some(volume) = midi.volume_cc {
                self.xml
                    .text_element("volume", &format!("{:.2}", f64::from(volume) / 1.27));
            }
            if let Some(pan) = midi.pan_cc {
                self.xml.text_element(
                    "pan",
                    &format!("{:.2}", f64::from(pan) / 127.0 * 180.0 - 90.0),
                );
            }
            self.xml.end("midi-instrument");
        }
    }

    pub(crate) fn write_part(&mut self, part: &'score Part, part_index: usize) {
        let id = part_xml_id(part, part_index);
        self.active_key = self.score.metadata.key.clone();
        self.slur_numbers = Default::default();
        self.lyric_hyphen_open.clear();
        self.xml.start("part", &[("id", id.as_str())]);
        let mut pending_left_repeat = false;
        let measure_ids = part_measure_ids(part);
        let overlay_voice_numbers = overlay_voice_numbers(part);
        let ending_stops = ending_stop_schedule(part, &measure_ids);
        for (measure_position, measure_id) in measure_ids.iter().enumerate() {
            let number = measure_id.number.to_string();
            self.xml.start("measure", &[("number", number.as_str())]);
            if measure_position == 0 {
                self.write_attributes(part);
                self.write_initial_directions(part_index == 0);
            }

            let measure_refs = part_measure_refs(part, *measure_id);
            let left_barlines = unique_barlines(&measure_refs, true);
            let endings = unique_endings(&measure_refs);
            // A left forward-repeat is either deferred from a previous
            // RepeatBoth / trailing `|:` (`pending_left_repeat`) or a leading
            // `|:` in this measure (`unique_barlines(left)` only ever yields
            // RepeatStart).
            let has_left_repeat = pending_left_repeat
                || left_barlines
                    .iter()
                    .any(|barline| barline.kind == BarlineKind::RepeatStart);
            pending_left_repeat = false;

            if has_left_repeat && !endings.is_empty() {
                // `:|:[2` / `|:[2`: the forward repeat and the ending start share
                // one measure edge — emit a SINGLE <barline location="left">
                // carrying bar-style + <ending> + <repeat> so a standard consumer
                // (music21) keeps the forward repeat instead of dropping it (Bug 8).
                self.write_ending_barline(
                    BarlineLocation::Left,
                    &endings,
                    EndingType::Start,
                    Some(BarlineKind::RepeatStart),
                );
            } else {
                if has_left_repeat {
                    self.write_barline(BarlineLocation::Left, BarlineKind::RepeatStart, &[]);
                }
                if !endings.is_empty() {
                    self.write_ending_barline(
                        BarlineLocation::Left,
                        &endings,
                        EndingType::Start,
                        None,
                    );
                }
            }

            if let Some(count) = unique_multiple_rest(&measure_refs) {
                self.write_multiple_rest_measure_style(count);
            }

            let sequences = measure_sequences(part, *measure_id, &overlay_voice_numbers);
            for (sequence_index, sequence) in sequences.iter().enumerate() {
                let cursor = self.write_sequence(sequence, part);
                if sequence_index + 1 < sequences.len() && cursor != Fraction::zero() {
                    self.write_backup(cursor);
                }
            }

            // An ending bracket may span several measures (ABC 2.1 §4.9); the
            // schedule says which measure's right barline closes the bracket
            // opened at its `[N`.
            let mut stop_endings = ending_stops[measure_position].as_deref();
            let right_barlines = unique_barlines(&measure_refs, false);
            for barline in &right_barlines {
                if let Some(stops) = stop_endings.take() {
                    self.write_ending_barline(
                        BarlineLocation::Right,
                        stops,
                        EndingType::Stop,
                        Some(barline.kind),
                    );
                } else {
                    self.write_barline(BarlineLocation::Right, barline.kind, &[]);
                }
                if barline.kind == BarlineKind::RepeatBoth {
                    pending_left_repeat = true;
                }
            }
            // A bracket forced shut here (next measure opens another ending,
            // or the part ends) with no written right barline still needs its
            // <ending type="stop"> on an implicit regular barline.
            if let Some(stops) = stop_endings.take() {
                self.write_ending_barline(BarlineLocation::Right, stops, EndingType::Stop, None);
            }

            // A trailing `|:` (a forward repeat that follows content rather than
            // opening its measure) is stored in this measure but begins the
            // *next* measure's repeated section. Defer it like a `RepeatBoth`'s
            // left half so it is emitted as the next measure's LEFT barline.
            if trailing_left_repeat_pending(&measure_refs) {
                pending_left_repeat = true;
            }

            self.xml.end("measure");
        }
        self.xml.end("part");
    }
}

fn part_xml_id(part: &Part, index: usize) -> String {
    let value = part.id.value.trim();
    let fallback = format!("P{}", index + 1);
    let raw = if value.is_empty() {
        fallback.as_str()
    } else {
        value
    };
    let mut id = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if id
        .chars()
        .next()
        .is_none_or(|ch| !ch.is_ascii_alphabetic() && ch != '_')
    {
        id.insert(0, 'P');
    }
    id
}

fn part_name(part: &Part, score: &Score) -> String {
    part.name
        .as_ref()
        .or(score.metadata.title.as_ref())
        .map(|line| line.text.clone())
        .or_else(|| {
            part.voices.iter().find_map(|voice| {
                voice
                    .properties
                    .name
                    .as_ref()
                    .or(voice.properties.nm.as_ref())
                    .map(|line| line.text.clone())
            })
        })
        .unwrap_or_else(|| "Music".to_owned())
}

/// General MIDI program name for a 0-based program number, used for the
/// `<instrument-name>` of a translated `%%MIDI program`. Matches the abc2xml
/// `inst_tb` table (GM Level 1 sound set). `program` is guaranteed `<= 127` by
/// the lowering, but an out-of-range value falls back to the GM default.
fn gm_program_name(program: u8) -> &'static str {
    GM_PROGRAM_NAMES
        .get(program as usize)
        .copied()
        .unwrap_or(GM_PROGRAM_NAMES[0])
}

/// General MIDI Level 1 instrument names, indexed by 0-based program number.
/// Spelling follows abc2xml's `inst_tb` for parity with reference output.
const GM_PROGRAM_NAMES: [&str; 128] = [
    "acoustic_grand_piano",
    "bright_acoustic_piano",
    "electric_grand_piano",
    "honkytonk_piano",
    "electric_piano_1",
    "electric_piano_2",
    "harpsichord",
    "clavinet",
    "celesta",
    "glockenspiel",
    "music_box",
    "vibraphone",
    "marimba",
    "xylophone",
    "tubular_bells",
    "dulcimer",
    "drawbar_organ",
    "percussive_organ",
    "rock_organ",
    "church_organ",
    "reed_organ",
    "accordion",
    "harmonica",
    "tango_accordion",
    "acoustic_guitar_nylon",
    "acoustic_guitar_steel",
    "electric_guitar_jazz",
    "electric_guitar_clean",
    "electric_guitar_muted",
    "overdriven_guitar",
    "distortion_guitar",
    "guitar_harmonics",
    "acoustic_bass",
    "electric_bass_finger",
    "electric_bass_pick",
    "fretless_bass",
    "slap_bass_1",
    "slap_bass_2",
    "synth_bass_1",
    "synth_bass_2",
    "violin",
    "viola",
    "cello",
    "contrabass",
    "tremolo_strings",
    "pizzicato_strings",
    "orchestral_harp",
    "timpani",
    "string_ensemble_1",
    "string_ensemble_2",
    "synth_strings_1",
    "synth_strings_2",
    "choir_aahs",
    "voice_oohs",
    "synth_choir",
    "orchestra_hit",
    "trumpet",
    "trombone",
    "tuba",
    "muted_trumpet",
    "french_horn",
    "brass_section",
    "synth_brass_1",
    "synth_brass_2",
    "soprano_sax",
    "alto_sax",
    "tenor_sax",
    "baritone_sax",
    "oboe",
    "english_horn",
    "bassoon",
    "clarinet",
    "piccolo",
    "flute",
    "recorder",
    "pan_flute",
    "blown_bottle",
    "shakuhachi",
    "whistle",
    "ocarina",
    "lead_1_square",
    "lead_2_sawtooth",
    "lead_3_calliope",
    "lead_4_chiff",
    "lead_5_charang",
    "lead_6_voice",
    "lead_7_fifths",
    "lead_8_bass__lead",
    "pad_1_new_age",
    "pad_2_warm",
    "pad_3_polysynth",
    "pad_4_choir",
    "pad_5_bowed",
    "pad_6_metallic",
    "pad_7_halo",
    "pad_8_sweep",
    "fx_1_rain",
    "fx_2_soundtrack",
    "fx_3_crystal",
    "fx_4_atmosphere",
    "fx_5_brightness",
    "fx_6_goblins",
    "fx_7_echoes",
    "fx_8_scifi",
    "sitar",
    "banjo",
    "shamisen",
    "koto",
    "kalimba",
    "bagpipe",
    "fiddle",
    "shanai",
    "tinkle_bell",
    "agogo",
    "steel_drums",
    "woodblock",
    "taiko_drum",
    "melodic_tom",
    "synth_drum",
    "reverse_cymbal",
    "guitar_fret_noise",
    "breath_noise",
    "seashore",
    "bird_tweet",
    "telephone_ring",
    "helicopter",
    "applause",
    "gunshot",
];

fn part_measure_ids(part: &Part) -> Vec<MeasureId> {
    let mut ids = part
        .voices
        .iter()
        .flat_map(|voice| voice.measures.iter().map(|measure| measure.id))
        .collect::<Vec<_>>();
    ids.sort_by_key(|id| (id.index, id.number));
    ids.dedup();
    ids
}

fn part_measure_refs(part: &Part, id: MeasureId) -> Vec<&Measure> {
    part.voices
        .iter()
        .flat_map(|voice| &voice.measures)
        .filter(|measure| measure.id == id)
        .collect()
}

fn measure_sequences<'score>(
    part: &'score Part,
    id: MeasureId,
    overlay_voice_numbers: &BTreeMap<(usize, usize), String>,
) -> Vec<MeasureSequence<'score>> {
    let mut sequences = Vec::new();
    for (voice_index, voice) in part.voices.iter().enumerate() {
        let voice_number = (voice_index + 1).to_string();
        let slur_voice_key = voice.id.value.clone();
        let events = voice
            .events
            .iter()
            .filter(|event| event.measure == id)
            .filter(|event| {
                matches!(
                    event.kind,
                    TimedEventKind::Note(_)
                        | TimedEventKind::Chord(_)
                        | TimedEventKind::Rest(_)
                        | TimedEventKind::Spacer
                        | TimedEventKind::KeyChange(_)
                        | TimedEventKind::MeterChange(_)
                        | TimedEventKind::ClefChange(_)
                        | TimedEventKind::TempoChange(_)
                        | TimedEventKind::SectionLabel(_)
                )
            })
            .map(SequenceEvent::Timed)
            .collect::<Vec<_>>();
        if !events.is_empty() {
            let measure = voice.measures.iter().find(|measure| measure.id == id);
            sequences.push(MeasureSequence {
                voice_number,
                slur_voice_key: slur_voice_key.clone(),
                staff: voice.staff,
                expected_duration: measure.and_then(|measure| measure.expected_duration),
                actual_duration: measure
                    .map(|measure| measure.actual_duration)
                    .unwrap_or_else(Fraction::zero),
                events,
            });
        }
        if let Some(measure) = voice.measures.iter().find(|measure| measure.id == id) {
            for (overlay_index, overlay) in measure.overlays.iter().enumerate() {
                let overlay_events = overlay
                    .events
                    .iter()
                    .filter(|event| {
                        matches!(
                            event.kind,
                            TimelineEventKind::Note { .. }
                                | TimelineEventKind::Rest { .. }
                                | TimelineEventKind::Spacer
                        )
                    })
                    .map(SequenceEvent::Overlay)
                    .collect::<Vec<_>>();
                if overlay_events.is_empty() {
                    continue;
                }
                sequences.push(MeasureSequence {
                    voice_number: overlay_voice_numbers
                        .get(&(voice_index, overlay_index))
                        .cloned()
                        .unwrap_or_else(|| (part.voices.len() + overlay_index + 1).to_string()),
                    slur_voice_key: slur_voice_key.clone(),
                    staff: voice.staff,
                    expected_duration: Some(overlay.expected_duration),
                    actual_duration: overlay.actual_duration,
                    events: overlay_events,
                });
            }
        }
    }
    for sequence in &mut sequences {
        sequence.events.sort_by(|left, right| {
            compare_fraction(left.onset(), right.onset())
                .then_with(|| left.source_start().cmp(&right.source_start()))
        });
    }
    sequences
}

fn overlay_voice_numbers(part: &Part) -> BTreeMap<(usize, usize), String> {
    let mut numbers = BTreeMap::new();
    let mut next = part.voices.len() + 1;
    for (voice_index, voice) in part.voices.iter().enumerate() {
        for measure in &voice.measures {
            for (overlay_index, overlay) in measure.overlays.iter().enumerate() {
                if overlay.events.is_empty() {
                    continue;
                }
                numbers
                    .entry((voice_index, overlay_index))
                    .or_insert_with(|| {
                        let number = next.to_string();
                        next += 1;
                        number
                    });
            }
        }
    }
    numbers
}

fn compare_fraction(left: Fraction, right: Fraction) -> Ordering {
    (u64::from(left.numerator) * u64::from(right.denominator))
        .cmp(&(u64::from(right.numerator) * u64::from(left.denominator)))
}

fn unique_barlines(measures: &[&Measure], left: bool) -> Vec<MeasureBarline> {
    let mut barlines = measures
        .iter()
        .flat_map(|measure| measure.barlines.iter().map(|barline| (*measure, barline)))
        .filter(|(measure, barline)| {
            // A `RepeatStart` is a legitimate LEFT barline only when it leads
            // its measure (no note content precedes it). A trailing `|:` that
            // follows content marks the start of the *next* section's body and
            // is deferred (see `trailing_left_repeat_pending`), so it is never
            // emitted as this measure's left barline.
            let leading = is_leading_barline(measure, barline);
            matches!(
                (left, leading, barline.kind),
                (true, true, BarlineKind::RepeatStart)
                    | (
                        false,
                        false,
                        BarlineKind::Double
                            | BarlineKind::Final
                            | BarlineKind::Initial
                            | BarlineKind::RepeatEnd
                            | BarlineKind::RepeatBoth
                            | BarlineKind::Dotted
                            | BarlineKind::Invisible
                    )
            )
        })
        .map(|(_, barline)| *barline)
        .collect::<Vec<_>>();
    barlines.sort_by_key(|barline| (barline.span.start, barline.span.end, barline.kind as u8));
    barlines.dedup_by_key(|barline| (barline.kind, barline.span));
    barlines
}

/// A barline "leads" its measure when nothing in the measure precedes it, i.e.
/// it opens the measure rather than closing it. ABC stores a trailing `|:`
/// (one that follows note content, like a pickup `E|:` or a mid-tune `...c|:`)
/// in the *preceding* measure's barline vector; such a barline is not leading
/// and its forward-repeat belongs to the measure that begins the repeated body.
fn is_leading_barline(measure: &Measure, barline: &MeasureBarline) -> bool {
    measure.source_span.start == barline.span.start
}

/// True when this measure carries a trailing `RepeatStart` (a `|:` that follows
/// content) whose forward repeat must be deferred to the LEFT barline of the
/// next measure — the one that actually begins the repeated section.
fn trailing_left_repeat_pending(measures: &[&Measure]) -> bool {
    measures.iter().any(|measure| {
        measure.barlines.iter().any(|barline| {
            barline.kind == BarlineKind::RepeatStart && !is_leading_barline(measure, barline)
        })
    })
}

/// For each measure position, the ending-number strings whose volta bracket
/// closes at that measure's right barline. A bracket opened by `[N` (ABC 2.1
/// §4.9: "The Nth ending starts with [N and ends with one of ||, :| |] or
/// [|") closes at the first stopping right barline; a new `[M` while one is
/// open force-closes it at the previous measure. A bracket still open at part
/// end stays open so we do not synthesize a closing barline absent from the
/// source.
fn ending_stop_schedule(part: &Part, measure_ids: &[MeasureId]) -> Vec<Option<Vec<EndingDisplay>>> {
    let mut stops: Vec<Option<Vec<EndingDisplay>>> = vec![None; measure_ids.len()];
    let mut open: Option<Vec<EndingDisplay>> = None;
    for (position, measure_id) in measure_ids.iter().enumerate() {
        let measure_refs = part_measure_refs(part, *measure_id);
        if position > 0
            && open.is_some()
            && unique_barlines(&measure_refs, true)
                .iter()
                .any(|barline| barline.kind == BarlineKind::RepeatStart)
        {
            stops[position - 1] = open.take();
        }
        let starts = unique_endings(&measure_refs);
        if !starts.is_empty() {
            if let Some(open_endings) = open.take()
                && position > 0
            {
                stops[position - 1] = Some(open_endings);
            }
            open = Some(starts);
        }
        if open.is_some()
            && (unique_barlines(&measure_refs, false)
                .iter()
                .any(|barline| stops_repeat_ending_barline(barline.kind))
                || trailing_left_repeat_pending(&measure_refs))
        {
            stops[position] = open.take();
        }
    }
    // A bracket still open at the part end stays open: the source never wrote
    // a closing bar (`||`/`:|`/`|]`/`[|`), and synthesizing a stop here would
    // fabricate a barline the source does not have.
    stops
}

fn stops_repeat_ending_barline(kind: BarlineKind) -> bool {
    // ABC 2.1 §4.10: "The Nth ending starts with [N and ends with one of ||,
    // :| |] or [|" — so the thick-thin `[|` (BarlineKind::Initial) closes an
    // open ending bracket, exactly like ||/|]/:|. Only consulted when an ending
    // is already open (see the `open.is_some()` guard), so this never closes a
    // section-opening `[|` that has no ending in flight.
    matches!(
        kind,
        BarlineKind::Double
            | BarlineKind::Final
            | BarlineKind::RepeatEnd
            | BarlineKind::RepeatBoth
            | BarlineKind::Initial
    )
}

/// Render one volta bracket for MusicXML. Numeric ABC endings use their pass
/// list as the required MusicXML `number` attribute. Text labels from the
/// `["label"` extension need a numeric XML number for readers such as music21,
/// with the source label carried as element text.
fn ending_display(ending: &crate::model::RepeatEndingModel) -> EndingDisplay {
    let text = ending.endings.iter().find_map(|part| match part {
        crate::model::RepeatEndingPartModel::Text(text) => Some(text.clone()),
        _ => None,
    });
    if let Some(text) = text {
        return EndingDisplay {
            number: "33".to_owned(),
            text: Some(text),
        };
    }

    let number = ending
        .endings
        .iter()
        .map(|part| match part {
            crate::model::RepeatEndingPartModel::Single(number) => number.to_string(),
            crate::model::RepeatEndingPartModel::Range { start, end } => {
                format!("{start}-{end}")
            }
            crate::model::RepeatEndingPartModel::Text(_) => unreachable!(),
        })
        .collect::<Vec<_>>()
        .join(",");
    EndingDisplay { number, text: None }
}

fn unique_endings(measures: &[&Measure]) -> Vec<EndingDisplay> {
    let mut endings = measures
        .iter()
        .flat_map(|measure| &measure.repeat_endings)
        .map(ending_display)
        .collect::<Vec<_>>();
    endings.sort();
    endings.dedup();
    endings
}

fn unique_multiple_rest(measures: &[&Measure]) -> Option<u32> {
    measures
        .iter()
        .filter_map(|measure| measure.multiple_rest)
        .find(|count| *count > 1)
}
