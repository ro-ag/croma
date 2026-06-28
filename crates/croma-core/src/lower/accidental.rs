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
    /// `true` when this entry was re-inserted by
    /// `reset_measure_accidentals_at_barline` solely on behalf of a pending
    /// tie. If that tie is later dropped without finding a stop note, the
    /// entry is removed again (see `drop_pending_tie_carry`); a matched tie
    /// consumes the entry after the stop note has resolved.
    pub(crate) from_pending_tie: bool,
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

pub(crate) fn key_accidental_policy_from_model(
    key: &crate::model::KeySignatureModel,
) -> Vec<KeyAccidentalPolicy> {
    let key_span = key.source_span;
    let mut accidentals: Vec<KeyAccidentalPolicy> = if key.fifths > 0 {
        ['F', 'C', 'G', 'D', 'A', 'E', 'B']
            .into_iter()
            .take(key.fifths as usize)
            .map(|step| KeyAccidentalPolicy {
                step,
                accidental: Accidental::Sharp,
                span: key_span,
            })
            .collect()
    } else if key.fifths < 0 {
        ['B', 'E', 'A', 'D', 'G', 'C', 'F']
            .into_iter()
            .take(key.fifths.unsigned_abs() as usize)
            .map(|step| KeyAccidentalPolicy {
                step,
                accidental: Accidental::Flat,
                span: key_span,
            })
            .collect()
    } else {
        Vec::new()
    };
    for explicit in &key.explicit_accidentals {
        if let Some(existing) = accidentals
            .iter_mut()
            .find(|entry| entry.step == explicit.step)
        {
            existing.accidental = explicit.accidental;
            existing.span = explicit.source_span;
        } else {
            accidentals.push(KeyAccidentalPolicy {
                step: explicit.step,
                accidental: explicit.accidental,
                span: explicit.source_span,
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
            // A fresh written accidental re-legitimizes the entry: it must
            // survive even if a pending tie on the same pitch is later dropped.
            entry.from_pending_tie = false;
        } else {
            self.accidental_state.push(MeasureAccidental {
                step,
                octave,
                accidental,
                span,
                from_pending_tie: false,
            });
        }
    }

    /// Reset measure accidentals at a bar line, but preserve the accidental of a
    /// note whose tie is still open.
    ///
    /// Per ABC 2.1 §4.20 a tie continues the same sounding pitch across the bar
    /// line; the bar must not cancel it. Without this, the stop note would be
    /// re-resolved against the key signature, changing the tied pitch. Normal
    /// (non-tied) accidentals are still cleared as usual.
    ///
    /// The carry must be preserved eagerly here and undone later if the tie is
    /// dropped (rather than added only once a tie matches) because the stop
    /// note's accidental is resolved via `effective_accidental` when the note
    /// is lowered, BEFORE `finish_pending_tie_if_possible` runs — so the carry
    /// must already exist at the barline and a drop can only be retroactive.
    pub(crate) fn reset_measure_accidentals_at_barline(&mut self) {
        let preserved: Vec<MeasureAccidental> = self
            .pending_ties
            .iter()
            .filter_map(|tie| {
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
                            from_pending_tie: true,
                        })
                    } else {
                        None
                    }
                })
            })
            .collect();
        self.accidental_state.clear();
        self.accidental_state.extend(preserved);
    }

    /// Preserve the effective pitch of a note that is about to start a pending
    /// tie. This matters for chains such as `^G-|G-G`: the first stop note gets
    /// its sharp from a synthetic cross-bar carry, then immediately starts a new
    /// tie before any later same-pitch note can re-resolve naturally.
    pub(crate) fn preserve_pending_tie_carry_from_event(&mut self, event_index: usize) {
        let Some((step, octave, accidental, span)) =
            lowered_timed_note(self.lowered.get(event_index)).and_then(|timed| {
                if let LoweredEventAtomKind::Note {
                    step,
                    octave,
                    effective_accidental,
                    accidental_source,
                    ..
                } = timed.event.kind
                {
                    effective_accidental.map(|accidental| {
                        (
                            step.to_ascii_uppercase(),
                            octave,
                            accidental,
                            accidental_source.unwrap_or_else(|| {
                                Span::new(self.source_span.end, self.source_span.end)
                            }),
                        )
                    })
                } else {
                    None
                }
            })
        else {
            return;
        };

        if let Some(entry) = self
            .accidental_state
            .iter_mut()
            .find(|entry| entry.step == step && entry.octave == octave)
        {
            if entry.from_pending_tie {
                entry.accidental = accidental;
                entry.span = span;
            }
            return;
        }

        self.accidental_state.push(MeasureAccidental {
            step,
            octave,
            accidental,
            span,
            from_pending_tie: true,
        });
    }

    /// Preserve open-tie pitches across a mid-measure key change: per ABC 2.1
    /// §4.20 a tie continues the same sounding pitch, so the stop note must
    /// not re-resolve under the NEW key. Unlike the barline rule above (which
    /// carries only explicit/ledger accidentals — key-derived pitches
    /// re-resolve identically across a barline), a key change also re-pitches
    /// key-derived notes, so the resolved alter under the OLD key — effective
    /// accidental, else the old key policy's accidental, else an explicit
    /// natural — is materialized into the measure ledger before the swap.
    pub(crate) fn preserve_tie_pitches_for_key_change(&mut self) {
        let preserved: Vec<MeasureAccidental> = self
            .pending_ties
            .iter()
            .filter_map(|tie| {
                lowered_timed_note(self.lowered.get(tie.event_index)).and_then(|timed| {
                    if let LoweredEventAtomKind::Note {
                        step,
                        octave,
                        effective_accidental,
                        accidental_source,
                        ..
                    } = timed.event.kind
                    {
                        let step = step.to_ascii_uppercase();
                        let resolved = effective_accidental.unwrap_or_else(|| {
                            self.key_accidentals
                                .iter()
                                .find(|entry| entry.step == step)
                                .map(|entry| entry.accidental)
                                .unwrap_or(Accidental::Natural)
                        });
                        Some(MeasureAccidental {
                            step,
                            octave,
                            accidental: resolved,
                            span: accidental_source.unwrap_or_else(|| {
                                Span::new(self.source_span.end, self.source_span.end)
                            }),
                            from_pending_tie: true,
                        })
                    } else {
                        None
                    }
                })
            })
            .collect();
        self.accidental_state.extend(preserved);
    }

    /// Undo the accidental carry that `reset_measure_accidentals_at_barline`
    /// preserved on behalf of a pending tie that was dropped without finding a
    /// stop note. Entries that come from a written accidental in the current
    /// measure (`from_pending_tie == false`) are left untouched.
    pub(crate) fn drop_pending_tie_carry(&mut self, signature: (char, i8)) {
        // Comparing `entry.step == signature.0` raw is sound: note signatures
        // are always uppercase (`LoweredEventAtomKind::Note` constructors
        // uppercase the step), as are `MeasureAccidental` entries.
        self.accidental_state.retain(|entry| {
            !(entry.from_pending_tie && entry.step == signature.0 && entry.octave == signature.1)
        });
    }

    /// Consume the synthetic carry for a matched cross-bar tie. The carry is
    /// needed before matching so the stop note resolves to the tied pitch, but
    /// it must not become an ordinary measure accidental for later same-pitch
    /// notes in the stop note's bar. A freshly written accidental on the stop
    /// note has already re-legitimized the entry (`from_pending_tie == false`)
    /// and is intentionally preserved.
    pub(crate) fn consume_pending_tie_carry(&mut self, signature: (char, i8)) {
        self.accidental_state.retain(|entry| {
            !(entry.from_pending_tie && entry.step == signature.0 && entry.octave == signature.1)
        });
    }
}
