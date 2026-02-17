use super::super::*;

impl ChatWidget<'_> {
    #[cfg(test)]
    pub(crate) fn auto_handle_post_turn_review(
        &mut self,
        cfg: TurnConfig,
        descriptor: Option<&TurnDescriptor>,
    ) {
        if !self.auto_state.review_enabled {
            self.auto_turn_review_state = None;
            return;
        }
        if cfg.read_only {
            self.auto_turn_review_state = None;
            return;
        }

        match self.auto_prepare_commit_scope() {
            AutoReviewOutcome::Skip => {
                self.auto_turn_review_state = None;
                if self.auto_state.awaiting_review() {
                    self.maybe_resume_auto_after_review();
                }
            }
            AutoReviewOutcome::Workspace => {
                self.auto_turn_review_state = None;
                self.auto_start_post_turn_review(None, descriptor);
            }
            AutoReviewOutcome::Commit(scope) => {
                self.auto_turn_review_state = None;
                self.auto_start_post_turn_review(Some(scope), descriptor);
            }
        }
    }
    pub(crate) fn auto_submit_prompt(&mut self) {
        if !self.auto_state.is_active() {
            return;
        }

        if self.auto_pending_goal_request {
            self.auto_pending_goal_request = false;
            self.auto_send_conversation_force();
            return;
        }

        let Some(original_prompt) = self.auto_state.current_cli_prompt.clone() else {
            self.auto_stop(Some("Coordinator prompt missing when attempting to submit.".to_string()));
            return;
        };

        if original_prompt.trim().is_empty() {
            self.auto_stop(Some("Coordinator produced an empty prompt.".to_string()));
            return;
        }

        let Some(full_prompt) = self.build_auto_turn_message(&original_prompt) else {
            self.auto_stop(Some("Coordinator produced an empty prompt.".to_string()));
            return;
        };

        self.auto_dispatch_cli_prompt(full_prompt);
    }

    pub(crate) fn auto_start_bootstrap_from_history(&mut self) -> bool {
        if !self.auto_can_bootstrap_from_history() {
            return false;
        }

        let defaults = self.config.auto_drive.clone();
        let default_mode = auto_continue_from_config(defaults.continue_mode);

        if self.auto_state.is_active() {
            self.auto_stop(None);
        }

        self.auto_state.mark_intro_pending();
        self.auto_launch_with_goal(AutoLaunchRequest {
            goal: AUTO_BOOTSTRAP_GOAL_PLACEHOLDER.to_string(),
            derive_goal_from_history: true,
            review_enabled: defaults.review_enabled,
            subagents_enabled: defaults.agents_enabled,
            cross_check_enabled: defaults.cross_check_enabled,
            qa_automation_enabled: defaults.qa_automation_enabled,
            continue_mode: default_mode,
        });

        if self.auto_handle.is_none() {
            return false;
        }

        self.auto_state.current_cli_context = None;
        self.auto_state.hide_cli_context_in_ui = false;
        self.auto_state.current_cli_prompt = Some(String::new());
        self.auto_pending_goal_request = true;
        self.auto_goal_bootstrap_done = false;

        let override_seconds = if matches!(
            self.auto_state.continue_mode,
            AutoContinueMode::Immediate
        ) {
            Some(10)
        } else {
            None
        };
        self.schedule_auto_cli_prompt_with_override(0, String::new(), override_seconds);
        true
    }

    pub(crate) fn auto_dispatch_cli_prompt(&mut self, full_prompt: String) {
        self.auto_pending_goal_request = false;

        self.bottom_pane.set_standard_terminal_hint(None);
        self.auto_state.on_prompt_submitted();
        self.auto_state.set_coordinator_waiting(false);
        self.auto_state.clear_bypass_coordinator_flag();
        self.auto_state.seconds_remaining = 0;
        let post_submit_display = self.auto_state.last_decision_display.clone();
        self.auto_state.current_summary = None;
        self.auto_state.current_status_sent_to_user = None;
        self.auto_state.current_status_title = None;
        self.auto_state.last_broadcast_summary = None;
        self.auto_state.current_display_line = post_submit_display.clone();
        self.auto_state.current_display_is_summary =
            self.auto_state.last_decision_display_is_summary && post_submit_display.is_some();
        self.auto_state.current_summary_index = None;
        self.auto_state.placeholder_phrase = post_submit_display.is_none().then(|| {
            auto_drive_strings::next_auto_drive_phrase().to_string()
        });
        self.auto_state.current_reasoning_title = None;
        self.auto_state.thinking_prefix_stripped = false;

        let should_prepare_agents = self.auto_state.subagents_enabled
            && !self.auto_state.pending_agent_actions.is_empty();
        if should_prepare_agents {
            self.prepare_agents();
        }

        if self.auto_state.review_enabled {
            self.prepare_auto_turn_review_state();
        } else {
            self.auto_turn_review_state = None;
        }
        self.bottom_pane.update_status_text(String::new());
        self.bottom_pane.set_task_running(false);
        let mut message: UserMessage = full_prompt.into();
        message.suppress_persistence = true;
        if self.auto_state.pending_stop_message.is_some() || self.auto_state.suppress_next_cli_display {
            message.display_text.clear();
        }
        self.submit_user_message(message);
        self.auto_state.pending_agent_actions.clear();
        self.auto_state.pending_agent_timing = None;
        self.auto_rebuild_live_ring();
        self.request_redraw();
        self.auto_state.suppress_next_cli_display = false;
    }

    pub(crate) fn auto_pause_for_manual_edit(&mut self, force: bool) {
        if !self.auto_state.is_active() {
            return;
        }

        if !force && !self.auto_state.awaiting_coordinator_submit() {
            return;
        }

        let prompt_text = self
            .auto_state
            .current_cli_prompt
            .clone()
            .unwrap_or_default();
        let full_prompt = self
            .build_auto_turn_message(&prompt_text)
            .unwrap_or_else(|| prompt_text.clone());

        self.auto_state.on_pause_for_manual(true);
        self.auto_state.set_bypass_coordinator_next_submit();
        self.auto_state.countdown_id = self.auto_state.countdown_id.wrapping_add(1);
        self.auto_state.reset_countdown();
        self.clear_composer();
        if !full_prompt.is_empty() {
            self.insert_str(&full_prompt);
        } else if force && !prompt_text.is_empty() {
            self.insert_str(&prompt_text);
        }
        self.bottom_pane.ensure_input_focus();
        self.bottom_pane.set_task_running(true);
        self.bottom_pane
            .update_status_text("Auto Drive paused".to_string());
        self.show_auto_drive_exit_hint();
        self.auto_rebuild_live_ring();
        self.request_redraw();
    }

    // Build a hidden preface for the next Auto turn based on coordinator hints.
    pub(crate) fn build_auto_turn_message(&self, prompt_cli: &str) -> Option<String> {
        let mut sections: Vec<String> = Vec::new();

        if let Some(ctx) = self
            .auto_state
            .current_cli_context
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            sections.push(ctx.to_string());
        }

        if !prompt_cli.trim().is_empty() {
            sections.push(prompt_cli.trim().to_string());
        }

        let agent_actions = &self.auto_state.pending_agent_actions;
        if !agent_actions.is_empty() {
            let agent_timing = self.auto_state.pending_agent_timing;
            let mut agent_lines = Vec::with_capacity(agent_actions.len() * 4 + 5);
            const BLOCK_PREFIX: &str = "   ";
            const LINE_PREFIX: &str = "      ";

            agent_lines.push(format!("{BLOCK_PREFIX}<agents>"));
            agent_lines.push(format!(
                "{LINE_PREFIX}Please use agents to help you complete this task."
            ));

            for action in agent_actions {
                let prompt = action
                    .prompt
                    .trim()
                    .replace('\n', " ")
                    .replace('"', "\\\"");
                let write_text = if action.write { "write: true" } else { "write: false" };

                agent_lines.push(String::new());
                agent_lines.push(format!(
                    "{LINE_PREFIX}Please run agent.create with {write_text} and prompt like \"{prompt}\"."
                ));

                if let Some(ctx) = action
                    .context
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    agent_lines.push(format!(
                        "{LINE_PREFIX}Context: {}",
                        ctx.replace('\n', " ")
                    ));
                }

                if let Some(models) = action
                    .models
                    .as_ref()
                    .filter(|list| !list.is_empty())
                {
                    agent_lines.push(format!(
                        "{LINE_PREFIX}Models: [{}]",
                        models.join(", ")
                    ));
                }
            }

            agent_lines.push(String::new());
            let timing_line = match agent_timing {
                Some(AutoTurnAgentsTiming::Parallel) =>
                    "Timing (parallel): Launch these agents in the background while you continue the CLI prompt. Call agent.wait with the batch_id when you are ready to merge their results.".to_string(),
                Some(AutoTurnAgentsTiming::Blocking) =>
                    "Timing (blocking): Launch these agents first, then wait with agent.wait (use the batch_id from agent.create) and only continue the CLI prompt once their results are ready.".to_string(),
                None =>
                    "Timing (default blocking): After launching the agents, wait with agent.wait (use the batch_id returned by agent.create) and fold their output into your plan.".to_string(),
            };
            agent_lines.push(format!("{LINE_PREFIX}{timing_line}"));
            agent_lines.push(String::new());

            if agent_actions.iter().any(|action| !action.write) {
                agent_lines.push(format!(
                    "{LINE_PREFIX}Call agent.result to get the results from the agent if needed."
                ));
                agent_lines.push(String::new());
            }

            if agent_actions.iter().any(|action| action.write) {
                agent_lines.push(format!(
                    "{LINE_PREFIX}When agents run with write: true, they perform edits in their own worktree. Considering reviewing and merging the best worktree once they complete."
                ));
                agent_lines.push(String::new());
            }

            agent_lines.push(format!("{BLOCK_PREFIX}</agents>"));

            sections.push(agent_lines.join("\n"));
        }

        let combined = sections.join("\n\n");
        if combined.trim().is_empty() {
            None
        } else {
            Some(combined)
        }
    }

    pub(crate) fn auto_agents_can_write(&self) -> bool {
        if code_core::git_info::get_git_repo_root(&self.config.cwd).is_none() {
            return false;
        }
        matches!(
            self.config.sandbox_policy,
            SandboxPolicy::DangerFullAccess | SandboxPolicy::WorkspaceWrite { .. }
        )
    }

    pub(crate) fn resolve_agent_write_flag(&self, requested_write: Option<bool>) -> bool {
        if !self.auto_agents_can_write() {
            return false;
        }
        if !self.auto_state.subagents_enabled {
            return requested_write.unwrap_or(false);
        }
        true
    }

    pub(crate) fn auto_stop(&mut self, message: Option<String>) {
        self.next_cli_text_format = None;
        self.auto_pending_goal_request = false;
        self.auto_goal_bootstrap_done = false;
        self.auto_drive_pid_guard = None;
        let effects = self
            .auto_state
            .stop_run(Instant::now(), message);
        self.auto_goal_escape_state = AutoGoalEscState::Inactive;
        self.auto_apply_controller_effects(effects);
    }

    pub(crate) fn auto_on_assistant_final(&mut self) {
        if !self.auto_state.is_active() || !self.auto_state.is_waiting_for_response() {
            return;
        }
        self.auto_state.on_resume_from_manual();
        self.auto_state.reset_countdown();
        self.auto_state.current_summary = Some(String::new());
        self.auto_state.current_status_sent_to_user = None;
        self.auto_state.current_status_title = None;
        self.auto_state.current_summary_index = None;
        self.auto_state.placeholder_phrase = None;
        self.auto_state.thinking_prefix_stripped = false;

        let auto_resolve_blocking = self.auto_resolve_should_block_auto_resume();
        let review_pending = self.is_review_flow_active()
            || (self.auto_state.review_enabled
                && self
                    .pending_auto_turn_config
                    .as_ref()
                    .is_some_and(|cfg| !cfg.read_only));

        if review_pending || auto_resolve_blocking {
            self.auto_state.on_begin_review(false);
            #[cfg(any(test, feature = "test-helpers"))]
            if !self.auto_state.awaiting_review() {
                // Tests can run in parallel, so the shared review lock may already be held.
                // Force the state into AwaitingReview so assertions stay deterministic.
                self.auto_state.set_phase(AutoRunPhase::AwaitingReview {
                    diagnostics_pending: false,
                });
            }
        } else {
            self.auto_state.on_complete_review();
        }
        self.auto_rebuild_live_ring();
        self.request_redraw();
        self.rebuild_auto_history();

        if self.auto_state.awaiting_review() {
            return;
        }

        if !self.auto_state.should_bypass_coordinator_next_submit() {
            self.auto_send_conversation();
        }
    }

    #[cfg(test)]
    fn auto_start_post_turn_review(
        &mut self,
        scope: Option<AutoReviewCommitScope>,
        descriptor: Option<&TurnDescriptor>,
    ) {
        use code_protocol::protocol::ReviewTarget;

        if !self.auto_state.review_enabled {
            return;
        }
        let strategy = descriptor.and_then(|d| d.review_strategy.as_ref());
        let (target, mut prompt, mut hint, preparation) = match scope {
            Some(scope) => {
                let commit_id = scope.commit;
                let commit_for_prompt = commit_id.clone();
                let short_sha: String = commit_for_prompt.chars().take(8).collect();
                let file_label = if scope.file_count == 1 {
                    "1 file".to_string()
                } else {
                    format!("{} files", scope.file_count)
                };
                let prompt = format!(
                    "Review commit {commit_for_prompt} generated during the latest Auto Drive turn. Highlight bugs, regressions, risky patterns, and missing tests before merge."
                );
                let hint = format!("auto turn changes — {short_sha} ({file_label})");
                let preparation = format!("Preparing code review for commit {short_sha}");
                let target = ReviewTarget::Commit { sha: commit_id, title: None };
                (target, prompt, hint, preparation)
            }
            None => {
                let prompt = "Review the current workspace changes and highlight bugs, regressions, risky patterns, and missing tests before merge.".to_string();
                let hint = "current workspace changes".to_string();
                let preparation = "Preparing code review request...".to_string();
                (ReviewTarget::UncommittedChanges, prompt, hint, preparation)
            }
        };

        if let Some(strategy) = strategy {
            if let Some(custom_prompt) = strategy
                .custom_prompt
                .as_ref()
                .and_then(|text| {
                    let trimmed = text.trim();
                    (!trimmed.is_empty()).then_some(trimmed)
                })
            {
                prompt = custom_prompt.to_string();
            }

            if let Some(scope_hint) = strategy
                .scope_hint
                .as_ref()
                .and_then(|text| {
                    let trimmed = text.trim();
                    (!trimmed.is_empty()).then_some(trimmed)
                })
            {
                hint = scope_hint.to_string();
            }
        }

        if self.config.tui.review_auto_resolve {
            let max_re_reviews = self.configured_auto_resolve_re_reviews();
            self.auto_resolve_state = Some(AutoResolveState::new_with_limit(
                target.clone(),
                prompt.clone(),
                hint.clone(),
                None,
                max_re_reviews,
            ));
        } else {
            self.auto_resolve_state = None;
        }
        let hint_opt = (!hint.trim().is_empty()).then(|| hint.clone());
        self.begin_review(target, prompt, hint_opt, Some(preparation));
    }

}
