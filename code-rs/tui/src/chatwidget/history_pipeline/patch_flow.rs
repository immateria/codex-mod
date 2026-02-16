use super::*;

impl ChatWidget<'_> {
    /// Handle patch apply end immediately
    pub(in super::super) fn handle_patch_apply_end_now(&mut self, ev: PatchApplyEndEvent) {
        if ev.success {
            let _ = self.update_latest_patch_summary_record(
                HistoryPatchEventType::ApplySuccess,
                None,
                true,
            );
            self.maybe_hide_spinner();
            return;
        }

        let failure_meta = Self::build_patch_failure_metadata(&ev.stdout, &ev.stderr);
        if !self.update_latest_patch_summary_record(
            HistoryPatchEventType::ApplyFailure,
            Some(failure_meta.clone()),
            false,
        ) {
            self.insert_patch_failure_summary(failure_meta);
        }
        self.maybe_hide_spinner();
    }

    fn latest_patch_summary_cell_index(&self) -> Option<usize> {
        self.history_cells.iter().rposition(|cell| {
            matches!(
                cell.kind(),
                crate::history_cell::HistoryCellType::Patch {
                    kind: crate::history_cell::PatchKind::ApplyBegin
                } | crate::history_cell::HistoryCellType::Patch {
                    kind: crate::history_cell::PatchKind::Proposed
                }
            )
        })
    }

    fn update_latest_patch_summary_record(
        &mut self,
        patch_type: HistoryPatchEventType,
        failure: Option<PatchFailureMetadata>,
        sync_history_cell_id: bool,
    ) -> bool {
        let Some(idx) = self.latest_patch_summary_cell_index() else {
            return false;
        };
        let Some(record) = self
            .history_cells
            .get(idx)
            .and_then(|existing| self.record_from_cell_or_state(idx, existing.as_ref()))
        else {
            return false;
        };
        let HistoryRecord::Patch(mut patch_record) = record else {
            return false;
        };

        patch_record.patch_type = patch_type;
        patch_record.failure = failure;
        let record_index = self
            .record_index_for_cell(idx)
            .unwrap_or_else(|| self.record_index_for_position(idx));
        let mutation = self
            .history_state
            .apply_domain_event(HistoryDomainEvent::Replace {
                index: record_index,
                record: HistoryDomainRecord::Patch(patch_record),
            });
        if let Some(id) = self.apply_mutation_to_cell_index(idx, mutation) {
            if sync_history_cell_id && idx < self.history_cell_ids.len() {
                self.history_cell_ids[idx] = Some(id);
            }
            return true;
        }
        false
    }

    fn insert_patch_failure_summary(&mut self, failure: PatchFailureMetadata) {
        let record = PatchRecord {
            id: HistoryId::ZERO,
            patch_type: HistoryPatchEventType::ApplyFailure,
            changes: HashMap::new(),
            failure: Some(failure),
        };
        let cell = history_cell::PatchSummaryCell::from_record(record.clone());
        let key = self.next_internal_key();
        let _ = self.history_insert_with_key_global_tagged(
            Box::new(cell),
            key,
            "patch-failure",
            Some(HistoryDomainRecord::Patch(record)),
        );
    }
}
