impl ChatWidget<'_> {
    fn spawn_update_refresh(&self, shared_state: std::sync::Arc<std::sync::Mutex<UpdateSharedState>>) {
        let config = self.config.clone();
        let tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let result = crate::updates::check_for_updates_now(&config).await;
            let mut state = shared_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match result {
                Ok(info) => {
                    state.checking = false;
                    state.latest_version = info.latest_version;
                    state.error = None;
                }
                Err(err) => {
                    state.checking = false;
                    state.latest_version = None;
                    state.error = Some(err.to_string());
                }
            }
            drop(state);
            tx.send(AppEvent::RequestRedraw);
        });
    }

    fn prepare_update_settings_view(&mut self) -> Option<UpdateSettingsView> {
        let allow_refresh = crate::updates::upgrade_ui_enabled();

        let shared_state = std::sync::Arc::new(std::sync::Mutex::new(UpdateSharedState {
            checking: allow_refresh,
            latest_version: None,
            error: None,
        }));

        let resolution = crate::updates::resolve_upgrade_resolution();
        let (command, display, instructions) = match &resolution {
            crate::updates::UpgradeResolution::Command { command, display } => (
                Some(command.clone()),
                Some(display.clone()),
                None,
            ),
            crate::updates::UpgradeResolution::Manual { instructions } => {
                (None, None, Some(instructions.clone()))
            }
        };

        let view = UpdateSettingsView::new(UpdateSettingsInit {
            app_event_tx: self.app_event_tx.clone(),
            ticket: self.make_background_tail_ticket(),
            current_version: code_version::version().to_string(),
            auto_enabled: self.config.auto_upgrade_enabled,
            command,
            command_display: display,
            manual_instructions: instructions,
            shared: shared_state.clone(),
        });

        if allow_refresh {
            self.spawn_update_refresh(shared_state);
        }
        Some(view)
    }

    fn build_updates_settings_content(&mut self) -> Option<UpdatesSettingsContent> {
        self.prepare_update_settings_view()
            .map(UpdatesSettingsContent::new)
    }

    fn build_accounts_settings_content(&self) -> AccountsSettingsContent {
        AccountsSettingsContent::new(
            self.app_event_tx.clone(),
            self.config.auto_switch_accounts_on_rate_limit,
            self.config.api_key_fallback_on_all_accounts_limited,
            self.config.cli_auth_credentials_store_mode,
        )
    }

    fn build_validation_settings_view(&mut self) -> ValidationSettingsView {
        let groups = vec![
            (
                GroupStatus {
                    group: ValidationGroup::Functional,
                    name: "Functional checks",
                },
                self.config.validation.groups.functional,
            ),
            (
                GroupStatus {
                    group: ValidationGroup::Stylistic,
                    name: "Stylistic checks",
                },
                self.config.validation.groups.stylistic,
            ),
        ];

        let tool_rows: Vec<ToolRow> = crate::bottom_pane::settings_pages::validation::detect_tools()
            .into_iter()
            .map(|status| {
                let group = match status.category {
                    ValidationCategory::Functional => ValidationGroup::Functional,
                    ValidationCategory::Stylistic => ValidationGroup::Stylistic,
                };
                let requested = self.validation_tool_requested(status.name);
                let group_enabled = self.validation_group_enabled(group);
                ToolRow { status, enabled: requested, group_enabled }
            })
            .collect();

        ValidationSettingsView::new(
            groups,
            tool_rows,
            self.app_event_tx.clone(),
        )
    }

    fn build_validation_settings_content(&mut self) -> ValidationSettingsContent {
        ValidationSettingsContent::new(self.build_validation_settings_view())
    }

    fn build_review_settings_view(&mut self) -> ReviewSettingsView {
        let auto_resolve_enabled = self.config.tui.review_auto_resolve;
        let auto_review_enabled = self.config.tui.auto_review_enabled;
        let attempts = self.configured_auto_resolve_re_reviews();
        ReviewSettingsView::new(ReviewSettingsInit {
            review_use_chat_model: self.config.review_use_chat_model,
            review_model: self.config.review_model.clone(),
            review_reasoning: self.config.review_model_reasoning_effort,
            review_resolve_use_chat_model: self.config.review_resolve_use_chat_model,
            review_resolve_model: self.config.review_resolve_model.clone(),
            review_resolve_reasoning: self.config.review_resolve_model_reasoning_effort,
            review_auto_resolve_enabled: auto_resolve_enabled,
            review_followups: attempts,
            auto_review_enabled,
            auto_review_use_chat_model: self.config.auto_review_use_chat_model,
            auto_review_model: self.config.auto_review_model.clone(),
            auto_review_reasoning: self.config.auto_review_model_reasoning_effort,
            auto_review_resolve_use_chat_model: self.config.auto_review_resolve_use_chat_model,
            auto_review_resolve_model: self.config.auto_review_resolve_model.clone(),
            auto_review_resolve_reasoning: self.config.auto_review_resolve_model_reasoning_effort,
            auto_review_followups: self.config.auto_drive.auto_review_followup_attempts.get(),
            app_event_tx: self.app_event_tx.clone(),
        })
    }

    fn build_review_settings_content(&mut self) -> ReviewSettingsContent {
        ReviewSettingsContent::new(self.build_review_settings_view())
    }

    fn build_planning_settings_view(&mut self) -> PlanningSettingsView {
        PlanningSettingsView::new(
            self.config.planning_use_chat_model,
            self.config.planning_model.clone(),
            self.config.planning_model_reasoning_effort,
            self.app_event_tx.clone(),
        )
    }

    fn build_planning_settings_content(&mut self) -> PlanningSettingsContent {
        PlanningSettingsContent::new(self.build_planning_settings_view())
    }

    fn build_auto_drive_settings_view(&mut self) -> AutoDriveSettingsView {
        let model = self.config.auto_drive.model.clone();
        let model_effort = self.config.auto_drive.model_reasoning_effort;
        let use_chat_model = self.config.auto_drive_use_chat_model;
        let review = self.auto_state.review_enabled;
        let agents = self.auto_state.subagents_enabled;
        let cross = self.auto_state.cross_check_enabled;
        let qa = self.auto_state.qa_automation_enabled;
        let model_routing_enabled = self.config.auto_drive.model_routing_enabled;
        let model_routing_entries = self.config.auto_drive.model_routing_entries.clone();
        let routing_model_options = self
            .available_model_presets()
            .into_iter()
            .map(|preset| preset.model)
            .collect();
        let mode = self.auto_state.continue_mode;
        AutoDriveSettingsView::new(AutoDriveSettingsInit {
            app_event_tx: self.app_event_tx.clone(),
            model,
            model_reasoning: model_effort,
            use_chat_model,
            review_enabled: review,
            agents_enabled: agents,
            cross_check_enabled: cross,
            qa_automation_enabled: qa,
            model_routing_enabled,
            model_routing_entries,
            routing_model_options,
            continue_mode: mode,
        })
    }

    fn build_auto_drive_settings_content(&mut self) -> AutoDriveSettingsContent {
        AutoDriveSettingsContent::new(self.build_auto_drive_settings_view())
    }

    fn ensure_updates_settings_overlay(&mut self) {
        if self.settings.overlay.is_none() {
            self.show_settings_overlay(Some(SettingsSection::Updates));
            return;
        }
        if let Some(content) = self.build_updates_settings_content()
            && let Some(overlay) = self.settings.overlay.as_mut() {
                overlay.set_updates_content(content);
            }
        self.ensure_settings_overlay_section(SettingsSection::Updates);
        self.request_redraw();
    }

    fn ensure_validation_settings_overlay(&mut self) {
        if self.settings.overlay.is_none() {
            self.show_settings_overlay(Some(SettingsSection::Validation));
            return;
        }
        let content = self.build_validation_settings_content();
        if let Some(overlay) = self.settings.overlay.as_mut() {
            overlay.set_validation_content(content);
        }
        self.ensure_settings_overlay_section(SettingsSection::Validation);
        self.request_redraw();
    }

    fn ensure_auto_drive_settings_overlay(&mut self) {
        if self.settings.overlay.is_none() {
            self.show_settings_overlay(Some(SettingsSection::AutoDrive));
            return;
        }
        let content = self.build_auto_drive_settings_content();
        if let Some(overlay) = self.settings.overlay.as_mut() {
            overlay.set_auto_drive_content(content);
        }
        self.ensure_settings_overlay_section(SettingsSection::AutoDrive);
        self.request_redraw();
    }

}
