use super::*;

impl ChatWidget<'_> {
    pub(super) fn is_cli_running(&self) -> bool {
        if !self.exec.running_commands.is_empty() {
            return true;
        }
        if !self.tools_state.running_custom_tools.is_empty()
            || !self.tools_state.web_search_sessions.is_empty()
            || !self.tools_state.running_wait_tools.is_empty()
            || !self.tools_state.running_kill_tools.is_empty()
        {
            return true;
        }
        if self.stream.is_write_cycle_active() {
            return true;
        }
        if !self.active_task_ids.is_empty() {
            return true;
        }
        if self.active_agents.iter().any(|agent| {
            let is_auto_review = matches!(agent.source_kind, Some(AgentSourceKind::AutoReview))
                || agent
                    .batch_id
                    .as_deref()
                    .is_some_and(|batch| batch.eq_ignore_ascii_case("auto-review"));
            matches!(agent.status, AgentStatus::Pending | AgentStatus::Running) && !is_auto_review
        }) {
            return true;
        }
        false
    }

    pub(super) fn refresh_auto_drive_visuals(&mut self) {
        if self.auto_state.is_active()
            || self.auto_state.should_show_goal_entry()
            || self.auto_state.last_run_summary.is_some()
        {
            self.auto_rebuild_live_ring();
        }
    }

    pub(super) fn auto_reduced_motion_preference() -> bool {
        match std::env::var("CODE_TUI_REDUCED_MOTION") {
            Ok(value) => {
                let normalized = value.trim().to_ascii_lowercase();
                !matches!(normalized.as_str(), "" | "0" | "false" | "off" | "no")
            }
            Err(_) => false,
        }
    }

    pub(super) fn auto_reset_intro_timing(&mut self) {
        self.auto_state.reset_intro_timing();
    }

    pub(super) fn auto_ensure_intro_timing(&mut self) {
        let reduced_motion = Self::auto_reduced_motion_preference();
        self.auto_state.ensure_intro_timing(reduced_motion);
    }

    pub(super) fn auto_show_goal_entry_panel(&mut self) {
        self.auto_state.set_phase(AutoRunPhase::AwaitingGoalEntry);
        self.auto_state.goal = None;
        self.auto_pending_goal_request = false;
        self.auto_goal_bootstrap_done = false;
        let seed_intro = self.auto_state.take_intro_pending();
        if seed_intro {
            self.auto_reset_intro_timing();
            self.auto_ensure_intro_timing();
        }
        self.auto_goal_escape_state = AutoGoalEscState::Inactive;
        let hint = "Let's do this! What's your goal?".to_string();
        let status_lines = vec![hint];
        let model = AutoCoordinatorViewModel::Active(AutoActiveViewModel {
            goal: None,
            status_lines,
            cli_prompt: None,
            cli_context: None,
            show_composer: true,
            awaiting_submission: false,
            waiting_for_response: false,
            coordinator_waiting: false,
            waiting_for_review: false,
            countdown: None,
            button: None,
            manual_hint: None,
            ctrl_switch_hint: String::new(),
            cli_running: false,
            turns_completed: 0,
            started_at: None,
            elapsed: None,
            status_sent_to_user: None,
            status_title: None,
            session_tokens: self.auto_session_tokens(),
            editing_prompt: false,
            intro_started_at: self.auto_state.intro_started_at,
            intro_reduced_motion: self.auto_state.intro_reduced_motion,
        });
        self.bottom_pane.show_auto_coordinator_view(model);
        self.bottom_pane.set_task_running(false);
        self.bottom_pane.clear_live_ring();
        self.bottom_pane.update_status_text("Auto Drive".to_string());
        self.auto_update_terminal_hint();
        self.bottom_pane.ensure_input_focus();
        self.clear_composer();
        self.request_redraw();
    }

    pub(super) fn auto_exit_goal_entry_preserve_draft(&mut self) -> bool {
        if !self.auto_state.should_show_goal_entry() {
            return false;
        }

        let last_run_summary = self.auto_state.last_run_summary.clone();
        let last_decision_summary = self.auto_state.last_decision_summary.clone();
        let last_decision_status_sent_to_user =
            self.auto_state.last_decision_status_sent_to_user.clone();
        let last_decision_status_title =
            self.auto_state.last_decision_status_title.clone();
        let last_decision_display = self.auto_state.last_decision_display.clone();
        let last_decision_display_is_summary = self.auto_state.last_decision_display_is_summary;

        self.auto_state.reset();
        self.auto_state.last_run_summary = last_run_summary;
        self.auto_state.last_decision_summary = last_decision_summary;
        self.auto_state.last_decision_status_sent_to_user = last_decision_status_sent_to_user;
        self.auto_state.last_decision_status_title = last_decision_status_title;
        self.auto_state.last_decision_display = last_decision_display;
        self.auto_state.last_decision_display_is_summary = last_decision_display_is_summary;
        self.auto_state.set_phase(AutoRunPhase::Idle);
        self.auto_goal_escape_state = AutoGoalEscState::Inactive;
        self.bottom_pane.clear_auto_coordinator_view(true);
        self.bottom_pane.clear_live_ring();
        self.bottom_pane.set_task_running(false);
        self.bottom_pane.update_status_text(String::new());
        self.auto_rebuild_live_ring();
        self.request_redraw();
        true
    }

    pub(super) fn auto_launch_with_goal(&mut self, request: AutoLaunchRequest) {
        let AutoLaunchRequest {
            goal,
            derive_goal_from_history,
            review_enabled,
            subagents_enabled,
            cross_check_enabled,
            qa_automation_enabled,
            continue_mode,
        } = request;
        let conversation = self.rebuild_auto_history();
        let reduced_motion = Self::auto_reduced_motion_preference();
        self.auto_state.prepare_launch(
            goal.clone(),
            code_auto_drive_core::AutoLaunchSettings {
                review_enabled,
                subagents_enabled,
                cross_check_enabled,
                qa_automation_enabled,
                continue_mode,
                reduced_motion,
            },
        );
        self.config.auto_drive.cross_check_enabled = cross_check_enabled;
        self.config.auto_drive.qa_automation_enabled = qa_automation_enabled;
        let coordinator_events = {
            let app_event_tx = self.app_event_tx.clone();
            AutoCoordinatorEventSender::new(move |event| {
                match event {
                    AutoCoordinatorEvent::Decision {
                        seq,
                        status,
                        status_title,
                        status_sent_to_user,
                        goal,
                        cli,
                        agents_timing,
                        agents,
                        transcript,
                    } => {
                        app_event_tx.send(AppEvent::AutoCoordinatorDecision {
                            seq,
                            status,
                            status_title,
                            status_sent_to_user,
                            goal,
                            cli,
                            agents_timing,
                            agents,
                            transcript,
                        });
                    }
                    AutoCoordinatorEvent::Thinking { delta, summary_index } => {
                        app_event_tx.send(AppEvent::AutoCoordinatorThinking { delta, summary_index });
                    }
                    AutoCoordinatorEvent::Action { message } => {
                        app_event_tx.send(AppEvent::AutoCoordinatorAction { message });
                    }
                    AutoCoordinatorEvent::UserReply {
                        user_response,
                        cli_command,
                    } => {
                        app_event_tx.send(AppEvent::AutoCoordinatorUserReply {
                            user_response,
                            cli_command,
                        });
                    }
                    AutoCoordinatorEvent::TokenMetrics {
                        total_usage,
                        last_turn_usage,
                        turn_count,
                        duplicate_items,
                        replay_updates,
                    } => {
                        app_event_tx.send(AppEvent::AutoCoordinatorTokenMetrics {
                            total_usage,
                            last_turn_usage,
                            turn_count,
                            duplicate_items,
                            replay_updates,
                        });
                    }
                    AutoCoordinatorEvent::CompactedHistory { conversation, show_notice } => {
                        app_event_tx.send(AppEvent::AutoCoordinatorCompactedHistory {
                            conversation,
                            show_notice,
                        });
                    }
                    AutoCoordinatorEvent::StopAck => {
                        app_event_tx.send(AppEvent::AutoCoordinatorStopAck);
                    }
                }
            })
        };

        let mut auto_config = self.config.clone();
        auto_config.model = self.config.auto_drive.model.trim().to_string();
        if auto_config.model.is_empty() {
            auto_config.model = code_auto_drive_core::MODEL_SLUG.to_string();
        }
        auto_config.model_reasoning_effort = self.config.auto_drive.model_reasoning_effort;

        let mut pid_guard = AutoDrivePidFile::write(
            &self.config.code_home,
            Some(goal.as_str()),
            AutoDriveMode::Tui,
        );

        match start_auto_coordinator(
            coordinator_events,
            goal.clone(),
            conversation,
            auto_config,
            self.config.debug,
            derive_goal_from_history,
        ) {
            Ok(handle) => {
                self.auto_handle = Some(handle);
                self.auto_drive_pid_guard = pid_guard.take();
                let placeholder = auto_drive_strings::next_auto_drive_phrase().to_string();
                let effects = self
                    .auto_state
                    .launch_succeeded(goal, Some(placeholder), Instant::now());
                self.auto_apply_controller_effects(effects);
            }
            Err(err) => {
                drop(pid_guard);
                let effects = self
                    .auto_state
                    .launch_failed(goal, err.to_string());
                self.auto_apply_controller_effects(effects);
            }
        }
    }

    pub(crate) fn handle_auto_command(&mut self, goal: Option<String>) {
        let provided = goal.unwrap_or_default();
        let trimmed = provided.trim();

        if trimmed.eq_ignore_ascii_case("settings") {
            self.ensure_auto_drive_settings_overlay();
            return;
        }

        let full_auto_enabled = matches!(
            (&self.config.sandbox_policy, self.config.approval_policy),
            (SandboxPolicy::DangerFullAccess, AskForApproval::Never)
        );

        if !(full_auto_enabled || (trimmed.is_empty() && self.auto_state.is_active())) {
            self.push_background_tail(
                "Please use Shift+Tab to switch to Full Auto before using Auto Drive"
                    .to_string(),
            );
            self.request_redraw();
            return;
        }
        if trimmed.is_empty() {
            if self.auto_state.is_active() {
                self.auto_stop(None);
            }
            let started = self.auto_start_bootstrap_from_history();
            if !started {
                self.auto_state.reset();
                self.auto_state.set_phase(AutoRunPhase::Idle);
                self.auto_show_goal_entry_panel();
            }
            self.request_redraw();
            return;
        }

        let goal_text = trimmed.to_string();

        if self.auto_state.is_active() {
            self.auto_stop(None);
        }

        let defaults = self.config.auto_drive.clone();
        let default_mode = auto_continue_from_config(defaults.continue_mode);

        self.auto_state.mark_intro_pending();
        self.auto_launch_with_goal(AutoLaunchRequest {
            goal: goal_text,
            derive_goal_from_history: false,
            review_enabled: defaults.review_enabled,
            subagents_enabled: defaults.agents_enabled,
            cross_check_enabled: defaults.cross_check_enabled,
            qa_automation_enabled: defaults.qa_automation_enabled,
            continue_mode: default_mode,
        });
    }

    pub(crate) fn show_auto_drive_settings(&mut self) {
        self.history_render.invalidate_all();
        self.mark_render_requests_dirty();
        self.history_prefix_append_only.set(false);
        self.ensure_auto_drive_settings_overlay();
    }

    pub(crate) fn close_auto_drive_settings(&mut self) {
        if matches!(
            self.settings
                .overlay
                .as_ref()
                .map(settings_overlay::SettingsOverlayView::active_section),
            Some(SettingsSection::AutoDrive)
        ) {
            self.close_settings_overlay();
        }
        self.history_render.invalidate_all();
        self.mark_render_requests_dirty();
        self.history_prefix_append_only.set(false);
        let should_rebuild_view = if self.auto_state.is_active() {
            !self.auto_state.is_paused_manual()
        } else {
            self.auto_state.should_show_goal_entry() || self.auto_state.last_run_summary.is_some()
        };

        if should_rebuild_view {
            self.auto_rebuild_live_ring();
        }
        self.bottom_pane.ensure_input_focus();
    }

    pub(crate) fn apply_auto_drive_settings(
        &mut self,
        review_enabled: bool,
        agents_enabled: bool,
        cross_check_enabled: bool,
        qa_automation_enabled: bool,
        continue_mode: AutoContinueMode,
    ) {
        let mut changed = false;
        if self.auto_state.review_enabled != review_enabled {
            self.auto_state.review_enabled = review_enabled;
            changed = true;
        }
        if self.auto_state.subagents_enabled != agents_enabled {
            self.auto_state.subagents_enabled = agents_enabled;
            changed = true;
        }
        if self.auto_state.cross_check_enabled != cross_check_enabled {
            self.auto_state.cross_check_enabled = cross_check_enabled;
            changed = true;
        }
        if self.auto_state.qa_automation_enabled != qa_automation_enabled {
            self.auto_state.qa_automation_enabled = qa_automation_enabled;
            changed = true;
        }
        if self.auto_state.continue_mode != continue_mode {
            let effects = self.auto_state.update_continue_mode(continue_mode);
            self.auto_apply_controller_effects(effects);
            changed = true;
        }

        if !changed {
            return;
        }

        self.config.auto_drive.review_enabled = review_enabled;
        self.config.auto_drive.agents_enabled = agents_enabled;
        self.config.auto_drive.cross_check_enabled = cross_check_enabled;
        self.config.auto_drive.qa_automation_enabled = qa_automation_enabled;
        self.config.auto_drive.continue_mode = auto_continue_to_config(continue_mode);
        self.restore_auto_resolve_attempts_if_lost();

        if let Ok(home) = code_core::config::find_code_home() {
            if let Err(err) = code_core::config::set_auto_drive_settings(
                &home,
                &self.config.auto_drive,
                self.config.auto_drive_use_chat_model,
            )
            {
                tracing::warn!("Failed to persist Auto Drive settings: {err}");
            }
        } else {
            tracing::warn!("Could not locate config home to persist Auto Drive settings");
        }

        self.refresh_settings_overview_rows();
        self.refresh_auto_drive_visuals();
        self.request_redraw();
    }

    pub(super) fn auto_send_conversation(&mut self) {
        if !self.auto_state.is_active() || self.auto_state.is_waiting_for_response() {
            return;
        }
        self.auto_state.on_complete_review();
        if !self.auto_state.is_paused_manual() {
            self.auto_state.clear_bypass_coordinator_flag();
        }
        let conversation = std::sync::Arc::<[ResponseItem]>::from(self.current_auto_history());
        let Some(handle) = self.auto_handle.as_ref() else {
            return;
        };
        if handle
            .send(AutoCoordinatorCommand::UpdateConversation(conversation))
            .is_err()
        {
            self.auto_stop(Some("Coordinator stopped unexpectedly.".to_string()));
        } else {
            self.bottom_pane.set_standard_terminal_hint(None);
            self.auto_state.on_prompt_submitted();
            self.auto_state.set_coordinator_waiting(true);
            self.auto_state.current_summary = None;
            self.auto_state.current_status_sent_to_user = None;
            self.auto_state.current_status_title = None;
            self.auto_state.current_cli_prompt = None;
            self.auto_state.current_cli_context = None;
            self.auto_state.hide_cli_context_in_ui = false;
            self.auto_state.last_broadcast_summary = None;
            self.auto_state.current_summary_index = None;
            self.auto_state.current_display_line = None;
            self.auto_state.current_display_is_summary = false;
            self.auto_state.current_reasoning_title = None;
            self.auto_state.placeholder_phrase =
                Some(auto_drive_strings::next_auto_drive_phrase().to_string());
            self.auto_state.thinking_prefix_stripped = false;
            self.auto_rebuild_live_ring();
            self.request_redraw();
        }
    }

    pub(super) fn auto_send_conversation_force(&mut self) {
        if !self.auto_state.is_active() {
            return;
        }
        if !self.auto_state.is_paused_manual() {
            self.auto_state.clear_bypass_coordinator_flag();
        }
        let conversation = std::sync::Arc::<[ResponseItem]>::from(self.current_auto_history());
        let Some(handle) = self.auto_handle.as_ref() else {
            return;
        };
        if handle
            .send(AutoCoordinatorCommand::UpdateConversation(conversation))
            .is_err()
        {
            self.auto_stop(Some("Coordinator stopped unexpectedly.".to_string()));
        } else {
            self.bottom_pane.set_standard_terminal_hint(None);
            self.auto_state.on_prompt_submitted();
            self.auto_state.set_coordinator_waiting(true);
            self.auto_state.current_summary = None;
            self.auto_state.current_status_sent_to_user = None;
            self.auto_state.current_status_title = None;
            self.auto_state.current_cli_prompt = None;
            self.auto_state.current_cli_context = None;
            self.auto_state.hide_cli_context_in_ui = false;
            self.auto_state.last_broadcast_summary = None;
            self.auto_state.current_summary_index = None;
            self.auto_state.current_display_line = None;
            self.auto_state.current_display_is_summary = false;
            self.auto_state.current_reasoning_title = None;
            self.auto_state.placeholder_phrase =
                Some(auto_drive_strings::next_auto_drive_phrase().to_string());
            self.auto_state.thinking_prefix_stripped = false;
            self.auto_rebuild_live_ring();
            self.request_redraw();
        }
    }

    pub(super) fn auto_send_user_prompt_to_coordinator(
        &mut self,
        prompt: String,
        conversation: Vec<ResponseItem>,
    ) -> bool {
        let Some(handle) = self.auto_handle.as_ref() else {
            return false;
        };
        let command = AutoCoordinatorCommand::HandleUserPrompt {
            _prompt: prompt,
            conversation: conversation.into(),
        };
        match handle.send(command) {
            Ok(()) => {
                self.auto_state.on_prompt_submitted();
                self.auto_state.set_coordinator_waiting(true);
                self.auto_state.placeholder_phrase =
                    Some(auto_drive_strings::next_auto_drive_phrase().to_string());
                self.auto_rebuild_live_ring();
                self.request_redraw();
                true
            }
            Err(err) => {
                tracing::warn!("failed to dispatch user prompt to coordinator: {err}");
                false
            }
        }
    }

    pub(super) fn auto_failure_is_transient(message: &str) -> bool {
        let lower = message.to_ascii_lowercase();
        const TRANSIENT_MARKERS: &[&str] = &[
            "stream error",
            "network error",
            "timed out",
            "timeout",
            "temporarily unavailable",
            "retry window exceeded",
            "retry limit exceeded",
            "connection reset",
            "connection refused",
            "broken pipe",
            "dns error",
            "host unreachable",
            "send request",
        ];
        TRANSIENT_MARKERS.iter().any(|needle| lower.contains(needle))
    }

    pub(super) fn auto_schedule_restart_event(&self, token: u64, attempt: u32, delay: Duration) {
        let tx = self.app_event_tx.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                tx.send(AppEvent::AutoCoordinatorRestart { token, attempt });
            });
        } else {
            std::thread::spawn(move || {
                if !delay.is_zero() {
                    std::thread::sleep(delay);
                }
                tx.send(AppEvent::AutoCoordinatorRestart { token, attempt });
            });
        }
    }

    pub(super) fn auto_pause_for_transient_failure(&mut self, message: String) {
        warn!("auto drive transient failure: {}", message);

        if let Some(handle) = self.auto_handle.take() {
            handle.cancel();
        }

        self.pending_turn_descriptor = None;
        self.pending_auto_turn_config = None;

        let effects = self
            .auto_state
            .pause_for_transient_failure(Instant::now(), message);
        self.auto_apply_controller_effects(effects);
    }

    pub(crate) fn auto_handle_decision(&mut self, event: AutoDecisionEvent) {
        let AutoDecisionEvent {
            seq,
            status,
            status_title,
            status_sent_to_user,
            goal,
            cli,
            agents_timing,
            agents,
            transcript,
        } = event;
        if !self.auto_state.is_active() {
            if let Some(handle) = self.auto_handle.as_ref() {
                let _ = handle.send(code_auto_drive_core::AutoCoordinatorCommand::AckDecision { seq });
            }
            return;
        }

        self.auto_pending_goal_request = false;

        if let Some(goal_text) = goal.as_ref().map(|g| g.trim()).filter(|g| !g.is_empty()) {
            let derived_goal = goal_text.to_string();
            self.auto_state.goal = Some(derived_goal.clone());
            self.auto_goal_bootstrap_done = true;
            self.auto_card_set_goal(Some(derived_goal));
        }

        let status_title = Self::normalize_status_field(status_title);
        let status_sent_to_user = Self::normalize_status_field(status_sent_to_user);

        self.auto_state.turns_completed = self.auto_state.turns_completed.saturating_add(1);

        if !transcript.is_empty() {
            self.auto_history.append_raw(&transcript);
        }

        if let Some(handle) = self.auto_handle.as_ref() {
            let _ = handle.send(code_auto_drive_core::AutoCoordinatorCommand::AckDecision { seq });
        }

        self.auto_state.current_status_sent_to_user = status_sent_to_user.clone();
        self.auto_state.current_status_title = status_title.clone();
        self.auto_state.last_decision_status_sent_to_user = status_sent_to_user.clone();
        self.auto_state.last_decision_status_title = status_title.clone();
        let planning_turn = cli
            .as_ref()
            .map(|action| action.suppress_ui_context)
            .unwrap_or(false);
        let cli_context_raw = cli
            .as_ref()
            .and_then(|action| action.context.clone());
        let cli_context = Self::normalize_status_field(cli_context_raw);
        let cli_prompt = cli.as_ref().map(|action| action.prompt.clone());

        self.auto_state.current_cli_context = cli_context;
        self.auto_state.hide_cli_context_in_ui = planning_turn;
        self.auto_state.suppress_next_cli_display = planning_turn;
        if let Some(ref prompt_text) = cli_prompt {
            self.auto_state.current_cli_prompt = Some(prompt_text.clone());
        } else {
            self.auto_state.current_cli_prompt = None;
        }

        let summary_text = Self::compose_status_summary(&status_title, &status_sent_to_user);
        self.auto_state.last_decision_summary = Some(summary_text.clone());
        self.auto_state.set_coordinator_waiting(false);
        self.auto_on_reasoning_final(&summary_text);
        self.auto_state.last_decision_display = self.auto_state.current_display_line.clone();
        self.auto_state.last_decision_display_is_summary =
            self.auto_state.current_display_is_summary;
            self.auto_state.on_resume_from_manual();

        self.pending_turn_descriptor = None;
        self.pending_auto_turn_config = None;

        if let Some(current) = status_title
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            self.auto_card_add_action(
                format!("Status: {current}"),
                AutoDriveActionKind::Info,
            );
        }

        let mut promoted_agents: Vec<String> = Vec::new();
        let continue_status = matches!(status, AutoCoordinatorStatus::Continue);

        let resolved_agents: Vec<AutoTurnAgentsAction> = agents
            .into_iter()
            .map(|mut action| {
                let original = action.write;
                let requested = action.write_requested;
                let resolved = self.resolve_agent_write_flag(requested);
                if resolved && !original {
                    promoted_agents.push(action.prompt.clone());
                }
                action.write = resolved;
                action
            })
            .collect();

        if continue_status {
            self.auto_state.pending_agent_actions = resolved_agents;
            self.auto_state.pending_agent_timing = agents_timing
                .filter(|_| !self.auto_state.pending_agent_actions.is_empty());
        } else {
            self.auto_state.pending_agent_actions.clear();
            self.auto_state.pending_agent_timing = None;
        }

        if !promoted_agents.is_empty() {
            let joined = promoted_agents
                .into_iter()
                .map(|prompt| {
                    let trimmed = prompt.trim();
                    if trimmed.is_empty() {
                        "<empty prompt>".to_string()
                    } else {
                        format!("\"{trimmed}\"")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            self.auto_card_add_action(
                format!("Auto Drive enabled write mode for agent prompt(s): {joined}"),
                AutoDriveActionKind::Info,
            );
        }

        if !matches!(status, AutoCoordinatorStatus::Failed) {
            self.auto_state.transient_restart_attempts = 0;
           self.auto_state.on_recovery_attempt();
            self.auto_state.pending_restart = None;
        }

        match status {
            AutoCoordinatorStatus::Continue => {
                let Some(prompt_text) = cli_prompt else {
                    self.auto_stop(Some("Coordinator response omitted a prompt.".to_string()));
                    return;
                };
                if planning_turn {
                    self.push_background_tail("Auto Drive: Planning started".to_string());
                    if let Some(full_prompt) = self.build_auto_turn_message(&prompt_text) {
                        self.auto_dispatch_cli_prompt(full_prompt);
                    } else {
                        self.auto_stop(Some(
                            "Coordinator produced an empty planning prompt.".to_string(),
                        ));
                    }
                } else {
                    self.schedule_auto_cli_prompt(seq, prompt_text);
                }
            }
            AutoCoordinatorStatus::Success => {
                let normalized = summary_text.trim();
                let message = if normalized.is_empty() {
                    "Coordinator success.".to_string()
                } else if normalized
                    .to_ascii_lowercase()
                    .starts_with("coordinator success:")
                {
                    summary_text
                } else {
                    format!("Coordinator success: {summary_text}")
                };

                let diagnostics_goal = self
                    .auto_state
                    .goal
                    .as_deref()
                    .unwrap_or("(goal unavailable)");

                let prompt_text = format!(
                    r#"Here was the original goal:
{diagnostics_goal}

Have we met every part of this goal and is there no further work to do?"#
                );

                let tf = TextFormat {
                    r#type: "json_schema".to_string(),
                    name: Some("auto_drive_diagnostics".to_string()),
                    strict: Some(true),
                    schema: Some(code_auto_drive_diagnostics::AutoDriveDiagnostics::completion_schema()),
                };
                self.submit_op(Op::SetNextTextFormat { format: tf.clone() });
                self.next_cli_text_format = Some(tf);
                self.auto_state.pending_stop_message = Some(message);
                self.auto_card_add_action(
                    "Auto Drive Diagnostics: Validating progress".to_string(),
                    AutoDriveActionKind::Info,
                );
                self.schedule_auto_cli_prompt(seq, prompt_text);
                self.auto_submit_prompt();
            }
            AutoCoordinatorStatus::Failed => {
                let normalized = summary_text.trim();
                let message = if normalized.is_empty() {
                    "Coordinator error.".to_string()
                } else if normalized
                    .to_ascii_lowercase()
                    .starts_with("coordinator error:")
                {
                    summary_text
                } else {
                    format!("Coordinator error: {summary_text}")
                };
                if Self::auto_failure_is_transient(&message) {
                    self.auto_pause_for_transient_failure(message);
                } else {
                    self.auto_stop(Some(message));
                }
            }
        }
    }

    pub(crate) fn auto_handle_user_reply(
        &mut self,
        user_response: Option<String>,
        cli_command: Option<String>,
    ) {
        if let Some(text) = user_response {
            if let Some(item) = Self::auto_drive_make_assistant_message(text.clone()) {
                self.auto_history
                    .append_raw(std::slice::from_ref(&item));
            }
            let lines = vec!["AUTO DRIVE RESPONSE".to_string(), text];
            self.history_push_plain_paragraphs(PlainMessageKind::Notice, lines);
        }

        if let Some(command) = cli_command {
            if command.trim_start().starts_with('/') {
                self.app_event_tx
                    .send(AppEvent::DispatchCommand(SlashCommand::Auto, command));
            } else {
                let mut message: UserMessage = command.into();
                message.suppress_persistence = true;
                self.submit_user_message(message);
            }
        } else {
            self.auto_state.set_phase(AutoRunPhase::Active);
            self.auto_state.placeholder_phrase = None;
        }

        self.auto_rebuild_live_ring();
        self.request_redraw();
    }

    pub(crate) fn auto_handle_token_metrics(
        &mut self,
        total_usage: TokenUsage,
        last_turn_usage: TokenUsage,
        turn_count: u32,
        duplicate_items: u32,
        replay_updates: u32,
    ) {
        self.auto_history
            .apply_token_metrics(
                total_usage,
                last_turn_usage,
                turn_count,
                duplicate_items,
                replay_updates,
            );
        self.request_redraw();
    }

    pub(super) fn auto_session_tokens(&self) -> Option<u64> {
        let total = self.auto_history.total_tokens().blended_total();
        (total > 0).then_some(total)
    }

    pub(crate) fn auto_handle_compacted_history(
        &mut self,
        conversation: std::sync::Arc<[ResponseItem]>,
        show_notice: bool,
    ) {
        let (previous_items, previous_indices) = self.export_auto_drive_items_with_indices();
        let conversation = conversation.as_ref().to_vec();
        self.auto_compaction_overlay = self
            .derive_compaction_overlay(&previous_items, &previous_indices, &conversation);
        self.auto_history.replace_all(conversation);
        if show_notice {
            self.history_push_plain_paragraphs(
                PlainMessageKind::Notice,
                [COMPACTION_CHECKPOINT_MESSAGE],
            );
        }
        self.auto_rebuild_live_ring();
        self.request_redraw();
    }

    pub(super) fn schedule_auto_cli_prompt(&mut self, decision_seq: u64, prompt_text: String) {
        self.schedule_auto_cli_prompt_with_override(decision_seq, prompt_text, None);
    }

    pub(super) fn schedule_auto_cli_prompt_with_override(
        &mut self,
        decision_seq: u64,
        prompt_text: String,
        countdown_override: Option<u8>,
    ) {
        self.auto_state.suppress_next_cli_display = false;
        let effects = self
            .auto_state
            .schedule_cli_prompt(decision_seq, prompt_text, countdown_override);
        self.auto_apply_controller_effects(effects);
    }

    pub(super) fn auto_can_bootstrap_from_history(&self) -> bool {
        self.history_cells.iter().any(|cell| {
            matches!(
                cell.kind(),
                HistoryCellType::User
                    | HistoryCellType::Assistant
                    | HistoryCellType::Plain
                    | HistoryCellType::Exec { .. }
            )
        })
    }

    pub(super) fn auto_apply_controller_effects(&mut self, effects: Vec<AutoControllerEffect>) {
        for effect in effects {
        match effect {
            AutoControllerEffect::RefreshUi => {
                    self.auto_rebuild_live_ring();
                    self.request_redraw();
                }
                AutoControllerEffect::StartCountdown {
                    countdown_id,
                    decision_seq,
                    seconds,
                } => {
                    if seconds == 0 {
                        self.app_event_tx.send(AppEvent::AutoCoordinatorCountdown {
                            countdown_id,
                            seconds_left: 0,
                        });
                    } else {
                        self.auto_spawn_countdown(countdown_id, decision_seq, seconds);
                    }
                }
                AutoControllerEffect::SubmitPrompt => {
                    if self.auto_state.should_bypass_coordinator_next_submit()
                        && self.auto_state.is_paused_manual()
                    {
                        self.auto_state.clear_bypass_coordinator_flag();
                        self.auto_state.set_phase(AutoRunPhase::Active);
                    }
                    if !self.auto_state.should_bypass_coordinator_next_submit() {
                        self.auto_submit_prompt();
                    }
                }
                AutoControllerEffect::LaunchStarted { goal } => {
                    self.bottom_pane.set_task_running(false);
                    self.bottom_pane.update_status_text("Auto Drive".to_string());
                    self.auto_card_start(Some(goal.clone()));
                    self.auto_card_add_action(
                        format!("Auto Drive started: {goal}"),
                        AutoDriveActionKind::Info,
                    );
                    self.auto_card_set_status(AutoDriveStatus::Running);
                }
                AutoControllerEffect::LaunchFailed { goal, error } => {
                    let message = format!(
                        "Coordinator failed to start for goal '{goal}': {error}"
                    );
                    self.auto_card_finalize(
                        Some(message),
                        AutoDriveStatus::Failed,
                        AutoDriveActionKind::Error,
                    );
                    self.auto_request_session_summary();
                }
                AutoControllerEffect::StopCompleted { summary, message } => {
                    if let Some(handle) = self.auto_handle.take() {
                        handle.cancel();
                        let _ = handle.send(AutoCoordinatorCommand::Stop);
                    }
                    let final_message = message.or_else(|| summary.message.clone());
                    if let Some(msg) = final_message.clone() {
                        if !msg.trim().is_empty() {
                            self.auto_card_finalize(
                                Some(msg),
                                AutoDriveStatus::Stopped,
                                AutoDriveActionKind::Info,
                            );
                        } else {
                            self.auto_card_finalize(None, AutoDriveStatus::Stopped, AutoDriveActionKind::Info);
                        }
                    } else {
                        self.auto_card_finalize(None, AutoDriveStatus::Stopped, AutoDriveActionKind::Info);
                    }
                    self.schedule_auto_drive_card_celebration(
                        Duration::from_secs(0),
                        self.auto_state.last_completion_explanation.clone(),
                    );
                    self.auto_turn_review_state = None;
                    if ENABLE_WARP_STRIPES {
                        self.header_wave.set_enabled(false, Instant::now());
                    }
                    self.auto_request_session_summary();
                }
                AutoControllerEffect::TransientPause {
                    attempt,
                    delay,
                    reason,
                } => {
                    let human_delay = format_duration(delay);
                    self.bottom_pane.set_task_running(false);
                    self.bottom_pane
                        .update_status_text("Auto Drive paused".to_string());
                    self.bottom_pane.set_standard_terminal_hint(Some(
                        AUTO_ESC_EXIT_HINT.to_string(),
                    ));
                    let message = format!(
                        "Auto Drive will retry automatically in {human_delay} (attempt {attempt}). Last error: {reason}"
                    );
                    self.auto_card_add_action(message, AutoDriveActionKind::Warning);
                    self.auto_card_set_status(AutoDriveStatus::Paused);
                }
                AutoControllerEffect::ScheduleRestart {
                    token,
                    attempt,
                    delay,
                } => {
                    self.auto_schedule_restart_event(token, attempt, delay);
                }
                AutoControllerEffect::CancelCoordinator => {
                    if let Some(handle) = self.auto_handle.take() {
                        handle.cancel();
                        let _ = handle.send(AutoCoordinatorCommand::Stop);
                    }
                }
                AutoControllerEffect::ResetHistory => {
                    self.auto_history.clear();
                    self.reset_auto_compaction_overlay();
                }
                AutoControllerEffect::UpdateTerminalHint { hint } => {
                    self.bottom_pane.set_standard_terminal_hint(hint);
                }
                AutoControllerEffect::SetTaskRunning { running } => {
                    let has_activity = running
                        || !self.exec.running_commands.is_empty()
                        || !self.tools_state.running_custom_tools.is_empty()
                        || !self.tools_state.web_search_sessions.is_empty()
                        || !self.tools_state.running_wait_tools.is_empty()
                        || !self.tools_state.running_kill_tools.is_empty()
                        || self.stream.is_write_cycle_active()
                        || !self.active_task_ids.is_empty();

                    self.bottom_pane.set_task_running(has_activity);
                    if !has_activity {
                        self.bottom_pane.update_status_text(String::new());
                    }
                }
                AutoControllerEffect::EnsureInputFocus => {
                    self.bottom_pane.ensure_input_focus();
                }
                AutoControllerEffect::ClearCoordinatorView => {
                    self.bottom_pane.clear_auto_coordinator_view(true);
                }
                AutoControllerEffect::ShowGoalEntry => {
                    self.auto_show_goal_entry_panel();
                }
            }
        }
    }

    pub(super) fn auto_spawn_countdown(&self, countdown_id: u64, decision_seq: u64, seconds: u8) {
        let tx = self.app_event_tx.clone();
        let fallback_tx = tx.clone();
        if thread_spawner::spawn_lightweight("countdown", move || {
            let mut remaining = seconds;
            tracing::debug!(
                target: "auto_drive::coordinator",
                countdown_id,
                decision_seq,
                seconds,
                "spawned countdown"
            );
            while remaining > 0 {
                std::thread::sleep(std::time::Duration::from_secs(1));
                remaining -= 1;
                if !tx.send_with_result(AppEvent::AutoCoordinatorCountdown {
                    countdown_id,
                    seconds_left: remaining,
                }) {
                    break;
                }
            }
        })
        .is_none()
        {
            fallback_tx.send(AppEvent::AutoCoordinatorCountdown {
                countdown_id,
                seconds_left: 0,
            });
        }
    }

    pub(crate) fn auto_handle_countdown(&mut self, countdown_id: u64, seconds_left: u8) {
        let decision_seq = self.auto_state.countdown_decision_seq;
        let effects = self
            .auto_state
            .handle_countdown_tick(countdown_id, decision_seq, seconds_left);
        if effects.is_empty() {
            return;
        }
        self.auto_apply_controller_effects(effects);
    }

    pub(crate) fn auto_handle_restart(&mut self, token: u64, attempt: u32) {
        if !self.auto_state.is_active() || !self.auto_state.in_transient_recovery() {
            return;
        }
        let Some(restart) = self.auto_state.pending_restart.clone() else {
            return;
        };
        if restart.token != token || restart.attempt != attempt {
            return;
        }

        let Some(goal) = self.auto_state.goal.clone() else {
            self.auto_card_add_action(
                "Auto Drive restart skipped because the goal is no longer available.".to_string(),
                AutoDriveActionKind::Warning,
            );
            self.auto_state.pending_restart = None;
            self.auto_state.on_recovery_attempt();
            self.auto_stop(Some("Auto Drive restart aborted.".to_string()));
            return;
        };

        let cross_check_enabled = self.auto_state.cross_check_enabled;
        let continue_mode = self.auto_state.continue_mode;
        let previous_turns = self.auto_state.turns_completed;
        let previous_started_at = self.auto_state.started_at;
        let restart_attempts = self.auto_state.transient_restart_attempts;
        let review_enabled = self.auto_state.review_enabled;
        let agents_enabled = self.auto_state.subagents_enabled;
        let qa_automation_enabled = self.auto_state.qa_automation_enabled;

        self.auto_state.pending_restart = None;
        self.auto_state.on_recovery_attempt();
        self.auto_state.restart_token = token;

        let resume_message = if restart.reason.is_empty() {
            format!("Auto Drive resuming automatically (attempt {attempt}).")
        } else {
            format!(
                "Auto Drive resuming automatically (attempt {attempt}); previous error: {}",
                restart.reason
            )
        };
        self.auto_card_add_action(resume_message, AutoDriveActionKind::Info);
        self.auto_card_set_status(AutoDriveStatus::Running);

        self.auto_launch_with_goal(AutoLaunchRequest {
            goal,
            derive_goal_from_history: false,
            review_enabled,
            subagents_enabled: agents_enabled,
            cross_check_enabled,
            qa_automation_enabled,
            continue_mode,
        });

        if previous_turns > 0 {
            self.auto_state.turns_completed = previous_turns;
        }
        if let Some(started_at) = previous_started_at {
            self.auto_state.started_at = Some(started_at);
        }
        self.auto_state.transient_restart_attempts = restart_attempts;
        self.auto_state.current_status_title = None;
        self.auto_state.current_status_sent_to_user = None;
        self.auto_rebuild_live_ring();
        self.auto_update_terminal_hint();
        self.request_redraw();
        self.rebuild_auto_history();
    }

    pub(crate) fn auto_handle_thinking(&mut self, delta: String, summary_index: Option<u32>) {
        if !self.auto_state.is_active() {
            return;
        }
        self.auto_on_reasoning_delta(&delta, summary_index);
    }

    pub(crate) fn auto_handle_action(&mut self, message: String) {
        if !self.auto_state.is_active() {
            return;
        }
        self.auto_card_add_action(message, AutoDriveActionKind::Info);
    }

    #[cfg(any(test, feature = "test-helpers"))]
    #[allow(dead_code)]
    pub(super) fn auto_handle_post_turn_review(
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

    #[cfg(any(test, feature = "test-helpers"))]
    #[allow(dead_code)]
    pub(super) fn auto_prepare_commit_scope(&mut self) -> AutoReviewOutcome {
        let Some(state) = self.auto_turn_review_state.take() else {
            return AutoReviewOutcome::Workspace;
        };

        let Some(base_commit) = state.base_commit else {
            return AutoReviewOutcome::Workspace;
        };

        let final_commit = match self.capture_auto_turn_commit("auto turn change snapshot", Some(&base_commit)) {
            Ok(commit) => commit,
            Err(err) => {
                tracing::warn!("failed to capture auto turn change snapshot: {err}");
                return AutoReviewOutcome::Workspace;
            }
        };

        let diff_paths = match self.git_diff_name_only_between(base_commit.id(), final_commit.id()) {
            Ok(paths) => paths,
            Err(err) => {
                tracing::warn!("failed to diff auto turn snapshots: {err}");
                return AutoReviewOutcome::Workspace;
            }
        };

        if diff_paths.is_empty() {
            self.push_background_tail("Auto review skipped: no file changes detected this turn.".to_string());
            return AutoReviewOutcome::Skip;
        }

        AutoReviewOutcome::Commit(AutoReviewCommitScope {
            commit: final_commit.id().to_string(),
            file_count: diff_paths.len(),
        })
    }

    #[cfg(any(test, feature = "test-helpers"))]
    #[allow(dead_code)]
    pub(super) fn auto_turn_has_diff(&self) -> bool {
        if self.worktree_has_uncommitted_changes().unwrap_or(false) {
            return true;
        }

        if let Some(base_commit) = self
            .auto_turn_review_state
            .as_ref()
            .and_then(|state| state.base_commit.as_ref())
            && let Some(head) = self.current_head_commit_sha()
            && let Ok(paths) = self.git_diff_name_only_between(base_commit.id(), &head)
            && !paths.is_empty()
        {
            return true;
        }

        false
    }

    pub(super) fn prepare_auto_turn_review_state(&mut self) {
        if !self.auto_state.is_active() || !self.auto_state.review_enabled {
            self.auto_turn_review_state = None;
            return;
        }

        let read_only = self
            .pending_auto_turn_config
            .as_ref()
            .map(|cfg| cfg.read_only)
            .unwrap_or(false);

        if read_only {
            self.auto_turn_review_state = None;
            return;
        }

        let existing_base = self
            .auto_turn_review_state
            .as_ref()
            .and_then(|state| state.base_commit.as_ref());

        if existing_base.is_some() {
            return;
        }

        #[cfg(test)]
        {
            if CAPTURE_AUTO_TURN_COMMIT_STUB.lock().unwrap().is_some() {
                return;
            }
        }

        match self.capture_auto_turn_commit("auto turn base snapshot", None) {
            Ok(commit) => {
                self.auto_turn_review_state = Some(AutoTurnReviewState {
                    base_commit: Some(commit),
                });
            }
            Err(err) => {
                tracing::warn!("failed to capture auto turn base snapshot: {err}");
                self.auto_turn_review_state = None;
            }
        }
    }

    pub(super) fn capture_auto_turn_commit(
        &self,
        message: &'static str,
        parent: Option<&GhostCommit>,
    ) -> Result<GhostCommit, GitToolingError> {
        #[cfg(test)]
        if let Some(stub) = CAPTURE_AUTO_TURN_COMMIT_STUB.lock().unwrap().as_ref() {
            let parent_id = parent.map(|commit| commit.id().to_string());
            return stub(message, parent_id);
        }
        let mut options = CreateGhostCommitOptions::new(self.config.cwd.as_path()).message(message);
        if let Some(parent_commit) = parent {
            options = options.parent(parent_commit.id());
        }
        let hook_repo_follow = self.config.cwd.clone();
        let hook = move || bump_snapshot_epoch_for(&hook_repo_follow);
        let result = create_ghost_commit(&options.post_commit_hook(&hook));
        if result.is_ok() {
            bump_snapshot_epoch_for(&self.config.cwd);
        }
        result
    }

    pub(super) fn capture_auto_review_baseline_for_path(
        repo_path: PathBuf,
    ) -> Result<GhostCommit, GitToolingError> {
        #[cfg(test)]
        if let Some(stub) = CAPTURE_AUTO_TURN_COMMIT_STUB.lock().unwrap().as_ref() {
            return stub("auto review baseline snapshot", None);
        }
        let hook_repo = repo_path.clone();
        let options =
            CreateGhostCommitOptions::new(repo_path.as_path()).message("auto review baseline snapshot");
        let hook = move || bump_snapshot_epoch_for(&hook_repo);
        let result = create_ghost_commit(&options.post_commit_hook(&hook));
        if result.is_ok() {
            bump_snapshot_epoch_for(&repo_path);
        }
        result
    }

    pub(super) fn spawn_auto_review_baseline_capture(&mut self) {
        let turn_sequence = self.turn_sequence;
        let repo_path = self.config.cwd.clone();
        let app_event_tx = self.app_event_tx.clone();

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let result = tokio::task::spawn_blocking(move || {
                    ChatWidget::capture_auto_review_baseline_for_path(repo_path)
                })
                .await
                .unwrap_or_else(|err| {
                    Err(GitToolingError::Io(io::Error::other(
                        format!("auto review baseline task failed: {err}"),
                    )))
                });
                app_event_tx.send(AppEvent::AutoReviewBaselineCaptured {
                    turn_sequence,
                    result,
                });
            });
        } else {
            std::thread::spawn(move || {
                let result = ChatWidget::capture_auto_review_baseline_for_path(repo_path);
                app_event_tx.send(AppEvent::AutoReviewBaselineCaptured {
                    turn_sequence,
                    result,
                });
            });
        }
    }

    pub(crate) fn handle_auto_review_baseline_captured(
        &mut self,
        turn_sequence: u64,
        result: Result<GhostCommit, GitToolingError>,
    ) {
        if turn_sequence != self.turn_sequence {
            tracing::debug!(
                "ignored auto review baseline for stale turn_sequence={turn_sequence}"
            );
            return;
        }
        if self.auto_review_baseline.is_some() {
            tracing::debug!("auto review baseline already set; skipping update");
            return;
        }
        match result {
            Ok(commit) => {
                self.auto_review_baseline = Some(commit);
            }
            Err(err) => {
                tracing::warn!("failed to capture auto review baseline: {err}");
            }
        }
    }

    #[cfg(any(test, feature = "test-helpers"))]
    #[allow(dead_code)]
    pub(super) fn git_diff_name_only_between(
        &self,
        base_commit: &str,
        head_commit: &str,
    ) -> Result<Vec<String>, String> {
        #[cfg(test)]
        if let Some(stub) = GIT_DIFF_NAME_ONLY_BETWEEN_STUB.lock().unwrap().as_ref() {
            return stub(base_commit.to_string(), head_commit.to_string());
        }
        self.run_git_command(
            ["diff", "--name-only", base_commit, head_commit],
            |stdout| {
                let changes = stdout
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(std::string::ToString::to_string)
                    .collect();
                Ok(changes)
            },
        )
    }

    pub(super) fn auto_submit_prompt(&mut self) {
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

    pub(super) fn auto_start_bootstrap_from_history(&mut self) -> bool {
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

    pub(super) fn auto_dispatch_cli_prompt(&mut self, full_prompt: String) {
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

    pub(super) fn auto_pause_for_manual_edit(&mut self, force: bool) {
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
    pub(super) fn build_auto_turn_message(&self, prompt_cli: &str) -> Option<String> {
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

    pub(super) fn auto_agents_can_write(&self) -> bool {
        if code_core::git_info::get_git_repo_root(&self.config.cwd).is_none() {
            return false;
        }
        matches!(
            self.config.sandbox_policy,
            SandboxPolicy::DangerFullAccess | SandboxPolicy::WorkspaceWrite { .. }
        )
    }

    pub(super) fn resolve_agent_write_flag(&self, requested_write: Option<bool>) -> bool {
        if !self.auto_agents_can_write() {
            return false;
        }
        if !self.auto_state.subagents_enabled {
            return requested_write.unwrap_or(false);
        }
        true
    }

    pub(super) fn auto_stop(&mut self, message: Option<String>) {
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

    pub(super) fn auto_on_assistant_final(&mut self) {
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

    #[cfg(any(test, feature = "test-helpers"))]
    #[allow(dead_code)]
    pub(super) fn auto_start_post_turn_review(
        &mut self,
        scope: Option<AutoReviewCommitScope>,
        descriptor: Option<&TurnDescriptor>,
    ) {
        if !self.auto_state.review_enabled {
            return;
        }
        let strategy = descriptor.and_then(|d| d.review_strategy.as_ref());
        let (mut prompt, mut hint, mut auto_metadata, mut review_metadata, preparation) = match scope {
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
                let review_metadata = Some(ReviewContextMetadata {
                    scope: Some("commit".to_string()),
                    commit: Some(commit_id),
                    ..Default::default()
                });
                let auto_metadata = Some(ReviewContextMetadata {
                    scope: Some("workspace".to_string()),
                    ..Default::default()
                });
                (prompt, hint, auto_metadata, review_metadata, preparation)
            }
            None => {
                let prompt = "Review the current workspace changes and highlight bugs, regressions, risky patterns, and missing tests before merge.".to_string();
                let hint = "current workspace changes".to_string();
                let review_metadata = Some(ReviewContextMetadata {
                    scope: Some("workspace".to_string()),
                    ..Default::default()
                });
                let preparation = "Preparing code review request...".to_string();
                (
                    prompt,
                    hint,
                    review_metadata.clone(),
                    review_metadata,
                    preparation,
                )
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

                let apply_scope = |meta: &mut ReviewContextMetadata| {
                    meta.scope = Some(scope_hint.to_string());
                };

                match review_metadata.as_mut() {
                    Some(meta) => apply_scope(meta),
                    None => {
                        review_metadata = Some(ReviewContextMetadata {
                            scope: Some(scope_hint.to_string()),
                            ..Default::default()
                        });
                    }
                }

                match auto_metadata.as_mut() {
                    Some(meta) => apply_scope(meta),
                    None => {
                        auto_metadata = Some(ReviewContextMetadata {
                            scope: Some(scope_hint.to_string()),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        if self.config.tui.review_auto_resolve {
            let max_re_reviews = self.configured_auto_resolve_re_reviews();
            self.auto_resolve_state = Some(AutoResolveState::new_with_limit(
                prompt.clone(),
                hint.clone(),
                auto_metadata.clone(),
                max_re_reviews,
            ));
        } else {
            self.auto_resolve_state = None;
        }
        self.begin_review(prompt, hint, Some(preparation), review_metadata);
    }

    pub(super) fn auto_rebuild_live_ring(&mut self) {
        if !self.auto_state.is_active() {
            if self.auto_state.should_show_goal_entry() {
                self.auto_show_goal_entry_panel();
                return;
            }
            if let Some(summary) = self.auto_state.last_run_summary.clone() {
                self.bottom_pane.clear_live_ring();
                self.auto_reset_intro_timing();
                self.auto_ensure_intro_timing();
                let mut status_lines: Vec<String> = Vec::new();
                if let Some(msg) = summary.message.as_ref() {
                    let trimmed = msg.trim();
                    if !trimmed.is_empty() {
                        status_lines.push(trimmed.to_string());
                    }
                }
                if status_lines.is_empty() {
                    if let Some(goal) = summary.goal.as_ref() {
                        status_lines.push(format!("Auto Drive completed: {goal}"));
                    } else {
                        status_lines.push("Auto Drive completed.".to_string());
                    }
                }
                let model = AutoCoordinatorViewModel::Active(AutoActiveViewModel {
                    goal: summary.goal.clone(),
                    status_lines,
                    cli_prompt: None,
                    cli_context: None,
                    show_composer: true,
            awaiting_submission: false,
            waiting_for_response: false,
            coordinator_waiting: false,
            waiting_for_review: false,
                    countdown: None,
                    button: None,
                    manual_hint: None,
                    ctrl_switch_hint: "Esc to exit Auto Drive".to_string(),
                    cli_running: false,
                    turns_completed: summary.turns_completed,
                    started_at: None,
                    elapsed: Some(summary.duration),
                    status_sent_to_user: None,
                    status_title: None,
                    session_tokens: self.auto_session_tokens(),
                    editing_prompt: false,
                    intro_started_at: self.auto_state.intro_started_at,
                    intro_reduced_motion: self.auto_state.intro_reduced_motion,
                });
            self
                .bottom_pane
                .show_auto_coordinator_view(model);
            self.bottom_pane.release_auto_drive_style();
            self.bottom_pane.set_standard_terminal_hint(None);
            return;
        }

        self.bottom_pane.clear_auto_coordinator_view(true);
        self.bottom_pane.clear_live_ring();
        self.bottom_pane.set_standard_terminal_hint(None);
        self.auto_reset_intro_timing();
        return;
    }

    // AutoDrive is active: if intro animation was mid-flight, force reduced motion
    // so a rebuild cannot leave the header half-rendered (issue #431).
    if self.auto_state.intro_started_at.is_some() && !self.auto_state.intro_reduced_motion {
        self.auto_state.intro_reduced_motion = true;
    }

    if self.auto_state.is_paused_manual() {
        self.bottom_pane.clear_auto_coordinator_view(false);
        self.bottom_pane.clear_live_ring();
        self.bottom_pane.set_standard_terminal_hint(None);
        return;
    }

        self.bottom_pane.clear_live_ring();

        let status_text = if self.auto_state.awaiting_review() {
            "waiting for code review...".to_string()
        } else if let Some(line) = self
            .auto_state
            .current_display_line
            .as_ref()
            .filter(|line| !line.trim().is_empty())
        {
            line.clone()
        } else {
            self
                .auto_state
                .placeholder_phrase
                .get_or_insert_with(|| auto_drive_strings::next_auto_drive_phrase().to_string())
                .clone()
        };

        let headline = self.auto_format_status_headline(&status_text);
        let mut status_lines = vec![headline];
        if !self.auto_state.awaiting_review() {
            self.auto_append_status_lines(
                &mut status_lines,
                self.auto_state.current_status_title.as_ref(),
                self.auto_state.current_status_sent_to_user.as_ref(),
            );
            if self.auto_state.is_waiting_for_response() && !self.auto_state.is_coordinator_waiting() {
                let appended = self.auto_append_status_lines(
                    &mut status_lines,
                    self.auto_state.last_decision_status_title.as_ref(),
                    self.auto_state.last_decision_status_sent_to_user.as_ref(),
                );
                if !appended
                    && let Some(summary) = self.auto_state.last_decision_summary.as_ref() {
                        let trimmed = summary.trim();
                        if !trimmed.is_empty() {
                            let collapsed = trimmed
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ");
                            if !collapsed.is_empty() {
                                let current_line = status_lines
                                    .first()
                                    .map(|line| line.trim_end_matches('…').trim())
                                    .unwrap_or("");
                                if collapsed != current_line {
                                    let display = Self::truncate_with_ellipsis(&collapsed, 160);
                                    status_lines.push(display);
                                }
                            }
                        }
                    }
            }
        }
        let cli_running = self.is_cli_running();
        let progress_hint_active = self.auto_state.awaiting_coordinator_submit()
            || (self.auto_state.is_waiting_for_response() && !self.auto_state.is_coordinator_waiting())
            || cli_running;

        // Keep the most recent coordinator status visible across approval and
        // CLI execution. The coordinator clears the current status fields once it
        // starts streaming the next turn, so fall back to the last decision while
        // we are still acting on it.
        let status_title_for_view = if progress_hint_active {
            self.auto_state
                .current_status_title
                .clone()
                .or_else(|| self.auto_state.last_decision_status_title.clone())
        } else {
            None
        };
        let status_sent_to_user_for_view = if progress_hint_active {
            self.auto_state
                .current_status_sent_to_user
                .clone()
                .or_else(|| self.auto_state.last_decision_status_sent_to_user.clone())
        } else {
            None
        };

        let cli_prompt = self
            .auto_state
            .current_cli_prompt
            .clone()
            .filter(|p| !p.trim().is_empty());
        let cli_context = if self.auto_state.hide_cli_context_in_ui {
            None
        } else {
            self.auto_state
                .current_cli_context
                .clone()
                .filter(|value| !value.trim().is_empty())
        };
        let has_cli_prompt = cli_prompt.is_some();

        let bootstrap_pending = self.auto_pending_goal_request;
        let continue_cta_active = self.auto_should_show_continue_cta();

        let countdown_limit = self.auto_state.countdown_seconds();
        let countdown_active = self.auto_state.countdown_active();
        let countdown = if self.auto_state.awaiting_coordinator_submit() {
            match countdown_limit {
                Some(limit) if limit > 0 => Some(CountdownState {
                    remaining: self.auto_state.seconds_remaining.min(limit),
                }),
                _ => None,
            }
        } else {
            None
        };

        let button = if self.auto_state.awaiting_coordinator_submit() {
            let base_label = if bootstrap_pending {
                "Complete Current Task"
            } else if has_cli_prompt {
                "Send prompt"
            } else if continue_cta_active {
                "Continue current task"
            } else {
                "Send prompt"
            };
            let label = if countdown_active {
                format!("{base_label} ({}s)", self.auto_state.seconds_remaining)
            } else {
                base_label.to_string()
            };
            Some(AutoCoordinatorButton {
                label,
                enabled: true,
            })
        } else {
            None
        };

        let manual_hint = if self.auto_state.awaiting_coordinator_submit() {
            if self.auto_state.is_paused_manual() {
                Some("Edit the prompt, then press Enter to continue.".to_string())
            } else if bootstrap_pending {
                None
            } else if has_cli_prompt {
                if countdown_active {
                    Some("Enter to send now • Esc to edit".to_string())
                } else {
                    Some("Enter to send • Esc to edit".to_string())
                }
            } else if continue_cta_active {
                if countdown_active {
                    Some("Enter to continue now • Esc to stop".to_string())
                } else {
                    Some("Enter to continue • Esc to stop".to_string())
                }
            } else if countdown_active {
                Some("Enter to send now • Esc to stop".to_string())
            } else {
                Some("Enter to send • Esc to stop".to_string())
            }
        } else {
            None
        };

        let ctrl_switch_hint = if self.auto_state.awaiting_coordinator_submit() {
            if self.auto_state.is_paused_manual() {
                "Esc to cancel".to_string()
            } else if bootstrap_pending {
                "Esc enter new goal".to_string()
            } else if has_cli_prompt {
                "Esc to edit".to_string()
            } else {
                "Esc to stop".to_string()
            }
        } else {
            String::new()
        };

        let show_composer =
            !self.auto_state.awaiting_coordinator_submit() || self.auto_state.is_paused_manual();

        let model = AutoCoordinatorViewModel::Active(AutoActiveViewModel {
            goal: self.auto_state.goal.clone(),
            status_lines,
            cli_prompt,
            awaiting_submission: self.auto_state.awaiting_coordinator_submit(),
            waiting_for_response: self.auto_state.is_waiting_for_response(),
            coordinator_waiting: self.auto_state.is_coordinator_waiting(),
            waiting_for_review: self.auto_state.awaiting_review(),
            countdown,
            button,
            manual_hint,
            ctrl_switch_hint,
            cli_running,
            turns_completed: self.auto_state.turns_completed,
            started_at: self.auto_state.started_at,
            elapsed: self.auto_state.elapsed_override,
            status_sent_to_user: status_sent_to_user_for_view,
            status_title: status_title_for_view,
            session_tokens: self.auto_session_tokens(),
            cli_context,
            show_composer,
            editing_prompt: self.auto_state.is_paused_manual(),
            intro_started_at: self.auto_state.intro_started_at,
            intro_reduced_motion: self.auto_state.intro_reduced_motion,
        });

        self
            .bottom_pane
            .show_auto_coordinator_view(model);

        self.auto_update_terminal_hint();

        if self.auto_state.started_at.is_some() {
            self.app_event_tx
                .send(AppEvent::ScheduleFrameIn(Duration::from_secs(1)));
        }
    }

    pub(super) fn auto_should_show_continue_cta(&self) -> bool {
        self.auto_state.is_active()
            && self.auto_state.awaiting_coordinator_submit()
            && !self.auto_state.is_paused_manual()
            && self.config.auto_drive.coordinator_routing
            && self.auto_state.continue_mode != AutoContinueMode::Manual
    }

    pub(super) fn auto_format_status_headline(&self, text: &str) -> String {
        let trimmed = text.trim_end();
        if trimmed.is_empty() {
            return String::new();
        }

        if self.auto_state.current_display_is_summary {
            return trimmed.to_string();
        }

        let show_summary_without_ellipsis = self.auto_state.awaiting_coordinator_submit()
            && self.auto_state.current_reasoning_title.is_none()
            && self
                .auto_state
                .current_summary
                .as_ref()
                .map(|summary| !summary.trim().is_empty())
                .unwrap_or(false);

        if show_summary_without_ellipsis {
            trimmed.to_string()
        } else {
            append_thought_ellipsis(trimmed)
        }
    }

    pub(super) fn auto_update_terminal_hint(&mut self) {
        if !self.auto_state.is_active() && !self.auto_state.should_show_goal_entry() {
            self.bottom_pane.set_standard_terminal_hint(None);
            return;
        }

        let agents_label = if self.auto_state.subagents_enabled {
            "Agents Enabled"
        } else {
            "Agents Disabled"
        };
        let diagnostics_enabled = self.auto_state.qa_automation_enabled
            && (self.auto_state.review_enabled || self.auto_state.cross_check_enabled);
        let diagnostics_label = if diagnostics_enabled {
            "Diagnostics Enabled"
        } else {
            "Diagnostics Disabled"
        };

        let left = format!("• {agents_label}  • {diagnostics_label}");

        let hint = left;
        self.bottom_pane
            .set_standard_terminal_hint(Some(hint));
    }

    pub(super) fn auto_update_display_title(&mut self) {
        if !self.auto_state.is_active() {
            return;
        }

        let Some(summary) = self.auto_state.current_summary.as_ref() else {
            return;
        };

        let display = summary.lines().find_map(|line| {
            let trimmed = line.trim();
            (!trimmed.is_empty()).then(|| Self::truncate_with_ellipsis(trimmed, 160))
        });

        let Some(display) = display else {
            return;
        };

        let needs_update = self
            .auto_state
            .current_display_line
            .as_ref()
            .map(|current| current != &display)
            .unwrap_or(true);

        if needs_update {
            self.auto_state.current_display_line = Some(display);
            self.auto_state.current_display_is_summary = true;
            self.auto_state.placeholder_phrase = None;
            self.auto_state.current_reasoning_title = None;
        }
    }

    pub(super) fn auto_broadcast_summary(&mut self, raw: &str) {
        if !self.auto_state.is_active() {
            return;
        }

        let display_text = extract_latest_bold_title(raw).or_else(|| {
            raw.lines().find_map(|line| {
                let trimmed = line.trim();
                (!trimmed.is_empty()).then_some(trimmed.to_string())
            })
        });

        let Some(display_text) = display_text else {
            return;
        };

        if self
            .auto_state
            .last_broadcast_summary
            .as_ref()
            .map(|prev| prev == &display_text)
            .unwrap_or(false)
        {
            return;
        }

        self.auto_state.last_broadcast_summary = Some(display_text);
    }

    pub(super) fn auto_on_reasoning_delta(&mut self, delta: &str, summary_index: Option<u32>) {
        if !self.auto_state.is_active() || delta.trim().is_empty() {
            return;
        }

        let mut needs_refresh = false;

        if let Some(idx) = summary_index
            && self.auto_state.current_summary_index != Some(idx) {
                self.auto_state.current_summary_index = Some(idx);
                self.auto_state.current_summary = Some(String::new());
                self.auto_state.thinking_prefix_stripped = false;
                self.auto_state.current_reasoning_title = None;
                self.auto_state.current_display_line = None;
                self.auto_state.current_display_is_summary = false;
                self.auto_state.placeholder_phrase =
                    Some(auto_drive_strings::next_auto_drive_phrase().to_string());
                needs_refresh = true;
            }

        let cleaned_delta = if !self.auto_state.thinking_prefix_stripped {
            let (without_prefix, stripped) = strip_role_prefix_if_present(delta);
            if stripped {
                self.auto_state.thinking_prefix_stripped = true;
            }
            without_prefix.to_string()
        } else {
            delta.to_string()
        };

        if !self.auto_state.thinking_prefix_stripped && !cleaned_delta.trim().is_empty() {
            self.auto_state.thinking_prefix_stripped = true;
        }

        {
            let entry = self
                .auto_state
                .current_summary
                .get_or_insert_with(String::new);

            if auto_drive_strings::is_auto_drive_phrase(entry) {
                entry.clear();
            }

            entry.push_str(&cleaned_delta);

            let mut display_updated = false;

            if let Some(title) = extract_latest_bold_title(entry) {
                let needs_update = self
                    .auto_state
                    .current_reasoning_title
                    .as_ref()
                    .map(|existing| existing != &title)
                    .unwrap_or(true);
                if needs_update {
                    self.auto_state.current_reasoning_title = Some(title.clone());
                    self.auto_state.current_display_line = Some(title);
                    self.auto_state.current_display_is_summary = false;
                    self.auto_state.placeholder_phrase = None;
                    display_updated = true;
                }
            } else if self.auto_state.current_reasoning_title.is_none() {
                let previous_line = self.auto_state.current_display_line.clone();
                let previous_is_summary = self.auto_state.current_display_is_summary;
                self.auto_update_display_title();
                let updated_line = self.auto_state.current_display_line.clone();
                let updated_is_summary = self.auto_state.current_display_is_summary;
                if updated_is_summary
                    && (updated_line != previous_line || !previous_is_summary)
                {
                    display_updated = true;
                }
            }

            if display_updated {
                needs_refresh = true;
            }
        }

        if needs_refresh {
            self.auto_rebuild_live_ring();
            self.request_redraw();
        }
    }

    pub(super) fn auto_on_reasoning_final(&mut self, text: &str) {
        if !self.auto_state.is_active() {
            return;
        }

        self.auto_state.current_reasoning_title = None;
        self.auto_state.current_summary = Some(text.to_string());
        self.auto_state.thinking_prefix_stripped = true;
        self.auto_state.current_summary_index = None;
        self.auto_update_display_title();
        self.auto_broadcast_summary(text);

        if self.auto_state.is_waiting_for_response() {
            self.auto_rebuild_live_ring();
            self.request_redraw();
        }
    }

    pub(super) fn truncate_with_ellipsis(text: &str, max_chars: usize) -> String {
        if max_chars == 0 {
            return String::new();
        }
        let total = text.chars().count();
        if total <= max_chars {
            return text.to_string();
        }
        let take = max_chars.saturating_sub(1);
        let mut out = String::with_capacity(max_chars);
        for (idx, ch) in text.chars().enumerate() {
            if idx >= take {
                break;
            }
            out.push(ch);
        }
        out.push('…');
        out
    }

    pub(super) fn normalize_status_field(field: Option<String>) -> Option<String> {
        field.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }

    pub(super) fn compose_status_summary(
        status_title: &Option<String>,
        status_sent_to_user: &Option<String>,
    ) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(title) = status_title
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            parts.push(title.to_string());
        }
        if let Some(sent) = status_sent_to_user
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            && !parts.iter().any(|existing| existing.eq_ignore_ascii_case(sent)) {
                parts.push(sent.to_string());
            }

        match parts.len() {
            0 => String::new(),
            1 => parts.into_iter().next().unwrap_or_default(),
            _ => parts.join(" · "),
        }
    }

    pub(super) fn auto_append_status_lines(
        &self,
        lines: &mut Vec<String>,
        status_title: Option<&String>,
        status_sent_to_user: Option<&String>,
    ) -> bool {
        let initial_len = lines.len();
        Self::append_status_line(lines, status_title);
        Self::append_status_line(lines, status_sent_to_user);
        lines.len() > initial_len
    }

    pub(super) fn append_status_line(lines: &mut Vec<String>, status: Option<&String>) {
        if let Some(status) = status {
            let trimmed = status.trim();
            if trimmed.is_empty() {
                return;
            }
            let display = Self::truncate_with_ellipsis(trimmed, 160);
            if !lines.iter().any(|existing| existing.trim() == display) {
                lines.push(display);
            }
        }
    }

}
