//! Voice timeline construction: turning a lowered event stream into measure
//! timelines, including measure segmentation and overlay handling.

use crate::diagnostic::{Diagnostic, Span};
use crate::lower::voice::{LoweredEvent, LoweredTimedEvent, LoweringState};
use crate::lower::{
    extend_span, overlay_incomplete_measure_warning, overlay_overfull_measure_warning,
};
use crate::model::{
    BarlineKind, Event, EventAttachments, Fraction, LoweredEventAtom, LoweredEventAtomKind,
    OverlaySegment, RepeatEndingPartModel, TimelineEventKind, VoiceId, VoiceMeasureTimeline,
    VoiceTimedEvent, VoiceTimeline,
};
use crate::syntax::VariantEndingPart;

pub(crate) fn build_voice_timeline(
    voice: LoweringState,
    meter_duration: Option<Fraction>,
    diagnostics: &mut Vec<Diagnostic>,
) -> VoiceTimeline {
    let mut builder = VoiceTimelineBuilder::new(voice.id.clone(), meter_duration);
    for event in voice.lowered {
        builder.push(event, diagnostics);
    }
    let measures = builder.finish(diagnostics);
    VoiceTimeline {
        id: voice.id,
        properties: voice.properties,
        measures,
        source_span: voice.source_span,
    }
}

struct VoiceTimelineBuilder {
    voice_id: VoiceId,
    meter_duration: Option<Fraction>,
    measures: Vec<VoiceMeasureTimeline>,
    measure_index: u32,
    onset: Fraction,
    last_group_onset: Fraction,
    active_overlay: Option<OverlayBuilder>,
    overlay_count: u32,
}

impl VoiceTimelineBuilder {
    fn new(voice_id: VoiceId, meter_duration: Option<Fraction>) -> Self {
        Self {
            voice_id,
            meter_duration,
            measures: vec![VoiceMeasureTimeline {
                index: 0,
                span: Span::new(0, 0),
                events: Vec::new(),
                overlays: Vec::new(),
            }],
            measure_index: 0,
            onset: Fraction::zero(),
            last_group_onset: Fraction::zero(),
            active_overlay: None,
            overlay_count: 0,
        }
    }

    fn push(&mut self, event: LoweredEvent, diagnostics: &mut Vec<Diagnostic>) {
        match event {
            LoweredEvent::Timed(timed) => self.push_timed(timed),
            LoweredEvent::Untimed(Event::Barline { kind, span }) => {
                self.finish_overlay(diagnostics);
                let starts_current_measure = self.is_empty_measure_start()
                    && (starts_measure_barline(kind)
                        || (self.is_first_measure_start()
                            && starts_first_body_measure_barline(kind)))
                    || self.is_first_measure_combined_repeat_start(kind, span);
                self.push_barline(kind, span);
                if starts_current_measure {
                    return;
                }
                self.start_measure_after_barline(span);
            }
            LoweredEvent::Untimed(Event::Spacer { span }) => {
                let onset = self.onset;
                self.current_measure_mut().events.push(VoiceTimedEvent {
                    onset,
                    duration: Fraction::zero(),
                    span,
                    line_index: 0,
                    source_order: 0,
                    alignable: false,
                    kind: TimelineEventKind::Spacer,
                    lyrics: Vec::new(),
                    symbols: Vec::new(),
                    attachments: EventAttachments::default(),
                });
            }
            LoweredEvent::Untimed(Event::Note { .. } | Event::Rest { .. }) => {}
            LoweredEvent::Overlay(overlay) => {
                self.finish_overlay(diagnostics);
                let expected_duration = if self.onset == Fraction::zero() {
                    self.meter_duration.unwrap_or_else(Fraction::zero)
                } else {
                    self.onset
                };
                let overlay_id = VoiceId {
                    value: format!("{}.overlay{}", self.voice_id.value, self.overlay_count + 1),
                    span: overlay.span,
                };
                self.overlay_count = self.overlay_count.saturating_add(1);
                self.active_overlay = Some(OverlayBuilder {
                    id: overlay_id,
                    start_span: overlay.span,
                    span: overlay.span,
                    measure_index: self.measure_index,
                    expected_duration,
                    actual_duration: Fraction::zero(),
                    last_group_onset: Fraction::zero(),
                    events: Vec::new(),
                });
            }
            LoweredEvent::VariantEnding(ending) => {
                let onset = self.onset;
                let span = ending.span;
                self.current_measure_mut().events.push(VoiceTimedEvent {
                    onset,
                    duration: Fraction::zero(),
                    span,
                    line_index: 0,
                    source_order: 0,
                    alignable: false,
                    kind: TimelineEventKind::VariantEnding {
                        endings: repeat_ending_parts_model(&ending.endings),
                    },
                    attachments: EventAttachments::default(),
                    lyrics: Vec::new(),
                    symbols: Vec::new(),
                });
                self.current_measure_mut().span =
                    extend_span(self.current_measure_mut().span, span);
            }
        }
    }

