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
}
