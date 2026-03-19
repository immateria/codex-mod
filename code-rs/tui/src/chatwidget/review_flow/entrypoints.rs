use super::super::*;
use code_protocol::protocol::ReviewTarget;

impl ChatWidget<'_> {
    pub(crate) fn open_review_dialog(&mut self) {
        if self.is_task_running() {
            self.history_push_plain_state(crate::history_cell::new_error_event(
                "`/review` — complete or cancel the current task before starting a new review.".to_string(),
            ));
            self.request_redraw();
            return;
        }

        let mut items: Vec<SelectionItem> = Vec::new();

        let max_attempts = self.configured_auto_resolve_re_reviews();
        let auto_note = if self.config.tui.review_auto_resolve {
            if max_attempts == 0 {
                "Auto Resolve is enabled (no automatic re-reviews)."
            } else if max_attempts == 1 {
                "Auto Resolve is enabled (max 1 re-review)."
            } else {
                "Auto Resolve is enabled."
            }
        } else {
            "Auto Resolve is disabled."
        };
        items.push(SelectionItem {
            name: "Auto Resolve settings moved to /settings".to_string(),
            description: Some(format!(
                "{auto_note} Manage Auto Resolve reviews and max re-reviews via `/settings review`."
            )),
            is_current: false,
            actions: vec![Box::new(|tx: &crate::app_event_sender::AppEventSender| {
                tx.send(crate::app_event::AppEvent::DispatchCommand(
                    SlashCommand::Settings,
                    "review".to_string(),
                ));
            })],
        });

        let workspace_prompt = "Review the current workspace changes (staged, unstaged, and untracked files) and highlight bugs, regressions, risky patterns, and missing tests before merge.".to_string();
        let workspace_hint = "current workspace changes".to_string();
        let workspace_preparation = "Preparing code review for current changes".to_string();
        let workspace_auto_resolve = self.config.tui.review_auto_resolve;
        items.push(SelectionItem {
            name: "Review uncommitted changes".to_string(),
            description: Some("Look at staged, unstaged, and untracked files".to_string()),
            is_current: false,
            actions: vec![Box::new({
                let prompt = workspace_prompt;
                let hint = workspace_hint;
                let preparation = workspace_preparation;
                move |tx: &crate::app_event_sender::AppEventSender| {
                    tx.send(crate::app_event::AppEvent::RunReviewWithScope {
                        target: ReviewTarget::UncommittedChanges,
                        prompt: prompt.clone(),
                        hint: Some(hint.clone()),
                        preparation_label: Some(preparation.clone()),
                        auto_resolve: workspace_auto_resolve,
                    });
                }
            })],
        });

        items.push(SelectionItem {
            name: "Review /branch changes".to_string(),
            description: Some("Compare your worktree branch against its merge target".to_string()),
            is_current: false,
            actions: vec![Box::new(|tx: &crate::app_event_sender::AppEventSender| {
                tx.send(crate::app_event::AppEvent::RunReviewCommand(String::new()));
            })],
        });

        items.push(SelectionItem {
            name: "Review a specific commit".to_string(),
            description: Some("Pick from recent commits".to_string()),
            is_current: false,
            actions: vec![Box::new(|tx: &crate::app_event_sender::AppEventSender| {
                tx.send(crate::app_event::AppEvent::StartReviewCommitPicker);
            })],
        });

        items.push(SelectionItem {
            name: "Review against a base branch".to_string(),
            description: Some("Diff current branch against another".to_string()),
            is_current: false,
            actions: vec![Box::new(|tx: &crate::app_event_sender::AppEventSender| {
                tx.send(crate::app_event::AppEvent::StartReviewBranchPicker);
            })],
        });

        items.push(SelectionItem {
            name: "Custom review instructions".to_string(),
            description: Some("Describe exactly what to audit".to_string()),
            is_current: false,
            actions: vec![Box::new(|tx: &crate::app_event_sender::AppEventSender| {
                tx.send(crate::app_event::AppEvent::OpenReviewCustomPrompt);
            })],
        });

        let view: ListSelectionView = ListSelectionView::new(
            " Review options ".to_string(),
            Some("Choose what scope to review".to_string()),
            Some("Enter select · Esc cancel".to_string()),
            items,
            self.app_event_tx.clone(),
            6,
        );

        self.bottom_pane.show_list_selection(view);
    }

    pub(crate) fn show_review_custom_prompt(&mut self) {
        let submit_tx = self.app_event_tx.clone();
        let on_submit: Box<dyn Fn(String) + Send + Sync> = Box::new(move |text: String| {
            submit_tx.send(crate::app_event::AppEvent::RunReviewCommand(text));
        });
        let view = CustomPromptView::new(
            "Custom review instructions".to_string(),
            "Describe the files or changes you want reviewed".to_string(),
            Some("Press Enter to submit · Esc cancel".to_string()),
            self.app_event_tx.clone(),
            None,
            on_submit,
        );
        self.bottom_pane.show_custom_prompt(view);
    }

    pub(crate) fn set_review_auto_resolve_enabled(&mut self, enabled: bool) {
        if self.config.tui.review_auto_resolve == enabled {
            return;
        }

        self.config.tui.review_auto_resolve = enabled;
        if !enabled {
            self.auto_resolve_clear();
        }

        let message = if let Ok(home) = code_core::config::find_code_home() {
            match code_core::config::set_tui_review_auto_resolve(&home, enabled) {
                Ok(_) => {
                    tracing::info!("Persisted review auto resolve toggle: {}", enabled);
                    if enabled {
                        "Auto Resolve reviews enabled."
                    } else {
                        "Auto Resolve reviews disabled."
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to persist review auto resolve toggle: {}", e);
                    if enabled {
                        "Auto Resolve enabled for this session (failed to persist)."
                    } else {
                        "Auto Resolve disabled for this session (failed to persist)."
                    }
                }
            }
        } else {
            tracing::warn!("Could not locate Codex home to persist review auto resolve toggle");
            if enabled {
                "Auto Resolve enabled for this session."
            } else {
                "Auto Resolve disabled for this session."
            }
        };

        self.bottom_pane.flash_footer_notice(message.to_string());
        self.refresh_settings_overview_rows();
        self.update_review_settings_model_row();
        self.request_redraw();
    }

    pub(crate) fn set_auto_review_enabled(&mut self, enabled: bool) {
        if self.config.tui.auto_review_enabled == enabled {
            return;
        }

        self.config.tui.auto_review_enabled = enabled;

        let message = if let Ok(home) = code_core::config::find_code_home() {
            match code_core::config::set_tui_auto_review_enabled(&home, enabled) {
                Ok(_) => {
                    tracing::info!("Persisted auto review toggle: {}", enabled);
                    if enabled {
                        "Auto Review enabled."
                    } else {
                        "Auto Review disabled."
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to persist auto review toggle: {}", e);
                    if enabled {
                        "Auto Review enabled for this session (failed to persist)."
                    } else {
                        "Auto Review disabled for this session (failed to persist)."
                    }
                }
            }
        } else {
            tracing::warn!("Could not locate Codex home to persist auto review toggle");
            if enabled {
                "Auto Review enabled for this session."
            } else {
                "Auto Review disabled for this session."
            }
        };

        self.bottom_pane.flash_footer_notice(message.to_string());
        self.refresh_settings_overview_rows();
        self.update_review_settings_model_row();
        self.request_redraw();
    }

    pub(crate) fn set_review_auto_resolve_attempts(&mut self, attempts: u32) {
        use code_core::config_types::AutoResolveAttemptLimit;

        let Ok(limit) = AutoResolveAttemptLimit::try_new(attempts) else {
            tracing::warn!("Ignoring invalid auto-resolve attempt value: {}", attempts);
            return;
        };

        self.auto_resolve_attempts_baseline = limit.get();

        if self
            .config
            .auto_drive
            .auto_resolve_review_attempts
            .get()
            == limit.get()
        {
            return;
        }

        self.config.auto_drive.auto_resolve_review_attempts = limit;
        if let Some(state) = self.auto_resolve_state.as_mut() {
            state.max_attempts = limit.get();
            let allowed_total = state.max_attempts.saturating_add(1);
            if state.attempt >= allowed_total {
                self.auto_resolve_clear();
            }
        }

        let message = if let Ok(home) = code_core::config::find_code_home() {
            match code_core::config::set_auto_drive_settings(
                &home,
                &self.config.auto_drive,
                self.config.auto_drive_use_chat_model,
            ) {
                Ok(_) => {
                    tracing::info!(
                        "Persisted auto resolve attempt limit: {}",
                        limit.get()
                    );
                    format!("Max re-reviews set to {}.", limit.get())
                }
                Err(err) => {
                    tracing::warn!("Failed to persist auto resolve attempts: {err}");
                    format!(
                        "Max re-reviews set to {} for this session (failed to persist).",
                        limit.get()
                    )
                }
            }
        } else {
            tracing::warn!("Could not locate Codex home to persist auto resolve attempts");
            format!("Max re-reviews set to {} for this session.", limit.get())
        };

        self.bottom_pane.flash_footer_notice(message);
        self.update_review_settings_model_row();
        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    pub(crate) fn set_auto_review_followup_attempts(&mut self, attempts: u32) {
        use code_core::config_types::AutoResolveAttemptLimit;

        let Ok(limit) = AutoResolveAttemptLimit::try_new(attempts) else {
            tracing::warn!("Ignoring invalid auto-review follow-up value: {}", attempts);
            return;
        };

        if self
            .config
            .auto_drive
            .auto_review_followup_attempts
            .get()
            == limit.get()
        {
            return;
        }

        self.config.auto_drive.auto_review_followup_attempts = limit;

        if let Ok(home) = code_core::config::find_code_home() {
            match code_core::config::set_auto_drive_settings(
                &home,
                &self.config.auto_drive,
                self.config.auto_drive_use_chat_model,
            ) {
                Ok(_) => {
                    tracing::info!(
                        "Persisted auto-review follow-up limit: {}",
                        limit.get()
                    );
                    self.bottom_pane.flash_footer_notice(format!(
                        "Auto Review follow-ups set to {}.",
                        limit.get()
                    ));
                }
                Err(err) => {
                    tracing::warn!("Failed to persist auto-review follow-up attempts: {err}");
                    self.bottom_pane.flash_footer_notice(format!(
                        "Auto Review follow-ups set to {} for this session (failed to persist).",
                        limit.get()
                    ));
                }
            }
        }

        self.refresh_settings_overview_rows();
        self.update_review_settings_model_row();
        self.request_redraw();
    }

    pub(crate) fn handle_review_command(&mut self, args: String) {
        if self.is_task_running() {
            self.history_push_plain_state(crate::history_cell::new_error_event(
                "`/review` — complete or cancel the current task before starting a new review.".to_string(),
            ));
            self.request_redraw();
            return;
        }

        let trimmed = args.trim();
        let auto_resolve = self.config.tui.review_auto_resolve;
        if trimmed.is_empty() {
            if Self::is_branch_worktree_path(&self.config.cwd)
                && let Some(git_root) =
                    code_core::git_info::resolve_root_git_project_for_trust(&self.config.cwd)
                {
                    let worktree_cwd = self.config.cwd.clone();
                    let tx = self.app_event_tx.clone();
                    let auto_flag = auto_resolve;
                    tokio::spawn(async move {
                        let branch_metadata =
                            code_core::git_worktree::load_branch_metadata(&worktree_cwd);
                        let metadata_base = branch_metadata.as_ref().and_then(|meta| {
                            meta.remote_ref.clone().or_else(|| {
                                if let (Some(remote_name), Some(base_branch)) =
                                    (meta.remote_name.clone(), meta.base_branch.clone())
                                {
                                    Some(format!("{remote_name}/{base_branch}"))
                                } else {
                                    None
                                }
                            })
                            .or_else(|| meta.base_branch.clone())
                        });
                        let default_branch = match metadata_base {
                            Some(value) => Some(value),
                            None => code_core::git_worktree::detect_default_branch(&git_root)
                                .await
                                .map(|name| name.trim().to_string())
                                .filter(|name| !name.is_empty()),
                        };
                        let current_branch = code_core::git_info::current_branch_name(&worktree_cwd)
                            .await
                            .map(|name| name.trim().to_string())
                            .filter(|name| !name.is_empty());

                        if let (Some(base_branch), Some(current_branch)) =
                            (default_branch, current_branch)
                            && base_branch != current_branch {
                                let prompt = format!(
                                    "Review the code changes between the current branch '{current_branch}' and '{base_branch}'. Identify the intent of the changes in '{current_branch}' and ensure no obvious gaps remain. Find all geniune bugs or regressions which need to be addressed before merging. Return ALL issues which need to be addressed, not just the first one you find."
                                );
                                let hint = Some(format!("against {base_branch}"));
                                let preparation_label =
                                    Some(format!("Preparing code review against {base_branch}"));
                                let target = ReviewTarget::Custom { instructions: prompt.clone() };
                                tx.send(crate::app_event::AppEvent::RunReviewWithScope {
                                    target,
                                    prompt,
                                    hint,
                                    preparation_label,
                                    auto_resolve: auto_flag,
                                });
                                return;
                            }

                        let prompt = "Review the current workspace changes and highlight bugs, regressions, risky patterns, and missing tests before merge.".to_string();
                        tx.send(crate::app_event::AppEvent::RunReviewWithScope {
                            target: ReviewTarget::Custom { instructions: prompt.clone() },
                            prompt,
                            hint: Some("current workspace changes".to_string()),
                            preparation_label: Some("Preparing code review request...".to_string()),
                            auto_resolve: auto_flag,
                        });
                    });
                    return;
                }

            let prompt = "Review the current workspace changes and highlight bugs, regressions, risky patterns, and missing tests before merge.".to_string();
            self.start_review_with_scope(
                ReviewTarget::Custom { instructions: prompt.clone() },
                prompt,
                Some("current workspace changes".to_string()),
                Some("Preparing code review request...".to_string()),
                auto_resolve,
            );
        } else {
            let value = trimmed.to_string();
            let preparation = format!("Preparing code review for {value}");
            self.start_review_with_scope(
                ReviewTarget::Custom { instructions: value.clone() },
                value.clone(),
                Some(value),
                Some(preparation),
                auto_resolve,
            );
        }
    }

    pub(crate) fn start_review_with_scope(
        &mut self,
        target: ReviewTarget,
        prompt: String,
        hint: Option<String>,
        preparation_label: Option<String>,
        auto_resolve: bool,
    ) {
        if auto_resolve {
            let max_re_reviews = self.configured_auto_resolve_re_reviews();
            self.auto_resolve_state = Some(AutoResolveState::new_with_limit(
                target.clone(),
                prompt.clone(),
                hint.clone().unwrap_or_default(),
                None,
                max_re_reviews,
            ));
        } else {
            self.auto_resolve_state = None;
        }

        self.begin_review(target, prompt, hint, preparation_label);
    }

    pub(in crate::chatwidget) fn begin_review(
        &mut self,
        target: ReviewTarget,
        prompt: String,
        hint: Option<String>,
        preparation_label: Option<String>,
    ) {
        self.active_review_hint = None;
        self.active_review_prompt = None;

        let trimmed_hint = hint.as_deref().unwrap_or("").trim();
        let preparation_notice = preparation_label.unwrap_or_else(|| {
            if trimmed_hint.is_empty() {
                "Preparing code review request...".to_string()
            } else {
                format!("Preparing code review for {trimmed_hint}")
            }
        });

        self.insert_background_event_early(preparation_notice);
        self.request_redraw();

        let review_request = ReviewRequest {
            target,
            prompt,
            user_facing_hint: hint,
        };
        match try_acquire_lock("review", &self.config.cwd) {
            Ok(Some(guard)) => {
                self.review_guard = Some(guard);
                self.submit_op(Op::Review { review_request });
            }
            Ok(None) => {
                self.push_background_tail("Review skipped: another review is already running.".to_string());
            }
            Err(err) => {
                self.push_background_tail(format!("Review skipped: could not acquire review lock ({err})"));
            }
        }
    }

    pub(in crate::chatwidget) fn is_review_flow_active(&self) -> bool {
        self.active_review_hint.is_some() || self.active_review_prompt.is_some()
    }

    pub(in crate::chatwidget) fn build_review_summary_cell(
        &self,
        hint: Option<&str>,
        prompt: Option<&str>,
        output: &ReviewOutputEvent,
    ) -> history_cell::AssistantMarkdownCell {
        let mut sections: Vec<String> = Vec::new();
        let title = match hint {
            Some(h) if !h.trim().is_empty() => {
                let trimmed = h.trim();
                format!("**Review summary — {trimmed}**")
            }
            _ => "**Review summary**".to_string(),
        };
        sections.push(title);

        if let Some(p) = prompt {
            let trimmed_prompt = p.trim();
            if !trimmed_prompt.is_empty() {
                sections.push(format!("**Prompt:** {trimmed_prompt}"));
            }
        }

        let explanation = output.overall_explanation.trim();
        if !explanation.is_empty() {
            sections.push(explanation.to_string());
        }
        if !output.findings.is_empty() {
            sections.push(format_review_findings_block(&output.findings, None).trim().to_string());
        }
        let correctness = output.overall_correctness.trim();
        if !correctness.is_empty() {
            sections.push(format!("**Overall correctness:** {correctness}"));
        }
        if output.overall_confidence_score > 0.0 {
            let score = output.overall_confidence_score;
            sections.push(format!("**Confidence score:** {score:.1}"));
        }
        if sections.len() == 1 {
            sections.push("No detailed findings were provided.".to_string());
        }

        let markdown = sections
            .into_iter()
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");

        let state = AssistantMessageState {
            id: HistoryId::ZERO,
            stream_id: None,
            markdown,
            citations: Vec::new(),
            metadata: None,
            token_usage: None,
            mid_turn: false,
            created_at: SystemTime::now(),
        };
        history_cell::AssistantMarkdownCell::from_state(state, &self.config)
    }
}
