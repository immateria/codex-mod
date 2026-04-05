impl ChatWidget<'_> {
    pub(crate) fn show_limits_settings_ui(&mut self) {
        self.ensure_settings_overlay_section(SettingsSection::Limits);

        if let Some(cached) = self.limits.cached_content.take() {
            self.update_limits_settings_content(cached);
        }

        let snapshot = self.rate_limit_snapshot.clone();
        let needs_refresh = self.should_refresh_limits();

        if self.rate_limit_fetch_inflight || needs_refresh {
            self.set_limits_overlay_content(LimitsOverlayContent::Loading);
        } else {
            let reset_info = self.rate_limit_reset_info();
            let tabs = self.build_limits_tabs(snapshot.clone(), reset_info);
            self.set_limits_overlay_tabs(tabs);
        }

        self.request_redraw();

        if needs_refresh {
            self.request_latest_rate_limits(snapshot.is_none());
        }

        self.refresh_limits_for_other_accounts_if_due();
    }

    fn refresh_limits_for_other_accounts_if_due(&mut self) {
        let code_home = self.config.code_home.clone();
        let active_id = auth_accounts::get_active_account_id(&code_home)
            .ok()
            .flatten();
        let accounts = auth_accounts::list_accounts(&code_home).unwrap_or_default();
        if accounts.is_empty() {
            return;
        }

        let usage_records = account_usage::list_rate_limit_snapshots(&code_home).unwrap_or_default();
        let snapshot_map: HashMap<String, StoredRateLimitSnapshot> = usage_records
            .into_iter()
            .map(|record| (record.account_id.clone(), record))
            .collect();
        let now = Utc::now();
        let stale_interval = account_usage::rate_limit_refresh_stale_interval();

        for account in accounts {
            if active_id.as_deref() == Some(account.id.as_str()) {
                continue;
            }

            let reset_at = snapshot_map
                .get(&account.id)
                .and_then(|record| record.secondary_next_reset_at);
            let plan = account
                .tokens
                .as_ref()
                .and_then(|tokens| tokens.id_token.get_chatgpt_plan_type());

            let should_refresh = account_usage::mark_rate_limit_refresh_attempt_if_due(
                &code_home,
                &account.id,
                plan.as_deref(),
                reset_at,
                now,
                stale_interval,
            )
            .unwrap_or(false);

            if should_refresh {
                start_rate_limit_refresh_for_account(
                    self.app_event_tx.clone(),
                    self.config.clone(),
                    self.config.debug,
                    account,
                    false,
                    false,
                );
            }
        }
    }

    fn request_latest_rate_limits(&mut self, show_loading: bool) {
        if self.rate_limit_fetch_inflight {
            return;
        }

        if show_loading {
            self.set_limits_overlay_content(LimitsOverlayContent::Loading);
            self.request_redraw();
        }

        self.rate_limit_fetch_inflight = true;

        start_rate_limit_refresh(
            self.app_event_tx.clone(),
            self.config.clone(),
            self.config.debug,
        );
    }

    fn should_refresh_limits(&self) -> bool {
        if self.rate_limit_fetch_inflight {
            return false;
        }
        match self.rate_limit_last_fetch_at {
            Some(ts) => Utc::now() - ts > RATE_LIMIT_REFRESH_INTERVAL,
            None => true,
        }
    }

    pub(crate) fn on_auto_upgrade_completed(&mut self, version: String) {
        let notice = format!("Auto-upgraded to version {version}");
        self.latest_upgrade_version = None;
        self.push_background_tail(notice.clone());
        self.bottom_pane.flash_footer_notice(notice);
        self.request_redraw();
    }

    pub(crate) fn on_rate_limit_refresh_failed(&mut self, message: String) {
        self.rate_limit_fetch_inflight = false;

        let content = if self.rate_limit_snapshot.is_some() {
            LimitsOverlayContent::Error(message.clone())
        } else {
            LimitsOverlayContent::Placeholder
        };
        self.set_limits_overlay_content(content);
        self.request_redraw();

        if self.rate_limit_snapshot.is_some() {
            self.history_push_plain_state(history_cell::new_warning_event(message));
        }
    }

    pub(crate) fn on_rate_limit_snapshot_stored(&mut self, _account_id: String) {
        self.refresh_settings_overview_rows();
        let refresh_limits_settings = self
            .settings
            .overlay
            .as_ref()
            .map(|overlay| {
                overlay.active_section() == SettingsSection::Limits && !overlay.is_menu_active()
            })
            .unwrap_or(false);
        if refresh_limits_settings {
            self.show_limits_settings_ui();
        } else {
            self.request_redraw();
        }
    }

    fn rate_limit_reset_info(&self) -> RateLimitResetInfo {
        let auto_compact_limit = self
            .config
            .model_auto_compact_token_limit
            .and_then(|limit| (limit > 0).then_some(limit as u64));
        let auto_compact_tokens_used = auto_compact_limit.map(|_| {
            // Use the latest turn's context footprint, which best matches when
            // auto-compaction triggers, instead of the lifetime session total.
            self.last_token_usage.tokens_in_context_window()
        });
        let context_window = self.config.model_context_window;
        let context_tokens_used = context_window.map(|_| self.last_token_usage.tokens_in_context_window());

        RateLimitResetInfo {
            primary_next_reset: self.rate_limit_primary_next_reset_at,
            secondary_next_reset: self.rate_limit_secondary_next_reset_at,
            auto_compact_tokens_used,
            auto_compact_limit,
            overflow_auto_compact: true,
            context_window,
            context_tokens_used,
        }
    }

    fn rate_limit_display_config_for_account(
        account: Option<&StoredAccount>,
    ) -> RateLimitDisplayConfig {
        if matches!(account.map(|acc| acc.mode), Some(AuthMode::ApiKey)) {
            RateLimitDisplayConfig {
                show_usage_sections: false,
                show_chart: false,
            }
        } else {
            DEFAULT_DISPLAY_CONFIG
        }
    }

    fn update_rate_limit_resets(&mut self, current: &RateLimitSnapshotEvent) {
        let now = Utc::now();
        if let Some(secs) = current.primary_reset_after_seconds {
            self.rate_limit_primary_next_reset_at =
                Some(now + ChronoDuration::seconds(secs as i64));
        } else {
            self.rate_limit_primary_next_reset_at = None;
        }
        if let Some(secs) = current.secondary_reset_after_seconds {
            self.rate_limit_secondary_next_reset_at =
                Some(now + ChronoDuration::seconds(secs as i64));
        } else {
            self.rate_limit_secondary_next_reset_at = None;
        }
        self.maybe_schedule_rate_limit_refresh();
    }

    fn maybe_schedule_rate_limit_refresh(&mut self) {
        let Some(reset_at) = self.rate_limit_secondary_next_reset_at else {
            self.rate_limit_refresh_scheduled_for = None;
            self.rate_limit_refresh_schedule_id.fetch_add(1, Ordering::SeqCst);
            return;
        };

        if self.rate_limit_refresh_scheduled_for == Some(reset_at) {
            return;
        }

        self.rate_limit_refresh_scheduled_for = Some(reset_at);
        let schedule_id = self
            .rate_limit_refresh_schedule_id
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        let schedule_token = self.rate_limit_refresh_schedule_id.clone();
        let app_event_tx = self.app_event_tx.clone();
        let config = self.config.clone();
        let debug_enabled = self.config.debug;
        let account = auth_accounts::get_active_account_id(&config.code_home)
            .ok()
            .flatten()
            .and_then(|id| auth_accounts::find_account(&config.code_home, &id).ok())
            .flatten();

        if account.is_none() {
            return;
        }

        if thread_spawner::spawn_lightweight("rate-reset-refresh", move || {
            let now = Utc::now();
            let delay = reset_at.signed_duration_since(now) + ChronoDuration::seconds(1);
            if let Ok(delay) = delay.to_std()
                && !delay.is_zero() {
                    std::thread::sleep(delay);
                }

            if schedule_token.load(Ordering::SeqCst) != schedule_id {
                return;
            }

            let Some(account) = account else {
                return;
            };

            let plan = account
                .tokens
                .as_ref()
                .and_then(|tokens| tokens.id_token.get_chatgpt_plan_type());
            let should_refresh = account_usage::mark_rate_limit_refresh_attempt_if_due(
                &config.code_home,
                &account.id,
                plan.as_deref(),
                Some(reset_at),
                Utc::now(),
                account_usage::rate_limit_refresh_stale_interval(),
            )
            .unwrap_or(false);

            if should_refresh {
                start_rate_limit_refresh_for_account(
                    app_event_tx,
                    config,
                    debug_enabled,
                    account,
                    true,
                    false,
                );
            }
        })
        .is_none()
        {
            tracing::warn!("rate reset refresh scheduling failed: worker unavailable");
        }
    }

    pub(crate) fn handle_update_command(&mut self, command_args: &str) {
        let trimmed = command_args.trim();
        if trimmed.eq_ignore_ascii_case("settings")
            || trimmed.eq_ignore_ascii_case("ui")
            || trimmed.eq_ignore_ascii_case("config")
        {
            self.ensure_updates_settings_overlay();
            return;
        }

        // Always surface the update settings overlay before kicking off any upgrade flow.
        self.ensure_updates_settings_overlay();

        if !crate::updates::upgrade_ui_enabled() {
            return;
        }

        match crate::updates::resolve_upgrade_resolution(&self.config) {
            crate::updates::UpgradeResolution::Command { command, display } => {
                if command.is_empty() {
                    self.history_push_plain_state(history_cell::new_error_event(
                        "`/update` — no upgrade command available for this install.".to_string(),
                    ));
                    self.request_redraw();
                    return;
                }

                let latest = self.latest_upgrade_version.clone();
                self.push_background_tail(
                    "Opening a guided upgrade terminal to finish installing updates.".to_string(),
                );
                if let Some(launch) = self.launch_update_command(command, display, latest) {
                    self.app_event_tx.send(AppEvent::OpenTerminal(launch));
                }
            }
            crate::updates::UpgradeResolution::Manual { instructions } => {
                self.push_background_tail(instructions);
                self.request_redraw();
            }
        }
    }

    pub(crate) fn handle_notifications_command(&mut self, args: String) {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            self.show_settings_overlay(Some(SettingsSection::Notifications));
            return;
        }

        let keyword = trimmed.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
        match keyword.as_str() {
            "status" => {
                match &self.config.tui.notifications {
                    Notifications::Enabled(true) => {
                        self.push_background_tail("TUI notifications are enabled.".to_string());
                    }
                    Notifications::Enabled(false) => {
                        self.push_background_tail("TUI notifications are disabled.".to_string());
                    }
                    Notifications::Custom(entries) => {
                        let filters = if entries.is_empty() {
                            "<none>".to_string()
                        } else {
                            entries.join(", ")
                        };
                        self.push_background_tail(format!(
                            "TUI notifications use custom filters: [{filters}]"
                        ));
                    }
                }
            }
            "on" | "off" => {
                let enable = keyword == "on";
                match &self.config.tui.notifications {
                    Notifications::Enabled(current) => {
                        if *current == enable {
                            self.push_background_tail(format!(
                                "TUI notifications already {}.",
                                if enable { "enabled" } else { "disabled" }
                            ));
                        } else {
                            self.app_event_tx
                                .send(AppEvent::UpdateTuiNotifications(enable));
                        }
                    }
                    Notifications::Custom(entries) => {
                        let filters = if entries.is_empty() {
                            "<none>".to_string()
                        } else {
                            entries.join(", ")
                        };
                        self.push_background_tail(format!(
                            "TUI notifications use custom filters ([{filters}]); edit ~/.code/config.toml to change them."
                        ));
                    }
                }
            }
            _ => {
                self.push_background_tail(
                    "Usage: /notifications [status|on|off]".to_string(),
                );
            }
        }
    }

    pub(crate) fn handle_prompts_command(&mut self, args: &str) {
        if !args.trim().is_empty() {
            self.history_push_plain_state(history_cell::new_error_event(
                "Usage: /prompts".to_string(),
            ));
            return;
        }

        self.submit_op(Op::ListCustomPrompts);
        self.show_settings_overlay(Some(SettingsSection::Prompts));
    }

    pub(crate) fn handle_skills_command(&mut self, args: &str) {
        if !args.trim().is_empty() {
            self.history_push_plain_state(history_cell::new_error_event(
                "Usage: /skills".to_string(),
            ));
            return;
        }

        self.submit_op(Op::ListSkills);
        self.show_settings_overlay(Some(SettingsSection::Skills));
    }

    pub(crate) fn handle_agents_command(&mut self, args: String) {
        if !args.trim().is_empty() {
            self.history_push_plain_state(history_cell::new_error_event(
                "Usage: /agents".to_string(),
            ));
        }
        self.show_settings_overlay(Some(SettingsSection::Agents));
    }

    pub(crate) fn handle_limits_command(&mut self, args: String) {
        if !args.trim().is_empty() {
            self.history_push_plain_state(history_cell::new_error_event(
                "Usage: /limits".to_string(),
            ));
        }
        self.show_settings_overlay(Some(SettingsSection::Limits));
    }

    pub(crate) fn handle_memories_command(&mut self, args: String) {
        let trimmed = args.trim();
        match trimmed {
            "" | "settings" => {
                self.show_settings_overlay(Some(SettingsSection::Memories));
            }
            "status" => {
                self.flash_footer_notice("Loading memories status…".to_string());
                self.app_event_tx.send(AppEvent::RunMemoriesStatusLoad {
                    target: crate::app_event::MemoriesStatusLoadTarget::SlashCommand,
                });
            }
            "refresh" => {
                self.flash_footer_notice("Refreshing memories artifacts…".to_string());
                self.app_event_tx.send(AppEvent::RunMemoriesArtifactsAction {
                    action: crate::app_event::MemoriesArtifactsAction::Refresh,
                });
            }
            "clear" => {
                self.flash_footer_notice("Clearing generated memories artifacts…".to_string());
                self.app_event_tx.send(AppEvent::RunMemoriesArtifactsAction {
                    action: crate::app_event::MemoriesArtifactsAction::Clear,
                });
            }
            _ => {
                self.history_push_plain_state(history_cell::new_error_event(
                    "Usage: /memories [status|refresh|clear|settings]".to_string(),
                ));
            }
        }
    }

}
