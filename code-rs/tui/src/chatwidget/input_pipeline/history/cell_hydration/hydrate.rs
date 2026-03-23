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
            HistoryRecord::RunningTool(state) => {
                if let Some(running_tool) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::RunningToolCallCell>()
                {
                    *running_tool.state_mut() = state.clone();
                    return true;
                }
            }
            HistoryRecord::ToolCall(state) => {
                if let Some(tool_call) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::ToolCallCell>()
                {
                    *tool_call.state_mut() = state.clone();
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
                if let Some(js_cell) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::JsReplCell>()
                {
                    js_cell.sync_from_exec_record(state);
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
}
