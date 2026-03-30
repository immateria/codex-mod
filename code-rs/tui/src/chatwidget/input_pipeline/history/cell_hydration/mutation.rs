impl ChatWidget<'_> {
    pub(in crate::chatwidget) fn apply_mutation_to_cell(
        &self,
        cell: &mut Box<dyn HistoryCell>,
        mutation: HistoryMutation,
    ) -> Option<HistoryId> {
        match mutation {
            HistoryMutation::Inserted { id, record, .. }
            | HistoryMutation::Replaced { id, record, .. } => {
                // Some cells carry transient metadata (e.g. `parent_call_id`) or
                // additional fields that are not stored in the domain record.
                // Preserve those by hydrating the existing cell rather than
                // rebuilding from the record.
                let existing_prefers_hydrate = cell
                    .as_any()
                    .is::<crate::history_cell::JsReplCell>()
                    || cell.as_any().is::<crate::history_cell::ExecCell>()
                    || cell.as_any().is::<crate::history_cell::PatchSummaryCell>()
                    || cell.as_any().is::<crate::history_cell::ToolCallCell>()
                    || cell.as_any().is::<crate::history_cell::RunningToolCallCell>();

                if existing_prefers_hydrate {
                    if !self.hydrate_cell_from_record(cell, &record) {
                        self.assign_history_id(cell, id);
                    } else {
                        self.assign_history_id(cell, id);
                    }
                } else if let Some(mut new_cell) = self.build_cell_from_record(&record) {
                    self.assign_history_id(&mut new_cell, id);
                    *cell = new_cell;
                } else if !self.hydrate_cell_from_record(cell, &record) {
                    self.assign_history_id(cell, id);
                }
                Some(id)
            }
            _ => None,
        }
    }

    pub(in crate::chatwidget) fn apply_mutation_to_cell_index(
        &mut self,
        idx: usize,
        mutation: HistoryMutation,
    ) -> Option<HistoryId> {
        if idx >= self.history_cells.len() {
            return None;
        }
        match mutation {
            HistoryMutation::Inserted { id, record, .. }
            | HistoryMutation::Replaced { id, record, .. } => {
                // Callers already resolved the target cell index (e.g. latest
                // patch summary, context cell). Updating by index avoids a
                // second lookup by history id, and prevents warning noise when
                // the id mapping is temporarily out of sync.
                self.history_render.invalidate_history_id(id);

                let existing_prefers_hydrate = self
                    .history_cells
                    .get(idx)
                    .map(|cell_slot| {
                        cell_slot.as_any().is::<crate::history_cell::JsReplCell>()
                            || cell_slot.as_any().is::<crate::history_cell::ExecCell>()
                            || cell_slot.as_any().is::<crate::history_cell::PatchSummaryCell>()
                            || cell_slot.as_any().is::<crate::history_cell::ToolCallCell>()
                            || cell_slot.as_any().is::<crate::history_cell::RunningToolCallCell>()
                    })
                    .unwrap_or(false);

                // Build any replacement cell up-front so we don't borrow `self`
                // immutably while holding a mutable reference to `history_cells`.
                let mut rebuilt = if existing_prefers_hydrate {
                    None
                } else {
                    self.build_cell_from_record(&record)
                };

                if let Some(cell_slot) = self.history_cells.get_mut(idx) {

                    if existing_prefers_hydrate {
                        Self::hydrate_cell_from_record_inner(cell_slot, &record, &self.config);
                        Self::assign_history_id_inner(cell_slot, id);
                    } else if let Some(mut next) = rebuilt.take() {
                        Self::assign_history_id_inner(&mut next, id);
                        *cell_slot = next;
                    } else if !Self::hydrate_cell_from_record_inner(cell_slot, &record, &self.config)
                    {
                        Self::assign_history_id_inner(cell_slot, id);
                    } else {
                        Self::assign_history_id_inner(cell_slot, id);
                    }
                }

                if idx < self.history_cell_ids.len() {
                    self.history_cell_ids[idx] = Some(id);
                }
                self.invalidate_height_cache();
                self.request_redraw();
                Some(id)
            }
            _ => None,
        }
    }

    pub(in crate::chatwidget) fn cell_index_for_history_id(&self, id: HistoryId) -> Option<usize> {
        if let Some(idx) = self
            .history_cell_ids
            .iter()
            .rposition(|maybe| maybe.as_ref() == Some(&id))
        {
            return Some(idx);
        }

        self.history_cells.iter().enumerate().find_map(|(idx, cell)| {
            let record = history_cell::record_from_cell(cell.as_ref())?;
            if record.id() == id {
                Some(idx)
            } else {
                None
            }
        })
    }

    pub(in crate::chatwidget) fn update_cell_from_record(&mut self, id: HistoryId, record: HistoryRecord) {
        if id == HistoryId::ZERO {
            tracing::debug!("skip update_cell_from_record: zero id");
            return;
        }

        self.history_render.invalidate_history_id(id);

        if let Some(idx) = self.cell_index_for_history_id(id) {
            // JsReplCell stores JS-specific metadata (code, runtime) that would be
            // lost if we rebuilt it from a plain ExecRecord. Additionally, some
            // cells carry transient metadata (like `parent_call_id`) that is not
            // stored in the history domain record. For these cells, hydrate
            // in-place rather than rebuilding.
            let existing_prefers_hydrate = self
                .history_cells
                .get(idx)
                .map(|c| {
                    c.as_any().is::<crate::history_cell::JsReplCell>()
                        || c.as_any().is::<crate::history_cell::ExecCell>()
                        || c.as_any().is::<crate::history_cell::PatchSummaryCell>()
                        || c.as_any().is::<crate::history_cell::ToolCallCell>()
                        || c.as_any().is::<crate::history_cell::RunningToolCallCell>()
                })
                .unwrap_or(false);
            if existing_prefers_hydrate {
                if let Some(cell_slot) = self.history_cells.get_mut(idx) {
                    Self::hydrate_cell_from_record_inner(cell_slot, &record, &self.config);
                    Self::assign_history_id_inner(cell_slot, id);
                }
            } else if let Some(mut rebuilt) = self.build_cell_from_record(&record) {
                Self::assign_history_id_inner(&mut rebuilt, id);
                self.history_cells[idx] = rebuilt;
            } else if let Some(cell_slot) = self.history_cells.get_mut(idx)
                && !Self::hydrate_cell_from_record_inner(cell_slot, &record, &self.config) {
                    Self::assign_history_id_inner(cell_slot, id);
                }

            if idx < self.history_cell_ids.len() {
                self.history_cell_ids[idx] = Some(id);
            }
            self.invalidate_height_cache();
            self.request_redraw();
        } else {
            let record_kind = match &record {
                HistoryRecord::PlainMessage(_) => "plain",
                HistoryRecord::WaitStatus(_) => "wait_status",
                HistoryRecord::Loading(_) => "loading",
                HistoryRecord::RunningTool(_) => "running_tool",
                HistoryRecord::ToolCall(_) => "tool_call",
                HistoryRecord::PlanUpdate(_) => "plan_update",
                HistoryRecord::UpgradeNotice(_) => "upgrade_notice",
                HistoryRecord::Reasoning(_) => "reasoning",
                HistoryRecord::Exec(_) => "exec",
                HistoryRecord::MergedExec(_) => "merged_exec",
                HistoryRecord::AssistantStream(_) => "assistant_stream",
                HistoryRecord::AssistantMessage(_) => "assistant_message",
                HistoryRecord::ProposedPlan(_) => "proposed_plan",
                HistoryRecord::Diff(_) => "diff",
                HistoryRecord::Image(_) => "image",
                HistoryRecord::Explore(_) => "explore",
                HistoryRecord::RateLimits(_) => "rate_limits",
                HistoryRecord::Patch(_) => "patch",
                HistoryRecord::BackgroundEvent(_) => "background",
                HistoryRecord::Notice(_) => "notice",
                HistoryRecord::Context(_) => "context",
            };
            tracing::warn!(
                record_kind,
                "history-state mismatch: unable to locate cell for id {:?}",
                id,
            );
        }
    }
}
