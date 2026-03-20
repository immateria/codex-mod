impl ChatWidget<'_> {
    pub(crate) fn handle_model_selection_closed(&mut self, target: ModelSelectionKind, _accepted: bool) {
        let expected_section = match target {
            ModelSelectionKind::Session => SettingsSection::Model,
            ModelSelectionKind::Review => SettingsSection::Review,
            ModelSelectionKind::Planning => SettingsSection::Planning,
            ModelSelectionKind::AutoDrive => SettingsSection::AutoDrive,
            ModelSelectionKind::ReviewResolve => SettingsSection::Review,
            ModelSelectionKind::AutoReview => SettingsSection::Review,
            ModelSelectionKind::AutoReviewResolve => SettingsSection::Review,
        };

        if let Some(section) = self.pending_settings_return {
            if section == expected_section {
                self.ensure_settings_overlay_section(section);
            }
            self.pending_settings_return = None;
        }

        self.request_redraw();
    }

    pub(crate) fn apply_planning_model_selection(
        &mut self,
        model: String,
        effort: ReasoningEffort,
    ) {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return;
        }

        self.config.planning_use_chat_model = false;

        let clamped_effort = Self::clamp_reasoning_for_model(trimmed, effort);

        let mut updated = false;
        if !self.config.planning_model.eq_ignore_ascii_case(trimmed) {
            self.config.planning_model = trimmed.to_string();
            updated = true;
        }
        if self.config.planning_model_reasoning_effort != clamped_effort {
            self.config.planning_model_reasoning_effort = clamped_effort;
            updated = true;
        }

        if !updated {
            self.bottom_pane
                .flash_footer_notice("Planning model unchanged.".to_string());
            return;
        }

        if let Ok(home) = code_core::config::find_code_home()
            && let Err(err) = code_core::config::set_planning_model(
                &home,
                &self.config.planning_model,
                self.config.planning_model_reasoning_effort,
                false,
            ) {
                tracing::warn!("Failed to persist planning model: {err}");
            }

        self.bottom_pane.flash_footer_notice(format!(
            "Planning model set to {} ({} reasoning)",
            self.config.planning_model,
            Self::format_reasoning_effort(self.config.planning_model_reasoning_effort)
        ));
        self.refresh_settings_overview_rows();
        self.update_planning_settings_model_row();
        // If we're currently in plan mode, switch the session model immediately.
        if self.current_collaboration_mode() == CollaborationModeKind::Plan {
            self.apply_planning_session_model();
        }
        self.request_redraw();
    }

    pub(super) fn apply_planning_session_model(&mut self) {
        if self.config.planning_use_chat_model {
            self.restore_planning_session_model();
            return;
        }

        // If we're already on the planning model, do nothing.
        if self.config.model.eq_ignore_ascii_case(&self.config.planning_model)
            && self.config.model_reasoning_effort == self.config.planning_model_reasoning_effort
        {
            return;
        }

        // Save current chat model to restore later.
        self.planning_restore = Some((
            self.config.model.clone(),
            self.config.model_reasoning_effort,
        ));

        self.config.model = self.config.planning_model.clone();
        self.config.model_reasoning_effort = self.config.planning_model_reasoning_effort;

        let op = self.current_configure_session_op();
        self.submit_op(op);
    }

    pub(super) fn restore_planning_session_model(&mut self) {
        if let Some((model, effort)) = self.planning_restore.take() {
            self.config.model = model;
            self.config.model_reasoning_effort = effort;

            let op = self.current_configure_session_op();
            self.submit_op(op);
        }
    }

    pub(crate) fn set_planning_use_chat_model(&mut self, use_chat: bool) {
        if self.config.planning_use_chat_model == use_chat {
            return;
        }
        self.config.planning_use_chat_model = use_chat;

        if let Ok(home) = code_core::config::find_code_home()
            && let Err(err) = code_core::config::set_planning_model(
                &home,
                &self.config.planning_model,
                self.config.planning_model_reasoning_effort,
                use_chat,
            ) {
                tracing::warn!("Failed to persist planning use-chat toggle: {err}");
            }

        if use_chat {
            self.bottom_pane
                .flash_footer_notice("Planning model now follows Chat model".to_string());
        } else {
            self.bottom_pane.flash_footer_notice(format!(
                "Planning model set to {} ({} reasoning)",
                self.config.planning_model,
                Self::format_reasoning_effort(self.config.planning_model_reasoning_effort)
            ));
        }

        self.update_planning_settings_model_row();
        self.refresh_settings_overview_rows();

        if matches!(self.config.sandbox_policy, code_core::protocol::SandboxPolicy::ReadOnly) {
            self.apply_planning_session_model();
        }
        self.request_redraw();
    }

    pub(crate) fn apply_auto_drive_model_selection(
        &mut self,
        model: String,
        effort: ReasoningEffort,
    ) {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return;
        }

        self.config.auto_drive_use_chat_model = false;

        let clamped_effort = Self::clamp_reasoning_for_model(trimmed, effort);

        let mut updated = false;
        if !self.config.auto_drive.model.eq_ignore_ascii_case(trimmed) {
            self.config.auto_drive.model = trimmed.to_string();
            updated = true;
        }

        if self.config.auto_drive.model_reasoning_effort != clamped_effort {
            self.config.auto_drive.model_reasoning_effort = clamped_effort;
            updated = true;
        }

        if !updated {
            self.bottom_pane
                .flash_footer_notice("Auto Drive model unchanged.".to_string());
            return;
        }

        let message = if let Ok(home) = code_core::config::find_code_home() {
            match code_core::config::set_auto_drive_settings(
                &home,
                &self.config.auto_drive,
                self.config.auto_drive_use_chat_model,
            ) {
                Ok(_) => format!(
                    "Auto Drive model set to {} ({} reasoning)",
                    self.config.auto_drive.model,
                    Self::format_reasoning_effort(self.config.auto_drive.model_reasoning_effort)
                ),
                Err(err) => {
                    tracing::warn!("Failed to persist Auto Drive model: {err}");
                    format!(
                        "Auto Drive model set for this session (failed to persist): {}",
                        self.config.auto_drive.model
                    )
                }
            }
        } else {
            tracing::warn!("Could not locate Code home to persist Auto Drive model");
            format!(
                "Auto Drive model set for this session: {}",
                self.config.auto_drive.model
            )
        };

        self.bottom_pane.flash_footer_notice(message);
        self.refresh_settings_overview_rows();
        self.update_auto_drive_settings_model_row();
        self.request_redraw();
    }

    pub(crate) fn handle_reasoning_command(&mut self, command_args: String) {
        // command_args contains only the arguments after the command (e.g., "high" not "/reasoning high")
        let trimmed = command_args.trim();

        if !trimmed.is_empty() {
            // User specified a level: e.g., "high"
            let new_effort = match trimmed.to_lowercase().as_str() {
                "minimal" | "min" => ReasoningEffort::Minimal,
                "low" => ReasoningEffort::Low,
                "medium" | "med" => ReasoningEffort::Medium,
                "xhigh" | "extra-high" | "extra_high" => ReasoningEffort::XHigh,
                "high" => ReasoningEffort::High,
                // Backwards compatibility: map legacy values to minimal.
                "none" | "off" => ReasoningEffort::Minimal,
                _ => {
                    // Invalid parameter, show error and return
                    let message = format!(
                        "Invalid reasoning level: '{trimmed}'. Use: minimal, low, medium, or high"
                    );
                    self.history_push_plain_state(history_cell::new_error_event(message));
                    return;
                }
            };
            self.set_reasoning_effort(new_effort);
        } else {
            let presets = self.available_model_presets();
            if presets.is_empty() {
                let message =
                    "No model presets are available. Update your configuration to define models."
                        .to_string();
                self.history_push_plain_state(history_cell::new_error_event(message));
                return;
            }

            self.bottom_pane.show_model_selection(ModelSelectionViewParams {
                presets,
                current_model: self.config.model.clone(),
                current_effort: self.config.model_reasoning_effort,
                current_service_tier: self.config.service_tier,
                current_context_mode: self.config.context_mode,
                use_chat_model: false,
                target: ModelSelectionTarget::Session,
            });
        }
    }

    pub(crate) fn handle_verbosity_command(&mut self, command_args: String) {
        // Verbosity is not supported with ChatGPT auth
        if self.config.using_chatgpt_auth {
            let message =
                "Text verbosity is not available when using Sign in with ChatGPT".to_string();
            self.history_push_plain_state(history_cell::new_error_event(message));
            return;
        }

        // command_args contains only the arguments after the command (e.g., "high" not "/verbosity high")
        let trimmed = command_args.trim();

        if !trimmed.is_empty() {
            // User specified a level: e.g., "high"
            let new_verbosity = match trimmed.to_lowercase().as_str() {
                "low" => TextVerbosity::Low,
                "medium" | "med" => TextVerbosity::Medium,
                "high" => TextVerbosity::High,
                _ => {
                    // Invalid parameter, show error and return
                    let message = format!(
                        "Invalid verbosity level: '{trimmed}'. Use: low, medium, or high"
                    );
                    self.history_push_plain_state(history_cell::new_error_event(message));
                    return;
                }
            };

            // Update the configuration
            self.config.model_text_verbosity = new_verbosity;

            // Display success message
            let message = format!("Text verbosity set to: {new_verbosity}");
            self.push_background_tail(message);

            // Send the update to the backend
            let op = self.current_configure_session_op();
            let _ = self.code_op_tx.send(op);
        } else {
            // No parameter specified, show interactive UI
            self.bottom_pane
                .show_verbosity_selection(self.config.model_text_verbosity);
        }
    }

    pub(crate) fn prepare_agents(&mut self) {
        // Set the flag to show agents are ready to start
        self.agents_ready_to_start = true;
        self.agents_terminal.reset();
        if self.agents_terminal.active {
            // Reset scroll offset when a new batch starts to avoid stale positions
            self.layout.scroll_offset.set(0);
        }

        self.request_redraw();
    }

    pub(crate) fn set_reasoning_effort(&mut self, new_effort: ReasoningEffort) {
        let clamped_effort = Self::clamp_reasoning_for_model(&self.config.model, new_effort);

        if clamped_effort != new_effort {
            let requested = Self::format_reasoning_effort(new_effort);
            let applied = Self::format_reasoning_effort(clamped_effort);
            self.bottom_pane.flash_footer_notice(format!(
                "{} does not support {} reasoning; using {} instead.",
                self.config.model, requested, applied
            ));
        }

        // Update the config
        self.config.preferred_model_reasoning_effort = Some(new_effort);
        self.config.model_reasoning_effort = clamped_effort;

        // Send ConfigureSession op to update the backend
        let op = self.current_configure_session_op();

        self.submit_op(op);

        // Add status message to history (replaceable system notice)
        let placement = self.ui_placement_for_now();
        let state = history_cell::new_reasoning_output(self.config.model_reasoning_effort);
        let cell = crate::history_cell::PlainHistoryCell::from_state(state.clone());
        self.push_system_cell(
            Box::new(cell),
            placement,
            Some("ui:reasoning".to_string()),
            None,
            "system",
            Some(HistoryDomainRecord::Plain(state)),
        );
        self.refresh_settings_overview_rows();
    }

    pub(crate) fn set_text_verbosity(&mut self, new_verbosity: TextVerbosity) {
        // Update the config
        self.config.model_text_verbosity = new_verbosity;

        // Send ConfigureSession op to update the backend
        let op = self.current_configure_session_op();

        self.submit_op(op);

        // Add status message to history
        let message = format!("Text verbosity set to: {new_verbosity}");
        self.push_background_tail(message);
    }

}
