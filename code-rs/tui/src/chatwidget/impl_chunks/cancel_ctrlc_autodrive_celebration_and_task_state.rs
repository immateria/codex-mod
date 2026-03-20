impl ChatWidget<'_> {
    /// Handle Ctrl-C key press.
    /// Returns CancellationEvent::Handled if the event was consumed by the UI, or
    /// CancellationEvent::Ignored if the caller should handle it (e.g. exit).
    pub(crate) fn on_ctrl_c(&mut self) -> CancellationEvent {
        if let Some(id) = self.terminal_overlay_id() {
            if self.terminal_is_running() {
                self.request_terminal_cancel(id);
            } else {
                self.close_terminal_overlay();
            }
            return CancellationEvent::Handled;
        }
        match self.bottom_pane.on_ctrl_c() {
            CancellationEvent::Handled => return CancellationEvent::Handled,
            CancellationEvent::Ignored => {}
        }
        if self.is_task_running() || self.wait_running() {
            self.interrupt_running_task();
            CancellationEvent::Ignored
        } else if self.bottom_pane.ctrl_c_quit_hint_visible() {
            self.submit_op(Op::Shutdown);
            CancellationEvent::Handled
        } else {
            self.bottom_pane.show_ctrl_c_quit_hint();
            CancellationEvent::Ignored
        }
    }

    pub(crate) fn composer_is_empty(&self) -> bool {
        self.bottom_pane.composer_is_empty()
    }

    // --- Double‑Escape helpers ---
    fn schedule_auto_drive_card_celebration(
        &self,
        delay: Duration,
        message: Option<String>,
    ) {
        let event = AppEvent::StartAutoDriveCelebration { message };
        self.spawn_app_event_after(delay, event);
    }

    pub(crate) fn start_auto_drive_card_celebration(&mut self, message: Option<String>) {
        let mut started = auto_drive_cards::start_celebration(self, message.clone());
        if !started
            && let Some(card) = self.latest_auto_drive_card_mut() {
                card.start_celebration(message.clone());
                started = true;
            }
        if !started {
            return;
        }

        self.spawn_app_event_after(
            AUTO_COMPLETION_CELEBRATION_DURATION,
            AppEvent::StopAutoDriveCelebration,
        );

        if let Some(msg) = message
            && !auto_drive_cards::update_completion_message(self, Some(msg.clone()))
                && let Some(card) = self.latest_auto_drive_card_mut() {
                    card.set_completion_message(Some(msg));
                }

        self.mark_history_dirty();
        self.request_redraw();
    }

    pub(crate) fn stop_auto_drive_card_celebration(&mut self) {
        let mut stopped = auto_drive_cards::stop_celebration(self);
        if !stopped
            && let Some(card) = self.latest_auto_drive_card_mut() {
                card.stop_celebration();
                stopped = true;
            }
        if stopped {
            self.mark_history_dirty();
            self.request_redraw();
        }
    }

    fn spawn_app_event_after(&self, delay: Duration, event: AppEvent) {
        if delay.is_zero() {
            self.app_event_tx.send(event);
            return;
        }

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let tx = self.app_event_tx.clone();
            handle.spawn(async move {
                tokio::time::sleep(delay).await;
                tx.send(event);
            });
        } else {
            #[cfg(test)]
            {
                let tx = self.app_event_tx.clone();
                if let Err(err) = std::thread::Builder::new()
                    .name("delayed-app-event".to_string())
                    .spawn(move || {
                        tx.send(event);
                    })
                {
                    tracing::warn!("failed to spawn delayed app event: {err}");
                }
            }
            #[cfg(not(test))]
            {
                let _ = event;
            }
        }
    }

    fn latest_auto_drive_card_mut(
        &mut self,
    ) -> Option<&mut history_cell::AutoDriveCardCell> {
        self.history_cells
            .iter_mut()
            .rev()
            .find_map(|cell| cell.as_any_mut().downcast_mut::<history_cell::AutoDriveCardCell>())
    }

    pub(crate) fn auto_manual_entry_active(&self) -> bool {
        self.auto_state.should_show_goal_entry()
            || (self.auto_state.is_active() && self.auto_state.awaiting_coordinator_submit())
    }

    fn has_running_commands_or_tools(&self) -> bool {
        let wait_running = self.wait_running();
        let wait_blocks = self.wait_blocking_enabled();

        self.terminal_is_running()
            || !self.exec.running_commands.is_empty()
            || !self.tools_state.running_custom_tools.is_empty()
            || !self.tools_state.web_search_sessions.is_empty()
            || (wait_running && wait_blocks)
    }

    pub(crate) fn is_task_running(&self) -> bool {
        let wait_running = self.wait_running();
        let wait_blocks = self.wait_blocking_enabled();

        self.bottom_pane.is_task_running()
            || self.terminal_is_running()
            || !self.exec.running_commands.is_empty()
            || !self.tools_state.running_custom_tools.is_empty()
            || !self.tools_state.web_search_sessions.is_empty()
            || !self.active_task_ids.is_empty()
            || self.stream.is_write_cycle_active()
            || (wait_running && wait_blocks)
    }

    #[inline]
    fn wait_running(&self) -> bool {
        !self.tools_state.running_wait_tools.is_empty()
            || !self.tools_state.running_kill_tools.is_empty()
    }

    #[inline]
    fn wait_blocking_enabled(&self) -> bool {
        self.queued_user_messages.is_empty()
    }

    /// True when the only ongoing activity is a wait/kill tool (no exec/stream/agents/tasks),
    /// meaning we can safely unlock the composer without cancelling the work.
    ///
    /// Historically this returned false whenever any exec was running, which caused user input
    /// submitted during a `wait` tool to be queued instead of interrupting the wait. That meant
    /// the core never received `Op::UserInput`, so waits could not be cancelled mid-flight.
    /// We treat execs that are only being observed by a wait tool as "wait-only" so input can
    /// flow through immediately and interrupt the wait.
    fn wait_only_activity(&self) -> bool {
        if !self.wait_running() {
            return false;
        }

        // Consider execs "wait-only" when every running command is being waited on and marked
        // as such. Any other exec activity keeps the composer blocked.
        let execs_wait_only = self.exec.running_commands.is_empty()
            || self
                .exec
                .running_commands
                .iter()
                .all(|(id, cmd)| {
                    cmd.wait_active
                        && self
                            .tools_state
                            .running_wait_tools
                            .values()
                            .any(|wait_id| wait_id == id)
                });

        execs_wait_only
            && self.tools_state.running_custom_tools.is_empty()
            && self.tools_state.web_search_sessions.is_empty()
            && !self.stream.is_write_cycle_active()
            && !self.agents_are_actively_running()
            && self.active_task_ids.is_empty()
    }

    /// If queued user messages have been blocked longer than the SLA while only a wait/kill
    /// tool is running, unlock the composer and dispatch the queue.
    fn maybe_enforce_queue_unblock(&mut self) {
        if self.queued_user_messages.is_empty() {
            self.queue_block_started_at = None;
            return;
        }

        let Some(started) = self.queue_block_started_at else {
            self.queue_block_started_at = Some(Instant::now());
            return;
        };

        if started.elapsed() < Duration::from_secs(10) {
            return;
        }

        if !self.wait_only_activity() {
            // Another activity is running; keep waiting.
            return;
        }

        let wait_ids: Vec<String> = self
            .tools_state
            .running_wait_tools
            .keys()
            .map(|k| k.0.clone())
            .collect();

        tracing::warn!(
            "queue watchdog fired; unblocking input (waits={:?}, queued={})",
            wait_ids,
            self.queued_user_messages.len()
        );

        self.bottom_pane.set_task_running(false);
        self.bottom_pane
            .update_status_text("Waiting in background".to_string());

        if !wait_ids.is_empty() {
            self.push_background_tail(format!(
                "Input unblocked after 10s; wait still running ({}).",
                wait_ids.join(", ")
            ));
        } else {
            self.push_background_tail("Input unblocked after 10s; wait still running.".to_string());
        }

        if let Some(front) = self.queued_user_messages.front().cloned() {
            self.dispatch_queued_user_message_now(front);
        }

        // Reset timer only if messages remain; otherwise leave it cleared so the next queue
        // submission can schedule a fresh watchdog.
        if self.queued_user_messages.is_empty() {
            self.queue_block_started_at = None;
        } else {
            self.queue_block_started_at = Some(Instant::now());
        }
        self.request_redraw();
    }

    /// Clear the composer text and any pending paste placeholders/history cursors.
    pub(crate) fn clear_composer(&mut self) {
        self.bottom_pane.clear_composer();
        if self.auto_state.should_show_goal_entry() {
            self.auto_goal_escape_state = AutoGoalEscState::Inactive;
        }
        // Mark a height change so layout adjusts immediately if the composer shrinks.
        self.height_manager
            .borrow_mut()
            .record_event(crate::height_manager::HeightEvent::ComposerModeChange);
        self.request_redraw();
    }

    /// Forward an `Op` directly to codex.
    pub(crate) fn submit_op(&self, op: Op) {
        if let Err(e) = self.code_op_tx.send(op) {
            tracing::error!("failed to submit op: {e}");
        }
    }

    /// Cancel the current running task from a non-keyboard context (e.g. approval modal).
    /// This bypasses modal key handling and invokes the same immediate UI cleanup path
    /// as pressing Ctrl-C/Esc while a task is running.
    pub(crate) fn cancel_running_task_from_approval(&mut self) {
        self.interrupt_running_task();
    }

    /// Stop any in-flight turn (Auto Drive, agents, streaming responses) before
    /// starting a brand new chat so that stale output cannot leak into the new
    /// conversation.
    pub(crate) fn abort_active_turn_for_new_chat(&mut self) {
        if self.has_cancelable_agents() {
            self.cancel_active_agents();
        }

        if self.auto_state.is_active() {
            self.auto_stop(None);
        }

        self.interrupt_running_task();
        self.finalize_active_stream();
        self.stream_state.drop_streaming = true;
        self.bottom_pane.set_task_running(false);
        self.maybe_hide_spinner();
    }

    pub(crate) fn register_approved_command(
        &self,
        command: Vec<String>,
        match_kind: ApprovedCommandMatchKind,
        semantic_prefix: Option<Vec<String>>,
    ) {
        if command.is_empty() {
            return;
        }
        let op = Op::RegisterApprovedCommand {
            command,
            match_kind,
            semantic_prefix,
        };
        self.submit_op(op);
    }

    /// Clear transient spinner/status after a denial without interrupting core
    /// execution. Only hide the spinner when there is no remaining activity so
    /// we avoid masking in-flight work (e.g. follow-up reasoning).
    pub(crate) fn mark_task_idle_after_denied(&mut self) {
        let any_tools_running = !self.exec.running_commands.is_empty()
            || !self.tools_state.running_custom_tools.is_empty()
            || !self.tools_state.web_search_sessions.is_empty();
        let any_streaming = self.stream.is_write_cycle_active();
        let any_agents_active = self.agents_are_actively_running();
        let any_tasks_active = !self.active_task_ids.is_empty();

        if !(any_tools_running || any_streaming || any_agents_active || any_tasks_active) {
            self.bottom_pane.set_task_running(false);
            self.bottom_pane.update_status_text(String::new());
            self.bottom_pane.clear_ctrl_c_quit_hint();
            self.mark_needs_redraw();
        }
    }

}
