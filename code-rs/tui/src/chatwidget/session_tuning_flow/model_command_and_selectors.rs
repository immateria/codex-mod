impl ChatWidget<'_> {
    pub(crate) fn handle_model_command(&mut self, command_args: String) {
        if self.is_task_running() {
            let message = "'/model' is disabled while a task is in progress.".to_string();
            self.history_push_plain_state(history_cell::new_error_event(message));
            return;
        }

        let presets = self.available_model_presets();
        if presets.is_empty() {
            let message =
                "No model presets are available. Update your configuration to define models."
                    .to_string();
            self.history_push_plain_state(history_cell::new_error_event(message));
            return;
        }

        let trimmed = command_args.trim();
        if !trimmed.is_empty() {
            if let Some(preset) = self.find_model_preset(trimmed, &presets) {
                let effort = Self::preset_effort_for_model(&preset);
                self.apply_model_selection(preset.model, Some(effort));
            } else {
                let message = format!(
                    "Unknown model preset: '{trimmed}'. Use /model with no arguments to open the selector."
                );
                self.history_push_plain_state(history_cell::new_error_event(message));
            }
            return;
        }

        // Check if model selector is already open
        if self.bottom_pane.is_view_kind_active(crate::bottom_pane::ActiveViewKind::ModelSelection) {
            return;
        }

        self.bottom_pane.show_model_selection(ModelSelectionViewParams {
            presets,
            current_model: self.config.model.clone(),
            current_effort: self.config.model_reasoning_effort,
            current_service_tier: self.config.service_tier,
            current_context_mode: self.config.context_mode,
            current_context_window: self.config.model_context_window,
            current_auto_compact_token_limit: self.config.model_auto_compact_token_limit,
            use_chat_model: false,
            target: ModelSelectionTarget::Session,
        });
    }

    pub(crate) fn show_review_model_selector(&mut self) {
        let presets = self.available_model_presets();
        if presets.is_empty() {
            self.bottom_pane.flash_footer_notice(
                "No model presets are available for review. Update configuration to define models."
                    .to_string(),
            );
            return;
        }
        if self.settings.overlay.is_some() {
            self.pending_settings_return = Some(SettingsSection::Review);
            self.close_settings_overlay();
        }
        self.bottom_pane.show_model_selection(ModelSelectionViewParams {
            presets,
            current_model: self.config.review_model.clone(),
            current_effort: self.config.review_model_reasoning_effort,
            current_service_tier: self.config.service_tier,
            current_context_mode: None,
            current_context_window: self.config.model_context_window,
            current_auto_compact_token_limit: self.config.model_auto_compact_token_limit,
            use_chat_model: self.config.review_use_chat_model,
            target: ModelSelectionTarget::Review,
        });
    }

    pub(crate) fn show_review_resolve_model_selector(&mut self) {
        let presets = self.available_model_presets();
        if presets.is_empty() {
            self.bottom_pane.flash_footer_notice(
                "No model presets are available for review resolution.".to_string(),
            );
            return;
        }
        if self.settings.overlay.is_some() {
            self.pending_settings_return = Some(SettingsSection::Review);
            self.close_settings_overlay();
        }
        let current = if self.config.review_resolve_use_chat_model {
            self.config.model.clone()
        } else {
            self.config.review_resolve_model.clone()
        };
        let effort = if self.config.review_resolve_use_chat_model {
            self.config.model_reasoning_effort
        } else {
            self.config.review_resolve_model_reasoning_effort
        };
        self.bottom_pane.show_model_selection(ModelSelectionViewParams {
            presets,
            current_model: current,
            current_effort: effort,
            current_service_tier: self.config.service_tier,
            current_context_mode: None,
            current_context_window: self.config.model_context_window,
            current_auto_compact_token_limit: self.config.model_auto_compact_token_limit,
            use_chat_model: self.config.review_resolve_use_chat_model,
            target: ModelSelectionTarget::ReviewResolve,
        });
    }

    pub(crate) fn show_auto_review_model_selector(&mut self) {
        let presets = self.available_model_presets();
        if presets.is_empty() {
            self.bottom_pane.flash_footer_notice(
                "No model presets are available for Auto Review. Update configuration to define models.".to_string(),
            );
            return;
        }
        if self.settings.overlay.is_some() {
            self.pending_settings_return = Some(SettingsSection::Review);
            self.close_settings_overlay();
        }
        let current = if self.config.auto_review_use_chat_model {
            self.config.model.clone()
        } else {
            self.config.auto_review_model.clone()
        };
        let effort = if self.config.auto_review_use_chat_model {
            self.config.model_reasoning_effort
        } else {
            self.config.auto_review_model_reasoning_effort
        };
        self.bottom_pane.show_model_selection(ModelSelectionViewParams {
            presets,
            current_model: current,
            current_effort: effort,
            current_service_tier: self.config.service_tier,
            current_context_mode: None,
            current_context_window: self.config.model_context_window,
            current_auto_compact_token_limit: self.config.model_auto_compact_token_limit,
            use_chat_model: self.config.auto_review_use_chat_model,
            target: ModelSelectionTarget::AutoReview,
        });
    }

    pub(crate) fn show_auto_review_resolve_model_selector(&mut self) {
        let presets = self.available_model_presets();
        if presets.is_empty() {
            self.bottom_pane.flash_footer_notice(
                "No model presets are available for Auto Review resolution.".to_string(),
            );
            return;
        }
        if self.settings.overlay.is_some() {
            self.pending_settings_return = Some(SettingsSection::Review);
            self.close_settings_overlay();
        }
        let current = if self.config.auto_review_resolve_use_chat_model {
            self.config.model.clone()
        } else {
            self.config.auto_review_resolve_model.clone()
        };
        let effort = if self.config.auto_review_resolve_use_chat_model {
            self.config.model_reasoning_effort
        } else {
            self.config.auto_review_resolve_model_reasoning_effort
        };
        self.bottom_pane.show_model_selection(ModelSelectionViewParams {
            presets,
            current_model: current,
            current_effort: effort,
            current_service_tier: self.config.service_tier,
            current_context_mode: None,
            current_context_window: self.config.model_context_window,
            current_auto_compact_token_limit: self.config.model_auto_compact_token_limit,
            use_chat_model: self.config.auto_review_resolve_use_chat_model,
            target: ModelSelectionTarget::AutoReviewResolve,
        });
    }

    pub(crate) fn show_planning_model_selector(&mut self) {
        let presets = self.available_model_presets();
        if presets.is_empty() {
            self.bottom_pane.flash_footer_notice(
                "No model presets are available for planning. Update configuration to define models."
                    .to_string(),
            );
            return;
        }
        if self.settings.overlay.is_some() {
            self.pending_settings_return = Some(SettingsSection::Planning);
            self.close_settings_overlay();
        }
        let current = if self.config.planning_use_chat_model {
            self.config.model.clone()
        } else {
            self.config.planning_model.clone()
        };
        let effort = self.config.planning_model_reasoning_effort;
        self.bottom_pane.show_model_selection(ModelSelectionViewParams {
            presets,
            current_model: current,
            current_effort: effort,
            current_service_tier: self.config.service_tier,
            current_context_mode: None,
            current_context_window: self.config.model_context_window,
            current_auto_compact_token_limit: self.config.model_auto_compact_token_limit,
            use_chat_model: self.config.planning_use_chat_model,
            target: ModelSelectionTarget::Planning,
        });
    }

    pub(crate) fn show_auto_drive_model_selector(&mut self) {
        let presets = self.available_model_presets();
        if presets.is_empty() {
            self.bottom_pane.flash_footer_notice(
                "No model presets are available for Auto Drive. Update configuration to define models."
                    .to_string(),
            );
            return;
        }
        if self.settings.overlay.is_some() {
            self.pending_settings_return = Some(SettingsSection::AutoDrive);
            self.close_settings_overlay();
        }
        self.bottom_pane.show_model_selection(ModelSelectionViewParams {
            presets,
            current_model: self.config.auto_drive.model.clone(),
            current_effort: self.config.auto_drive.model_reasoning_effort,
            current_service_tier: self.config.service_tier,
            current_context_mode: None,
            current_context_window: self.config.model_context_window,
            current_auto_compact_token_limit: self.config.model_auto_compact_token_limit,
            use_chat_model: self.config.auto_drive_use_chat_model,
            target: ModelSelectionTarget::AutoDrive,
        });
    }

    pub(crate) fn apply_model_selection(&mut self, model: String, effort: Option<ReasoningEffort>) {
        self.apply_model_selection_inner(model, effort, true, true);
    }

    pub(crate) fn apply_service_tier_selection(
        &mut self,
        service_tier: Option<code_core::config_types::ServiceTier>,
    ) {
        if self.config.service_tier == service_tier {
            return;
        }

        self.config.service_tier = service_tier;
        self.submit_op(self.current_configure_session_op());
        let status = if matches!(
            self.config.service_tier,
            Some(code_core::config_types::ServiceTier::Fast)
        ) {
            "enabled"
        } else {
            "disabled"
        };
        self.bottom_pane
            .flash_footer_notice(format!("Fast mode {status}."));
        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    pub(crate) fn apply_session_context_mode_selection(
        &mut self,
        context_mode: Option<code_core::config_types::ContextMode>,
    ) {
        let context_mode = context_mode.or(Some(code_core::config_types::ContextMode::Disabled));
        self.apply_session_context_settings(context_mode, None, None);
    }

    pub(crate) fn apply_shell_selection(
        &mut self,
        path: String,
        args: Vec<String>,
        script_style: Option<String>,
    ) {
        let path_trimmed = path.trim();
        if path_trimmed == "-" || path_trimmed.eq_ignore_ascii_case("auto") {
            self.update_shell_config(None);
            return;
        }

        let parsed_style = script_style
            .as_deref()
            .and_then(ShellScriptStyle::parse)
            .or_else(|| ShellScriptStyle::infer_from_shell_program(&path));
        let shell_config =
            Self::build_shell_config(path, args, parsed_style, self.config.shell.as_ref());
        self.update_shell_config(Some(shell_config));
    }

    pub(crate) fn on_shell_selection_closed(&mut self, confirmed: bool) {
        if !confirmed {
            self.history_push_plain_paragraphs(
                crate::history::state::PlainMessageKind::Notice,
                vec!["Shell selection cancelled.".to_string()],
            );
        }
    }

    pub(crate) fn show_shell_selector(&mut self) {
        // Check if shell selector is already open
        if self.bottom_pane.is_view_kind_active(crate::bottom_pane::ActiveViewKind::ShellSelection) {
            return;
        }
        self.bottom_pane
            .show_shell_selection(self.config.shell.clone(), self.available_shell_presets());
    }

}
