impl ChatWidget<'_> {
    pub(in super::super) fn set_limits_overlay_content(&mut self, content: LimitsOverlayContent) {
        let handled_by_settings = self.update_limits_settings_content(content.clone());
        if handled_by_settings {
            self.limits.cached_content = None;
        } else {
            self.limits.cached_content = Some(content);
        }
    }

    pub(in super::super) fn update_limits_settings_content(&mut self, content: LimitsOverlayContent) -> bool {
        if let Some(overlay) = self.settings.overlay.as_mut() {
            if let Some(view) = overlay.limits_content_mut() {
                view.set_content(content);
            } else {
                overlay.set_limits_content(LimitsSettingsContent::new(
                    content,
                    self.config.tui.limits.layout_mode,
                ));
            }
            self.request_redraw();
            true
        } else {
            false
        }
    }

    pub(in super::super) fn set_limits_overlay_tabs(&mut self, tabs: Vec<LimitsTab>) {
        let content = if tabs.is_empty() {
            LimitsOverlayContent::Placeholder
        } else {
            LimitsOverlayContent::Tabs(tabs)
        };
        self.set_limits_overlay_content(content);
    }

    pub(in super::super) fn build_limits_tabs(
        &self,
        current_snapshot: Option<RateLimitSnapshotEvent>,
        current_reset: RateLimitResetInfo,
    ) -> Vec<LimitsTab> {
        use std::collections::HashSet;

        let code_home = self.config.code_home.clone();
        let accounts = auth_accounts::list_accounts(&code_home).unwrap_or_default();
        let account_map: HashMap<String, StoredAccount> = accounts
            .into_iter()
            .map(|account| (account.id.clone(), account))
            .collect();

        let active_id = auth_accounts::get_active_account_id(&code_home)
            .ok()
            .flatten();

        let usage_records = account_usage::list_rate_limit_snapshots(&code_home).unwrap_or_default();
        let mut snapshot_map: HashMap<String, StoredRateLimitSnapshot> = usage_records
            .into_iter()
            .filter(|record| account_map.contains_key(&record.account_id))
            .map(|record| (record.account_id.clone(), record))
            .collect();

        let mut usage_summary_map: HashMap<String, StoredUsageSummary> = HashMap::new();
        for id in account_map.keys() {
            if let Ok(Some(summary)) = account_usage::load_account_usage(&code_home, id) {
                usage_summary_map.insert(id.clone(), summary);
            }
        }

        if let Some(active_id) = active_id.as_ref()
            && !usage_summary_map.contains_key(active_id)
                && let Ok(Some(summary)) = account_usage::load_account_usage(&code_home, active_id) {
                    usage_summary_map.insert(active_id.clone(), summary);
                }

        let mut tabs: Vec<LimitsTab> = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();

        if let Some(snapshot) = current_snapshot {
            let account_ref = active_id
                .as_ref()
                .and_then(|id| account_map.get(id));
            let snapshot_ref = active_id
                .as_ref()
                .and_then(|id| snapshot_map.get(id));
            let summary_ref = active_id
                .as_ref()
                .and_then(|id| usage_summary_map.get(id));

            let title = account_ref
                .map(account_display_label)
                .or_else(|| active_id.clone())
                .unwrap_or_else(|| "Current session".to_string());
            let header = Self::account_header_lines(account_ref, snapshot_ref, summary_ref);
            let is_api_key_account = matches!(
                account_ref.map(|acc| acc.mode),
                Some(AuthMode::ApiKey)
            );
            let extra = Self::usage_history_lines(summary_ref, is_api_key_account);
            let display = Self::rate_limit_display_config_for_account(account_ref);
            let view = build_limits_view(
                &snapshot,
                current_reset,
                DEFAULT_GRID_CONFIG,
                display,
            );
            tabs.push(LimitsTab::view(title, header, view, extra));

            if let Some(active_id) = active_id.as_ref()
                && account_map.contains_key(active_id) {
                    seen_ids.insert(active_id.clone());
                    snapshot_map.remove(active_id);
                    usage_summary_map.remove(active_id);
                }
        }

        let mut remaining_ids: Vec<String> = account_map
            .keys()
            .filter(|id| !seen_ids.contains(*id))
            .cloned()
            .collect();

        let account_sort_key = |id: &String| {
            if let Some(account) = account_map.get(id) {
                let label = account_display_label(account);
                (
                    account_mode_priority(account.mode),
                    label.to_ascii_lowercase(),
                    label,
                )
            } else {
                (u8::MAX, id.to_ascii_lowercase(), id.clone())
            }
        };

        remaining_ids.sort_by(|a, b| {
            let (a_priority, a_lower, a_label) = account_sort_key(a);
            let (b_priority, b_lower, b_label) = account_sort_key(b);
            a_priority
                .cmp(&b_priority)
                .then_with(|| a_lower.cmp(&b_lower))
                .then_with(|| a_label.cmp(&b_label))
                .then_with(|| a.cmp(b))
        });

        for id in remaining_ids {
            let account = account_map.get(&id);
            let record = snapshot_map.remove(&id);
            let usage_summary = usage_summary_map.remove(&id);
            let title = account
                .map(account_display_label)
                .unwrap_or_else(|| id.clone());
            match record {
                Some(record) => {
                    if let Some(snapshot) = record.snapshot.clone() {
                        let view_snapshot = snapshot.clone();
                        let view_reset = RateLimitResetInfo {
                            primary_next_reset: record.primary_next_reset_at,
                            secondary_next_reset: record.secondary_next_reset_at,
                            ..RateLimitResetInfo::default()
                        };
                        let display = Self::rate_limit_display_config_for_account(account);
                        let view = build_limits_view(
                            &view_snapshot,
                            view_reset,
                            DEFAULT_GRID_CONFIG,
                            display,
                        );
                        let header = Self::account_header_lines(
                            account,
                            Some(&record),
                            usage_summary.as_ref(),
                        );
                        let is_api_key_account = matches!(
                            account.map(|acc| acc.mode),
                            Some(AuthMode::ApiKey)
                        );
                        let extra = Self::usage_history_lines(
                            usage_summary.as_ref(),
                            is_api_key_account,
                        );
                        tabs.push(LimitsTab::view(title, header, view, extra));
                    } else {
                        let is_api_key_account = matches!(
                            account.map(|acc| acc.mode),
                            Some(AuthMode::ApiKey)
                        );
                        let mut lines = Self::usage_history_lines(
                            usage_summary.as_ref(),
                            is_api_key_account,
                        );
                        lines.push(Self::dim_line(
                            " Rate limit snapshot not yet available.",
                        ));
                        let header = Self::account_header_lines(
                            account,
                            Some(&record),
                            usage_summary.as_ref(),
                        );
                        tabs.push(LimitsTab::message(title, header, lines));
                    }
                }
                None => {
                    let is_api_key_account = matches!(
                        account.map(|acc| acc.mode),
                        Some(AuthMode::ApiKey)
                    );
                    let mut lines = Self::usage_history_lines(
                        usage_summary.as_ref(),
                        is_api_key_account,
                    );
                    lines.push(Self::dim_line(
                        " Rate limit snapshot not yet available.",
                    ));
                    let header = Self::account_header_lines(
                        account,
                        None,
                        usage_summary.as_ref(),
                    );
                    tabs.push(LimitsTab::message(title, header, lines));
                }
            }
        }

        if tabs.is_empty() {
            let mut lines = Self::usage_history_lines(None, false);
            lines.push(Self::dim_line(
                " Rate limit snapshot not yet available.",
            ));
            tabs.push(LimitsTab::message("Usage", Vec::new(), lines));
        }

        tabs
    }
}
