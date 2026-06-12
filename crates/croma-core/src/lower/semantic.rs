//! Timeline -> semantic-model lowering of voices, measures, and events.

use crate::model::{
    Accidental, AccidentalMark, ChordEvent, ChordMemberEvent, Fraction, Measure, MeasureBarline,
    MeasureId, NoteEvent, Pitch, RepeatEndingModel, RestEvent, StaffId, TimedEvent, TimedEventKind,
    TimelineEventKind, Voice, VoiceMeasureTimeline, VoiceTimedEvent, VoiceTimeline,
};
use crate::parse::field::FieldState;

use crate::lower::{extend_span, meter_duration};

pub(crate) fn semantic_voice_from_timeline(
    voice: &VoiceTimeline,
    staff_id: StaffId,
    field_state: &FieldState,
) -> Voice {
    let expected_duration = field_state
        .meter
        .as_ref()
        .and_then(|meter| meter_duration(&meter.value.kind));
    let mut events = Vec::new();
    let measures = voice
        .measures
        .iter()
        .map(|measure| {
            let measure_id = MeasureId {
                index: measure.index,
                number: measure.index.saturating_add(1),
            };
            events.extend(semantic_events_for_measure(measure, measure_id));
            semantic_measure_from_timeline(measure, measure_id, expected_duration)
        })
        .collect();

    Voice {
        id: voice.id.clone(),
        staff: staff_id,
        properties: voice.properties.clone(),
        measures,
        events,
        source_span: voice.source_span,
    }
}

fn semantic_measure_from_timeline(
    measure: &VoiceMeasureTimeline,
    id: MeasureId,
    expected_duration: Option<Fraction>,
) -> Measure {
    let actual_duration = measure_actual_duration(measure);
    let complete = expected_duration
        .map(|expected| expected == actual_duration)
        .unwrap_or(true);
    let pickup = id.index == 0
        && expected_duration.is_some_and(|expected| {
            actual_duration != Fraction::zero() && actual_duration.less_than(expected)
        });
    let multiple_rest = measure.events.iter().find_map(|event| match event.kind {
        TimelineEventKind::Rest {
            multiple_rest: Some(count),
            ..
        } => Some(count),
        _ => None,
    });
    let barlines = measure
        .events
        .iter()
        .filter_map(|event| match event.kind {
            TimelineEventKind::Barline { kind } => Some(MeasureBarline {
                kind,
                span: event.span,
            }),
            _ => None,
        })
        .collect();
    let repeat_endings = measure
        .events
        .iter()
        .filter_map(|event| match &event.kind {
            TimelineEventKind::VariantEnding { endings } => Some(RepeatEndingModel {
                span: event.span,
                endings: endings.clone(),
            }),
            _ => None,
        })
        .collect();

    Measure {
        id,
        source_span: measure.span,
        expected_duration,
        actual_duration,
        multiple_rest,
        pickup,
        complete,
        barlines,
        repeat_endings,
        overlays: measure.overlays.clone(),
    }
}

fn semantic_events_for_measure(
    measure: &VoiceMeasureTimeline,
    measure_id: MeasureId,
) -> Vec<TimedEvent> {
    let mut events = Vec::new();
    let mut index = 0;
    while index < measure.events.len() {
        let event = &measure.events[index];
        if let TimelineEventKind::Note { .. } = event.kind {
            let mut group_end = index + 1;
            while group_end < measure.events.len()
                && same_chord_group(event, &measure.events[group_end])
            {
                group_end += 1;
            }
            if group_end - index > 1 {
                events.push(chord_event_from_timeline(
                    &measure.events[index..group_end],
                    measure_id,
                ));
            } else {
                events.push(note_event_from_timeline(event, measure_id));
            }
            index = group_end;
            continue;
        }

        events.push(non_note_event_from_timeline(event, measure_id));
        index += 1;
    }
    events
}

fn same_chord_group(first: &VoiceTimedEvent, next: &VoiceTimedEvent) -> bool {
    first.source_order == next.source_order
        && first.onset == next.onset
        && matches!(next.kind, TimelineEventKind::Note { chord: true, .. })
}

