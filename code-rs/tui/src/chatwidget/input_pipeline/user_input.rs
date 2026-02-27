use super::prelude::*;

impl ChatWidget<'_> {
    pub(in super::super) fn try_coordinator_route(
        &mut self,
        original_text: &str,
    ) -> Option<CoordinatorRouterResponse> {
        let trimmed = original_text.trim();
        if trimmed.is_empty() {
            return None;
        }
        if !self.auto_state.is_active() {
            return None;
        }
        if self.auto_state.is_paused_manual()
            && self.auto_state.should_bypass_coordinator_next_submit()
        {
            return None;
        }
        if !self.config.auto_drive.coordinator_routing {
            return None;
        }
        if trimmed.starts_with('/') {
            return None;
        }

        let mut updates = Vec::new();
        if let Some(summary) = self.auto_state.last_decision_summary.clone()
            && !summary.trim().is_empty() {
                updates.push(summary);
            }
        if let Some(current) = self.auto_state.current_summary.clone()
            && !current.trim().is_empty() && updates.iter().all(|existing| existing != &current) {
                updates.push(current);
            }

        let context = CoordinatorContext::new(self.auto_state.pending_agent_actions.len(), updates);
        let response = route_user_message(trimmed, &context);
        if response.user_response.is_some() || response.cli_command.is_some() {
            Some(response)
        } else {
            None
        }
    }

    pub(in super::super) fn submit_request_user_input_answer(&mut self, pending: PendingRequestUserInput, raw: String) {
        use code_protocol::request_user_input::RequestUserInputAnswer;
        use code_protocol::request_user_input::RequestUserInputResponse;

        tracing::info!(
            "[request_user_input] answer turn_id={} call_id={}",
            pending.turn_id,
            pending.call_id
        );

        let response = serde_json::from_str::<RequestUserInputResponse>(&raw).unwrap_or_else(|_| {
            let question_count = pending.questions.len();
            let mut lines: Vec<String> = raw
                .lines()
                .map(|line| line.trim_end().to_string())
                .collect();

            if question_count <= 1 {
                lines = vec![raw.trim().to_string()];
            } else if lines.len() > question_count {
                let tail = lines.split_off(question_count - 1);
                lines.push(tail.join("\n"));
            }

            while lines.len() < question_count {
                lines.push(String::new());
            }

            let mut answers = std::collections::HashMap::new();
            for (idx, question) in pending.questions.iter().enumerate() {
                let value = lines.get(idx).cloned().unwrap_or_default();
                answers.insert(
                    question.id.clone(),
                    RequestUserInputAnswer {
                        answers: vec![value],
                    },
                );
            }
            RequestUserInputResponse { answers }
        });

        let display_text =
            Self::format_request_user_input_display(&pending.questions, &response);
        if !display_text.trim().is_empty() {
            let key = Self::order_key_successor(pending.anchor_key);
            let state = history_cell::new_user_prompt(display_text);
            let _ =
                self.history_insert_plain_state_with_key(state, key, "request_user_input_answer");
            self.restore_reasoning_in_progress_if_streaming();
        }

        if let Err(e) = self.code_op_tx.send(Op::UserInputAnswer {
            id: pending.turn_id,
            response,
        }) {
            tracing::error!("failed to send Op::UserInputAnswer: {e}");
        }

        self.clear_composer();
        self.bottom_pane
            .update_status_text("waiting for model".to_string());
        self.request_redraw();
    }

    pub(in super::super) fn format_request_user_input_display(
        questions: &[code_protocol::request_user_input::RequestUserInputQuestion],
        response: &code_protocol::request_user_input::RequestUserInputResponse,
    ) -> String {
        let mut lines = Vec::new();
        for question in questions {
            let answer: &[String] = response
                .answers
                .get(&question.id)
                .map(|a| a.answers.as_slice())
                .unwrap_or(&[]);
            let value = answer.first().map(String::as_str).unwrap_or("");
            let value = if value.trim().is_empty() {
                "(skipped)"
            } else if question.is_secret {
                "[hidden]"
            } else {
                value
            };

            if questions.len() == 1 {
                lines.push(value.to_string());
            } else {
                let header = question.header.trim();
                if header.is_empty() {
                    lines.push(value.to_string());
                } else {
                    lines.push(format!("{header}: {value}"));
                }
            }
        }
        lines.join("\n")
    }

    pub(crate) fn on_request_user_input_answer(
        &mut self,
        turn_id: String,
        response: code_protocol::request_user_input::RequestUserInputResponse,
    ) {
        let Some(pending) = self.pending_request_user_input.take() else {
            tracing::warn!(
                "[request_user_input] received UI answer but no request is pending (turn_id={turn_id})"
            );
            return;
        };

        if pending.turn_id != turn_id {
            tracing::warn!(
                "[request_user_input] received UI answer for unexpected turn_id (expected={}, got={turn_id})",
                pending.turn_id,
            );
        }

        self.bottom_pane.close_request_user_input_view();

        let display_text =
            Self::format_request_user_input_display(&pending.questions, &response);

        if !display_text.trim().is_empty() {
            let key = Self::order_key_successor(pending.anchor_key);
            let state = history_cell::new_user_prompt(display_text);
            let _ =
                self.history_insert_plain_state_with_key(state, key, "request_user_input_answer");
            self.restore_reasoning_in_progress_if_streaming();
        }

        if let Err(e) = self.code_op_tx.send(Op::UserInputAnswer {
            id: pending.turn_id,
            response,
        }) {
            tracing::error!("failed to send Op::UserInputAnswer: {e}");
        }

        self.clear_composer();
        self.bottom_pane
            .update_status_text("waiting for model".to_string());
        self.request_redraw();
    }

    pub(in super::super) fn submit_user_message(&mut self, user_message: UserMessage) {
        if self.layout.scroll_offset.get() > 0 {
            layout_scroll::to_bottom(self);
        }
        // Surface a local diagnostic note and anchor it to the NEXT turn,
        // placing it directly after the user prompt so ordering is stable.
        // (debug message removed)
        // Fade the welcome cell only when a user actually posts a message.
        for cell in &self.history_cells {
            cell.trigger_fade();
        }
        let mut message = user_message;
        // If our configured cwd no longer exists (e.g., a worktree folder was
        // deleted outside the app), try to automatically recover to the repo
        // root for worktrees and re-submit the same message there.
        if !self.config.cwd.exists() {
            let missing = self.config.cwd.clone();
            let mut fallback: Option<(PathBuf, &'static str)> =
                worktree_root_hint_for(&missing).map(|p| (p, "recorded repo root"));
            if fallback.is_none()
                && let Some(parent) = missing.parent().and_then(worktree_root_hint_for) {
                    fallback = Some((parent, "recorded repo root"));
                }
            if fallback.is_none()
                && let Some(prev) = last_existing_cwd(&missing) {
                    fallback = Some((prev, "last known directory"));
                }
            let missing_s = missing.display().to_string();
            if fallback.is_none() && missing_s.contains("/.code/branches/") {
                let mut current = missing.as_path();
                let mut first_existing: Option<PathBuf> = None;
                while let Some(parent) = current.parent() {
                    current = parent;
                    if !current.exists() {
                        continue;
                    }
                    if first_existing.is_none() {
                        first_existing = Some(current.to_path_buf());
                    }
                    if let Some(repo_root) =
                        code_core::git_info::resolve_root_git_project_for_trust(current)
                    {
                        fallback = Some((repo_root, "repository root"));
                        break;
                    }
                }
                if fallback.is_none()
                    && let Some(existing) = first_existing {
                        fallback = Some((existing, "parent directory"));
                    }
            }
            if fallback.is_none()
                && let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
                    fallback = Some((home, "home directory"));
                }
            if let Some((fallback_root, label)) = fallback {
                let msg = format!(
                    "WARN: Worktree directory is missing: {}\nSwitching to {}: {}",
                    missing.display(),
                    label,
                    fallback_root.display()
                );
                self.send_background_tail_ordered(msg);
                self.app_event_tx.send(AppEvent::SwitchCwd(
                    fallback_root,
                    Some(message.display_text.clone()),
                ));
                return;
            }
            self.history_push_plain_state(history_cell::new_error_event(format!(
                "Working directory is missing: {}",
                self.config.cwd.display()
            )));
            return;
        }
        let original_text = message.display_text.clone();

        let mut submitted_cli = false;
        let manual_edit_pending = self.auto_state.is_paused_manual()
            && self.auto_state.resume_after_submit();
        let manual_override_active = self.auto_state.is_paused_manual();
        let bypass_active = self.auto_state.should_bypass_coordinator_next_submit();
        let coordinator_routing_allowed = if bypass_active {
            manual_edit_pending || manual_override_active
        } else {
            true
        };

        let should_route_through_coordinator = !message.suppress_persistence
            && !original_text.trim().starts_with('/')
            && self.auto_state.is_active()
            && self.config.auto_drive.coordinator_routing
            && coordinator_routing_allowed;

        if should_route_through_coordinator
        {
            let mut conversation = self.current_auto_history();
            if let Some(user_item) = Self::auto_drive_make_user_message(original_text.clone()) {
                conversation.push(user_item.clone());
                if self.auto_send_user_prompt_to_coordinator(original_text.clone(), conversation) {
                    self.finalize_sent_user_message(message);
                    self.consume_pending_prompt_for_ui_only_turn();
                    self.auto_history.append_raw(std::slice::from_ref(&user_item));
                    return;
                }
            }
        }

        if !message.suppress_persistence
            && self.auto_state.is_active()
            && self.config.auto_drive.coordinator_routing
            && coordinator_routing_allowed
            && let Some(mut routed) = self.try_coordinator_route(&original_text) {
                self.finalize_sent_user_message(message);
                self.consume_pending_prompt_for_ui_only_turn();

                if let Some(notice_text) = routed.user_response.take() {
                    if let Some(item) =
                        Self::auto_drive_make_assistant_message(notice_text.clone())
                    {
                        self.auto_history.append_raw(std::slice::from_ref(&item));
                    }
                    let lines = vec!["AUTO DRIVE RESPONSE".to_string(), notice_text];
                    self.history_push_plain_paragraphs(PlainMessageKind::Notice, lines);
                }

                let _ = self.rebuild_auto_history();

                if let Some(cli_command) = routed.cli_command {
                    let mut synthetic: UserMessage = cli_command.into();
                    synthetic.suppress_persistence = true;
                    self.submit_user_message(synthetic);
                    submitted_cli = true;
                }

                if !submitted_cli {
                    self.auto_send_conversation_force();
                }

                return;
            }

        let only_text_items = message
            .ordered_items
            .iter()
            .all(|item| matches!(item, InputItem::Text { .. }));
        if only_text_items
            && let Some((command_line, rest_text)) =
                Self::split_leading_slash_command(&original_text)
                && Self::multiline_slash_command_requires_split(&command_line) {
                    let preview = crate::slash_command::process_slash_command_message(
                        command_line.as_str(),
                    );
                    match preview {
                        ProcessedCommand::RegularCommand(SlashCommand::Auto, canonical_text) => {
                            let goal = rest_text.trim();
                            let command_text = if goal.is_empty() {
                                canonical_text
                            } else {
                                format!("{canonical_text} {goal}")
                            };
                            self.app_event_tx
                                .send(AppEvent::DispatchCommand(SlashCommand::Auto, command_text));
                            return;
                        }
                        ProcessedCommand::NotCommand(_) => {}
                        _ => {
                            self.submit_user_message(command_line.into());
                            let trimmed_rest = rest_text.trim();
                            if !trimmed_rest.is_empty() {
                                self.submit_user_message(rest_text.into());
                            }
                            return;
                        }
                    }
                }
        // Build a combined string view of the text-only parts to process slash commands
        let mut text_only = String::new();
        for it in &message.ordered_items {
            if let InputItem::Text { text } = it {
                if !text_only.is_empty() {
                    text_only.push('\n');
                }
                text_only.push_str(text);
            }
        }

        // Expand user-defined custom prompts, supporting both "/prompts:name" and "/name" forms.
        match prompt_args::expand_custom_prompt(&text_only, self.bottom_pane.custom_prompts()) {
            Ok(Some(expanded)) => {
                text_only = expanded.clone();
                message
                    .ordered_items
                    .clear();
                message
                    .ordered_items
                    .push(InputItem::Text { text: expanded });
            }
            Ok(None) => {}
            Err(err) => {
                self.history_push_plain_state(history_cell::new_error_event(err.user_message()));
                return;
            }
        }

        // Save the prompt if it's a multi-agent command
        let original_trimmed = original_text.trim();
        if original_trimmed.starts_with("/plan ")
            || original_trimmed.starts_with("/solve ")
            || original_trimmed.starts_with("/code ")
        {
            self.last_agent_prompt = Some(original_text.clone());
        }

        // Process slash commands and expand them if needed
        // First, allow custom subagent commands: if the message starts with a slash and the
        // command name matches a saved subagent in config, synthesize a unified prompt using
        // format_subagent_command and replace the message with that prompt.
        if let Some(first) = original_text.trim().strip_prefix('/') {
            let mut parts = first.splitn(2, ' ');
            let cmd_name = parts.next().unwrap_or("").trim();
            let args = parts.next().unwrap_or("").trim().to_string();
            if !cmd_name.is_empty() {
                let has_custom = self
                    .config
                    .subagent_commands
                    .iter()
                    .any(|c| c.name.eq_ignore_ascii_case(cmd_name));
                // Treat built-ins via the standard path below to preserve existing ack flow,
                // but allow any other saved subagent command to be executed here.
                let is_builtin = matches!(
                    cmd_name.to_ascii_lowercase().as_str(),
                    "plan" | "solve" | "code"
                );
                if has_custom && !is_builtin {
                    let res = code_core::slash_commands::format_subagent_command(
                        cmd_name,
                        &args,
                        Some(&self.config.agents),
                        Some(&self.config.subagent_commands),
                    );
                    if !res.read_only
                        && self.ensure_git_repo_for_action(
                            GitInitResume::SubmitText {
                                text: original_text.clone(),
                            },
                            "Write-enabled agents require a git repository.",
                        )
                    {
                        return;
                    }
                    // Acknowledge configuration
                    let mode = if res.read_only { "read-only" } else { "write" };
                    let agents = if res.models.is_empty() {
                        "<none>".to_string()
                    } else {
                        res.models.join(", ")
                    };
                    let lines = vec![
                        format!("/{} configured", res.name),
                        format!("mode: {}", mode),
                        format!("agents: {}", agents),
                        format!("command: {}", original_text.trim()),
                    ];
                    self.history_push_plain_paragraphs(PlainMessageKind::Notice, lines);

                    message
                        .ordered_items
                        .clear();
                    message
                        .ordered_items
                        .push(InputItem::Text { text: res.prompt });
                    // Continue with normal submission after this match block
                }
            }
        }

        let processed = crate::slash_command::process_slash_command_message(&text_only);
        match processed {
            crate::slash_command::ProcessedCommand::ExpandedPrompt(_expanded) => {
                // If a built-in multi-agent slash command was used, resolve
                // configured subagent settings and feed the synthesized prompt
                // without echoing an additional acknowledgement cell.
                let trimmed = original_trimmed;
                let (cmd_name, args_opt) = if let Some(rest) = trimmed.strip_prefix("/plan ") {
                    ("plan", Some(rest.trim().to_string()))
                } else if let Some(rest) = trimmed.strip_prefix("/solve ") {
                    ("solve", Some(rest.trim().to_string()))
                } else if let Some(rest) = trimmed.strip_prefix("/code ") {
                    ("code", Some(rest.trim().to_string()))
                } else {
                    ("", None)
                };

                if let Some(task) = args_opt {
                    let res = code_core::slash_commands::format_subagent_command(
                        cmd_name,
                        &task,
                        Some(&self.config.agents),
                        Some(&self.config.subagent_commands),
                    );
                    if !res.read_only
                        && self.ensure_git_repo_for_action(
                            GitInitResume::SubmitText {
                                text: original_text.clone(),
                            },
                            "Write-enabled agents require a git repository.",
                        )
                    {
                        return;
                    }

                    // Replace the message with the resolved prompt and suppress the
                    // agent launch hint that would otherwise echo back immediately.
                    self.suppress_next_agent_hint = true;
                    message
                        .ordered_items
                        .clear();
                    message
                        .ordered_items
                        .push(InputItem::Text { text: res.prompt });
                } else {
                    // Fallback to default expansion behavior
                    let expanded = _expanded;
                    message
                        .ordered_items
                        .clear();
                    message
                        .ordered_items
                        .push(InputItem::Text { text: expanded });
                }
            }
            crate::slash_command::ProcessedCommand::RegularCommand(cmd, command_text) => {
                if cmd == SlashCommand::Undo {
                    self.handle_undo_command();
                    return;
                }
                // This is a regular slash command, dispatch it normally
                self.app_event_tx
                    .send(AppEvent::DispatchCommand(cmd, command_text));
                return;
            }
            crate::slash_command::ProcessedCommand::Error(error_msg) => {
                // Show error in history
                self.history_push_plain_state(history_cell::new_error_event(error_msg));
                return;
            }
            crate::slash_command::ProcessedCommand::NotCommand(_) => {
                // Not a slash command, process normally
            }
        }

        let mut items: Vec<InputItem> = Vec::new();

        // Check if browser mode is enabled and capture screenshot
        // IMPORTANT: Always use global browser manager for consistency
        // The global browser manager ensures both TUI and agent tools use the same instance

        // Start async screenshot capture in background (non-blocking)
        {
            let latest_browser_screenshot_clone = Arc::clone(&self.latest_browser_screenshot);

            tokio::spawn(async move {
                tracing::info!("Evaluating background screenshot capture...");

                // Rate-limit: skip if a capture ran very recently (< 4000ms)
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let last = BG_SHOT_LAST_START_MS.load(Ordering::Relaxed);
                if now_ms.saturating_sub(last) < 4000 {
                    tracing::info!("Skipping background screenshot: rate-limited");
                    return;
                }

                // Single-flight: skip if another capture is in progress
                if BG_SHOT_IN_FLIGHT.swap(true, Ordering::AcqRel) {
                    tracing::info!("Skipping background screenshot: already in-flight");
                    return;
                }
                // Ensure we always clear the flag
                struct ShotGuard;
                impl Drop for ShotGuard {
                    fn drop(&mut self) {
                        BG_SHOT_IN_FLIGHT.store(false, Ordering::Release);
                    }
                }
                let _guard = ShotGuard;

                // Short settle to allow page to reach a stable state; keep it small
                tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;

                let Some(browser_manager) = code_browser::global::get_browser_manager().await else {
                    tracing::info!("Skipping background screenshot: browser manager unavailable");
                    return;
                };

                if !browser_manager.is_enabled().await {
                    tracing::info!("Skipping background screenshot: browser disabled");
                    return;
                }

                if browser_manager.idle_elapsed_past_timeout().await.is_some() {
                    tracing::info!("Skipping background screenshot: browser idle");
                    return;
                }

                BG_SHOT_LAST_START_MS.store(now_ms, Ordering::Relaxed);

                tracing::info!("Screenshot capture attempt 1 of 1");

                // Add timeout to screenshot capture
                let capture_result = tokio::time::timeout(
                    tokio::time::Duration::from_secs(5),
                    browser_manager.capture_screenshot_with_url(),
                )
                .await;

                match capture_result {
                    Ok(Ok((screenshot_paths, url))) => {
                        tracing::info!(
                            "Background screenshot capture succeeded with {} images on attempt 1",
                            screenshot_paths.len()
                        );

                        // Save the first screenshot path and URL for display in the TUI
                        if let Some(first_path) = screenshot_paths.first()
                            && let Ok(mut latest) = latest_browser_screenshot_clone.lock() {
                                let url_string = url.clone().unwrap_or_else(|| "Browser".to_string());
                                *latest = Some((first_path.clone(), url_string));
                            }

                        // Create screenshot items
                        let mut screenshot_items = Vec::new();
                        for path in screenshot_paths {
                            if path.exists() {
                                tracing::info!("Adding browser screenshot: {}", path.display());
                                let timestamp = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs();
                                let metadata = format!(
                                    "screenshot:{}:{}",
                                    timestamp,
                                    url.as_deref().unwrap_or("unknown")
                                );
                                screenshot_items.push(InputItem::EphemeralImage {
                                    path,
                                    metadata: Some(metadata),
                                });
                            }
                        }

                        // Do not enqueue screenshots as messages.
                        // They are now injected per-turn by the core session.
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("Background screenshot capture failed (attempt 1): {}", e);
                    }
                    Err(_timeout_err) => {
                        tracing::warn!("Background screenshot capture timed out (attempt 1)");
                    }
                }
            });
        }

        // Use the ordered items (text + images interleaved with markers)
        items.extend(message.ordered_items.clone());
        message.ordered_items = items;

        if message.ordered_items.is_empty() {
            return;
        }

        let wait_only_active = self.wait_only_activity();
        let turn_active = (self.is_task_running()
            || !self.active_task_ids.is_empty()
            || self.stream.is_write_cycle_active()
            || !self.queued_user_messages.is_empty())
            && !wait_only_active;

        if turn_active {
            tracing::info!(
                "[queue] Enqueuing user input while turn is active (queue_size={}, task_running={}, stream_active={}, active_tasks={})",
                self.queued_user_messages.len() + 1,
                self.is_task_running(),
                self.stream.is_write_cycle_active(),
                self.active_task_ids.len()
            );
            let queued_clone = message.clone();
            self.queued_user_messages.push_back(queued_clone);
            self.refresh_queued_user_messages(true);

            let prompt_summary = if message.display_text.trim().is_empty() {
                None
            } else {
                Some(message.display_text.clone())
            };

            let should_capture_snapshot = self.active_ghost_snapshot.is_none()
                && self.ghost_snapshot_queue.is_empty();
            if should_capture_snapshot {
                let _ = self.capture_ghost_snapshot(prompt_summary);
            }
            self.dispatch_queued_user_message_now(message);
            return;
        }

        if wait_only_active {
            // Keep long waits running but do not block user input.
            self.bottom_pane.set_task_running(false);
            self.bottom_pane
                .update_status_text("Waiting in background".to_string());
        }

        tracing::info!(
            "[queue] Turn idle, enqueuing and preparing to drain (auto_active={}, queue_size={})",
            self.auto_state.is_active(),
            self.queued_user_messages.len() + 1
        );

        let queued_clone = message.clone();
        self.queued_user_messages.push_back(queued_clone);
        self.refresh_queued_user_messages(false);

        let batch: Vec<UserMessage> = self.queued_user_messages.iter().cloned().collect();
        let summary = batch
            .last()
            .and_then(|msg| {
                let trimmed = msg.display_text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(msg.display_text.clone())
                }
            });

        let _ = self.capture_ghost_snapshot(summary);

        if self.auto_state.is_active() {
            tracing::info!(
                "[queue] Draining via coordinator path for Auto Drive (batch_size={})",
                batch.len()
            );
            self.dispatch_queued_batch_via_coordinator(batch);
        } else {
            tracing::info!(
                "[queue] Draining via direct batch dispatch (batch_size={})",
                batch.len()
            );
            self.dispatch_queued_batch(batch);
        }

        // (debug watchdog removed)
    }
}
