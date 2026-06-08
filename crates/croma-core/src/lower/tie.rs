use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span};
use crate::lower::{
    LoweredEvent, LoweringState, PendingTie, abc_slur_reference, is_note_atom, lowered_timed_note,
    note_signature,
};
use crate::model::{EventAttachments, LoweredEventAtom, TieAttachment, TieRole};
use crate::syntax::TieSyntax;

impl LoweringState {
    pub(crate) fn apply_tie(&mut self, tie: TieSyntax) {
        let Some(event_index) = self.last_note_event_index() else {
            self.diagnostics.push(unmatched_tie_warning(tie.span));
            return;
        };
        if self
            .pending_tie
            .replace(PendingTie {
                event_index,
                marker: tie,
            })
            .is_some()
        {
            self.diagnostics.push(unmatched_tie_warning(tie.span));
        }
    }

    pub(crate) fn finish_pending_tie_at_boundary(&mut self, _span: Span) {
        if let Some(tie) = self.pending_tie.take() {
            self.diagnostics
                .push(unmatched_tie_warning(tie.marker.span));
        }
    }

    pub(crate) fn finish_pending_tie_if_group_is_not_note(
        &mut self,
        events: &[(LoweredEventAtom, bool, EventAttachments)],
    ) {
        if self.pending_tie.is_none() || events.iter().any(|(event, _, _)| is_note_atom(*event)) {
            return;
        }
        self.finish_pending_tie_at_boundary(self.source_span);
    }

    pub(crate) fn finish_pending_tie_if_possible(&mut self, group: &[usize]) {
        let Some(tie) = self.pending_tie else {
            return;
        };
        let Some(next_index) = group
            .iter()
            .copied()
            .find(|index| lowered_timed_note(self.lowered.get(*index)).is_some())
        else {
            return;
        };

        let Some(previous_signature) = lowered_timed_note(self.lowered.get(tie.event_index))
            .and_then(|timed| note_signature(timed.event.kind))
        else {
            self.pending_tie = None;
            self.diagnostics
                .push(unmatched_tie_warning(tie.marker.span));
            return;
        };
        let Some(next_signature) = lowered_timed_note(self.lowered.get(next_index))
            .and_then(|timed| note_signature(timed.event.kind))
        else {
            self.pending_tie = None;
            self.diagnostics
                .push(unmatched_tie_warning(tie.marker.span));
            return;
        };

        if previous_signature == next_signature {
            let pair_id = self.next_tie_id;
            self.next_tie_id = self.next_tie_id.saturating_add(1);
            self.attach_tie(tie.event_index, pair_id, TieRole::Start, tie.marker);
            self.attach_tie(next_index, pair_id, TieRole::Stop, tie.marker);
        } else {
            self.diagnostics
                .push(unmatched_tie_warning(tie.marker.span));
        }
        self.pending_tie = None;
    }

    fn attach_tie(&mut self, event_index: usize, pair_id: u32, role: TieRole, marker: TieSyntax) {
        if let Some(LoweredEvent::Timed(timed)) = self.lowered.get_mut(event_index) {
            timed.attachments.ties.push(TieAttachment {
                pair_id,
                role,
                span: marker.span,
                dotted: marker.dotted,
            });
        }
    }
}

fn unmatched_tie_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.unmatched_tie",
        "Tie marker does not connect two matching notes",
        span,
    )
    .with_spec_reference(abc_slur_reference())
    .with_recovery_note(RecoveryNote::new(
        "The tie marker was preserved and note durations were not merged.",
    ))
}
