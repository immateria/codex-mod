impl ChatWidget<'_> {
    pub(crate) fn enable_perf(&mut self, enable: bool) {
        self.perf_state.enabled = enable;
    }
    pub(crate) fn perf_summary(&self) -> String {
        self.perf_state.stats.borrow().summary()
    }
    // Build an ordered key from model-provided OrderMeta. Callers must
    // guarantee presence by passing a concrete reference (compile-time guard).

    /// Show the "Shift+Up/Down" input history hint the first time the user scrolls.
    pub(super) fn maybe_show_history_nav_hint_on_first_scroll(&mut self) {
        if self.scroll_history_hint_shown {
            return;
        }
        self.scroll_history_hint_shown = true;
        self.bottom_pane.flash_footer_notice_for(
            "Use Shift+Up/Down to use previous input".to_string(),
            std::time::Duration::from_secs(6),
        );
    }

    pub(super) fn perf_track_scroll_delta(&self, before: u16, after: u16) {
        if !self.perf_state.enabled {
            return;
        }
        if before == after {
            return;
        }
        let delta = before.abs_diff(after) as u64;
        {
            let mut stats = self.perf_state.stats.borrow_mut();
            stats.record_scroll_trigger(delta);
        }
        let pending = self
            .perf_state
            .pending_scroll_rows
            .get()
            .saturating_add(delta);
        self.perf_state.pending_scroll_rows.set(pending);
    }

    /// Returns true if any agents are actively running (Pending or Running), or we're about to start them.
    /// Agents in terminal states (Completed/Failed) do not keep the spinner visible.
    fn agents_are_actively_running(&self) -> bool {
        let has_running_non_auto_review = self
            .active_agents
            .iter()
            .any(|a| {
                matches!(a.status, AgentStatus::Pending | AgentStatus::Running)
                    && !matches!(a.source_kind, Some(AgentSourceKind::AutoReview))
            });

        if has_running_non_auto_review {
            return true;
        }

        // If only Auto Review agents are active, don't drive the spinner.
        let has_running_auto_review = self
            .active_agents
            .iter()
            .any(|a| {
                matches!(a.status, AgentStatus::Pending | AgentStatus::Running)
                    && matches!(a.source_kind, Some(AgentSourceKind::AutoReview))
            });

        if has_running_auto_review {
            return false;
        }

        // Fall back to preparatory state (e.g., Auto Drive about to launch agents)
        self.agents_ready_to_start
    }

    fn has_cancelable_agents(&self) -> bool {
        self
            .active_agents
            .iter()
            .any(Self::agent_is_cancelable)
    }

    fn agent_is_cancelable(agent: &AgentInfo) -> bool {
        matches!(agent.status, AgentStatus::Pending | AgentStatus::Running)
            && !matches!(agent.source_kind, Some(AgentSourceKind::AutoReview))
    }

    fn collect_cancelable_agents(&self) -> (Vec<String>, Vec<String>) {
        let mut batch_ids: BTreeSet<String> = BTreeSet::new();
        let mut agent_ids: BTreeSet<String> = BTreeSet::new();

        for agent in &self.active_agents {
            if !Self::agent_is_cancelable(agent) {
                continue;
            }

            if let Some(batch) = agent.batch_id.as_ref() {
                let trimmed = batch.trim();
                if !trimmed.is_empty() {
                    batch_ids.insert(trimmed.to_string());
                    continue;
                }
            }

            let trimmed_id = agent.id.trim();
            if !trimmed_id.is_empty() {
                agent_ids.insert(trimmed_id.to_string());
            }
        }

        (
            batch_ids.into_iter().collect(),
            agent_ids.into_iter().collect(),
        )
    }

    fn cancel_active_agents(&mut self) -> bool {
        let (batch_ids, agent_ids) = self.collect_cancelable_agents();
        if batch_ids.is_empty() && agent_ids.is_empty() {
            return false;
        }

        let mut status_parts = Vec::new();
        if !batch_ids.is_empty() {
            let count = batch_ids.len();
            status_parts.push(if count == 1 {
                "1 batch".to_string()
            } else {
                format!("{count} batches")
            });
        }
        if !agent_ids.is_empty() {
            let count = agent_ids.len();
            status_parts.push(if count == 1 {
                "1 agent".to_string()
            } else {
                format!("{count} agents")
            });
        }

        let descriptor = if status_parts.is_empty() {
            "agents".to_string()
        } else {
            status_parts.join(", ")
        };
        let auto_active = self.auto_state.is_active();
        self.push_background_tail(format!("Cancelling {descriptor}…"));
        self.bottom_pane
            .update_status_text("Cancelling agents…".to_string());
        self.bottom_pane.set_task_running(true);
        self.submit_op(Op::CancelAgents { batch_ids, agent_ids });

        self.agents_ready_to_start = false;

        if auto_active {
            self.show_auto_drive_exit_hint();
        } else if self
            .bottom_pane
            .standard_terminal_hint()
            .is_some_and(|hint| hint == AUTO_ESC_EXIT_HINT || hint == AUTO_ESC_EXIT_HINT_DOUBLE)
        {
            self.bottom_pane.set_standard_terminal_hint(None);
        }
        self.request_redraw();

        true
    }

    /// Hide the bottom spinner/status if the UI is idle (no streams, tools, agents, or tasks).
    fn maybe_hide_spinner(&mut self) {
        let any_tools_running = !self.exec.running_commands.is_empty()
            || !self.tools_state.running_custom_tools.is_empty()
            || !self.tools_state.web_search_sessions.is_empty();
        let any_streaming = self.stream.is_write_cycle_active();
        let any_agents_active = self.agents_are_actively_running();
        let mut any_tasks_active = !self.active_task_ids.is_empty();
        let final_answer_seen =
            self.last_answer_history_id_in_turn.is_some() || self.stream_state.seq_answer_final.is_some();
        let terminal_running = self.terminal_is_running();

        // If the backend never emits TaskComplete but we already received the
        // final answer and no other activity is running, clear the spinner so
        // we don't stay stuck on "Thinking...".
        let stuck_on_completed_turn = any_tasks_active
            && final_answer_seen
            && !any_tools_running
            && !any_streaming
            && !any_agents_active
            && !terminal_running;
        if stuck_on_completed_turn {
            self.active_task_ids.clear();
            any_tasks_active = false;
            self.overall_task_status = "complete".to_string();
        }
        if !(any_tools_running
            || any_streaming
            || any_agents_active
            || any_tasks_active
            || terminal_running)
        {
            self.bottom_pane.set_task_running(false);
            self.bottom_pane.update_status_text(String::new());
        }
    }

    /// Ensure we show progress when work is visible but the spinner state drifted.
    fn ensure_spinner_for_activity(&mut self, reason: &'static str) {
        if self.bottom_pane.auto_drive_style_active()
            && !self.bottom_pane.auto_drive_view_active()
            && !self.bottom_pane.has_active_modal_view()
        {
            tracing::debug!(
                "Auto Drive style active without view; releasing style (reason: {reason})"
            );
            self.bottom_pane.release_auto_drive_style();
        }
        if !self.bottom_pane.is_task_running() {
            tracing::debug!("Activity without spinner; re-enabling (reason: {reason})");
            self.bottom_pane.set_task_running(true);
        }
    }

    #[inline]
    fn stop_spinner(&mut self) {
        self.bottom_pane.set_task_running(false);
        self.bottom_pane.update_status_text(String::new());
        self.maybe_hide_spinner();
    }

    #[cfg(any(test, feature = "test-helpers"))]
    fn seed_test_mode_greeting(&mut self) {
        if !self.test_mode {
            return;
        }
        let has_assistant = self
            .history_cells
            .iter()
            .any(|cell| matches!(cell.kind(), history_cell::HistoryCellType::Assistant));
        if has_assistant {
            return;
        }

        let sections = [
            "Hello! How can I help you today?",
            "I can help with various tasks including:\n\n- Writing code\n- Reading files\n- Running commands",
        ];

        for markdown in sections {
            let greeting_state = AssistantMessageState {
                id: HistoryId::ZERO,
                stream_id: None,
                markdown: markdown.to_string(),
                citations: Vec::new(),
                metadata: None,
                token_usage: None,
                mid_turn: false,
                created_at: SystemTime::now(),
            };
            let greeting_cell =
                history_cell::AssistantMarkdownCell::from_state(greeting_state, &self.config);
            self.history_push_top_next_req(greeting_cell);
        }
    }

    #[inline]
    fn overall_task_status_for(agents: &[AgentInfo]) -> &'static str {
        if agents.is_empty() {
            "preparing"
        } else if agents
            .iter()
            .any(|a| matches!(a.status, AgentStatus::Running))
        {
            "running"
        } else if agents
            .iter()
            .all(|a| matches!(a.status, AgentStatus::Completed))
        {
            "complete"
        } else if agents
            .iter()
            .any(|a| matches!(a.status, AgentStatus::Failed))
        {
            "failed"
        } else if agents
            .iter()
            .any(|a| matches!(a.status, AgentStatus::Cancelled))
        {
            "cancelled"
        } else {
            "planning"
        }
    }

    /// Mark all tracked agents as having reached a terminal state when a turn finishes.
    fn finalize_agent_activity(&mut self) {
        if self.active_agents.is_empty()
            && self.agent_runtime.is_empty()
            && self.agents_terminal.entries.is_empty()
        {
            self.agents_ready_to_start = false;
            return;
        }

        for agent in self.active_agents.iter_mut() {
            if matches!(agent.status, AgentStatus::Pending | AgentStatus::Running) {
                agent.status = AgentStatus::Completed;
            }
        }

        for entry in self.agents_terminal.entries.values_mut() {
            if matches!(entry.status, AgentStatus::Pending | AgentStatus::Running) {
                entry.status = AgentStatus::Completed;
                entry.push_log(
                    AgentLogKind::Status,
                    format!("Status → {}", agent_status_label(AgentStatus::Completed)),
                );
            }
        }

        self.agents_ready_to_start = false;
        let status = Self::overall_task_status_for(&self.active_agents);
        self.overall_task_status = status.to_string();
        self.bottom_pane.set_task_running(false);
        self.bottom_pane.update_status_text(String::new());
        self.maybe_hide_spinner();
    }


    fn remove_background_completion_message(&mut self, call_id: &str) {
        if let Some(idx) = self.history_cells.iter().rposition(|cell| {
            matches!(cell.kind(), HistoryCellType::BackgroundEvent)
                && cell
                    .as_any()
                    .downcast_ref::<PlainHistoryCell>()
                    .map(|plain| {
                        plain.state().lines.iter().any(|line| {
                            line.spans
                                .iter()
                                .any(|span| span.text.contains(call_id))
                        })
                    })
                    .unwrap_or(false)
        }) {
            self.history_remove_at(idx);
        }
    }


    /// Flush any ExecEnd events that arrived before their matching ExecBegin.
    /// We briefly stash such ends to allow natural pairing when the Begin shows up
    /// shortly after. If the pairing window expires, render a fallback completed
    /// Exec cell so users still see the output in history.
    pub(crate) fn flush_pending_exec_ends(&mut self) {
        use std::time::Duration;
        use std::time::Instant;
        let now = Instant::now();
        // Collect keys to avoid holding a mutable borrow while iterating
        let mut ready: Vec<ExecCallId> = Vec::new();
        for (k, (_ev, _order, t0)) in self.exec.pending_exec_ends.iter() {
            if now.saturating_duration_since(*t0) >= Duration::from_millis(110) {
                ready.push(k.clone());
            }
        }
        for key in &ready {
            if let Some((ev, order, _t0)) = self.exec.pending_exec_ends.remove(key) {
                // Regardless of whether a Begin has arrived by now, handle the End;
                // handle_exec_end_now pairs with a running Exec if present, or falls back.
                self.handle_exec_end_now(ev, &order);
            }
        }
        if !ready.is_empty() {
            self.request_redraw();
        }
    }

    /// Schedule a short-delay check to flush queued interrupts if the current
    /// stream stalls in an idle state. Avoids the UI appearing frozen when the
    /// model stops streaming before sending TaskComplete.
    fn schedule_interrupt_flush_check(&mut self) {
        if self.interrupt_flush_scheduled || !self.interrupts.has_queued() {
            return;
        }
        self.interrupt_flush_scheduled = true;
        let tx = self.app_event_tx.clone();
        let fallback_tx = tx.clone();
        if thread_spawner::spawn_lightweight("interrupt-flush", move || {
            std::thread::sleep(std::time::Duration::from_millis(180));
            tx.send(AppEvent::FlushInterruptsIfIdle);
        })
        .is_none()
        {
            fallback_tx.send(AppEvent::FlushInterruptsIfIdle);
        }
    }

    /// Finalize a stalled stream and flush queued interrupts once the stream is idle.
    /// Re-arms itself until either the stream clears or the queue drains.
    pub(crate) fn flush_interrupts_if_stream_idle(&mut self) {
        self.interrupt_flush_scheduled = false;
        if !self.stream.is_write_cycle_active() {
            if self.interrupts.has_queued() {
                self.flush_interrupt_queue();
                self.request_redraw();
            }
            return;
        }
        if self.stream.is_current_stream_idle() {
            streaming::finalize_active_stream(self);
            self.flush_interrupt_queue();
            self.request_redraw();
        } else if self.interrupts.has_queued() {
            // Still busy; try again shortly so we don't leave Exec/Tool updates stuck.
            self.schedule_interrupt_flush_check();
        }
    }

    fn finalize_all_running_as_interrupted(&mut self) {
        exec_tools::finalize_all_running_as_interrupted(self);
    }

    fn finalize_all_running_due_to_answer(&mut self) {
        exec_tools::finalize_all_running_due_to_answer(self);
    }

    fn ensure_lingering_execs_cleared(&mut self) {
        if self.cleared_lingering_execs_this_turn {
            return;
        }

        let nothing_running = self.exec.running_commands.is_empty()
            && self.tools_state.running_custom_tools.is_empty()
            && self.tools_state.running_wait_tools.is_empty()
            && self.tools_state.running_kill_tools.is_empty()
            && self.tools_state.web_search_sessions.is_empty();

        if nothing_running {
            self.cleared_lingering_execs_this_turn = true;
            return;
        }

        self.finalize_all_running_due_to_answer();
        self.cleared_lingering_execs_this_turn = true;
    }
    fn perf_label_for_item(&self, item: &dyn HistoryCell) -> String {
        use crate::history_cell::ExecKind;
        use crate::history::state::ExecStatus;
        use crate::history_cell::HistoryCellType;
        use crate::history_cell::PatchKind;
        use crate::history_cell::ToolCellStatus;
        match item.kind() {
            HistoryCellType::Plain => "Plain".to_string(),
            HistoryCellType::User => "User".to_string(),
            HistoryCellType::Assistant => "Assistant".to_string(),
            HistoryCellType::ProposedPlan => "ProposedPlan".to_string(),
            HistoryCellType::Reasoning => "Reasoning".to_string(),
            HistoryCellType::Error => "Error".to_string(),
            HistoryCellType::Exec { kind, status } => {
                let k = match kind {
                    ExecKind::Read => "Read",
                    ExecKind::Search => "Search",
                    ExecKind::List => "List",
                    ExecKind::Run => "Run",
                };
                let s = match status {
                    ExecStatus::Running => "Running",
                    ExecStatus::Success => "Success",
                    ExecStatus::Error => "Error",
                };
                format!("Exec:{k}:{s}")
            }
            HistoryCellType::Tool { status } => {
                let s = match status {
                    ToolCellStatus::Running => "Running",
                    ToolCellStatus::Success => "Success",
                    ToolCellStatus::Failed => "Failed",
                };
                format!("Tool:{s}")
            }
            HistoryCellType::Patch { kind } => {
                let k = match kind {
                    PatchKind::Proposed => "Proposed",
                    PatchKind::ApplyBegin => "ApplyBegin",
                    PatchKind::ApplySuccess => "ApplySuccess",
                    PatchKind::ApplyFailure => "ApplyFailure",
                };
                format!("Patch:{k}")
            }
            HistoryCellType::PlanUpdate => "PlanUpdate".to_string(),
            HistoryCellType::BackgroundEvent => "BackgroundEvent".to_string(),
            HistoryCellType::Notice => "Notice".to_string(),
            HistoryCellType::CompactionSummary => "CompactionSummary".to_string(),
            HistoryCellType::Diff => "Diff".to_string(),
            HistoryCellType::Image => "Image".to_string(),
            HistoryCellType::Context => "Context".to_string(),
            HistoryCellType::AnimatedWelcome => "AnimatedWelcome".to_string(),
            HistoryCellType::Loading => "Loading".to_string(),
            HistoryCellType::JsRepl { status } => {
                let s = match status {
                    ExecStatus::Running => "Running",
                    ExecStatus::Success => "Success",
                    ExecStatus::Error => "Error",
                };
                format!("JsRepl:{s}")
            }
        }
    }



    fn request_redraw(&mut self) {
        self.app_event_tx.send(AppEvent::RequestRedraw);
    }

    /// Notify the height manager that the bottom pane view has changed.
    /// This bypasses hysteresis so the new view's height is applied immediately.
    pub(crate) fn notify_bottom_pane_view_changed(&mut self) {
        self.height_manager
            .borrow_mut()
            .record_event(HeightEvent::ComposerModeChange);
    }

}
