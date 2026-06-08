use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span};
use crate::lower::{
    ActiveTuplet, CompletedTuplet, LoweredEvent, LoweringState, abc_tuplet_reference,
    invalid_tuplet_warning, lowered_timed_note,
};
use crate::model::{Fraction, TupletAttachment, TupletRole};
use crate::syntax::TupletSyntax;

impl LoweringState {
    pub(crate) fn record_tuplet_group(&mut self, group: &[usize]) {
        let mut completed = Vec::new();
        for tuplet in &mut self.active_tuplets {
            if tuplet.remaining == 0 {
                continue;
            }
            tuplet.groups.push(group.to_vec());
            tuplet.remaining -= 1;
            if tuplet.remaining == 0 {
                completed.push(CompletedTuplet {
                    pair_id: tuplet.pair_id,
                    span: tuplet.span,
                    actual_notes: tuplet.actual_notes,
                    normal_notes: tuplet.normal_notes,
                    groups: tuplet.groups.clone(),
                });
            }
        }
        self.active_tuplets.retain(|tuplet| tuplet.remaining > 0);
        for tuplet in completed {
            self.attach_completed_tuplet(tuplet);
        }
    }

    pub(crate) fn start_tuplet(&mut self, tuplet: &TupletSyntax) {
        let p = tuplet.p.value;
        let q = tuplet.q_value();
        let r = tuplet.r_value();
        if !(2..=9).contains(&p) || q == 0 || r == 0 {
            self.diagnostics.push(invalid_tuplet_warning(tuplet.span));
            return;
        }
        let pair_id = self.next_tuplet_id;
        self.next_tuplet_id = self.next_tuplet_id.saturating_add(1);
        self.active_tuplets.push(ActiveTuplet {
            pair_id,
            span: tuplet.span,
            remaining: r,
            actual_notes: p,
            normal_notes: q,
            multiplier: Fraction::new(q, p),
            groups: Vec::new(),
        });
    }

    pub(crate) fn finish_open_tuplets_at_boundary(&mut self) {
        for tuplet in std::mem::take(&mut self.active_tuplets) {
            if tuplet.remaining > 0 {
                self.diagnostics
                    .push(tuplet_too_few_notes_warning(tuplet.span));
            }
        }
    }

    fn attach_completed_tuplet(&mut self, tuplet: CompletedTuplet) {
        let groups_len = tuplet.groups.len();
        for (index, group) in tuplet.groups.iter().enumerate() {
            let role = if index == 0 {
                TupletRole::Start
            } else if index + 1 == groups_len {
                TupletRole::Stop
            } else {
                TupletRole::Continue
            };
            if let Some(event_index) = group
                .iter()
                .copied()
                .find(|index| lowered_timed_note(self.lowered.get(*index)).is_some())
            {
                self.attach_tuplet(event_index, &tuplet, role);
            }
        }
    }

    fn attach_tuplet(&mut self, event_index: usize, tuplet: &CompletedTuplet, role: TupletRole) {
        if let Some(LoweredEvent::Timed(timed)) = self.lowered.get_mut(event_index) {
            timed.attachments.tuplets.push(TupletAttachment {
                pair_id: tuplet.pair_id,
                actual_notes: tuplet.actual_notes,
                normal_notes: tuplet.normal_notes,
                role,
                span: tuplet.span,
            });
        }
    }
}

fn tuplet_too_few_notes_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.tuplet.too_few_notes",
        "Tuplet does not have enough following time-bearing note groups before the boundary",
        span,
    )
    .with_spec_reference(abc_tuplet_reference())
    .with_recovery_note(RecoveryNote::new(
        "The tuplet ratio was applied only to the available groups and was not carried across the boundary.",
    ))
}
