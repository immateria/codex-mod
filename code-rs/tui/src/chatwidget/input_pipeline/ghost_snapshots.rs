use super::*;

type NumstatRow = (Option<u32>, Option<u32>, String);

impl ChatWidget<'_> {
    pub(in super::super) fn capture_ghost_snapshot(&mut self, summary: Option<String>) -> GhostSnapshotJobHandle {
        if self.ghost_snapshots_disabled {
            return GhostSnapshotJobHandle::Skipped;
        }

        let request = GhostSnapshotRequest::new(
            summary,
            self.current_conversation_snapshot(),
            self.history_snapshot_for_persistence(),
        );
        self.enqueue_ghost_snapshot(request)
    }

    pub(in super::super) fn capture_ghost_snapshot_blocking(&mut self, summary: Option<String>) -> Option<GhostSnapshot> {
        if self.ghost_snapshots_disabled {
            return None;
        }

        let request = GhostSnapshotRequest::new(
            summary,
            self.current_conversation_snapshot(),
            self.history_snapshot_for_persistence(),
        );
        let repo_path = self.config.cwd.clone();
        let started_at = request.started_at;
        let hook_repo = repo_path.clone();
        let result = create_ghost_commit(
            &CreateGhostCommitOptions::new(repo_path.as_path())
                .post_commit_hook(&move || bump_snapshot_epoch_for(&hook_repo)),
        );
        let elapsed = started_at.elapsed();
        
        self.finalize_ghost_snapshot(request, result, elapsed)
    }

    pub(in super::super) fn dispatch_queued_batch(&mut self, batch: Vec<UserMessage>) {
        if batch.is_empty() {
            return;
        }

        let mut messages: Vec<UserMessage> = Vec::with_capacity(batch.len());

        for message in batch {
            let Some(message) = self.take_queued_user_message(&message) else {
                tracing::info!("Skipping queued user input removed before dispatch");
                continue;
            };
            messages.push(message);
        }

        if messages.is_empty() {
            return;
        }

        let mut combined_items: Vec<InputItem> = Vec::new();

        for (idx, message) in messages.iter().enumerate() {
            if idx > 0 && !combined_items.is_empty() && !message.ordered_items.is_empty() {
                combined_items.push(InputItem::Text {
                    text: "\n\n".to_string(),
                });
            }
            combined_items.extend(message.ordered_items.clone());
        }

        let total_items = combined_items.len();
        let ephemeral_count = combined_items
            .iter()
            .filter(|item| matches!(item, InputItem::EphemeralImage { .. }))
            .count();
        if ephemeral_count > 0 {
            tracing::info!(
                "Sending {} items to model (including {} ephemeral images)",
                total_items,
                ephemeral_count
            );
        }

        if !combined_items.is_empty() {
            self.flush_pending_agent_notes();
            if let Err(e) = self
                .code_op_tx
                .send(Op::UserInput {
                    items: combined_items,
                    final_output_json_schema: None,
                })
            {
                tracing::error!("failed to send Op::UserInput: {e}");
            }
        }

        for message in messages {
            self.finalize_sent_user_message(message);
        }
    }

    pub(in super::super) fn dispatch_queued_user_message_now(&mut self, message: UserMessage) {
        let message = self.take_queued_user_message(&message).unwrap_or(message);
        let items = message.ordered_items.clone();
        tracing::info!(
            "[queue] Dispatching single queued message via coordinator (queue_remaining={})",
            self.queued_user_messages.len()
        );
        match self.code_op_tx.send(Op::QueueUserInput { items }) {
            Ok(()) => {
                self.finalize_sent_user_message(message);
            }
            Err(err) => {
                tracing::error!("failed to send QueueUserInput op: {err}");
                self.queued_user_messages.push_front(message);
                self.refresh_queued_user_messages(true);
            }
        }
    }

    pub(in super::super) fn dispatch_queued_batch_via_coordinator(&mut self, batch: Vec<UserMessage>) {
        if batch.is_empty() {
            return;
        }

        tracing::info!(
            "[queue] Draining batch via coordinator path (batch_size={}, auto_active={})",
            batch.len(),
            self.auto_state.is_active()
        );

        for message in batch {
            let Some(message) = self.take_queued_user_message(&message) else {
                tracing::info!("[queue] Skipping queued user input removed before dispatch");
                continue;
            };

            let items = message.ordered_items.clone();
            match self.code_op_tx.send(Op::QueueUserInput { items }) {
                Ok(()) => {
                    self.finalize_sent_user_message(message);
                }
                Err(err) => {
                    tracing::error!("[queue] Failed to send QueueUserInput op: {err}");
                    self.queued_user_messages.push_front(message);
                    self.refresh_queued_user_messages(true);
                    break;
                }
            }
        }
    }

    pub(in super::super) fn take_queued_user_message(&mut self, target: &UserMessage) -> Option<UserMessage> {
        let position = self
            .queued_user_messages
            .iter()
            .position(|message| message == target)?;
        let removed = self.queued_user_messages.remove(position)?;
        self.refresh_queued_user_messages(false);
        Some(removed)
    }

    pub(in super::super) fn enqueue_ghost_snapshot(&mut self, request: GhostSnapshotRequest) -> GhostSnapshotJobHandle {
        let job_id = self.next_ghost_snapshot_id;
        self.next_ghost_snapshot_id = self.next_ghost_snapshot_id.wrapping_add(1);
        self.ghost_snapshot_queue.push_back((job_id, request));
        self.spawn_next_ghost_snapshot();
        GhostSnapshotJobHandle::Scheduled(job_id)
    }

    pub(in super::super) fn spawn_next_ghost_snapshot(&mut self) {
        if self.ghost_snapshots_disabled {
            self.ghost_snapshot_queue.clear();
            return;
        }
        if self.active_ghost_snapshot.is_some() {
            return;
        }
        let Some((job_id, request)) = self.ghost_snapshot_queue.pop_front() else {
            return;
        };

        let repo_path = self.config.cwd.clone();
        let app_event_tx = self.app_event_tx.clone();
        let notice_ticket = self.make_background_tail_ticket();
        let started_at = request.started_at;
        self.active_ghost_snapshot = Some((job_id, request));

        tokio::spawn(async move {
            let handle = tokio::task::spawn_blocking(move || {
                let hook_repo = repo_path.clone();
                let options = CreateGhostCommitOptions::new(repo_path.as_path());
                create_ghost_commit(&options.post_commit_hook(&move || bump_snapshot_epoch_for(&hook_repo)))
            });
            tokio::pin!(handle);

            let mut notice_sent = false;
            let notice_sleep = tokio::time::sleep(GHOST_SNAPSHOT_NOTICE_THRESHOLD);
            tokio::pin!(notice_sleep);
            let timeout_sleep = tokio::time::sleep(GHOST_SNAPSHOT_TIMEOUT);
            tokio::pin!(timeout_sleep);

            let join_result = loop {
                tokio::select! {
                    res = &mut handle => break res,
                    _ = &mut timeout_sleep => {
                        handle.as_mut().abort();
                        let elapsed = started_at.elapsed();
                        let err = GitToolingError::Io(io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!(
                                "ghost snapshot exceeded {}",
                                format_duration(GHOST_SNAPSHOT_TIMEOUT)
                            ),
                        ));
                        let event = AppEvent::GhostSnapshotFinished {
                            job_id,
                            result: Err(err),
                            elapsed,
                        };
                        app_event_tx.send(event);
                        return;
                    }
                    _ = &mut notice_sleep, if !notice_sent => {
                        notice_sent = true;
                        let elapsed = started_at.elapsed();
                        let message = format!(
                            "Git snapshot still running… {} elapsed.",
                            format_duration(elapsed)
                        );
                        app_event_tx.send_background_event_with_ticket(&notice_ticket, message);
                    }
                }
            };

            let elapsed = started_at.elapsed();
            let event = match join_result {
                Ok(Ok(commit)) => AppEvent::GhostSnapshotFinished {
                    job_id,
                    result: Ok(commit),
                    elapsed,
                },
                Ok(Err(err)) => AppEvent::GhostSnapshotFinished {
                    job_id,
                    result: Err(err),
                    elapsed,
                },
                Err(join_err) => {
                    let err = GitToolingError::Io(io::Error::other(
                        format!("ghost snapshot task failed: {join_err}"),
                    ));
                    AppEvent::GhostSnapshotFinished {
                        job_id,
                        result: Err(err),
                        elapsed,
                    }
                }
            };

            app_event_tx.send(event);
        });
    }

    pub(in super::super) fn finalize_ghost_snapshot(
        &mut self,
        request: GhostSnapshotRequest,
        result: Result<GhostCommit, GitToolingError>,
        elapsed: Duration,
    ) -> Option<GhostSnapshot> {
        match result {
            Ok(commit) => {
                self.ghost_snapshots_disabled = false;
                self.ghost_snapshots_disabled_reason = None;
                let snapshot = GhostSnapshot::new(
                    commit,
                    request.summary,
                    request.conversation,
                    request.history,
                );
                self.ghost_snapshots.push(snapshot.clone());
                session_log::log_history_snapshot(
                    snapshot.commit().id(),
                    snapshot.summary.as_deref(),
                    &snapshot.history,
                );
                if self.ghost_snapshots.len() > MAX_TRACKED_GHOST_COMMITS {
                    self.ghost_snapshots.remove(0);
                }
                if elapsed >= GHOST_SNAPSHOT_NOTICE_THRESHOLD {
                    self.push_background_tail(format!(
                        "Git snapshot captured in {}.",
                        format_duration(elapsed)
                    ));
                }
                Some(snapshot)
            }
            Err(err) => {
                if let GitToolingError::Io(io_err) = &err
                    && io_err.kind() == io::ErrorKind::TimedOut {
                        self.push_background_tail(format!(
                            "Git snapshot timed out after {}. Try again once the repository is less busy.",
                            format_duration(elapsed)
                        ));
                        tracing::warn!(
                            elapsed = %format_duration(elapsed),
                            "ghost snapshot timed out"
                        );
                        return None;
                    }
                self.ghost_snapshots_disabled = true;
                let (message, hint) = match &err {
                    GitToolingError::NotAGitRepository { .. } => (
                        "Snapshots disabled: this workspace is not inside a Git repository.".to_string(),
                        None,
                    ),
                    _ => (
                        format!("Snapshots disabled after Git error: {err}"),
                        Some(
                            "Restart Code after resolving the issue to re-enable snapshots.".to_string(),
                        ),
                    ),
                };
                self.ghost_snapshots_disabled_reason = Some(GhostSnapshotsDisabledReason {
                    message: message.clone(),
                    hint: hint.clone(),
                });
                self.push_background_tail(message);
                if let Some(hint) = hint {
                    self.push_background_tail(hint);
                }
                tracing::warn!("failed to create ghost snapshot: {err}");
                self.ghost_snapshot_queue.clear();
                None
            }
        }
    }

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

    pub(crate) fn handle_undo_command(&mut self) {
        if self.ghost_snapshots_disabled {
            let reason = self
                .ghost_snapshots_disabled_reason
                .as_ref()
                .map(|reason| reason.message.clone())
                .unwrap_or_else(|| "Snapshots are currently disabled.".to_string());
            self.push_background_tail(format!("/undo unavailable: {reason}"));
            self.show_undo_snapshots_disabled();
            return;
        }

        if self.ghost_snapshots.is_empty() {
            self.push_background_tail(
                "/undo unavailable: no snapshots captured yet. Run a file-modifying command to create one.".to_string(),
            );
            self.show_undo_empty_state();
            return;
        }

        self.show_undo_snapshot_picker();
    }

    pub(in super::super) fn show_undo_snapshots_disabled(&mut self) {
        let mut lines: Vec<String> = Vec::new();
        if let Some(reason) = &self.ghost_snapshots_disabled_reason {
            lines.push(reason.message.clone());
            if let Some(hint) = &reason.hint {
                lines.push(hint.clone());
            }
        } else {
            lines.push(
                "Snapshots are currently disabled. Resolve the Git issue and restart Code to re-enable them.".to_string(),
            );
        }

        self.show_undo_status_popup(
            "Snapshots unavailable",
            Some(
                "Restores workspace files only. Conversation history remains unchanged.".to_string(),
            ),
            Some("Automatic snapshotting failed, so /undo cannot restore the workspace.".to_string()),
            lines,
        );
    }

    pub(in super::super) fn show_undo_empty_state(&mut self) {
        self.show_undo_status_popup(
            "No snapshots yet",
            Some(
                "Restores workspace files only. Conversation history remains unchanged.".to_string(),
            ),
            Some("Snapshots appear once Code captures a Git checkpoint.".to_string()),
            vec![
                "No snapshot is available to restore.".to_string(),
                "Run a command that modifies files to create the first snapshot.".to_string(),
            ],
        );
    }

    pub(in super::super) fn show_undo_status_popup(
        &mut self,
        title: &str,
        scope_hint: Option<String>,
        subtitle: Option<String>,
        mut lines: Vec<String>,
    ) {
        if lines.is_empty() {
            lines.push("No snapshot information available.".to_string());
        }

        let headline = lines.remove(0);
        let description = if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        };

        let mut composed_subtitle = Vec::new();
        if let Some(hint) = scope_hint {
            composed_subtitle.push(hint);
        }
        if let Some(extra) = subtitle {
            composed_subtitle.push(extra);
        }
        let subtitle_for_view = if composed_subtitle.is_empty() {
            None
        } else {
            Some(composed_subtitle.join("\n"))
        };

        let items = vec![SelectionItem {
            name: headline,
            description,
            is_current: true,
            actions: Vec::new(),
        }];

        let view = ListSelectionView::new(
            format!(" {title} "),
            subtitle_for_view,
            Some("Esc close".to_string()),
            items,
            self.app_event_tx.clone(),
            1,
        );

        self.bottom_pane.show_list_selection(
            title.to_string(),
            None,
            Some("Esc close".to_string()),
            view,
        );
    }

    pub(in super::super) fn show_undo_snapshot_picker(&mut self) {
        let entries = self.build_undo_timeline_entries();
        if entries.len() <= 1 {
            self.push_background_tail(
                "/undo unavailable: no snapshots captured yet. Run a file-modifying command to create one.".to_string(),
            );
            self.show_undo_empty_state();
            return;
        }

        let current_index = entries.len().saturating_sub(1);
        let view = UndoTimelineView::new(entries, current_index, self.app_event_tx.clone());
        self.bottom_pane.show_undo_timeline_view(view);
    }

    pub(in super::super) fn build_undo_timeline_entries(&self) -> Vec<UndoTimelineEntry> {
        let mut entries: Vec<UndoTimelineEntry> = Vec::with_capacity(self.ghost_snapshots.len().saturating_add(1));
        for snapshot in self.ghost_snapshots.iter() {
            entries.push(self.timeline_entry_for_snapshot(snapshot));
        }
        entries.push(self.timeline_entry_for_current());
        entries
    }

    pub(in super::super) fn timeline_entry_for_snapshot(&self, snapshot: &GhostSnapshot) -> UndoTimelineEntry {
        let short_id = snapshot.short_id();
        let label = format!("Snapshot {short_id}");
        let summary = snapshot.summary.clone();
        let timestamp_line = Some(snapshot.captured_at.format("%Y-%m-%d %H:%M:%S").to_string());
        let relative_time = snapshot
            .age_from(Local::now())
            .map(|age| format!("captured {} ago", format_duration(age)));
        let (user_delta, assistant_delta) = self.conversation_delta_since(&snapshot.conversation);
        let stats_line = if user_delta == 0 && assistant_delta == 0 {
            Some("conversation already matches current state".to_string())
        } else if assistant_delta == 0 {
            Some(format!(
                "rewind {} user turn{}",
                user_delta,
                if user_delta == 1 { "" } else { "s" }
            ))
        } else {
            Some(format!(
                "rewind {} user turn{} and {} assistant repl{}",
                user_delta,
                if user_delta == 1 { "" } else { "s" },
                assistant_delta,
                if assistant_delta == 1 { "y" } else { "ies" }
            ))
        };

        let conversation_lines = Self::conversation_preview_lines_from_snapshot(&snapshot.history);
        let file_lines = self.timeline_file_lines_for_commit(snapshot.commit().id());

        UndoTimelineEntry {
            label,
            summary,
            timestamp_line,
            relative_time,
            stats_line,
            commit_line: Some(format!("commit {short_id}")),
            conversation_lines,
            file_lines,
            conversation_available: user_delta > 0,
            files_available: true,
            kind: UndoTimelineEntryKind::Snapshot {
                commit: snapshot.commit().id().to_string(),
            },
        }
    }

    pub(in super::super) fn timeline_entry_for_current(&self) -> UndoTimelineEntry {
        let history_snapshot = self.history_snapshot_for_persistence();
        let conversation_lines = Self::conversation_preview_lines_from_snapshot(&history_snapshot);
        let file_lines = self.timeline_file_lines_for_current();
        UndoTimelineEntry {
            label: "Current workspace".to_string(),
            summary: None,
            timestamp_line: Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string()),
            relative_time: Some("current point".to_string()),
            stats_line: Some("Already at this point in time".to_string()),
            commit_line: None,
            conversation_lines,
            file_lines,
            conversation_available: false,
            files_available: false,
            kind: UndoTimelineEntryKind::Current,
        }
    }

    pub(in super::super) fn conversation_preview_lines_from_snapshot(snapshot: &HistorySnapshot) -> Vec<Line<'static>> {
        let mut state = HistoryState::new();
        state.restore(snapshot);
        let mut messages: Vec<(UndoPreviewRole, String)> = Vec::new();
        for record in &state.records {
            match record {
                HistoryRecord::PlainMessage(msg) => match msg.kind {
                    PlainMessageKind::User => {
                        let text = Self::message_lines_to_plain_preview(&msg.lines);
                        if !text.is_empty() {
                            messages.push((UndoPreviewRole::User, text));
                        }
                    }
                    PlainMessageKind::Assistant => {
                        let text = Self::message_lines_to_plain_preview(&msg.lines);
                        if !text.is_empty() {
                            messages.push((UndoPreviewRole::Assistant, text));
                        }
                    }
                    _ => {}
                },
                HistoryRecord::AssistantMessage(msg) => {
                    let text = Self::markdown_to_plain_preview(&msg.markdown);
                    if !text.is_empty() {
                        messages.push((UndoPreviewRole::Assistant, text));
                    }
                }
                _ => {}
            }
        }

        if messages.is_empty() {
            return vec![Line::from(Span::styled(
                "No conversation captured in this snapshot.",
                Style::default().fg(crate::colors::text_dim()),
            ))];
        }

        let len = messages.len();
        let start = len.saturating_sub(Self::MAX_UNDO_CONVERSATION_MESSAGES);
        messages[start..]
            .iter()
            .map(|(role, text)| Self::conversation_line(*role, text.as_str()))
            .collect()
    }

    pub(in super::super) fn conversation_line(role: UndoPreviewRole, text: &str) -> Line<'static> {
        let (label, color) = match role {
            UndoPreviewRole::User => ("You", crate::colors::text_bright()),
            UndoPreviewRole::Assistant => ("Code", crate::colors::primary()),
        };
        let label_span = Span::styled(
            format!("{label}: "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        );
        let content_span = Span::styled(text.to_string(), Style::default().fg(crate::colors::text()));
        Line::from(vec![label_span, content_span])
    }

    pub(in super::super) fn message_lines_to_plain_preview(lines: &[MessageLine]) -> String {
        let mut segments: Vec<String> = Vec::new();
        for line in lines {
            match line.kind {
                MessageLineKind::Blank => continue,
                MessageLineKind::Metadata => continue,
                _ => {
                    let mut text = String::new();
                    for span in &line.spans {
                        text.push_str(&span.text);
                    }
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        segments.push(trimmed.to_string());
                    }
                }
            }
            if segments.len() >= Self::MAX_UNDO_CONVERSATION_MESSAGES {
                break;
            }
        }
        let joined = segments.join(" ");
        Self::truncate_preview_text(joined, Self::MAX_UNDO_PREVIEW_CHARS)
    }

    pub(in super::super) fn markdown_to_plain_preview(markdown: &str) -> String {
        let mut segments: Vec<String> = Vec::new();
        for line in markdown.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with('#') {
                segments.push(trimmed.trim_start_matches('#').trim().to_string());
            } else {
                segments.push(trimmed.to_string());
            }
            if segments.len() >= Self::MAX_UNDO_CONVERSATION_MESSAGES {
                break;
            }
        }
        if segments.is_empty() {
            return String::new();
        }
        let joined = segments.join(" ");
        Self::truncate_preview_text(joined, Self::MAX_UNDO_PREVIEW_CHARS)
    }

    pub(in super::super) fn truncate_preview_text(text: String, limit: usize) -> String {
        crate::text_formatting::truncate_chars_with_ellipsis(&text, limit)
    }

    pub(in super::super) fn timeline_file_lines_for_commit(&self, commit_id: &str) -> Vec<Line<'static>> {
        match self.git_numstat(["show", "--numstat", "--format=", commit_id]) {
            Ok(entries) => Self::file_change_lines(entries),
            Err(err) => vec![Line::from(Span::styled(
                err,
                Style::default().fg(crate::colors::error()),
            ))],
        }
    }

    pub(in super::super) fn timeline_file_lines_for_current(&self) -> Vec<Line<'static>> {
        match self.git_numstat(["diff", "--numstat", "HEAD"]) {
            Ok(entries) => {
                if entries.is_empty() {
                    vec![Line::from(Span::styled(
                        "Working tree clean",
                        Style::default().fg(crate::colors::text_dim()),
                    ))]
                } else {
                    Self::file_change_lines(entries)
                }
            }
            Err(err) => vec![Line::from(Span::styled(
                err,
                Style::default().fg(crate::colors::error()),
            ))],
        }
    }

    pub(in super::super) fn git_numstat<I, S>(
        &self,
        args: I,
    ) -> Result<Vec<NumstatRow>, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.run_git_command(args, |stdout| {
            let mut out = Vec::new();
            for line in stdout.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let mut parts = trimmed.splitn(3, '\t');
                let added = parts.next();
                let removed = parts.next();
                let path = parts.next();
                if let (Some(added), Some(removed), Some(path)) = (added, removed, path) {
                    out.push((
                        Self::parse_numstat_count(added),
                        Self::parse_numstat_count(removed),
                        path.to_string(),
                    ));
                }
            }
            Ok(out)
        })
    }

    pub(in super::super) fn run_git_command<I, S, F, T>(&self, args: I, parser: F) -> Result<T, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
        F: FnOnce(String) -> Result<T, String>,
    {
        let args_vec: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
        let output = Command::new("git")
            .current_dir(&self.config.cwd)
            .args(&args_vec)
            .output()
            .map_err(|err| format!("git {} failed: {err}", args_vec.join(" ")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = stderr.trim();
            if msg.is_empty() {
                Err(format!(
                    "git {} exited with status {}",
                    args_vec.join(" "),
                    output.status
                ))
            } else {
                Err(msg.to_string())
            }
        } else {
            if args_vec
                .iter()
                .any(|arg| matches!(arg.as_str(), "pull" | "checkout" | "merge" | "apply"))
            {
                bump_snapshot_epoch_for(&self.config.cwd);
            }
            parser(String::from_utf8_lossy(&output.stdout).to_string())
        }
    }

    pub(in super::super) fn parse_numstat_count(raw: &str) -> Option<u32> {
        if raw == "-" {
            None
        } else {
            raw.parse::<u32>().ok()
        }
    }

    pub(in super::super) fn file_change_lines(entries: Vec<(Option<u32>, Option<u32>, String)>) -> Vec<Line<'static>> {
        if entries.is_empty() {
            return vec![Line::from(Span::styled(
                "No file changes recorded for this snapshot.",
                Style::default().fg(crate::colors::text_dim()),
            ))];
        }

        let max_entries = (Self::MAX_UNDO_FILE_LINES / 2).max(1);
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (idx, (added, removed, path)) in entries.iter().enumerate() {
            if idx >= max_entries {
                break;
            }
            lines.push(Line::from(Span::styled(
                path.clone(),
                Style::default().fg(crate::colors::text()),
            )));

            let added_text = added.map_or("-".to_string(), |v| v.to_string());
            let removed_text = removed.map_or("-".to_string(), |v| v.to_string());
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    format!("+{added_text}"),
                    Style::default().fg(crate::colors::success()),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("-{removed_text}"),
                    Style::default().fg(crate::colors::error()),
                ),
            ]));
        }

        if entries.len() > max_entries {
            let remaining = entries.len() - max_entries;
            lines.push(Line::from(Span::styled(
                format!("… and {remaining} more file{}", if remaining == 1 { "" } else { "s" }),
                Style::default().fg(crate::colors::text_dim()),
            )));
        }

        lines
    }

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
        self.bottom_pane.clear_live_ring();
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
