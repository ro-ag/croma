use crate::diagnostic::Span;
use crate::lower::{LoweringState, key_fifths, lowered_timed_note};
use crate::model::{Accidental, LoweredEventAtomKind};
use crate::parse::field::{AccidentalSign, KeySignature};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyAccidentalPolicy {
    pub(crate) step: char,
    pub(crate) accidental: Accidental,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MeasureAccidental {
    pub(crate) step: char,
    pub(crate) octave: i8,
    pub(crate) accidental: Accidental,
    pub(crate) span: Span,
}

pub(crate) fn key_accidental_policy(key: Option<&KeySignature>) -> Vec<KeyAccidentalPolicy> {
    let Some(key) = key else {
        return Vec::new();
    };
    let mut accidentals = key_signature_accidentals(key);
    for explicit in &key.accidentals {
        let step = explicit.note.value.to_ascii_uppercase();
        let accidental = accidental_from_field_sign(explicit.sign);
        if let Some(existing) = accidentals.iter_mut().find(|entry| entry.step == step) {
            existing.accidental = accidental;
            existing.span = explicit.span;
        } else {
            accidentals.push(KeyAccidentalPolicy {
                step,
                accidental,
                span: explicit.span,
            });
        }
    }
    accidentals
}

fn key_signature_accidentals(key: &KeySignature) -> Vec<KeyAccidentalPolicy> {
    let fifths = key_fifths(key);
    let key_span = Span::new(0, 0);
    if fifths > 0 {
        ['F', 'C', 'G', 'D', 'A', 'E', 'B']
            .into_iter()
            .take(fifths as usize)
            .map(|step| KeyAccidentalPolicy {
                step,
                accidental: Accidental::Sharp,
                span: key_span,
            })
            .collect()
    } else if fifths < 0 {
        ['B', 'E', 'A', 'D', 'G', 'C', 'F']
            .into_iter()
            .take(fifths.unsigned_abs() as usize)
            .map(|step| KeyAccidentalPolicy {
                step,
                accidental: Accidental::Flat,
                span: key_span,
            })
            .collect()
    } else {
        Vec::new()
    }
}

pub(crate) fn accidental_from_field_sign(sign: AccidentalSign) -> Accidental {
    match sign {
        AccidentalSign::DoubleFlat => Accidental::DoubleFlat,
        AccidentalSign::Flat => Accidental::Flat,
        AccidentalSign::Natural => Accidental::Natural,
        AccidentalSign::Sharp => Accidental::Sharp,
        AccidentalSign::DoubleSharp => Accidental::DoubleSharp,
    }
}

impl LoweringState {
    pub(crate) fn effective_accidental(
        &mut self,
        step: char,
        octave: i8,
        written: Option<Accidental>,
        written_span: Option<Span>,
    ) -> (Option<Accidental>, Option<Span>) {
        let step = step.to_ascii_uppercase();
        if let Some(accidental) = written {
            let span = written_span
                .unwrap_or_else(|| Span::new(self.source_span.end, self.source_span.end));
            self.set_measure_accidental(step, octave, accidental, span);
            return (Some(accidental), Some(span));
        }

        if let Some(accidental) = self
            .accidental_state
            .iter()
            .rev()
            .find(|entry| entry.step == step && entry.octave == octave)
        {
            return (Some(accidental.accidental), Some(accidental.span));
        }

        self.key_accidentals
            .iter()
            .find(|entry| entry.step == step)
            .map(|entry| (Some(entry.accidental), Some(entry.span)))
            .unwrap_or((None, None))
    }

    fn set_measure_accidental(
        &mut self,
        step: char,
        octave: i8,
        accidental: Accidental,
        span: Span,
    ) {
        if let Some(entry) = self
            .accidental_state
            .iter_mut()
            .find(|entry| entry.step == step && entry.octave == octave)
        {
            entry.accidental = accidental;
            entry.span = span;
        } else {
            self.accidental_state.push(MeasureAccidental {
                step,
                octave,
                accidental,
                span,
            });
        }
    }

    pub(crate) fn reset_measure_accidentals(&mut self) {
        self.accidental_state.clear();
    }

    /// Reset measure accidentals at a bar line, but preserve the accidental of a
    /// note whose tie is still open.
    ///
    /// Per ABC 2.1 §4.20 a tie continues the same sounding pitch across the bar
    /// line; the bar must not cancel it. Without this, the stop note would be
    /// re-resolved against the key signature, changing the tied pitch. Normal
    /// (non-tied) accidentals are still cleared as usual.
    pub(crate) fn reset_measure_accidentals_at_barline(&mut self) {
        let preserved = self.pending_tie.and_then(|tie| {
            lowered_timed_note(self.lowered.get(tie.event_index)).and_then(|timed| {
                if let LoweredEventAtomKind::Note {
                    step,
                    octave,
                    effective_accidental,
                    accidental_source,
                    ..
                } = timed.event.kind
                {
                    effective_accidental.map(|accidental| MeasureAccidental {
                        step: step.to_ascii_uppercase(),
                        octave,
                        accidental,
                        span: accidental_source.unwrap_or_else(|| {
                            Span::new(self.source_span.end, self.source_span.end)
                        }),
                    })
                } else {
                    None
                }
            })
        });
        self.accidental_state.clear();
        if let Some(accidental) = preserved {
            self.accidental_state.push(accidental);
        }
    }
}
