use super::*;

impl ChatWidget<'_> {
    pub(in super::super::super) fn interrupt_running_task(&mut self) {
        let bottom_running = self.bottom_pane.is_task_running();
        let wait_running = self.wait_running();
        if !self.is_task_running() && !wait_running {
            return;
        }

        // If the user cancels mid-turn while Auto Review is enabled, preserve the
        // captured baseline so a review still runs after the next turn completes.
        if self.config.tui.auto_review_enabled
            && self.pending_auto_review_range.is_none()
            && self.background_review.is_none()
            && let Some(base) = self.auto_review_baseline.take() {
                self.pending_auto_review_range = Some(PendingAutoReviewRange {
                    base,
                    // Defer to the next turn so cancellation doesn’t immediately
                    // trigger auto-review in the same (cancelled) turn.
                    defer_until_turn: Some(self.turn_sequence),
                });
            }

        let mut has_wait_running = false;
        for (call_id, entry) in self.tools_state.running_custom_tools.iter() {
            if let Some(idx) = running_tools::resolve_entry_index(self, entry, &call_id.0)
                && let Some(cell) = self.history_cells.get(idx).and_then(|c| c
                    .as_any()
                    .downcast_ref::<history_cell::RunningToolCallCell>())
                    && cell.has_title("Waiting") {
                        has_wait_running = true;
                        break;
                    }
        }

        self.active_exec_cell = None;
        // Finalize any visible running indicators as interrupted (Exec/Web/Custom)
        self.finalize_all_running_as_interrupted();
        if bottom_running {
            self.bottom_pane.clear_ctrl_c_quit_hint();
        }
        // Stop any active UI streams immediately so output ceases at once.
        self.finalize_active_stream();
        self.stream_state.drop_streaming = true;
        // Surface an explicit notice in history so users see confirmation.
        if !has_wait_running {
            self.push_background_tail("Cancelled by user.".to_string());
        }
        self.submit_op(Op::Interrupt);
        // Immediately drop the running status so the next message can be typed/run,
        // even if backend cleanup (and Error event) arrives slightly later.
        self.bottom_pane.set_task_running(false);
        self.bottom_pane.clear_live_ring();
        // Reset with max width to disable wrapping
        self.live_builder = RowBuilder::new(usize::MAX);
        // Stream state is now managed by StreamController
        self.content_buffer.clear();
        // Defensive: clear transient flags so UI can quiesce
        self.agents_ready_to_start = false;
        self.active_task_ids.clear();
        // Restore any queued messages back into the composer so the user can
        // immediately press Enter to resume the conversation where they left off.
        if !self.queued_user_messages.is_empty() {
            let existing_input = self.bottom_pane.composer_text();
            let mut segments: Vec<String> = Vec::new();

            let mut queued_block = String::new();
            for (i, qm) in self.queued_user_messages.iter().enumerate() {
                if i > 0 {
                    queued_block.push_str("\n\n");
                }
                queued_block.push_str(qm.display_text.trim_end());
            }
            if !queued_block.trim().is_empty() {
                segments.push(queued_block);
            }

            if !existing_input.trim().is_empty() {
                segments.push(existing_input);
            }

            let combined = segments.join("\n\n");
            self.clear_composer();
            if !combined.is_empty() {
                self.insert_str(&combined);
            }
            self.queued_user_messages.clear();
            self.bottom_pane.update_status_text(String::new());
            self.pending_dispatched_user_messages.clear();
            self.refresh_queued_user_messages(false);
        }
        self.maybe_hide_spinner();
        self.request_redraw();
    }
}
