use super::*;
use code_core::protocol::OrderMeta;

impl ChatWidget<'_> {
    pub(in super::super) fn handle_custom_tool_call_end_event(
        &mut self,
        order: Option<&OrderMeta>,
        event_seq: u64,
        end_event: CustomToolCallEndEvent,
    ) {
        let CustomToolCallEndEvent {
            call_id,
            tool_name,
            parameters,
            duration,
            result,
        } = end_event;

        // 1) Route tool families that own their own completion rendering.
        let params_json = parameters;
        if self.handle_agent_or_browser_custom_tool_end(
            order,
            &call_id,
            &tool_name,
            params_json.clone(),
            duration,
            &result,
        ) {
            return;
        }

        let order_key = self.custom_tool_order_key(order, "CustomToolCallEnd");
        let image_view_path =
            self.resolve_image_view_path_for_custom_tool_end(&tool_name, &call_id, params_json.as_ref());
        tracing::info!(
            "[order] CustomToolCallEnd call_id={} tool={} seq={}",
            call_id,
            tool_name,
            event_seq
        );

        // Convert parameters to String if present.
        let params_string = params_json.map(|p| p.to_string());
        // Determine success and content from Result.
        let (success, content) = match result {
            Ok(content) => (true, content),
            Err(error) => (false, error),
        };

        // 2) Exec-scoped wait has bespoke state transitions and history patch-up.
        if self.try_handle_wait_end(&call_id, duration, success, &content) {
            return;
        }

        let ctx = CustomToolEndContext {
            resolved_idx: self.resolve_running_custom_tool_index(&call_id),
            call_id,
            tool_name,
            duration,
            success,
            content,
            params_string,
            order_key,
            image_view_path,
        };

        // 3) Event routing by tool kind.
        if self.route_custom_tool_end_by_kind(&ctx) {
            return;
        }

        self.handle_generic_custom_tool_end(ctx);
    }

    fn route_custom_tool_end_by_kind(&mut self, ctx: &CustomToolEndContext) -> bool {
        self.try_handle_apply_patch_end(ctx)
            || self.try_handle_wait_success_end(ctx)
            || self.try_handle_wait_cancelled_end(ctx)
            || self.try_handle_kill_end(ctx)
            || self.try_handle_fetch_end(ctx)
    }

    fn resolve_image_view_path_for_custom_tool_end(
        &mut self,
        tool_name: &str,
        call_id: &str,
        params_json: Option<&serde_json::Value>,
    ) -> Option<std::path::PathBuf> {
        if tool_name != "image_view" {
            return None;
        }
        let image_view_seen = self
            .tools_state
            .image_viewed_calls
            .remove(&ToolCallId(call_id.to_string()));
        if image_view_seen {
            return None;
        }
        params_json.and_then(|value| image_view_path_from_params(value, &self.config.cwd))
    }

    fn resolve_running_custom_tool_index(&mut self, call_id: &str) -> Option<usize> {
        let running_entry = self
            .tools_state
            .running_custom_tools
            .remove(&ToolCallId(call_id.to_string()));
        running_entry
            .as_ref()
            .and_then(|entry| running_tools::resolve_entry_index(self, entry, call_id))
            .or_else(|| running_tools::find_by_call_id(self, call_id))
    }

    fn try_handle_apply_patch_end(&mut self, ctx: &CustomToolEndContext) -> bool {
        if ctx.tool_name != "apply_patch" || !ctx.success {
            return false;
        }
        if let Some(idx) = ctx.resolved_idx
            && idx < self.history_cells.len()
        {
            let is_running_tool = self.history_cells[idx]
                .as_any()
                .downcast_ref::<history_cell::RunningToolCallCell>()
                .is_some();
            if is_running_tool {
                self.history_remove_at(idx);
            }
        }
        self.bottom_pane.update_status_text("responding".to_string());
        self.maybe_hide_spinner();
        true
    }

    fn try_handle_wait_success_end(&mut self, ctx: &CustomToolEndContext) -> bool {
        if ctx.tool_name != "wait" || !ctx.success {
            return false;
        }
        let target = wait_target_from_params(ctx.params_string.as_ref(), &ctx.call_id);
        let wait_cell = history_cell::new_completed_wait_tool_call(target, ctx.duration);
        let wait_state = wait_cell.state().clone();
        if let Some(idx) = ctx.resolved_idx {
            self.history_replace_with_record(
                idx,
                Box::new(wait_cell),
                HistoryDomainRecord::WaitStatus(wait_state),
            );
        } else {
            let _ = self.history_insert_with_key_global_tagged(
                Box::new(wait_cell),
                ctx.order_key,
                "untagged",
                Some(HistoryDomainRecord::WaitStatus(wait_state)),
            );
        }
        self.remove_background_completion_message(&ctx.call_id);
        self.bottom_pane.update_status_text("responding".to_string());
        self.maybe_hide_spinner();
        true
    }

    fn try_handle_wait_cancelled_end(&mut self, ctx: &CustomToolEndContext) -> bool {
        if ctx.tool_name != "wait" || ctx.success || ctx.content.trim() != WAIT_CANCELLED_BY_USER {
            return false;
        }

        let wait_state = Self::build_wait_cancelled_plain_state();
        if let Some(idx) = ctx.resolved_idx {
            self.history_replace_with_record(
                idx,
                Box::new(history_cell::PlainHistoryCell::from_state(wait_state.clone())),
                HistoryDomainRecord::Plain(wait_state),
            );
        } else {
            let _ = self.history_insert_plain_state_with_key(wait_state, ctx.order_key, "untagged");
        }

        self.bottom_pane.update_status_text("responding".to_string());
        self.maybe_hide_spinner();
        true
    }

    fn build_wait_cancelled_plain_state() -> PlainMessageState {
        let emphasis = TextEmphasis {
            bold: true,
            ..TextEmphasis::default()
        };
        PlainMessageState {
            id: HistoryId::ZERO,
            role: PlainMessageRole::Error,
            kind: PlainMessageKind::Error,
            header: None,
            lines: vec![MessageLine {
                kind: MessageLineKind::Paragraph,
                spans: vec![InlineSpan {
                    text: "Wait cancelled".into(),
                    tone: TextTone::Error,
                    emphasis,
                    entity: None,
                }],
            }],
            metadata: None,
        }
    }

    fn try_handle_kill_end(&mut self, ctx: &CustomToolEndContext) -> bool {
        if ctx.tool_name != "kill" {
            return false;
        }
        let _ = self
            .tools_state
            .running_kill_tools
            .remove(&ToolCallId(ctx.call_id.clone()));
        if ctx.success {
            self.remove_background_completion_message(&ctx.call_id);
            self.bottom_pane.update_status_text("responding".to_string());
        } else {
            let trimmed = ctx.content.trim();
            if !trimmed.is_empty() {
                self.push_background_tail(trimmed.to_string());
            }
            self.bottom_pane.update_status_text("kill failed".to_string());
        }
        self.maybe_hide_spinner();
        self.invalidate_height_cache();
        self.request_redraw();
        true
    }

    fn try_handle_fetch_end(&mut self, ctx: &CustomToolEndContext) -> bool {
        if ctx.tool_name != "web_fetch" && ctx.tool_name != "browser_fetch" {
            return false;
        }
        let completed = history_cell::new_completed_web_fetch_tool_call(
            &self.config,
            ctx.params_string.clone(),
            ctx.duration,
            ctx.success,
            ctx.content.clone(),
        );
        if let Some(idx) = ctx.resolved_idx {
            self.history_replace_at(idx, Box::new(completed));
        } else {
            running_tools::collapse_spinner(self, &ctx.call_id);
            let _ = self
                .history_insert_with_key_global(Box::new(completed), ctx.order_key);
        }

        self.bottom_pane.update_status_text("responding".to_string());
        self.maybe_hide_spinner();
        true
    }

    fn handle_generic_custom_tool_end(&mut self, ctx: CustomToolEndContext) {
        let CustomToolEndContext {
            call_id,
            tool_name,
            duration,
            success,
            content,
            params_string,
            order_key,
            resolved_idx,
            image_view_path,
        } = ctx;

        let mut completed = history_cell::new_completed_custom_tool_call(
            tool_name,
            params_string,
            duration,
            success,
            content,
        );
        completed.state_mut().call_id = Some(call_id.clone());
        if let Some(idx) = resolved_idx {
            self.history_debug(format!(
                "custom_tool_end.in_place call_id={} idx={} order=({}, {}, {})",
                call_id, idx, order_key.req, order_key.out, order_key.seq
            ));
            self.history_replace_at(idx, Box::new(completed));
        } else {
            self.history_debug(format!(
                "custom_tool_end.fallback_insert call_id={} order=({}, {}, {})",
                call_id, order_key.req, order_key.out, order_key.seq
            ));
            running_tools::collapse_spinner(self, &call_id);
            let _ = self.history_insert_with_key_global(Box::new(completed), order_key);
        }

        if let Some(path) = image_view_path.as_ref()
            && let Some(record) = image_record_from_path(path)
        {
            let cell = Box::new(history_cell::ImageOutputCell::from_record(record));
            let _ = self.history_insert_with_key_global(cell, order_key);
        }

        self.bottom_pane.update_status_text("responding".to_string());
        self.maybe_hide_spinner();
    }

    fn handle_agent_or_browser_custom_tool_end(
        &mut self,
        order: Option<&OrderMeta>,
        call_id: &str,
        tool_name: &str,
        params_json: Option<serde_json::Value>,
        duration: Duration,
        result: &Result<String, String>,
    ) -> bool {
        if agent_runs::is_agent_tool(tool_name)
            && agent_runs::handle_custom_tool_end(
                self,
                order,
                call_id,
                tool_name,
                params_json.clone(),
                duration,
                result,
            )
        {
            self.bottom_pane.update_status_text("responding".to_string());
            return true;
        }
        if tool_name.starts_with("browser_")
            && browser_sessions::handle_custom_tool_end(
                self,
                order,
                call_id,
                tool_name,
                params_json,
                duration,
                result,
            )
        {
            if tool_name == "browser_close" {
                self.bottom_pane.update_status_text("responding".to_string());
            } else {
                self.bottom_pane.update_status_text("using browser".to_string());
            }
            return true;
        }
        false
    }

    fn try_handle_wait_end(
        &mut self,
        call_id: &str,
        duration: Duration,
        success: bool,
        content: &str,
    ) -> bool {
        let Some(exec_call_id) = self
            .tools_state
            .running_wait_tools
            .remove(&ToolCallId(call_id.to_string()))
        else {
            return false;
        };

        // The wait-end flow has to reconcile three sources of truth:
        // running in-memory exec state, persisted history records, and tool output text.
        let mut state = Self::init_wait_end_state(content, success);
        self.capture_wait_end_runtime_state(&exec_call_id, duration, &mut state);
        self.bind_wait_history_id_if_missing(&exec_call_id, &mut state);
        self.merge_wait_end_history_state(duration, &mut state);
        self.apply_wait_end_history_update(&state);
        self.finalize_wait_end_ui(call_id, exec_call_id, success, &state);
        true
    }

    fn init_wait_end_state(content: &str, success: bool) -> WaitEndState {
        let trimmed = content.trim().to_string();
        let wait_missing_job = wait_result_missing_background_job(&trimmed);
        let wait_interrupted = wait_result_interrupted(&trimmed);
        let wait_still_pending = !success && trimmed != WAIT_CANCELLED_BY_USER && !wait_missing_job;
        let note_lines = Self::wait_note_lines_from_content(content, &trimmed);
        WaitEndState {
            trimmed,
            wait_missing_job,
            wait_interrupted,
            wait_still_pending,
            exec_running: false,
            exec_completed: false,
            note_lines,
            history_id: None,
            wait_total: None,
            wait_notes_snapshot: Vec::new(),
        }
    }

    fn wait_note_lines_from_content(content: &str, trimmed: &str) -> Vec<(String, bool)> {
        let suppress_json_notes = serde_json::from_str::<serde_json::Value>(trimmed)
            .ok()
            .and_then(|value| {
                value
                    .as_object()
                    .map(|obj| obj.contains_key("output") || obj.contains_key("metadata"))
            })
            .unwrap_or(false);
        if suppress_json_notes {
            return Vec::new();
        }

        let mut note_lines: Vec<(String, bool)> = Vec::new();
        for line in content.lines() {
            let note_text = line.trim();
            if note_text.is_empty() {
                continue;
            }
            let is_error_note = note_text == WAIT_CANCELLED_BY_USER;
            note_lines.push((note_text.to_string(), is_error_note));
        }
        note_lines
    }

    fn capture_wait_end_runtime_state(
        &mut self,
        exec_call_id: &ExecCallId,
        duration: Duration,
        state: &mut WaitEndState,
    ) {
        if let Some(running) = self.exec.running_commands.get_mut(exec_call_id) {
            state.exec_running = true;
            let base = running.wait_total.unwrap_or_default();
            let total = base.saturating_add(duration);
            running.wait_total = Some(total);
            running.wait_active = state.wait_still_pending;
            Self::append_wait_pairs(&mut running.wait_notes, &state.note_lines);
            state.wait_notes_snapshot = running.wait_notes.clone();
            state.wait_total = running.wait_total;
            state.history_id = running.history_id.or_else(|| {
                running
                    .history_index
                    .and_then(|idx| self.history_cell_ids.get(idx).and_then(|slot| *slot))
            });
            running.history_id = state.history_id;
        } else {
            Self::append_wait_pairs(&mut state.wait_notes_snapshot, &state.note_lines);
        }
    }

    fn bind_wait_history_id_if_missing(
        &mut self,
        exec_call_id: &ExecCallId,
        state: &mut WaitEndState,
    ) {
        if state.history_id.is_some() {
            return;
        }
        if let Some((idx, _)) = self.history_cells.iter().enumerate().rev().find(|(_, cell)| {
            cell.as_any()
                .downcast_ref::<history_cell::ExecCell>()
                .is_some()
        }) && let Some(id) = self.history_cell_ids.get(idx).and_then(|slot| *slot)
        {
            state.history_id = Some(id);
            if let Some(running) = self.exec.running_commands.get_mut(exec_call_id) {
                running.history_index = Some(idx);
                running.history_id = Some(id);
            }
        }
    }

    fn merge_wait_end_history_state(&mut self, duration: Duration, state: &mut WaitEndState) {
        let Some(history_id) = state.history_id else {
            return;
        };

        let exec_record = self
            .history_state
            .index_of(history_id)
            .and_then(|idx| self.history_state.get(idx).cloned());
        if let Some(HistoryRecord::Exec(record)) = exec_record {
            state.exec_completed = !matches!(record.status, ExecStatus::Running);
            if state.wait_total.is_none() {
                let base = record.wait_total.unwrap_or_default();
                state.wait_total = Some(base.saturating_add(duration));
            }
            if state.wait_notes_snapshot.is_empty() {
                state.wait_notes_snapshot = Self::wait_pairs_from_exec_notes(&record.wait_notes);
                Self::append_wait_pairs(&mut state.wait_notes_snapshot, &state.note_lines);
            }
        } else {
            if state.wait_total.is_none() {
                state.wait_total = Some(duration);
            }
            if state.wait_notes_snapshot.is_empty() {
                Self::append_wait_pairs(&mut state.wait_notes_snapshot, &state.note_lines);
            }
        }

        if state.exec_completed || (state.wait_interrupted && !state.exec_running) {
            state.wait_still_pending = false;
        }
    }

    fn apply_wait_end_history_update(&mut self, state: &WaitEndState) {
        if !state.exec_completed
            && let Some(history_id) = state.history_id
        {
            let _ = self.update_exec_wait_state_with_pairs(
                history_id,
                state.wait_total,
                state.wait_still_pending,
                &state.wait_notes_snapshot,
            );
        }
    }

    fn finalize_wait_end_ui(
        &mut self,
        call_id: &str,
        exec_call_id: ExecCallId,
        success: bool,
        state: &WaitEndState,
    ) {
        if state.exec_completed {
            self.bottom_pane.update_status_text("responding".to_string());
            self.maybe_hide_spinner();
            self.invalidate_height_cache();
            self.request_redraw();
            return;
        }

        if success {
            self.remove_background_completion_message(call_id);
            self.bottom_pane.update_status_text("responding".to_string());
            self.maybe_hide_spinner();
        } else if state.trimmed == WAIT_CANCELLED_BY_USER {
            self.bottom_pane
                .update_status_text("wait cancelled".to_string());
        } else if state.wait_missing_job || (state.wait_interrupted && !state.exec_running) {
            let finalized =
                exec_tools::finalize_wait_missing_exec(self, exec_call_id, &state.trimmed);
            if finalized {
                self.bottom_pane
                    .update_status_text("command finished (output unavailable)".to_string());
            } else {
                self.bottom_pane
                    .update_status_text("command status unavailable".to_string());
            }
        } else {
            self.bottom_pane
                .update_status_text("waiting for command".to_string());
        }
        self.invalidate_height_cache();
        self.request_redraw();
    }
}
