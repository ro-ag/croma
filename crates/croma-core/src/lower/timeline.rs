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
    single_voice: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> VoiceTimeline {
    let mut builder = VoiceTimelineBuilder::new(voice.id.clone(), meter_duration, single_voice);
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
    /// True iff the whole tune has exactly one voice. Bar-line-only "phantom"
    /// measures are only coalesced in single-voice music; in multi-voice tunes
    /// a bar-line-only measure is a legitimate tacet bar that must be kept so
    /// voices stay measure-aligned.
    single_voice: bool,
}

impl VoiceTimelineBuilder {
    fn new(voice_id: VoiceId, meter_duration: Option<Fraction>, single_voice: bool) -> Self {
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
            single_voice,
        }
    }

    /// Bar-line-only measures may be coalesced only when the tune is
    /// single-voice and this voice carries no overlays (`&`): overlays imply a
    /// multi-layer measure that abc2xml preserves.
    fn may_coalesce_barline_only(&self) -> bool {
        self.single_voice && self.overlay_count == 0
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
            LoweredEvent::KeyChange(key) => {
                let onset = self.onset;
                let span = key.source_span;
                self.current_measure_mut().events.push(VoiceTimedEvent {
                    onset,
                    duration: Fraction::zero(),
                    span,
                    line_index: 0,
                    source_order: 0,
                    alignable: false,
                    kind: TimelineEventKind::KeyChange(key),
                    lyrics: Vec::new(),
                    symbols: Vec::new(),
                    attachments: EventAttachments::default(),
                });
            }
            LoweredEvent::MeterChange(meter) => {
                let onset = self.onset;
                let span = meter.source_span;
                self.current_measure_mut().events.push(VoiceTimedEvent {
                    onset,
                    duration: Fraction::zero(),
                    span,
                    line_index: 0,
                    source_order: 0,
                    alignable: false,
                    kind: TimelineEventKind::MeterChange(meter),
                    lyrics: Vec::new(),
                    symbols: Vec::new(),
                    attachments: EventAttachments::default(),
                });
            }
            LoweredEvent::TempoChange(tempo) => {
                let onset = self.onset;
                let span = tempo.source_span;
                self.current_measure_mut().events.push(VoiceTimedEvent {
                    onset,
                    duration: Fraction::zero(),
                    span,
                    line_index: 0,
                    source_order: 0,
                    alignable: false,
                    kind: TimelineEventKind::TempoChange(tempo),
                    lyrics: Vec::new(),
                    symbols: Vec::new(),
                    attachments: EventAttachments::default(),
                });
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
            && self.measures.last().is_some_and(|measure| {
                // A brand-new measure (no events at all) always accepts a leading
                // bar line without opening a phantom. A measure that already holds
                // *only bar-line tokens* is a phantom produced by a run of bar
                // lines (`| |`, `|]|`, `]|`): a further bar line merges into the
                // same boundary (ABC 2.1 §4.8) instead of opening a new measure —
                // but only past the first measure, since abc2xml preserves a single
                // leading empty (pickup) measure. Spacers (`y`, annotation-only
                // bars) and overlays are *not* treated as collapsible: abc2xml
                // keeps those measures, so they must not be coalesced here.
                if is_empty_measure(measure) {
                    return true;
                }
                self.may_coalesce_barline_only()
                    && self.measure_index > 0
                    && is_barline_only_measure(measure)
            })
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
        // Pop a trailing measure that carries only bar-line tokens. Per ABC 2.1
        // §4.8 a run of adjacent or split bar lines (`| |`, `|]|`, `]|`) is a
        // single boundary, not a measure of music, so a trailing bar-line-only
        // measure is a phantom and must not survive. The first bar line of the
        // run already sits on the preceding real measure, so its right bar line
        // is not lost when the phantom is popped. Spacer-only or overlay-bearing
        // trailing measures are kept (abc2xml keeps them). Never pop the only
        // measure.
        let may_coalesce_barline_only = self.may_coalesce_barline_only();
        while self.measures.last().is_some_and(|measure| {
            is_empty_measure(measure)
                || (may_coalesce_barline_only && is_barline_only_measure(measure))
        }) && self.measures.len() > 1
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

/// A measure with no events and no overlays at all (the original phantom case:
/// a fresh measure opened after a trailing bar line with nothing following).
fn is_empty_measure(measure: &VoiceMeasureTimeline) -> bool {
    // Zero-duration key/meter change events do not make a measure real: in
    // `...| [K:C] |: ...` the change belongs to the measure the `|:` opens,
    // and the pending one must keep accepting that leading barline.
    measure.overlays.is_empty()
        && measure.events.iter().all(|event| {
            matches!(
                event.kind,
                TimelineEventKind::KeyChange(_)
                    | TimelineEventKind::MeterChange(_)
                    | TimelineEventKind::TempoChange(_)
            )
        })
}

/// A measure is *bar-line-only* when it carries no overlay and every event it
/// holds is a bar line — i.e. it has at least one `Barline` and nothing else.
/// Such a measure is a phantom produced by a run of bar lines (e.g. a trailing
/// `| |` / `|]|` / `]|`, or consecutive interior bar lines) and is not a measure
/// of music. A real rest (`z`/`x`/`Z`) is a `Rest` event, a `y` is a `Spacer`,
/// and an annotation/overlay carries its own events, so none of those count as
/// bar-line-only — abc2xml preserves those measures.
fn is_barline_only_measure(measure: &VoiceMeasureTimeline) -> bool {
    measure.overlays.is_empty()
        && !measure.events.is_empty()
        && measure.events.iter().all(|event| {
            // Zero-duration key/meter change events do not make a measure
            // real: `| [K:C] |:` must still merge into one boundary rather
            // than opening a phantom empty measure the reference lacks.
            matches!(
                event.kind,
                TimelineEventKind::Barline { .. }
                    | TimelineEventKind::KeyChange(_)
                    | TimelineEventKind::MeterChange(_)
            )
        })
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
        LoweredEventAtomKind::Rest {
            visibility,
            multiple_rest,
            ..
        } => TimelineEventKind::Rest {
            visibility,
            multiple_rest,
        },
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
