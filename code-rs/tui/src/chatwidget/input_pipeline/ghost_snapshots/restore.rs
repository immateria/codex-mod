impl ChatWidget<'_> {
    pub(in super::super) fn reset_resume_order_anchor(&mut self) {
        if self.history_cells.is_empty() {
            self.resume_expected_next_request = None;
        } else {
            let max_req = self
                .cell_order_seq
                .iter()
                .map(|key| key.req)
                .max()
                .unwrap_or(0);
            self.resume_expected_next_request = Some(max_req.saturating_add(1));
        }
        self.order_request_bias = 0;
        self.resume_provider_baseline = None;
    }

    pub(crate) fn restore_history_snapshot(&mut self, snapshot: &HistorySnapshot) {
        let perf_timer = self.perf_state.enabled.then(Instant::now);
        let preserved_system_entries: Vec<(String, HistoryId)> = self
            .system_cell_by_id
            .iter()
            .filter_map(|(key, &idx)| {
                self.history_cell_ids
                    .get(idx)
                    .and_then(|maybe| maybe.map(|hid| (key.clone(), hid)))
            })
            .collect();
        self.history_debug(format!(
            "restore_history_snapshot.start records={} cells_before={} order_before={}",
            snapshot.records.len(),
            self.history_cells.len(),
            self.cell_order_seq.len()
        ));
        self.history_state.restore(snapshot);

        self.history_render.invalidate_all();
        self.mark_render_requests_dirty();
        self.history_prefix_append_only.set(false);

        self.history_cells.clear();
        self.history_cell_ids.clear();
        self.history_live_window = None;
        self.history_frozen_width = 0;
        self.history_frozen_count = 0;
        self.history_virtualization_sync_pending.set(false);
        self.cell_order_seq.clear();
        self.cell_order_dbg.clear();

        for record in &self.history_state.records {
            if let Some(mut cell) = self.build_cell_from_record(record) {
                let id = record.id();
                Self::assign_history_id_inner(&mut cell, id);
                self.history_cells.push(cell);
                self.history_cell_ids.push(Some(id));
            } else {
                tracing::warn!("unable to rebuild history cell for record id {:?}", record.id());
                let fallback = history_cell::new_background_event(format!(
                    "Restored snapshot missing renderer for record {:?}",
                    record.id()
                ));
                self.history_cells.push(Box::new(fallback));
                self.history_cell_ids.push(None);
            }
        }

        if !snapshot.order.is_empty() {
            self.cell_order_seq = snapshot
                .order
                .iter()
                .copied()
                .map(OrderKey::from)
                .collect();
        } else {
            self.cell_order_seq = self
                .history_cells
                .iter()
                .enumerate()
                .map(|(idx, _)| OrderKey {
                    req: (idx as u64).saturating_add(1),
                    out: i32::MAX,
                    seq: (idx as u64).saturating_add(1),
                })
                .collect();
        }

        if self.cell_order_seq.len() < self.history_cells.len() {
            let mut next_req = self
                .cell_order_seq
                .iter()
                .map(|key| key.req)
                .max()
                .unwrap_or(0);
            let mut next_seq = self
                .cell_order_seq
                .iter()
                .map(|key| key.seq)
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            while self.cell_order_seq.len() < self.history_cells.len() {
                next_req = next_req.saturating_add(1);
                self.cell_order_seq.push(OrderKey {
                    req: next_req,
                    out: i32::MAX,
                    seq: next_seq,
                });
                next_seq = next_seq.saturating_add(1);
            }
        }

        if !snapshot.order_debug.is_empty() {
            self.cell_order_dbg = snapshot.order_debug.clone();
        }
        if self.cell_order_dbg.len() < self.history_cells.len() {
            self.cell_order_dbg
                .resize(self.history_cells.len(), None);
        }

        let max_req = self.cell_order_seq.iter().map(|key| key.req).max().unwrap_or(0);
        let max_seq = self.cell_order_seq.iter().map(|key| key.seq).max().unwrap_or(0);

        self.last_seen_request_index = max_req;
        self.current_request_index = max_req;
        self.internal_seq = max_seq;
        self.last_assigned_order = self.cell_order_seq.iter().copied().max();
        self.reset_resume_order_anchor();

        self.rebuild_ui_background_seq_counters();

        running_tools::rehydrate(self);
        self.rehydrate_system_order_cache(&preserved_system_entries);

        self.bottom_pane
            .set_has_chat_history(!self.history_cells.is_empty());
        self.refresh_reasoning_collapsed_visibility();
        self.refresh_explore_trailing_flags();
        self.invalidate_height_cache();
        self.request_redraw();

        if let (true, Some(started)) = (self.perf_state.enabled, perf_timer) {
            let elapsed = started.elapsed().as_nanos();
            self.perf_state
                .stats
                .borrow_mut()
                .record_undo_restore(elapsed);
        }
        self.history_snapshot_dirty = true;
        self.history_snapshot_last_flush = None;

        self.history_debug(format!(
            "restore_history_snapshot.done cells={} order={} system_cells={}",
            self.history_cells.len(),
            self.cell_order_seq.len(),
            self.system_cell_by_id.len()
        ));
    }

    pub(crate) fn perform_undo_restore(
        &mut self,
        commit: Option<&str>,
        restore_files: bool,
        restore_conversation: bool,
    ) {
        let Some(commit_id) = commit else {
            self.push_background_tail("No snapshot selected.".to_string());
            return;
        };

        let Some((index, snapshot)) = self
            .ghost_snapshots
            .iter()
            .enumerate()
            .find(|(_, snap)| snap.commit().id() == commit_id)
            .map(|(idx, snap)| (idx, snap.clone()))
        else {
            self.push_background_tail(
                "Selected snapshot is no longer available.".to_string(),
            );
            return;
        };

        if !restore_files && !restore_conversation {
            self.push_background_tail("No restore options selected.".to_string());
            return;
        }

        let mut files_restored = false;
        let mut conversation_rewind_requested = false;
        let mut errors: Vec<String> = Vec::new();
        let mut pre_restore_snapshot: Option<GhostSnapshot> = None;

        if restore_files {
            let previous_len = self.ghost_snapshots.len();
            let pre_summary = Some("Pre-undo checkpoint".to_string());
            let captured_snapshot = self.capture_ghost_snapshot_blocking(pre_summary);
            let added_snapshot = self.ghost_snapshots.len() > previous_len;
            if let Some(snapshot) = captured_snapshot {
                pre_restore_snapshot = Some(snapshot);
            }

            match restore_ghost_commit(&self.config.cwd, snapshot.commit()) {
                Ok(()) => {
                    files_restored = true;
                    self.ghost_snapshots.truncate(index);
                    if let Some(pre) = pre_restore_snapshot {
                        self.ghost_snapshots.push(pre);
                        if self.ghost_snapshots.len() > MAX_TRACKED_GHOST_COMMITS {
                            self.ghost_snapshots.remove(0);
                        }
                    }
                }
                Err(err) => {
                    if added_snapshot && !self.ghost_snapshots.is_empty() {
                        self.ghost_snapshots.pop();
                    }
                    errors.push(format!("Failed to restore workspace files: {err}"));
                }
            }
        }

        if restore_conversation {
            let (user_delta, assistant_delta) =
                self.conversation_delta_since(&snapshot.conversation);
            if user_delta == 0 {
                self.push_background_tail(
                    "Conversation already matches selected snapshot; nothing to rewind.".to_string(),
                );
            } else {
                self.app_event_tx.send(AppEvent::JumpBack {
                    nth: user_delta,
                    prefill: String::new(),
                    history_snapshot: Some(snapshot.history.clone()),
                });
                if assistant_delta > 0 {
                    self.push_background_tail(format!(
                        "Rewinding conversation by {} user turn{} and {} assistant repl{}",
                        user_delta,
                        if user_delta == 1 { "" } else { "s" },
                        assistant_delta,
                        if assistant_delta == 1 { "y" } else { "ies" }
                    ));
                } else {
                    self.push_background_tail(format!(
                        "Rewinding conversation by {} user turn{}",
                        user_delta,
                        if user_delta == 1 { "" } else { "s" }
                    ));
                }
                conversation_rewind_requested = true;
            }
        }

        for err in errors {
            self.history_push_plain_state(history_cell::new_error_event(err));
        }

        if files_restored {
            let mut message = format!("Restored workspace files to snapshot {}", snapshot.short_id());
            if let Some(snippet) = snapshot.summary_snippet(60) {
                message.push_str(&format!(" • {snippet}"));
            }
            if let Some(age) = snapshot.age_from(Local::now()) {
                message.push_str(&format!(" • captured {} ago", format_duration(age)));
            }
            if !restore_conversation {
                message.push_str(" • chat history unchanged");
            }
            self.push_background_tail(message);
        }

        if conversation_rewind_requested {
            // Ensure Auto Drive state does not point at the old session after a conversation rewind.
            // If we leave it active, subsequent user messages may be routed to a stale coordinator
            // handle and appear to "not go through".
            if self.auto_state.is_active() || self.auto_handle.is_some() {
                self.auto_stop(Some("Auto Drive reset after /undo restore.".to_string()));
                self.auto_handle = None;
                self.auto_history.clear();
            }

            // Conversation rewind will reload the chat widget via AppEvent::JumpBack.
            self.reset_after_conversation_restore();
        } else {
            // Even when only files are restored, clear any pending user prompts or transient state
            // so subsequent messages flow normally.
            self.reset_after_conversation_restore();
        }

        self.request_redraw();
    }

    pub(in super::super) fn reset_after_conversation_restore(&mut self) {
        self.pending_dispatched_user_messages.clear();
        self.pending_user_prompts_for_next_turn = 0;
        self.queued_user_messages.clear();
        self.refresh_queued_user_messages(false);
        self.bottom_pane.clear_composer();
        self.bottom_pane.clear_ctrl_c_quit_hint();
        self.bottom_pane.set_task_running(false);
        self.active_task_ids.clear();
        if !self.agents_terminal.active {
            self.bottom_pane.ensure_input_focus();
        }
    }

    pub(in super::super) fn flush_pending_agent_notes(&mut self) {
        for note in self.pending_agent_notes.drain(..) {
            if let Err(e) = self.code_op_tx.send(Op::AddToHistory { text: note }) {
                tracing::error!("failed to send AddToHistory op: {e}");
            }
        }
    }

    pub(in super::super) fn finalize_sent_user_message(&mut self, message: UserMessage) {
        let UserMessage {
            display_text,
            ordered_items,
            suppress_persistence,
        } = message;

        let combined_message_text = {
            let mut buffer = String::new();
            for item in &ordered_items {
                if let InputItem::Text { text } = item {
                    if !buffer.is_empty() {
                        buffer.push('\n');
                    }
                    buffer.push_str(text);
                }
            }
            let trimmed = buffer.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };

        if !display_text.is_empty() {
            let key = self.next_req_key_prompt();
            let state = history_cell::new_user_prompt(display_text.clone());
            let _ = self.history_insert_plain_state_with_key(state, key, "prompt");
            self.pending_user_prompts_for_next_turn =
                self.pending_user_prompts_for_next_turn.saturating_add(1);
        }

        self.flush_pending_agent_notes();

        if let Some(model_echo) = combined_message_text {
            self.pending_dispatched_user_messages.push_back(model_echo);
        }

        let suppress_history = suppress_persistence;

        if !display_text.is_empty() && !suppress_history
            && let Err(e) = self
                .code_op_tx
                .send(Op::AddToHistory { text: display_text })
            {
                tracing::error!("failed to send AddHistory op: {e}");
            }

        if self.auto_state.is_active() && self.auto_state.resume_after_submit() {
            self.auto_state.on_prompt_submitted();
            self.auto_state.seconds_remaining = 0;
            self.auto_rebuild_live_ring();
            self.bottom_pane.update_status_text(String::new());
            self.bottom_pane.set_task_running(false);
        }

        self.request_redraw();
    }

    pub(in super::super) fn refresh_queued_user_messages(&mut self, schedule_watchdog: bool) {
        let mut scheduled_watchdog = false;
        if self.queued_user_messages.is_empty() {
            self.queue_block_started_at = None;
        } else if schedule_watchdog
            && self.queue_block_started_at.is_none() {
                self.queue_block_started_at = Some(Instant::now());
                scheduled_watchdog = true;
            }

        if scheduled_watchdog {
            let tx = self.app_event_tx.clone();
            // Fire a CommitTick after ~10s to ensure the watchdog runs even when
            // no streaming/animation is active.
            if thread_spawner::spawn_lightweight("queue-watchdog", move || {
                std::thread::sleep(Duration::from_secs(10));
                tx.send(crate::app_event::AppEvent::CommitTick);
            })
            .is_none()
            {
                // If we cannot spawn another lightweight thread (e.g., thread cap reached),
                // fall back to a non-threaded timer using tokio when available, or a best-effort
                // regular thread; as a last resort mark the timer expired and send immediately so
                // the queue cannot remain blocked.
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    let tx = self.app_event_tx.clone();
                    handle.spawn(async move {
                        tokio::time::sleep(Duration::from_secs(10)).await;
                        tx.send(crate::app_event::AppEvent::CommitTick);
                    });
                } else {
                    let tx = self.app_event_tx.clone();
                    if std::thread::Builder::new()
                        .name("queue-watchdog-fallback".to_string())
                        .spawn(move || {
                            std::thread::sleep(Duration::from_secs(10));
                            tx.send(crate::app_event::AppEvent::CommitTick);
                        })
                        .is_err()
                    {
                        // No way to schedule a delayed tick; force the timer to appear expired
                        // and emit a tick now to avoid indefinite blocking.
                        self.queue_block_started_at = Some(Instant::now() - Duration::from_secs(10));
                        self.app_event_tx.send(crate::app_event::AppEvent::CommitTick);
                    }
                }
            }
        }

        self.request_redraw();
    }
}
