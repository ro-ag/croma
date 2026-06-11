use std::cmp::Ordering;

use crate::model::{
    BarlineKind, Fraction, Measure, MeasureBarline, MeasureId, Part, Score, TimedEventKind,
    TimelineEventKind,
};

use super::{BarlineLocation, EndingType, MeasureSequence, MusicXmlWriter, SequenceEvent};

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
            self.xml.end("score-part");
        }
        self.xml.end("part-list");
    }

    pub(crate) fn write_part(&mut self, part: &'score Part, part_index: usize) {
        let id = part_xml_id(part, part_index);
        self.active_key = self.score.metadata.key.clone();
        self.xml.start("part", &[("id", id.as_str())]);
        let mut pending_left_repeat = false;
        let measure_ids = part_measure_ids(part);
        let ending_stops = ending_stop_schedule(part, &measure_ids);
        for (measure_position, measure_id) in measure_ids.iter().enumerate() {
            let number = measure_id.number.to_string();
            self.xml.start("measure", &[("number", number.as_str())]);
            if measure_position == 0 {
                self.write_attributes(part);
                self.write_initial_directions(part_index == 0);
            }

            if pending_left_repeat {
                self.write_barline(BarlineLocation::Left, BarlineKind::RepeatStart, &[]);
                pending_left_repeat = false;
            }

            let measure_refs = part_measure_refs(part, *measure_id);
            let left_barlines = unique_barlines(&measure_refs, true);
            for barline in &left_barlines {
                self.write_barline(BarlineLocation::Left, barline.kind, &[]);
            }

            let endings = unique_endings(&measure_refs);
            if !endings.is_empty() {
                self.write_ending_barline(BarlineLocation::Left, &endings, EndingType::Start, None);
            }

            let sequences = measure_sequences(part, *measure_id);
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

fn measure_sequences<'score>(part: &'score Part, id: MeasureId) -> Vec<MeasureSequence<'score>> {
    let mut sequences = Vec::new();
    let base_count = part.voices.len();
    for (voice_index, voice) in part.voices.iter().enumerate() {
        let voice_number = (voice_index + 1).to_string();
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
                )
            })
            .map(SequenceEvent::Timed)
            .collect::<Vec<_>>();
        if !events.is_empty() {
            sequences.push(MeasureSequence {
                voice_number,
                staff: voice.staff,
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
                    voice_number: (base_count + overlay_index + 1).to_string(),
                    staff: voice.staff,
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
/// open force-closes it at the previous measure, and a bracket still open at
/// the part end closes on the last measure — an `<ending type="start">` must
/// never dangle.
fn ending_stop_schedule(part: &Part, measure_ids: &[MeasureId]) -> Vec<Option<Vec<String>>> {
    let mut stops: Vec<Option<Vec<String>>> = vec![None; measure_ids.len()];
    let mut open: Option<Vec<String>> = None;
    for (position, measure_id) in measure_ids.iter().enumerate() {
        let measure_refs = part_measure_refs(part, *measure_id);
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
            && unique_barlines(&measure_refs, false)
                .iter()
                .any(|barline| stops_repeat_ending_barline(barline.kind))
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
    matches!(
        kind,
        BarlineKind::Double | BarlineKind::Final | BarlineKind::RepeatEnd | BarlineKind::RepeatBoth
    )
}

/// Render one volta bracket's passes as a single MusicXML `ending-number`
/// value: a comma-separated list of its parts (e.g. `1,3`). A bracket may list
/// several passes (`|1,3`), and the MusicXML `<ending>` `number` attribute is
/// itself a comma list, so the whole bracket is ONE element — not one element
/// per pass. No space follows the comma, matching abc2xml's output.
fn ending_number_string(ending: &crate::model::RepeatEndingModel) -> String {
    ending
        .endings
        .iter()
        .map(|part| match part {
            crate::model::RepeatEndingPartModel::Single(number) => number.to_string(),
            crate::model::RepeatEndingPartModel::Range { start, end } => {
                format!("{start}-{end}")
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn unique_endings(measures: &[&Measure]) -> Vec<String> {
    let mut endings = measures
        .iter()
        .flat_map(|measure| &measure.repeat_endings)
        .map(ending_number_string)
        .collect::<Vec<_>>();
    endings.sort();
    endings.dedup();
    endings
}
