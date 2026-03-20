impl ChatWidget<'_> {
    fn clamp_reasoning_for_model_from_presets(
        model: &str,
        requested: ReasoningEffort,
        presets: &[ModelPreset],
    ) -> ReasoningEffort {
        fn rank(effort: ReasoningEffort) -> u8 {
            match effort {
                ReasoningEffort::Minimal => 0,
                ReasoningEffort::Low => 1,
                ReasoningEffort::Medium => 2,
                ReasoningEffort::High => 3,
                ReasoningEffort::XHigh => 4,
                ReasoningEffort::None => 5,
            }
        }

        let model_lower = model.to_ascii_lowercase();
        let Some(preset) = presets.iter().find(|preset| {
            preset.model.eq_ignore_ascii_case(&model_lower)
                || preset.id.eq_ignore_ascii_case(&model_lower)
                || preset.display_name.eq_ignore_ascii_case(&model_lower)
        }) else {
            return Self::clamp_reasoning_for_model(model, requested);
        };

        let supported: Vec<ReasoningEffort> = preset
            .supported_reasoning_efforts
            .iter()
            .map(|opt| ReasoningEffort::from(opt.effort))
            .collect();
        if supported.contains(&requested) {
            return requested;
        }

        let requested_rank = rank(requested);
        supported
            .into_iter()
            .min_by_key(|effort| {
                let effort_rank = rank(*effort);
                (requested_rank.abs_diff(effort_rank), u8::MAX - effort_rank)
            })
            .unwrap_or(requested)
    }

    fn apply_model_selection_inner(
        &mut self,
        model: String,
        effort: Option<ReasoningEffort>,
        mark_explicit: bool,
        announce: bool,
    ) {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return;
        }

        if mark_explicit {
            self.chat_model_selected_explicitly = true;
            self.config.model_explicit = true;
        }

        let mut updated = false;
        if !self.config.model.eq_ignore_ascii_case(trimmed) {
            self.config.model = trimmed.to_string();
            let family = find_family_for_model(&self.config.model)
                .unwrap_or_else(|| derive_default_model_family(&self.config.model));
            self.config.model_family = family;
            updated = true;
        }

        if let Some(explicit) = effort
            && self.config.preferred_model_reasoning_effort != Some(explicit) {
                self.config.preferred_model_reasoning_effort = Some(explicit);
                updated = true;
            }

        let requested_effort = effort
            .or(self.config.preferred_model_reasoning_effort)
            .unwrap_or(self.config.model_reasoning_effort);
        let presets = self.available_model_presets();
        let clamped_effort = Self::clamp_reasoning_for_model_from_presets(trimmed, requested_effort, &presets);

        if self.config.model_reasoning_effort != clamped_effort {
            self.config.model_reasoning_effort = clamped_effort;
            updated = true;
        }

        if updated {
            let op = self.current_configure_session_op();
            self.submit_op(op);

            self.sync_follow_chat_models();
            self.refresh_settings_overview_rows();
        }

        if announce {
            let placement = self.ui_placement_for_now();
            let state = history_cell::new_model_output(&self.config.model, self.config.model_reasoning_effort);
            let cell = crate::history_cell::PlainHistoryCell::from_state(state.clone());
            self.push_system_cell(
                Box::new(cell),
                placement,
                Some("ui:model".to_string()),
                None,
                "system",
                Some(HistoryDomainRecord::Plain(state)),
            );
        }

        self.request_redraw();
    }

    fn sync_follow_chat_models(&mut self) {
        if self.config.review_use_chat_model {
            self.config.review_model = self.config.model.clone();
            self.config.review_model_reasoning_effort = self.config.model_reasoning_effort;
            self.update_review_settings_model_row();
        }

        if self.config.review_resolve_use_chat_model {
            self.config.review_resolve_model = self.config.model.clone();
            self.config.review_resolve_model_reasoning_effort = self.config.model_reasoning_effort;
            self.update_review_settings_model_row();
        }

        if self.config.planning_use_chat_model {
            self.config.planning_model = self.config.model.clone();
            self.config.planning_model_reasoning_effort = self.config.model_reasoning_effort;
            self.update_planning_settings_model_row();
        }

        if self.config.auto_drive_use_chat_model {
            self.config.auto_drive.model = self.config.model.clone();
            self.config.auto_drive.model_reasoning_effort = self.config.model_reasoning_effort;
            self.update_auto_drive_settings_model_row();
        }

        if self.config.auto_review_use_chat_model {
            self.config.auto_review_model = self.config.model.clone();
            self.config.auto_review_model_reasoning_effort = self.config.model_reasoning_effort;
            self.update_review_settings_model_row();
        }

        if self.config.auto_review_resolve_use_chat_model {
            self.config.auto_review_resolve_model = self.config.model.clone();
            self.config.auto_review_resolve_model_reasoning_effort = self.config.model_reasoning_effort;
            self.update_review_settings_model_row();
        }
    }

    pub(crate) fn apply_review_model_selection(
        &mut self,
        model: String,
        effort: ReasoningEffort,
    ) {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return;
        }

        self.config.review_use_chat_model = false;

        let clamped_effort = Self::clamp_reasoning_for_model(trimmed, effort);

        let mut updated = false;
        if !self.config.review_model.eq_ignore_ascii_case(trimmed) {
            self.config.review_model = trimmed.to_string();
            updated = true;
        }

        if self.config.review_model_reasoning_effort != clamped_effort {
            self.config.review_model_reasoning_effort = clamped_effort;
            updated = true;
        }

        if !updated {
            self.bottom_pane
                .flash_footer_notice("Review model unchanged.".to_string());
            return;
        }

        let message = if let Ok(home) = code_core::config::find_code_home() {
            match code_core::config::set_review_model(
                &home,
                &self.config.review_model,
                self.config.review_model_reasoning_effort,
                self.config.review_use_chat_model,
            ) {
                Ok(_) => format!(
                    "Review model set to {} ({} reasoning)",
                    self.config.review_model,
                    Self::format_reasoning_effort(self.config.review_model_reasoning_effort)
                ),
                Err(err) => {
                    tracing::warn!("Failed to persist review model: {err}");
                    format!(
                        "Review model set for this session (failed to persist): {}",
                        self.config.review_model
                    )
                }
            }
        } else {
            tracing::warn!("Could not locate Code home to persist review model");
            format!(
                "Review model set for this session: {}",
                self.config.review_model
            )
        };

        self.bottom_pane.flash_footer_notice(message);
        self.refresh_settings_overview_rows();
        self.update_review_settings_model_row();
        self.request_redraw();
    }

    pub(crate) fn apply_review_resolve_model_selection(
        &mut self,
        model: String,
        effort: ReasoningEffort,
    ) {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return;
        }

        self.config.review_resolve_use_chat_model = false;

        let clamped_effort = Self::clamp_reasoning_for_model(trimmed, effort);

        let mut updated = false;
        if !self
            .config
            .review_resolve_model
            .eq_ignore_ascii_case(trimmed)
        {
            self.config.review_resolve_model = trimmed.to_string();
            updated = true;
        }

        if self.config.review_resolve_model_reasoning_effort != clamped_effort {
            self.config.review_resolve_model_reasoning_effort = clamped_effort;
            updated = true;
        }

        if !updated {
            self.bottom_pane
                .flash_footer_notice("Resolve model unchanged.".to_string());
            return;
        }

        let message = if let Ok(home) = code_core::config::find_code_home() {
            match code_core::config::set_review_resolve_model(
                &home,
                &self.config.review_resolve_model,
                self.config.review_resolve_model_reasoning_effort,
                self.config.review_resolve_use_chat_model,
            ) {
                Ok(_) => format!(
                    "Resolve model set to {} ({} reasoning)",
                    self.config.review_resolve_model,
                    Self::format_reasoning_effort(self.config.review_resolve_model_reasoning_effort)
                ),
                Err(err) => {
                    tracing::warn!("Failed to persist resolve model: {err}");
                    format!(
                        "Resolve model set for this session (failed to persist): {}",
                        self.config.review_resolve_model
                    )
                }
            }
        } else {
            tracing::warn!("Could not locate Code home to persist resolve model");
            format!(
                "Resolve model set for this session: {}",
                self.config.review_resolve_model
            )
        };

        self.bottom_pane.flash_footer_notice(message);
        self.refresh_settings_overview_rows();
        self.update_review_settings_model_row();
        self.request_redraw();
    }

    pub(crate) fn set_review_use_chat_model(&mut self, use_chat: bool) {
        if self.config.review_use_chat_model == use_chat {
            return;
        }
        self.config.review_use_chat_model = use_chat;
        if use_chat {
            self.config.review_model = self.config.model.clone();
            self.config.review_model_reasoning_effort = self.config.model_reasoning_effort;
        }

        if let Ok(home) = code_core::config::find_code_home()
            && let Err(err) = code_core::config::set_review_model(
                &home,
                &self.config.review_model,
                self.config.review_model_reasoning_effort,
                use_chat,
            ) {
                tracing::warn!("Failed to persist review use-chat toggle: {err}");
            }

        let notice = if use_chat {
            "Review model now follows Chat model".to_string()
        } else {
            format!(
                "Review model set to {} ({} reasoning)",
                self.config.review_model,
                Self::format_reasoning_effort(self.config.review_model_reasoning_effort)
            )
        };
        self.bottom_pane.flash_footer_notice(notice);
        self.update_review_settings_model_row();
        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    pub(crate) fn set_review_resolve_use_chat_model(&mut self, use_chat: bool) {
        if self.config.review_resolve_use_chat_model == use_chat {
            return;
        }
        self.config.review_resolve_use_chat_model = use_chat;
        if use_chat {
            self.config.review_resolve_model = self.config.model.clone();
            self.config.review_resolve_model_reasoning_effort = self.config.model_reasoning_effort;
        }

        if let Ok(home) = code_core::config::find_code_home()
            && let Err(err) = code_core::config::set_review_resolve_model(
                &home,
                &self.config.review_resolve_model,
                self.config.review_resolve_model_reasoning_effort,
                use_chat,
            ) {
                tracing::warn!("Failed to persist resolve use-chat toggle: {err}");
            }

        let notice = if use_chat {
            "Resolve model now follows Chat model".to_string()
        } else {
            format!(
                "Resolve model set to {} ({} reasoning)",
                self.config.review_resolve_model,
                Self::format_reasoning_effort(self.config.review_resolve_model_reasoning_effort)
            )
        };
        self.bottom_pane.flash_footer_notice(notice);
        self.update_review_settings_model_row();
        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    pub(crate) fn apply_auto_review_model_selection(
        &mut self,
        model: String,
        effort: ReasoningEffort,
    ) {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return;
        }

        self.config.auto_review_use_chat_model = false;
        let clamped_effort = Self::clamp_reasoning_for_model(trimmed, effort);

        let mut updated = false;
        if !self
            .config
            .auto_review_model
            .eq_ignore_ascii_case(trimmed)
        {
            self.config.auto_review_model = trimmed.to_string();
            updated = true;
        }

        if self.config.auto_review_model_reasoning_effort != clamped_effort {
            self.config.auto_review_model_reasoning_effort = clamped_effort;
            updated = true;
        }

        if !updated {
            self.bottom_pane
                .flash_footer_notice("Auto Review model unchanged.".to_string());
            return;
        }

        let notice = if let Ok(home) = code_core::config::find_code_home() {
            match code_core::config::set_auto_review_model(
                &home,
                &self.config.auto_review_model,
                self.config.auto_review_model_reasoning_effort,
                self.config.auto_review_use_chat_model,
            ) {
                Ok(_) => format!(
                    "Auto Review model set to {} ({} reasoning)",
                    self.config.auto_review_model,
                    Self::format_reasoning_effort(self.config.auto_review_model_reasoning_effort)
                ),
                Err(err) => {
                    tracing::warn!("Failed to persist Auto Review model: {err}");
                    format!(
                        "Auto Review model set for this session (failed to persist): {}",
                        self.config.auto_review_model
                    )
                }
            }
        } else {
            tracing::warn!("Could not locate Code home to persist Auto Review model");
            format!(
                "Auto Review model set for this session: {}",
                self.config.auto_review_model
            )
        };

        self.bottom_pane.flash_footer_notice(notice);
        self.refresh_settings_overview_rows();
        self.update_review_settings_model_row();
        self.request_redraw();
    }

    pub(crate) fn set_auto_review_use_chat_model(&mut self, use_chat: bool) {
        if self.config.auto_review_use_chat_model == use_chat {
            return;
        }
        self.config.auto_review_use_chat_model = use_chat;
        if use_chat {
            self.config.auto_review_model = self.config.model.clone();
            self.config.auto_review_model_reasoning_effort = self.config.model_reasoning_effort;
        }

        if let Ok(home) = code_core::config::find_code_home()
            && let Err(err) = code_core::config::set_auto_review_model(
                &home,
                &self.config.auto_review_model,
                self.config.auto_review_model_reasoning_effort,
                use_chat,
            ) {
                tracing::warn!("Failed to persist Auto Review use-chat toggle: {err}");
            }

        let notice = if use_chat {
            "Auto Review model now follows Chat model".to_string()
        } else {
            format!(
                "Auto Review model set to {} ({} reasoning)",
                self.config.auto_review_model,
                Self::format_reasoning_effort(self.config.auto_review_model_reasoning_effort)
            )
        };
        self.bottom_pane.flash_footer_notice(notice);
        self.update_review_settings_model_row();
        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    pub(crate) fn apply_auto_review_resolve_model_selection(
        &mut self,
        model: String,
        effort: ReasoningEffort,
    ) {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return;
        }

        self.config.auto_review_resolve_use_chat_model = false;
        let clamped_effort = Self::clamp_reasoning_for_model(trimmed, effort);

        let mut updated = false;
        if !self
            .config
            .auto_review_resolve_model
            .eq_ignore_ascii_case(trimmed)
        {
            self.config.auto_review_resolve_model = trimmed.to_string();
            updated = true;
        }

        if self.config.auto_review_resolve_model_reasoning_effort != clamped_effort {
            self.config.auto_review_resolve_model_reasoning_effort = clamped_effort;
            updated = true;
        }

        if !updated {
            self.bottom_pane
                .flash_footer_notice("Auto Review resolve model unchanged.".to_string());
            return;
        }

        let notice = if let Ok(home) = code_core::config::find_code_home() {
            match code_core::config::set_auto_review_resolve_model(
                &home,
                &self.config.auto_review_resolve_model,
                self.config.auto_review_resolve_model_reasoning_effort,
                self.config.auto_review_resolve_use_chat_model,
            ) {
                Ok(_) => format!(
                    "Auto Review resolve model set to {} ({} reasoning)",
                    self.config.auto_review_resolve_model,
                    Self::format_reasoning_effort(self.config.auto_review_resolve_model_reasoning_effort)
                ),
                Err(err) => {
                    tracing::warn!("Failed to persist Auto Review resolve model: {err}");
                    format!(
                        "Auto Review resolve model set for this session (failed to persist): {}",
                        self.config.auto_review_resolve_model
                    )
                }
            }
        } else {
            tracing::warn!("Could not locate Code home to persist Auto Review resolve model");
            format!(
                "Auto Review resolve model set for this session: {}",
                self.config.auto_review_resolve_model
            )
        };

        self.bottom_pane.flash_footer_notice(notice);
        self.refresh_settings_overview_rows();
        self.update_review_settings_model_row();
        self.request_redraw();
    }

    pub(crate) fn set_auto_review_resolve_use_chat_model(&mut self, use_chat: bool) {
        if self.config.auto_review_resolve_use_chat_model == use_chat {
            return;
        }
        self.config.auto_review_resolve_use_chat_model = use_chat;
        if use_chat {
            self.config.auto_review_resolve_model = self.config.model.clone();
            self.config.auto_review_resolve_model_reasoning_effort =
                self.config.model_reasoning_effort;
        }

        if let Ok(home) = code_core::config::find_code_home()
            && let Err(err) = code_core::config::set_auto_review_resolve_model(
                &home,
                &self.config.auto_review_resolve_model,
                self.config.auto_review_resolve_model_reasoning_effort,
                use_chat,
            ) {
                tracing::warn!("Failed to persist Auto Review resolve use-chat toggle: {err}");
            }

        let notice = if use_chat {
            "Auto Review resolve model now follows Chat model".to_string()
        } else {
            format!(
                "Auto Review resolve model set to {} ({} reasoning)",
                self.config.auto_review_resolve_model,
                Self::format_reasoning_effort(self.config.auto_review_resolve_model_reasoning_effort)
            )
        };
        self.bottom_pane.flash_footer_notice(notice);
        self.update_review_settings_model_row();
        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    pub(crate) fn set_auto_drive_use_chat_model(&mut self, use_chat: bool) {
        if self.config.auto_drive_use_chat_model == use_chat {
            return;
        }
        self.config.auto_drive_use_chat_model = use_chat;
        if use_chat {
            self.config.auto_drive.model = self.config.model.clone();
            self.config.auto_drive.model_reasoning_effort = self.config.model_reasoning_effort;
        }

        self.restore_auto_resolve_attempts_if_lost();

        if let Ok(home) = code_core::config::find_code_home()
            && let Err(err) = code_core::config::set_auto_drive_settings(
                &home,
                &self.config.auto_drive,
                use_chat,
            ) {
                tracing::warn!("Failed to persist Auto Drive use-chat toggle: {err}");
            }

        let notice = if use_chat {
            "Auto Drive model now follows Chat model".to_string()
        } else {
            format!(
                "Auto Drive model set to {} ({} reasoning)",
                self.config.auto_drive.model,
                Self::format_reasoning_effort(self.config.auto_drive.model_reasoning_effort)
            )
        };

        self.bottom_pane.flash_footer_notice(notice);
        self.refresh_settings_overview_rows();
        self.update_auto_drive_settings_model_row();
        self.request_redraw();
    }

}
