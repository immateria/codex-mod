use super::super::prelude::*;

impl ChatWidget<'_> {
    pub(in crate::chatwidget) fn append_wait_pairs(target: &mut Vec<(String, bool)>, additions: &[(String, bool)]) {
        for (text, is_error) in additions {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            if target
                .last()
                .map(|(existing, existing_err)| existing == trimmed && *existing_err == *is_error)
                .unwrap_or(false)
            {
                continue;
            }
            target.push((trimmed.to_string(), *is_error));
        }
    }

    pub(in crate::chatwidget) fn wait_pairs_from_exec_notes(notes: &[ExecWaitNote]) -> Vec<(String, bool)> {
        notes
            .iter()
            .map(|note| {
                (
                    note.message.clone(),
                    matches!(note.tone, TextTone::Error),
                )
            })
            .collect()
    }

    pub(in crate::chatwidget) fn update_exec_wait_state_with_pairs(
        &mut self,
        history_id: HistoryId,
        total_wait: Option<Duration>,
        wait_active: bool,
        notes: &[(String, bool)],
    ) -> bool {
        let Some(record_idx) = self.history_state.index_of(history_id) else {
            return false;
        };
        let note_records: Vec<ExecWaitNote> = notes
            .iter()
            .filter_map(|(text, is_error)| {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(ExecWaitNote {
                        message: trimmed.to_string(),
                        tone: if *is_error {
                            TextTone::Error
                        } else {
                            TextTone::Info
                        },
                        timestamp: SystemTime::now(),
                    })
                }
            })
            .collect();
        let mutation = self.history_state.apply_domain_event(HistoryDomainEvent::UpdateExecWait {
            index: record_idx,
            total_wait,
            wait_active,
            notes: note_records,
        });
        match mutation {
            HistoryMutation::Replaced {
                id,
                record: HistoryRecord::Exec(exec_record),
                ..
            }
            | HistoryMutation::Inserted {
                id,
                record: HistoryRecord::Exec(exec_record),
                ..
            } => {
                self.update_cell_from_record(id, HistoryRecord::Exec(exec_record));
                self.mark_history_dirty();
                true
            }
            _ => false,
        }
    }
}
