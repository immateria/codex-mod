impl ChatWidget<'_> {
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
        } else if let Some(js_cell) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::JsReplCell>()
        {
            js_cell.set_history_id(id);
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
