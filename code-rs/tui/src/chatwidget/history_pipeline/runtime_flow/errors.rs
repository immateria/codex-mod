use super::*;

impl ChatWidget<'_> {
    pub(in super::super::super) fn on_error(&mut self, message: String) {
        // Treat transient stream errors (which the core will retry) differently
        // from fatal errors so the status spinner remains visible while we wait.
        let lower = message.to_lowercase();
        let is_transient = lower.contains("retrying")
            || lower.contains("reconnecting")
            || lower.contains("disconnected")
            || lower.contains("stream error")
            || lower.contains("stream closed")
            || lower.contains("timeout")
            || lower.contains("temporar")
            || lower.contains("transport")
            || lower.contains("network")
            || lower.contains("connection")
            || lower.contains("failed to start stream");

        if is_transient {
            self.mark_reconnecting(message);
            return;
        }

        // Ensure reconnect banners are cleared once we pivot to a fatal error
        // without emitting the "Reconnected" toast (which would be misleading).
        if self.reconnect_notice_active {
            self.reconnect_notice_active = false;
            self.bottom_pane.update_status_text(String::new());
            self.request_redraw();
        }

        if self.is_startup_mcp_error(&message) {
            self.clear_resume_placeholder();
            let summarized = Self::summarize_startup_mcp_error(&message);
            self.startup_mcp_error_summary = Some(summarized.clone());
            self.bottom_pane.flash_footer_notice_for(
                summarized,
                Duration::from_secs(10),
            );
            self.mark_needs_redraw();
            return;
        }

        // Error path: show an error cell and clear running state.
        self.clear_resume_placeholder();
        let key = self.next_internal_key();
        let state = history_cell::new_error_event(message.clone());
        let cell = crate::history_cell::PlainHistoryCell::from_state(state.clone());
        let _ = self.history_insert_with_key_global_tagged(
            Box::new(cell),
            key,
            "epilogue",
            Some(HistoryDomainRecord::Plain(state)),
        );
        let should_recover_auto = self.auto_state.is_active();
        self.bottom_pane.set_task_running(false);
        // Ensure any running exec/tool cells are finalized so spinners don't linger
        // after errors.
        self.finalize_all_running_as_interrupted();
        self.stream.clear_all();
        self.stream_state.drop_streaming = false;
        self.agents_ready_to_start = false;
        self.active_task_ids.clear();
        self.maybe_hide_spinner();
        if should_recover_auto {
            self.auto_pause_for_transient_failure(message);
        }
        self.mark_needs_redraw();
    }

    pub(in super::super::super) fn mark_reconnecting(&mut self, message: String) {
        // Keep task running and surface a concise status in the input header.
        self.bottom_pane.set_task_running(true);
        self.bottom_pane.update_status_text("Retrying...".to_string());

        if !self.reconnect_notice_active {
            self.reconnect_notice_active = true;
            self.push_background_tail(format!("Auto-retrying… ({message})"));
        }

        // Do NOT clear running state or streams; the retry will resume them.
        self.request_redraw();
    }

    pub(in super::super::super) fn clear_reconnecting(&mut self) {
        if !self.reconnect_notice_active {
            return;
        }
        self.reconnect_notice_active = false;
        self.bottom_pane.update_status_text(String::new());
        self.bottom_pane
            .flash_footer_notice_for("Resuming".to_string(), Duration::from_secs(2));
        self.request_redraw();
    }
}
