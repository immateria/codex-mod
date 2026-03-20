impl ChatWidget<'_> {
    pub(crate) fn handle_ghost_snapshot_finished(
        &mut self,
        job_id: u64,
        result: Result<GhostCommit, GitToolingError>,
        elapsed: Duration,
    ) {
        let Some((active_id, request)) = self.active_ghost_snapshot.take() else {
            tracing::warn!("ghost snapshot finished without active job (id={job_id})");
            return;
        };

        if active_id != job_id {
            tracing::warn!(
                "ghost snapshot job id mismatch: expected {active_id}, got {job_id}"
            );
            self.active_ghost_snapshot = Some((active_id, request));
            return;
        }

        let _ = self.finalize_ghost_snapshot(request, result, elapsed);
        self.request_redraw();
        self.spawn_next_ghost_snapshot();
    }

    pub(in super::super) fn current_conversation_snapshot(&self) -> ConversationSnapshot {
        use crate::history_cell::HistoryCellType;
        let mut user_turns = 0usize;
        let mut assistant_turns = 0usize;
        for cell in &self.history_cells {
            match cell.kind() {
                HistoryCellType::User => user_turns = user_turns.saturating_add(1),
                HistoryCellType::Assistant => {
                    assistant_turns = assistant_turns.saturating_add(1)
                }
                _ => {}
            }
        }
        let mut snapshot = ConversationSnapshot::new(user_turns, assistant_turns);
        snapshot.history_len = self.history_cells.len();
        snapshot.order_len = self.cell_order_seq.len();
        snapshot.order_dbg_len = self.cell_order_dbg.len();
        snapshot
    }

    pub(in super::super) fn conversation_delta_since(
        &self,
        snapshot: &ConversationSnapshot,
    ) -> (usize, usize) {
        let current = self.current_conversation_snapshot();
        let user_delta = current
            .user_turns
            .saturating_sub(snapshot.user_turns);
        let assistant_delta = current
            .assistant_turns
            .saturating_sub(snapshot.assistant_turns);
        (user_delta, assistant_delta)
    }

    pub(in super::super) fn history_snapshot_for_persistence(&self) -> HistorySnapshot {
        let order: Vec<OrderKeySnapshot> = self
            .cell_order_seq
            .iter()
            .map(|key| (*key).into())
            .collect();
        let order_debug = self.cell_order_dbg.clone();
        self.history_state
            .snapshot()
            .with_order(order, order_debug)
    }

    pub(in super::super) fn mark_history_dirty(&mut self) {
        self.history_snapshot_dirty = true;
        self.render_request_cache_dirty.set(true);
        self.flush_history_snapshot_if_needed(false);
        self.sync_history_virtualization();
    }

    pub(in super::super) fn flush_history_snapshot_if_needed(&mut self, force: bool) {
        if !self.history_snapshot_dirty {
            return;
        }
        if !force
            && let Some(last) = self.history_snapshot_last_flush
                && last.elapsed() < Duration::from_millis(400) {
                    return;
                }
        let snapshot = self.history_snapshot_for_persistence();
        match serde_json::to_value(&snapshot) {
            Ok(snapshot_value) => {
                let send_result = self
                    .code_op_tx
                    .send(Op::PersistHistorySnapshot { snapshot: snapshot_value });
                if send_result.is_err() {
                    tracing::warn!("failed to send history snapshot to core");
                } else {
                    self.history_snapshot_dirty = false;
                }
                self.history_snapshot_last_flush = Some(Instant::now());
            }
            Err(err) => {
                tracing::warn!("failed to serialize history snapshot: {err}");
            }
        }
    }

    pub(crate) fn snapshot_ghost_state(&self) -> GhostState {
        GhostState {
            snapshots: self.ghost_snapshots.clone(),
            disabled: self.ghost_snapshots_disabled,
            disabled_reason: self.ghost_snapshots_disabled_reason.clone(),
            queue: self.ghost_snapshot_queue.clone(),
            active: self.active_ghost_snapshot.clone(),
            next_id: self.next_ghost_snapshot_id,
            queued_user_messages: self.queued_user_messages.clone(),
        }
    }

    pub(crate) fn adopt_ghost_state(&mut self, state: GhostState) {
        self.ghost_snapshots = state.snapshots;
        if self.ghost_snapshots.len() > MAX_TRACKED_GHOST_COMMITS {
            self.ghost_snapshots
                .truncate(MAX_TRACKED_GHOST_COMMITS);
        }
        self.ghost_snapshots_disabled = state.disabled;
        self.ghost_snapshots_disabled_reason = state.disabled_reason;
        self.ghost_snapshot_queue = state.queue;
        self.active_ghost_snapshot = state.active;
        self.next_ghost_snapshot_id = state.next_id;
        self.queued_user_messages = state.queued_user_messages;
        let blocked = self.is_task_running()
            || !self.active_task_ids.is_empty()
            || self.stream.is_write_cycle_active();
        self.refresh_queued_user_messages(blocked);
        self.spawn_next_ghost_snapshot();
    }
}
