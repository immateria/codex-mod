impl ChatWidget<'_> {
    pub(super) fn available_model_presets(&self) -> Vec<ModelPreset> {
        if let Some(presets) = self.remote_model_presets.as_ref() {
            return presets.clone();
        }
        let auth_mode = self
            .auth_manager
            .auth()
            .map(|auth| auth.mode)
            .or({
                if self.config.using_chatgpt_auth {
                    Some(AuthMode::ChatGPT)
                } else {
                    Some(AuthMode::ApiKey)
                }
            });
        let supports_pro_only_models = self.auth_manager.supports_pro_only_models();
        builtin_model_presets(auth_mode, supports_pro_only_models)
    }

    pub(crate) fn update_model_presets(
        &mut self,
        presets: Vec<ModelPreset>,
        default_model: Option<String>,
    ) {
        if presets.is_empty() {
            return;
        }

        self.remote_model_presets = Some(presets.clone());
        self.bottom_pane.update_model_selection_presets(presets);

        if let Some(default_model) = default_model {
            self.maybe_apply_remote_default_model(default_model);
        }

        self.request_redraw();
    }

    fn maybe_apply_remote_default_model(&mut self, default_model: String) {
        if !self.allow_remote_default_at_startup {
            return;
        }
        if self.chat_model_selected_explicitly {
            return;
        }
        if self.config.model_explicit {
            return;
        }
        if self.config.model.eq_ignore_ascii_case(&default_model) {
            return;
        }

        self.apply_model_selection_inner(default_model, None, false, false);
    }

    fn preset_effort_for_model(preset: &ModelPreset) -> ReasoningEffort {
        preset.default_reasoning_effort.into()
    }

    fn clamp_reasoning_for_model(model: &str, requested: ReasoningEffort) -> ReasoningEffort {
        let protocol_effort: code_protocol::config_types::ReasoningEffort = requested.into();
        let clamped = clamp_reasoning_effort_for_model(model, protocol_effort);
        ReasoningEffort::from(clamped)
    }

    fn find_model_preset(&self, input: &str, presets: &[ModelPreset]) -> Option<ModelPreset> {
        if presets.is_empty() {
            return None;
        }

        let input_lower = input.to_ascii_lowercase();
        let collapsed_input: String = input_lower
            .chars()
            .filter(|c| !c.is_ascii_whitespace() && *c != '-')
            .collect();

        let mut fallback_medium: Option<ModelPreset> = None;
        let mut fallback_first: Option<ModelPreset> = None;

        for preset in presets.iter() {
            let preset_effort = Self::preset_effort_for_model(preset);

            let id_lower = preset.id.to_ascii_lowercase();
            if Self::candidate_matches(&input_lower, &collapsed_input, &id_lower) {
                return Some(preset.clone());
            }

            let display_name_lower = preset.display_name.to_ascii_lowercase();
            if Self::candidate_matches(&input_lower, &collapsed_input, &display_name_lower) {
                return Some(preset.clone());
            }

            let effort_lower = preset_effort.to_string().to_ascii_lowercase();
            let model_lower = preset.model.to_ascii_lowercase();
            let spaced = format!("{model_lower} {effort_lower}");
            if Self::candidate_matches(&input_lower, &collapsed_input, &spaced) {
                return Some(preset.clone());
            }
            let dashed = format!("{model_lower}-{effort_lower}");
            if Self::candidate_matches(&input_lower, &collapsed_input, &dashed) {
                return Some(preset.clone());
            }

            if model_lower == input_lower
                || Self::candidate_matches(&input_lower, &collapsed_input, &model_lower)
            {
                if fallback_medium.is_none() && preset_effort == ReasoningEffort::Medium {
                    fallback_medium = Some(preset.clone());
                }
                if fallback_first.is_none() {
                    fallback_first = Some(preset.clone());
                }
            }
        }

        fallback_medium.or(fallback_first)
    }

    fn candidate_matches(input: &str, collapsed_input: &str, candidate: &str) -> bool {
        let candidate_lower = candidate.to_ascii_lowercase();
        if candidate_lower == input {
            return true;
        }
        let candidate_collapsed: String = candidate_lower
            .chars()
            .filter(|c| !c.is_ascii_whitespace() && *c != '-')
            .collect();
        candidate_collapsed == collapsed_input
    }

    fn collaboration_mode_display_name(mode: CollaborationModeKind) -> &'static str {
        match mode {
            CollaborationModeKind::Default => "default",
            CollaborationModeKind::Plan => "plan",
        }
    }

    fn parse_collaboration_mode(value: &str) -> Option<CollaborationModeKind> {
        match value.trim().to_ascii_lowercase().as_str() {
            "default" | "normal" => Some(CollaborationModeKind::Default),
            "plan" | "planning" => Some(CollaborationModeKind::Plan),
            _ => None,
        }
    }

    pub(crate) fn handle_mode_command(&mut self, command_args: String) {
        if self.is_task_running() {
            let message = "'/mode' is disabled while a task is in progress.".to_string();
            self.history_push_plain_state(history_cell::new_error_event(message));
            return;
        }

        let trimmed = command_args.trim();
        if trimmed.is_empty() {
            let mode = Self::collaboration_mode_display_name(self.current_collaboration_mode());
            self.push_background_tail(format!(
                "Collaboration mode: {mode} (use /mode <default|plan>)"
            ));
            return;
        }

        let Some(mode) = Self::parse_collaboration_mode(trimmed) else {
            let message = format!(
                "Invalid mode: '{trimmed}'. Use /mode <default|plan>."
            );
            self.history_push_plain_state(history_cell::new_error_event(message));
            return;
        };

        self.set_collaboration_mode(mode, true);
    }

    pub(crate) fn set_collaboration_mode(
        &mut self,
        mode: CollaborationModeKind,
        announce: bool,
    ) {
        let previous = self.collaboration_mode;
        if previous == mode {
            if announce {
                let label = Self::collaboration_mode_display_name(mode);
                self.bottom_pane
                    .flash_footer_notice(format!("Collaboration mode already set to {label}."));
            }
            return;
        }

        self.collaboration_mode = mode;
        if matches!(mode, CollaborationModeKind::Plan) {
            self.apply_planning_session_model();
        } else if matches!(previous, CollaborationModeKind::Plan) {
            self.restore_planning_session_model();
        }

        let op = self.current_configure_session_op();
        self.submit_op(op);
        self.refresh_settings_overview_rows();

        if announce {
            let label = Self::collaboration_mode_display_name(mode);
            self.push_background_tail(format!("Collaboration mode set to {label}."));
        }
        self.request_redraw();
    }

}
