use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span};
use crate::lower::{
    LoweredEvent, LoweringState, PendingTie, abc_slur_reference, is_note_atom, lowered_timed_note,
    note_signature,
};
use crate::model::{EventAttachments, LoweredEventAtom, TieAttachment, TieRole};
use crate::syntax::TieSyntax;

impl LoweringState {
    /// Handle a standalone tie marker (`-`) that follows a note or a whole
    /// chord, e.g. `C-` or `[CE]-`. Every note in the most recent time group
    /// becomes a pending tie start. For a single note that is the one note; for
    /// a chord it is every member, so a whole-chord tie connects each matching
    /// member (ABC 2.1 §4.11).
    pub(crate) fn apply_tie(&mut self, tie: TieSyntax) {
        let Some(group) = self.time_groups.last().cloned() else {
            self.diagnostics.push(unmatched_tie_warning(tie.span));
            return;
        };
        let indices: Vec<usize> = group
            .iter()
            .copied()
            .filter(|index| lowered_timed_note(self.lowered.get(*index)).is_some())
            .collect();
        if indices.is_empty() {
            self.diagnostics.push(unmatched_tie_warning(tie.span));
            return;
        }
        for index in indices {
            self.register_pending_tie(index, tie);
        }
    }

    /// Register a tie start for a specific lowered event index (e.g. an
    /// individual chord member carrying an internal tie marker `[DA-]`).
    pub(crate) fn register_pending_tie(&mut self, event_index: usize, marker: TieSyntax) {
        let Some(signature) = lowered_timed_note(self.lowered.get(event_index))
            .and_then(|timed| note_signature(timed.event.kind))
        else {
            self.diagnostics.push(unmatched_tie_warning(marker.span));
            return;
        };
        self.pending_ties.push(PendingTie {
            event_index,
            signature,
            marker,
        });
    }

    pub(crate) fn finish_pending_tie_at_boundary(&mut self, _span: Span) {
        for tie in std::mem::take(&mut self.pending_ties) {
            self.drop_pending_tie_carry(tie.signature);
            self.diagnostics
                .push(unmatched_tie_warning(tie.marker.span));
        }
    }

    pub(crate) fn finish_pending_tie_if_group_is_not_note(
        &mut self,
        events: &[(LoweredEventAtom, bool, EventAttachments)],
    ) {
        if self.pending_ties.is_empty() || events.iter().any(|(event, _, _)| is_note_atom(*event)) {
            return;
        }
        self.finish_pending_tie_at_boundary(self.source_span);
    }

    pub(crate) fn finish_pending_tie_if_possible(&mut self, group: &[usize]) {
        if self.pending_ties.is_empty() {
            return;
        }
        // Only resolve once the next group actually contains notes; otherwise
        // leave the pending ties in place to match a later note group.
        if !group
            .iter()
            .any(|index| lowered_timed_note(self.lowered.get(*index)).is_some())
        {
            return;
        }

        // Signatures of the notes in the next group, paired with their index.
        let next_notes: Vec<(usize, (char, i8))> = group
            .iter()
            .copied()
            .filter_map(|index| {
                lowered_timed_note(self.lowered.get(index))
                    .and_then(|timed| note_signature(timed.event.kind))
                    .map(|signature| (index, signature))
            })
            .collect();

        let mut used_next: Vec<usize> = Vec::new();
        for tie in std::mem::take(&mut self.pending_ties) {
            // Re-derive the start signature defensively (must still be a note).
            let start_signature = lowered_timed_note(self.lowered.get(tie.event_index))
                .and_then(|timed| note_signature(timed.event.kind));
            let Some(start_signature) = start_signature else {
                self.drop_pending_tie_carry(tie.signature);
                self.diagnostics
                    .push(unmatched_tie_warning(tie.marker.span));
                continue;
            };
            debug_assert_eq!(start_signature, tie.signature);

            let matched = next_notes
                .iter()
                .find(|(index, signature)| {
                    *signature == start_signature && !used_next.contains(index)
                })
                .map(|(index, _)| *index);

            match matched {
                Some(next_index) => {
                    used_next.push(next_index);
                    // The stop note consumed the barline-preserved accidental
                    // carry; keep it for the rest of the measure and protect
                    // it from a same-signature sibling tie dropping below.
                    self.confirm_pending_tie_carry(start_signature);
                    let pair_id = self.next_tie_id;
                    self.next_tie_id = self.next_tie_id.saturating_add(1);
                    self.attach_tie(tie.event_index, pair_id, TieRole::Start, tie.marker);
                    self.attach_tie(next_index, pair_id, TieRole::Stop, tie.marker);
                }
                None => {
                    // Dropped without a stop note: the carry preserved across
                    // the barline on this tie's behalf must not survive.
                    self.drop_pending_tie_carry(start_signature);
                    self.diagnostics
                        .push(unmatched_tie_warning(tie.marker.span));
                }
            }
        }
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
