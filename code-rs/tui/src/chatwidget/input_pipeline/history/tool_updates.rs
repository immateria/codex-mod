use super::super::prelude::*;

impl ChatWidget<'_> {
    pub(in crate::chatwidget) fn merge_tool_arguments(existing: &mut Vec<ToolArgument>, updates: Vec<ToolArgument>) {
        for update in updates {
            if let Some(existing_arg) = existing.iter_mut().find(|arg| arg.name == update.name) {
                *existing_arg = update;
            } else {
                existing.push(update);
            }
        }
    }

    pub(in crate::chatwidget) fn apply_custom_tool_update(
        &mut self,
        call_id: &str,
        parameters: Option<serde_json::Value>,
    ) {
        let Some(params) = parameters else {
            return;
        };
        let updates = history_cell::arguments_from_json(&params);
        if updates.is_empty() {
            return;
        }

        let running_entry = self
            .tools_state
            .running_custom_tools
            .get(&ToolCallId(call_id.to_string()))
            .copied();
        let resolved_idx = running_entry
            .as_ref()
            .and_then(|entry| running_tools::resolve_entry_index(self, entry, call_id))
            .or_else(|| running_tools::find_by_call_id(self, call_id));

        let Some(idx) = resolved_idx else {
            return;
        };
        if idx >= self.history_cells.len() {
            return;
        }
        let Some(running_cell) = self.history_cells[idx]
            .as_any()
            .downcast_ref::<history_cell::RunningToolCallCell>()
        else {
            return;
        };

        let mut state = running_cell.state().clone();
        Self::merge_tool_arguments(&mut state.arguments, updates);
        let mut updated_cell = history_cell::RunningToolCallCell::from_state(state.clone());
        updated_cell.state_mut().call_id = Some(call_id.to_string());
        self.history_replace_with_record(
            idx,
            Box::new(updated_cell),
            HistoryDomainRecord::from(state),
        );
    }
}