fn chord_event_from_timeline(events: &[VoiceTimedEvent], measure_id: MeasureId) -> TimedEvent {
    let mut span = events[0].span;
    let members = events
        .iter()
        .map(|event| {
            span = extend_span(span, event.span);
            ChordMemberEvent {
                pitch: pitch_from_timeline(event),
                duration: event.duration,
                written_accidental: written_accidental_from_timeline(event),
                source_span: event.span,
                attachments: event.attachments.clone(),
            }
        })
        .collect();
    TimedEvent {
        measure: measure_id,
        onset: events[0].onset,
        duration: events[0].duration,
        source: span,
        kind: TimedEventKind::Chord(ChordEvent {
            members,
            source_span: span,
        }),
        attachments: events[0].attachments.clone(),
    }
}

fn note_event_from_timeline(event: &VoiceTimedEvent, measure_id: MeasureId) -> TimedEvent {
    TimedEvent {
        measure: measure_id,
        onset: event.onset,
        duration: event.duration,
        source: event.span,
        kind: TimedEventKind::Note(NoteEvent {
            pitch: pitch_from_timeline(event),
            written_accidental: written_accidental_from_timeline(event),
            chord_member: matches!(event.kind, TimelineEventKind::Note { chord: true, .. }),
        }),
        attachments: event.attachments.clone(),
    }
}

fn non_note_event_from_timeline(event: &VoiceTimedEvent, measure_id: MeasureId) -> TimedEvent {
    let kind = match &event.kind {
        TimelineEventKind::Rest { visibility, .. } => TimedEventKind::Rest(RestEvent {
            visibility: *visibility,
        }),
        TimelineEventKind::Spacer => TimedEventKind::Spacer,
        TimelineEventKind::Barline { kind } => TimedEventKind::Barline(MeasureBarline {
            kind: *kind,
            span: event.span,
        }),
        TimelineEventKind::VariantEnding { endings } => {
            TimedEventKind::RepeatEnding(RepeatEndingModel {
                span: event.span,
                endings: endings.clone(),
            })
        }
        TimelineEventKind::KeyChange(key) => TimedEventKind::KeyChange(key.clone()),
        TimelineEventKind::MeterChange(meter) => TimedEventKind::MeterChange(meter.clone()),
        TimelineEventKind::TempoChange(tempo) => TimedEventKind::TempoChange(tempo.clone()),
        TimelineEventKind::Note { .. } => TimedEventKind::Spacer,
    };
    TimedEvent {
        measure: measure_id,
        onset: event.onset,
        duration: event.duration,
        source: event.span,
        kind,
        attachments: event.attachments.clone(),
    }
}

fn pitch_from_timeline(event: &VoiceTimedEvent) -> Pitch {
    match event.kind {
        TimelineEventKind::Note {
            step,
            octave,
            effective_accidental,
            ..
        } => Pitch {
            step,
            alter: effective_accidental.map(Accidental::alter).unwrap_or(0),
            octave,
            spelling_source: event.span,
        },
        _ => Pitch {
            step: 'C',
            alter: 0,
            octave: 4,
            spelling_source: event.span,
        },
    }
}

fn written_accidental_from_timeline(event: &VoiceTimedEvent) -> Option<AccidentalMark> {
    match event.kind {
        TimelineEventKind::Note {
            accidental,
            accidental_source,
            ..
        } => accidental.map(|kind| AccidentalMark {
            kind,
            explicit: true,
            courtesy: false,
            source: accidental_source.unwrap_or(event.span),
        }),
        _ => None,
    }
}

fn measure_actual_duration(measure: &VoiceMeasureTimeline) -> Fraction {
    let mut actual = Fraction::zero();
    for event in &measure.events {
        if matches!(
            event.kind,
            TimelineEventKind::Note { .. } | TimelineEventKind::Rest { .. }
        ) {
            let end = event.onset.checked_add(event.duration);
            if actual.less_than(end) {
                actual = end;
            }
        }
    }
    actual
}
