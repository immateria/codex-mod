use super::*;
use code_core::protocol::OrderMeta;

impl ChatWidget<'_> {
    pub(super) fn handle_task_started_event(&mut self, id: String) {
        // Defensive: if the previous turn never emitted TaskComplete (e.g. dropped event
        // due to reconnect), `active_task_ids` can stay non-empty. That makes every
        // subsequent Answer look like "mid-turn" forever and keeps the footer spinner
        // stuck.
        if let Some(last_id) = self.last_seen_answer_stream_id_in_turn.clone()
            && (self.mid_turn_answer_ids_in_turn.contains(&last_id)
                || !self.active_task_ids.is_empty())
        {
            self.mid_turn_answer_ids_in_turn.remove(&last_id);
            self.maybe_clear_mid_turn_for_last_answer(&last_id);
        }
        if !self.active_task_ids.is_empty() {
            tracing::warn!(
                "TaskStarted id={} while {} task(s) still active; assuming stale turn state",
                id,
                self.active_task_ids.len()
            );
            self.active_task_ids.clear();
        }
        // Reset per-turn cleanup guard and clear any lingering running
        // exec/tool cells if the prior turn never sent TaskComplete.
        // This runs once per turn and is intentionally later than
        // ToolEnd to avoid the earlier regression where we finalized
        // after every tool call.
        self.turn_sequence = self.turn_sequence.saturating_add(1);
        self.turn_had_code_edits = false;
        self.current_turn_origin = self.pending_turn_origin.take();
        self.cleared_lingering_execs_this_turn = false;
        self.ensure_lingering_execs_cleared();

        self.clear_reconnecting();
        // This begins the new turn; clear the pending prompt anchor count
        // so subsequent background events use standard placement.
        self.pending_user_prompts_for_next_turn = 0;
        self.pending_request_user_input = None;
        // Reset stream headers for new turn.
        self.stream.reset_headers_for_new_turn();
        self.stream_state.current_kind = None;
        self.stream_state.seq_answer_final = None;
        self.last_answer_stream_id_in_turn = None;
        self.last_answer_history_id_in_turn = None;
        self.last_seen_answer_stream_id_in_turn = None;
        self.mid_turn_answer_ids_in_turn.clear();
        // New turn: clear closed id guards.
        self.stream_state.closed_answer_ids.clear();
        self.stream_state.closed_reasoning_ids.clear();
        self.clear_answer_stream_markup_tracking();
        self.ended_call_ids.clear();
        self.bottom_pane.clear_ctrl_c_quit_hint();
        // Accept streaming again for this turn.
        self.stream_state.drop_streaming = false;
        // Mark this task id as active and ensure the status stays visible.
        self.active_task_ids.insert(id.clone());
        self.turn_sleep_inhibitor.set_turn_running(true);
        // Reset per-turn UI indicators; ordering is now global-only.
        self.reasoning_index.clear();
        self.bottom_pane.set_task_running(true);
        self.bottom_pane
            .update_status_text("waiting for model".to_string());
        self.ensure_spinner_for_activity("task-started");
        tracing::info!("[order] EventMsg::TaskStarted id={}", id);

        // Capture a baseline snapshot for this turn so background auto review only
        // covers changes made during the turn, not pre-existing local edits.
        self.auto_review_baseline = None;
        if self.config.tui.auto_review_enabled {
            self.spawn_auto_review_baseline_capture();
        }

        // Don't add loading cell - we have progress in the input area.
        // self.add_to_history(history_cell::new_loading_cell("waiting for model".to_string()));

        self.mark_needs_redraw();
    }

    pub(super) fn handle_task_complete_event(
        &mut self,
        id: String,
        last_agent_message: Option<String>,
        order: Option<OrderMeta>,
    ) {
        self.clear_reconnecting();
        self.pending_request_user_input = None;
        let had_running_execs = !self.exec.running_commands.is_empty();
        // Finalize any active streams.
        let finalizing_streams = self.stream.is_write_cycle_active();
        if finalizing_streams {
            // Finalize both streams via streaming facade.
            streaming::finalize(self, StreamKind::Reasoning, true);
            streaming::finalize(self, StreamKind::Answer, true);
        }
        // Remove this id from the active set (it may be a sub-agent).
        self.active_task_ids.remove(&id);
        if self.active_task_ids.is_empty() {
            self.turn_sleep_inhibitor.set_turn_running(false);
        }
        if !finalizing_streams
            && self.active_task_ids.is_empty()
            && let Some(last_id) = self.last_seen_answer_stream_id_in_turn.clone()
        {
            self.mid_turn_answer_ids_in_turn.remove(&last_id);
            self.maybe_clear_mid_turn_for_last_answer(&last_id);
        }
        if self.auto_resolve_enabled() {
            self.auto_resolve_on_task_complete(last_agent_message.clone());
        }
        // Defensive: mark any lingering agent state as complete so the spinner can quiesce.
        self.finalize_agent_activity();
        // Convert any lingering running exec/tool cells to completed so the UI doesn't hang.
        self.finalize_all_running_due_to_answer();
        // Mark any running web searches as completed.
        web_search_sessions::finalize_all_failed(self, "Search cancelled before completion");
        if had_running_execs {
            self.insert_background_event_with_placement(
                "Running commands finalized after turn end.".to_string(),
                BackgroundPlacement::Tail,
                order,
            );
        }
        // Now that streaming is complete, flush any queued interrupts.
        self.flush_interrupt_queue();

        // Only drop the working status if nothing is actually running.
        let any_tools_running = !self.exec.running_commands.is_empty()
            || !self.tools_state.running_custom_tools.is_empty()
            || !self.tools_state.web_search_sessions.is_empty();
        let any_streaming = self.stream.is_write_cycle_active();
        let any_agents_active = self.agents_are_actively_running();
        let any_tasks_active = !self.active_task_ids.is_empty();

        if !(any_tools_running || any_streaming || any_agents_active || any_tasks_active) {
            self.bottom_pane.set_task_running(false);
            // Ensure any transient footer text like "responding" is cleared when truly idle.
            self.bottom_pane.update_status_text(String::new());
        }
        self.stream_state.current_kind = None;
        // Final re-check for idle state.
        self.maybe_hide_spinner();
        self.maybe_trigger_auto_review();
        self.emit_turn_complete_notification(last_agent_message);
        self.suppress_next_agent_hint = false;
        self.mark_needs_redraw();
        self.flush_history_snapshot_if_needed(true);
    }
}
