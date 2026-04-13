impl ChatWidget<'_> {
    pub(crate) fn show_limits_settings_ui(&mut self) {
        self.ensure_settings_overlay_section(SettingsSection::Limits);
        self.check_inflight_timeout();

        if let Some(cached) = self.limits.cached_content.take() {
            self.update_limits_settings_content(cached);
        }

        let snapshot = self.rate_limit_snapshot.clone();
        let needs_refresh = self.should_refresh_limits();

        // Always try to build tabs from cached data first so that accounts
        // with stored snapshots remain visible even while a refresh for the
        // active account is in-flight or pending.
        let reset_info = self.rate_limit_reset_info();
        let tabs = self.build_limits_tabs(snapshot.clone(), reset_info);
        if !tabs.is_empty() {
            self.set_limits_overlay_tabs(tabs);
        } else if self.rate_limit_fetch_inflight || needs_refresh {
            self.set_limits_overlay_content(LimitsOverlayContent::Loading);
        } else {
            self.set_limits_overlay_content(LimitsOverlayContent::Placeholder);
        }

        self.request_redraw();

        if needs_refresh {
            self.request_latest_rate_limits(snapshot.is_none());
        }

        self.refresh_limits_for_other_accounts_if_due();
    }

    fn refresh_limits_for_other_accounts_if_due(&self) {
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
                // Skip if the thread pool is under pressure — we'll retry on
                // the next overlay open.
                if thread_spawner::active_thread_count()
                    >= thread_spawner::max_thread_count() / 2
                {
                    tracing::debug!(
                        "skipping background refresh for {} — thread pool under pressure",
                        account.id,
                    );
                    continue;
                }
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
        self.check_inflight_timeout();
        if self.rate_limit_fetch_inflight {
            return;
        }

        if show_loading {
            self.set_limits_overlay_content(LimitsOverlayContent::Loading);
            self.request_redraw();
        }

        self.rate_limit_fetch_inflight = true;
        self.rate_limit_fetch_inflight_since = Some(std::time::Instant::now());

        start_rate_limit_refresh(
            self.app_event_tx.clone(),
            self.config.clone(),
            self.config.debug,
        );
    }

    fn should_refresh_limits(&self) -> bool {
        if self.rate_limit_fetch_inflight {
            // If we've been inflight too long, the background thread likely
            // died or hung. The caller should invoke `check_inflight_timeout`
            // to clear the flag, but we also guard here to avoid permanent
            // stalls.
            return false;
        }
        match self.rate_limit_last_fetch_at {
            Some(ts) => Utc::now() - ts > RATE_LIMIT_REFRESH_INTERVAL,
            None => true,
        }
    }

    /// If a rate-limit fetch has been inflight for longer than 45 seconds,
    /// auto-clear the flag so the UI doesn't get permanently stuck in
    /// "Loading..." state.
    fn check_inflight_timeout(&mut self) {
        const INFLIGHT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);
        if self.rate_limit_fetch_inflight
            && let Some(since) = self.rate_limit_fetch_inflight_since
            && since.elapsed() > INFLIGHT_TIMEOUT
        {
            tracing::warn!(
                "rate-limit fetch inflight for {:.0}s — auto-clearing stuck flag",
                since.elapsed().as_secs_f64(),
            );
            self.rate_limit_fetch_inflight = false;
            self.rate_limit_fetch_inflight_since = None;
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
        self.rate_limit_fetch_inflight_since = None;

        // Record the failure time so should_refresh_limits() doesn't
        // immediately retry and create an infinite loop when background
        // account refreshes trigger show_limits_settings_ui().
        self.rate_limit_last_fetch_at = Some(Utc::now());

        // Instead of replacing all tabs with a single error, build tabs
        // from cached data so other accounts' limits remain visible.
        let snapshot = self.rate_limit_snapshot.clone();
        let reset_info = self.rate_limit_reset_info();
        let tabs = self.build_limits_tabs(snapshot, reset_info);

        if tabs.is_empty() {
            self.set_limits_overlay_content(LimitsOverlayContent::Placeholder);
        } else {
            self.set_limits_overlay_tabs(tabs);
        }
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
            .is_some_and(|overlay| {
                overlay.active_section() == SettingsSection::Limits && !overlay.is_menu_active()
            });
        if refresh_limits_settings {
            self.show_limits_settings_ui();
        } else {
            self.request_redraw();
        }
    }

    /// Switch the active account from the limits overlay (triggered by `S` key).
    pub(crate) fn on_switch_account_from_limits(&mut self, account_id: String) {
        let code_home = self.config.code_home.clone();
        let Ok(Some(account)) = auth_accounts::find_account(&code_home, &account_id) else {
            return;
        };
        let mode = account.mode;
        match auth::activate_account_with_store_mode(
            &code_home,
            &account_id,
            self.config.cli_auth_credentials_store_mode,
        ) {
            Ok(()) => {
                self.app_event_tx.send(AppEvent::LoginUsingChatGptChanged {
                    using_chatgpt_auth: mode.is_chatgpt(),
                });
                // Re-probe the newly-active account so the overlay refreshes
                // with live data.
                self.rate_limit_fetch_inflight = true;
                self.rate_limit_fetch_inflight_since = Some(std::time::Instant::now());
                start_rate_limit_refresh(
                    self.app_event_tx.clone(),
                    self.config.clone(),
                    self.config.debug,
                );
                self.set_limits_overlay_content(LimitsOverlayContent::Loading);
                self.request_redraw();
            }
            Err(err) => {
                tracing::warn!("failed to switch account from limits: {err}");
            }
        }
    }

    /// Warm all non-active accounts by sending a minimal probe to each, which
    /// starts their 5-hour usage timer and fetches fresh rate limit data.
    /// Caps concurrent warm-up threads to avoid exhausting the thread pool.
    pub(crate) fn on_warm_all_accounts(&mut self) {
        const MAX_CONCURRENT_WARMUPS: usize = 8;

        let code_home = self.config.code_home.clone();
        let active_id = auth_accounts::get_active_account_id(&code_home)
            .ok()
            .flatten();
        let accounts = auth_accounts::list_accounts(&code_home).unwrap_or_default();
        let mut launched = 0u32;
        for account in accounts {
            if active_id.as_deref() == Some(account.id.as_str()) {
                continue;
            }
            // Check thread pool pressure before each spawn.
            if thread_spawner::active_thread_count() >= MAX_CONCURRENT_WARMUPS {
                tracing::info!(
                    "warm-all: skipping remaining accounts — thread pool at capacity \
                     ({} active)",
                    thread_spawner::active_thread_count(),
                );
                break;
            }
            start_rate_limit_refresh_for_account(
                self.app_event_tx.clone(),
                self.config.clone(),
                self.config.debug,
                account,
                false,
                false,
            );
            launched += 1;
        }
        if launched > 0 {
            self.set_limits_overlay_content(LimitsOverlayContent::Loading);
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
                    // Sleep in short intervals so the thread releases promptly
                    // when the schedule is superseded by a newer reset time.
                    const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
                    let deadline = std::time::Instant::now() + delay;
                    while std::time::Instant::now() < deadline {
                        if schedule_token.load(Ordering::SeqCst) != schedule_id {
                            return;
                        }
                        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                        std::thread::sleep(remaining.min(POLL_INTERVAL));
                    }
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
                        "`/update` — no upgrade command available for this install.".to_owned(),
                    ));
                    self.request_redraw();
                    return;
                }

                let latest = self.latest_upgrade_version.clone();
                self.push_background_tail(
                    "Opening a guided upgrade terminal to finish installing updates.".to_owned(),
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
                        self.push_background_tail("TUI notifications are enabled.".to_owned());
                    }
                    Notifications::Enabled(false) => {
                        self.push_background_tail("TUI notifications are disabled.".to_owned());
                    }
                    Notifications::Custom(entries) => {
                        let filters = if entries.is_empty() {
                            "<none>".to_owned()
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
                            "<none>".to_owned()
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
                    "Usage: /notifications [status|on|off]".to_owned(),
                );
            }
        }
    }

    pub(crate) fn handle_prompts_command(&mut self, args: &str) {
        if !args.trim().is_empty() {
            self.history_push_plain_state(history_cell::new_error_event(
                "Usage: /prompts".to_owned(),
            ));
            return;
        }

        self.submit_op(Op::ListCustomPrompts);
        self.show_settings_overlay(Some(SettingsSection::Prompts));
    }

    pub(crate) fn handle_skills_command(&mut self, args: &str) {
        if !args.trim().is_empty() {
            self.history_push_plain_state(history_cell::new_error_event(
                "Usage: /skills".to_owned(),
            ));
            return;
        }

        self.submit_op(Op::ListSkills);
        self.show_settings_overlay(Some(SettingsSection::Skills));
    }

    pub(crate) fn handle_agents_command(&mut self, args: String) {
        if !args.trim().is_empty() {
            self.history_push_plain_state(history_cell::new_error_event(
                "Usage: /agents".to_owned(),
            ));
        }
        self.show_settings_overlay(Some(SettingsSection::Agents));
    }

    pub(crate) fn handle_limits_command(&mut self, args: String) {
        if !args.trim().is_empty() {
            self.history_push_plain_state(history_cell::new_error_event(
                "Usage: /limits".to_owned(),
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
                self.flash_footer_notice("Loading memories status…");
                self.app_event_tx.send(AppEvent::RunMemoriesStatusLoad {
                    target: crate::app_event::MemoriesStatusLoadTarget::SlashCommand,
                });
            }
            "refresh" => {
                self.flash_footer_notice("Refreshing memories artifacts…");
                self.app_event_tx.send(AppEvent::RunMemoriesArtifactsAction {
                    action: crate::app_event::MemoriesArtifactsAction::Refresh,
                });
            }
            "clear" => {
                self.flash_footer_notice("Clearing generated memories artifacts…");
                self.app_event_tx.send(AppEvent::RunMemoriesArtifactsAction {
                    action: crate::app_event::MemoriesArtifactsAction::Clear,
                });
            }
            _ => {
                self.history_push_plain_state(history_cell::new_error_event(
                    "Usage: /memories [status|refresh|clear|settings]".to_owned(),
                ));
            }
        }
    }

}
