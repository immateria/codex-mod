use super::super::prelude::*;

impl ChatWidget<'_> {
    pub(in crate::chatwidget) fn hydrate_cell_from_record(
        &self,
        cell: &mut Box<dyn HistoryCell>,
        record: &HistoryRecord,
    ) -> bool {
        Self::hydrate_cell_from_record_inner(cell, record, &self.config)
    }

    pub(in crate::chatwidget) fn hydrate_cell_from_record_inner(
        cell: &mut Box<dyn HistoryCell>,
        record: &HistoryRecord,
        config: &Config,
    ) -> bool {
        match record {
            HistoryRecord::PlainMessage(state) => {
                if let Some(plain) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::PlainHistoryCell>()
                {
                    *plain.state_mut() = state.clone();
                    return true;
                }
            }
            HistoryRecord::WaitStatus(state) => {
                if let Some(wait) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::WaitStatusCell>()
                {
                    *wait.state_mut() = state.clone();
                    return true;
                }
            }
            HistoryRecord::Loading(state) => {
                if let Some(loading) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::LoadingCell>()
                {
                    *loading.state_mut() = state.clone();
                    return true;
                }
            }
            HistoryRecord::BackgroundEvent(state) => {
                if let Some(background) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::BackgroundEventCell>()
                {
                    *background.state_mut() = state.clone();
                    return true;
                }
            }
            HistoryRecord::Exec(state) => {
                if let Some(exec) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::ExecCell>()
                {
                    exec.sync_from_record(state);
                    return true;
                }
            }
            HistoryRecord::AssistantStream(state) => {
                if let Some(stream) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::StreamingContentCell>()
                {
                    stream.set_state(state.clone());
                    stream.update_context(config.file_opener, &config.cwd);
                    return true;
                }
            }
            HistoryRecord::RateLimits(state) => {
                if let Some(rate_limits) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::RateLimitsCell>()
                {
                    *rate_limits.record_mut() = state.clone();
                    return true;
                }
            }
            HistoryRecord::Patch(state) => {
                if let Some(patch) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::PatchSummaryCell>()
                {
                    patch.update_record(state.clone());
                    return true;
                }
            }
            HistoryRecord::Image(state) => {
                if let Some(image) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::ImageOutputCell>()
                {
                    *image.record_mut() = state.clone();
                    return true;
                }
            }
            HistoryRecord::Context(state) => {
                if let Some(context) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::ContextCell>()
                {
                    context.update(state.clone());
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    pub(in crate::chatwidget) fn build_cell_from_record(&self, record: &HistoryRecord) -> Option<Box<dyn HistoryCell>> {
        use crate::history_cell;

        match record {
            HistoryRecord::PlainMessage(state) => Some(Box::new(
                history_cell::PlainHistoryCell::from_state(state.clone()),
            )),
            HistoryRecord::WaitStatus(state) => {
                Some(Box::new(history_cell::WaitStatusCell::from_state(state.clone())))
            }
            HistoryRecord::Loading(state) => {
                Some(Box::new(history_cell::LoadingCell::from_state(state.clone())))
            }
            HistoryRecord::RunningTool(state) => Some(Box::new(
                history_cell::RunningToolCallCell::from_state(state.clone()),
            )),
            HistoryRecord::ToolCall(state) => Some(Box::new(
                history_cell::ToolCallCell::from_state(state.clone()),
            )),
            HistoryRecord::PlanUpdate(state) => Some(Box::new(
                history_cell::PlanUpdateCell::from_state(state.clone()),
            )),
            HistoryRecord::UpgradeNotice(state) => Some(Box::new(
                history_cell::UpgradeNoticeCell::from_state(state.clone()),
            )),
            HistoryRecord::Reasoning(state) => Some(Box::new(
                history_cell::CollapsibleReasoningCell::from_state(state.clone()),
            )),
            HistoryRecord::Exec(state) => {
                Some(Box::new(history_cell::ExecCell::from_record(state.clone())))
            }
            HistoryRecord::MergedExec(state) => Some(Box::new(
                history_cell::MergedExecCell::from_state(state.clone()),
            )),
            HistoryRecord::AssistantStream(state) => Some(Box::new(
                history_cell::StreamingContentCell::from_state(
                    state.clone(),
                    self.config.file_opener,
                    self.config.cwd.clone(),
                ),
            )),
            HistoryRecord::AssistantMessage(state) => Some(Box::new(
                history_cell::AssistantMarkdownCell::from_state(state.clone(), &self.config),
            )),
            HistoryRecord::ProposedPlan(state) => Some(Box::new(
                history_cell::ProposedPlanCell::from_state(state.clone(), &self.config),
            )),
            HistoryRecord::Diff(state) => {
                Some(Box::new(history_cell::DiffCell::from_record(state.clone())))
            }
            HistoryRecord::Patch(state) => {
                Some(Box::new(history_cell::PatchSummaryCell::from_record(state.clone())))
            }
            HistoryRecord::Explore(state) => {
                Some(Box::new(history_cell::ExploreAggregationCell::from_record(state.clone())))
            }
            HistoryRecord::RateLimits(state) => Some(Box::new(
                history_cell::RateLimitsCell::from_record(state.clone()),
            )),
            HistoryRecord::BackgroundEvent(state) => {
                Some(Box::new(history_cell::BackgroundEventCell::new(state.clone())))
            }
            HistoryRecord::Image(state) => {
                let cell = history_cell::ImageOutputCell::from_record(state.clone());
                self.ensure_image_cell_picker(&cell);
                Some(Box::new(cell))
            }
            HistoryRecord::Context(state) => Some(Box::new(
                history_cell::ContextCell::new(state.clone()),
            )),
            HistoryRecord::Notice(state) => Some(Box::new(
                history_cell::PlainHistoryCell::from_notice_record(state.clone()),
            )),
        }
    }

    pub(in crate::chatwidget) fn apply_mutation_to_cell(
        &self,
        cell: &mut Box<dyn HistoryCell>,
        mutation: HistoryMutation,
    ) -> Option<HistoryId> {
        match mutation {
            HistoryMutation::Inserted { id, record, .. }
            | HistoryMutation::Replaced { id, record, .. } => {
                if let Some(mut new_cell) = self.build_cell_from_record(&record) {
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
                self.update_cell_from_record(id, record);
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
            if let Some(mut rebuilt) = self.build_cell_from_record(&record) {
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
            tracing::warn!(
                "history-state mismatch: unable to locate cell for id {:?}",
                id
            );
        }
    }

    pub(in crate::chatwidget) fn assign_history_id(&self, cell: &mut Box<dyn HistoryCell>, id: HistoryId) {
        Self::assign_history_id_inner(cell, id);
    }

    pub(in crate::chatwidget) fn assign_history_id_inner(cell: &mut Box<dyn HistoryCell>, id: HistoryId) {
        if let Some(tool_call) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::ToolCallCell>()
        {
            tool_call.state_mut().id = id;
        } else if let Some(running_tool) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::RunningToolCallCell>()
        {
            running_tool.state_mut().id = id;
        } else if let Some(plan) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::PlanUpdateCell>()
        {
            plan.state_mut().id = id;
        } else if let Some(upgrade) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::UpgradeNoticeCell>()
        {
            upgrade.state_mut().id = id;
        } else if let Some(reasoning) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::CollapsibleReasoningCell>()
        {
            reasoning.set_history_id(id);
        } else if let Some(exec) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::ExecCell>()
        {
            exec.record.id = id;
        } else if let Some(merged) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::MergedExecCell>()
        {
            merged.set_history_id(id);
        } else if let Some(stream) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::StreamingContentCell>()
        {
            stream.state_mut().id = id;
        } else if let Some(assistant) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::AssistantMarkdownCell>()
        {
            assistant.state_mut().id = id;
        } else if let Some(diff) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::DiffCell>()
        {
            diff.record_mut().id = id;
        } else if let Some(image) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::ImageOutputCell>()
        {
            image.record_mut().id = id;
        } else if let Some(patch) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::PatchSummaryCell>()
        {
            patch.record_mut().id = id;
        } else if let Some(explore) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::ExploreAggregationCell>()
        {
            explore.record_mut().id = id;
        } else if let Some(rate_limits) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::RateLimitsCell>()
        {
            rate_limits.record_mut().id = id;
        } else if let Some(plain) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::PlainHistoryCell>()
        {
            plain.state_mut().id = id;
        } else if let Some(wait) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::WaitStatusCell>()
        {
            wait.state_mut().id = id;
        } else if let Some(loading) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::LoadingCell>()
        {
            loading.state_mut().id = id;
        } else if let Some(background) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::BackgroundEventCell>()
        {
            background.state_mut().id = id;
        }
    }
}
