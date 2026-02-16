use super::*;
use code_core::protocol::OrderMeta;

impl ChatWidget<'_> {
    pub(in super::super) fn handle_custom_tool_call_begin_event(
        &mut self,
        order: Option<&OrderMeta>,
        call_id: String,
        tool_name: String,
        parameters: Option<serde_json::Value>,
    ) {
        // 1) Transition UI into "tool activity" mode.
        self.ensure_spinner_for_activity("tool-begin");
        // Any custom tool invocation should fade out the welcome animation.
        for cell in &self.history_cells {
            cell.trigger_fade();
        }
        self.finalize_active_stream();
        // Flush any queued interrupts when streaming ends.
        self.flush_interrupt_queue();

        // 2) Route tool families that own their own history/UI handling.
        let params_string = parameters
            .as_ref()
            .map(std::string::ToString::to_string);
        if self.handle_agent_or_browser_custom_tool_begin(
            order,
            &call_id,
            &tool_name,
            parameters,
        ) {
            return;
        }

        // 3) Special exec-scoped tools update existing exec state instead of inserting tool rows.
        if self.try_handle_exec_scoped_custom_tool_begin(&call_id, &tool_name, params_string.as_ref())
        {
            return;
        }

        // 4) Default path: insert a running tool cell and track it for completion replacement.
        self.insert_running_custom_tool_entry(order, &call_id, &tool_name, params_string);
        self.update_status_for_tool_begin(&tool_name);
    }
    fn update_status_for_tool_begin(&mut self, tool_name: &str) {
        if tool_name.starts_with("browser_") {
            self.bottom_pane
                .update_status_text("using browser".to_string());
        } else if agent_runs::is_agent_tool(tool_name) {
            self.bottom_pane
                .update_status_text("agents coordinating".to_string());
        } else {
            self.bottom_pane
                .update_status_text(format!("using tool: {tool_name}"));
        }
    }

    fn try_handle_exec_scoped_custom_tool_begin(
        &mut self,
        call_id: &str,
        tool_name: &str,
        params_string: Option<&String>,
    ) -> bool {
        self.try_handle_wait_begin(call_id, tool_name, params_string)
            || self.try_handle_kill_begin(call_id, tool_name, params_string)
    }

    fn insert_running_custom_tool_entry(
        &mut self,
        order: Option<&OrderMeta>,
        call_id: &str,
        tool_name: &str,
        params_string: Option<String>,
    ) {
        // Animated running cell with live timer and formatted args.
        let mut cell = if tool_name.starts_with("browser_") {
            history_cell::new_running_browser_tool_call(tool_name.to_string(), params_string)
        } else {
            history_cell::new_running_custom_tool_call(tool_name.to_string(), params_string)
        };
        cell.state_mut().call_id = Some(call_id.to_string());

        let order_key = self.custom_tool_order_key(order, "CustomToolCallBegin");
        let idx = self.history_insert_with_key_global(Box::new(cell), order_key);
        let history_id = self
            .history_state
            .history_id_for_tool_call(call_id)
            .or_else(|| self.history_cell_ids.get(idx).and_then(|slot| *slot));

        // Track index so we can replace it on completion.
        if idx < self.history_cells.len() {
            self.tools_state.running_custom_tools.insert(
                ToolCallId(call_id.to_string()),
                RunningToolEntry::new(order_key, idx).with_history_id(history_id),
            );
        }
    }

    fn handle_agent_or_browser_custom_tool_begin(
        &mut self,
        order: Option<&OrderMeta>,
        call_id: &str,
        tool_name: &str,
        params_json: Option<serde_json::Value>,
    ) -> bool {
        if agent_runs::is_agent_tool(tool_name)
            && agent_runs::handle_custom_tool_begin(
                self,
                order,
                call_id,
                tool_name,
                params_json.clone(),
            )
        {
            self.bottom_pane
                .update_status_text("agents coordinating".to_string());
            return true;
        }
        if tool_name.starts_with("browser_")
            && browser_sessions::handle_custom_tool_begin(self, order, call_id, tool_name, params_json)
        {
            self.bottom_pane
                .update_status_text("using browser".to_string());
            return true;
        }
        false
    }

    fn try_handle_wait_begin(
        &mut self,
        call_id: &str,
        tool_name: &str,
        params_string: Option<&String>,
    ) -> bool {
        if tool_name != "wait" {
            return false;
        }
        let Some(exec_call_id) = wait_exec_call_id_from_params(params_string) else {
            return false;
        };
        // Only treat this as an exec-scoped wait when the target exec is still running.
        // Background waits (e.g., waiting on a shell call_id) also carry `call_id`.
        if !self.exec.running_commands.contains_key(&exec_call_id) {
            return false;
        }

        self.tools_state
            .running_wait_tools
            .insert(ToolCallId(call_id.to_string()), exec_call_id.clone());

        let mut wait_update: Option<WaitHistoryUpdate> = None;
        if let Some(running) = self.exec.running_commands.get_mut(&exec_call_id) {
            running.wait_active = true;
            running.wait_notes.clear();
            let history_id = running.history_id.or_else(|| {
                running
                    .history_index
                    .and_then(|idx| self.history_cell_ids.get(idx).and_then(|slot| *slot))
            });
            running.history_id = history_id;
            if let Some(id) = history_id {
                wait_update = Some((id, running.wait_total, running.wait_notes.clone()));
            }
        }
        if let Some((history_id, total, notes)) = wait_update {
            let _ = self.update_exec_wait_state_with_pairs(history_id, total, true, &notes);
        }
        self.bottom_pane
            .update_status_text("waiting for command".to_string());
        self.invalidate_height_cache();
        self.request_redraw();
        true
    }

    fn try_handle_kill_begin(
        &mut self,
        call_id: &str,
        tool_name: &str,
        params_string: Option<&String>,
    ) -> bool {
        if tool_name != "kill" {
            return false;
        }
        let Some(exec_call_id) = wait_exec_call_id_from_params(params_string) else {
            return false;
        };
        if !self.exec.running_commands.contains_key(&exec_call_id) {
            return false;
        }

        self.tools_state
            .running_kill_tools
            .insert(ToolCallId(call_id.to_string()), exec_call_id);
        self.bottom_pane
            .update_status_text("cancelling command".to_string());
        self.invalidate_height_cache();
        self.request_redraw();
        true
    }

}