    fn push_barline(&mut self, kind: BarlineKind, span: Span) {
        let onset = self.onset;
        let measure = self.current_measure_mut();
        measure.events.push(VoiceTimedEvent {
            onset,
            duration: Fraction::zero(),
            span,
            line_index: 0,
            source_order: 0,
            alignable: false,
            kind: TimelineEventKind::Barline { kind },
            lyrics: Vec::new(),
            symbols: Vec::new(),
            attachments: EventAttachments::default(),
        });
        measure.span = extend_span(measure.span, span);
    }

    fn is_empty_measure_start(&self) -> bool {
        self.onset == Fraction::zero()
            && self.active_overlay.is_none()
            && self
                .measures
                .last()
                .is_some_and(|measure| measure.events.is_empty() && measure.overlays.is_empty())
    }

    fn is_first_measure_start(&self) -> bool {
        self.measure_index == 0 && self.measures.len() == 1
    }

    fn is_first_measure_combined_repeat_start(&self, kind: BarlineKind, span: Span) -> bool {
        kind == BarlineKind::RepeatStart
            && self.onset == Fraction::zero()
            && self.active_overlay.is_none()
            && self.is_first_measure_start()
            && self.measures.last().is_some_and(|measure| {
                !measure.events.is_empty()
                    && measure.overlays.is_empty()
                    && measure.events.iter().all(|event| {
                        event.duration == Fraction::zero()
                            && event.span == span
                            && matches!(
                                event.kind,
                                TimelineEventKind::Barline {
                                    kind: BarlineKind::Double | BarlineKind::Initial
                                }
                            )
                    })
            })
    }

    fn start_measure_after_barline(&mut self, span: Span) {
        self.measure_index = self.measure_index.saturating_add(1);
        self.onset = Fraction::zero();
        self.last_group_onset = Fraction::zero();
        self.measures.push(VoiceMeasureTimeline {
            index: self.measure_index,
            span: Span::new(span.end, span.end),
            events: Vec::new(),
            overlays: Vec::new(),
        });
    }

    fn push_timed(&mut self, timed: LoweredTimedEvent) {
        let span = timed_span(timed.event);
        let chord_member = matches!(
            timed.event.kind,
            LoweredEventAtomKind::Note { chord: true, .. }
        );
        let onset = if let Some(overlay) = &self.active_overlay {
            if chord_member {
                overlay.last_group_onset
            } else {
                overlay.actual_duration
            }
        } else if chord_member {
            self.last_group_onset
        } else {
            self.onset
        };
        let event = VoiceTimedEvent {
            onset,
            duration: timed.event.duration,
            span,
            line_index: timed.line_index,
            source_order: timed.source_order,
            alignable: timed.alignable
                && matches!(timed.event.kind, LoweredEventAtomKind::Note { .. }),
            kind: timeline_event_kind(timed.event.kind),
            attachments: timed.attachments,
            lyrics: Vec::new(),
            symbols: Vec::new(),
        };
        if let Some(overlay) = &mut self.active_overlay {
            if !chord_member {
                overlay.last_group_onset = event.onset;
                overlay.actual_duration = overlay.actual_duration.checked_add(timed.event.duration);
            }
            overlay.span = extend_span(overlay.span, span);
            overlay.events.push(event);
        } else {
            if !chord_member {
                self.last_group_onset = event.onset;
                self.onset = self.onset.checked_add(timed.event.duration);
            }
            self.current_measure_mut().span = extend_span(self.current_measure_mut().span, span);
            self.current_measure_mut().events.push(event);
        }
    }

    fn finish(mut self, diagnostics: &mut Vec<Diagnostic>) -> Vec<VoiceMeasureTimeline> {
        self.finish_overlay(diagnostics);
        while self
            .measures
            .last()
            .is_some_and(|measure| measure.events.is_empty() && measure.overlays.is_empty())
            && self.measures.len() > 1
        {
            self.measures.pop();
        }
        self.measures
    }

    fn finish_overlay(&mut self, diagnostics: &mut Vec<Diagnostic>) {
        let Some(overlay) = self.active_overlay.take() else {
            return;
        };
        if overlay.actual_duration.less_than(overlay.expected_duration) {
            diagnostics.push(overlay_incomplete_measure_warning(
                overlay.start_span,
                overlay.actual_duration,
                overlay.expected_duration,
            ));
        } else if overlay.expected_duration.less_than(overlay.actual_duration) {
            diagnostics.push(overlay_overfull_measure_warning(
                overlay.start_span,
                overlay.actual_duration,
                overlay.expected_duration,
            ));
        }
        self.current_measure_mut().overlays.push(OverlaySegment {
            id: overlay.id,
            span: overlay.span,
            measure_index: overlay.measure_index,
            expected_duration: overlay.expected_duration,
            actual_duration: overlay.actual_duration,
            events: overlay.events,
        });
    }

    fn current_measure_mut(&mut self) -> &mut VoiceMeasureTimeline {
        self.measures
            .last_mut()
            .expect("timeline builder always has a current measure")
    }
}

fn starts_measure_barline(kind: BarlineKind) -> bool {
    matches!(
        kind,
        BarlineKind::Regular | BarlineKind::Initial | BarlineKind::RepeatStart
    )
}

fn starts_first_body_measure_barline(kind: BarlineKind) -> bool {
    matches!(
        kind,
        BarlineKind::Double | BarlineKind::Final | BarlineKind::Liberal
    )
}

struct OverlayBuilder {
    id: VoiceId,
    start_span: Span,
    span: Span,
    measure_index: u32,
    expected_duration: Fraction,
    actual_duration: Fraction,
    last_group_onset: Fraction,
    events: Vec<VoiceTimedEvent>,
}

fn timed_span(event: LoweredEventAtom) -> Span {
    match event.kind {
        LoweredEventAtomKind::Note { span, .. } | LoweredEventAtomKind::Rest { span, .. } => span,
    }
}

fn timeline_event_kind(kind: LoweredEventAtomKind) -> TimelineEventKind {
    match kind {
        LoweredEventAtomKind::Note {
            step,
            octave,
            accidental,
            effective_accidental,
            accidental_source,
            chord,
            ..
        } => TimelineEventKind::Note {
            step,
            octave,
            accidental,
            effective_accidental,
            accidental_source,
            chord,
        },
        LoweredEventAtomKind::Rest { visibility, .. } => TimelineEventKind::Rest { visibility },
    }
}

fn repeat_ending_parts_model(parts: &[VariantEndingPart]) -> Vec<RepeatEndingPartModel> {
    parts
        .iter()
        .map(|part| match *part {
            VariantEndingPart::Single(number) => RepeatEndingPartModel::Single(number.value),
            VariantEndingPart::Range { start, end, .. } => RepeatEndingPartModel::Range {
                start: start.value,
                end: end.value,
            },
        })
        .collect()
}
