use super::prelude::*;
use code_protocol::num_format::format_with_separators_u64;

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

    pub(in super::super) fn usage_cost_usd_from_totals(totals: &TokenTotals) -> f64 {
        let non_cached_input = totals
            .input_tokens
            .saturating_sub(totals.cached_input_tokens);
        let input_cost = (non_cached_input as f64 / TOKENS_PER_MILLION)
            * INPUT_COST_PER_MILLION_USD;
        let cached_cost = (totals.cached_input_tokens as f64 / TOKENS_PER_MILLION)
            * CACHED_INPUT_COST_PER_MILLION_USD;
        let output_cost = (totals.output_tokens as f64 / TOKENS_PER_MILLION)
            * OUTPUT_COST_PER_MILLION_USD;
        input_cost + cached_cost + output_cost
    }

    pub(in super::super) fn format_usd(amount: f64) -> String {
        let cents = (amount * 100.0).round().max(0.0);
        let cents_u128 = cents as u128;
        let dollars_u128 = cents_u128 / 100;
        let cents_part = (cents_u128 % 100) as u8;
        let dollars = dollars_u128.min(i64::MAX as u128) as i64;
        if cents_part == 0 {
            format!("${} USD", format_with_separators(dollars))
        } else {
            format!(
                "${}.{:02} USD",
                format_with_separators(dollars),
                cents_part
            )
        }
    }

    pub(in super::super) fn accumulate_token_totals(target: &mut TokenTotals, delta: &TokenTotals) {
        target.input_tokens = target
            .input_tokens
            .saturating_add(delta.input_tokens);
        target.cached_input_tokens = target
            .cached_input_tokens
            .saturating_add(delta.cached_input_tokens);
        target.output_tokens = target
            .output_tokens
            .saturating_add(delta.output_tokens);
        target.reasoning_output_tokens = target
            .reasoning_output_tokens
            .saturating_add(delta.reasoning_output_tokens);
        target.total_tokens = target
            .total_tokens
            .saturating_add(delta.total_tokens);
    }

    pub(in super::super) fn account_header_lines(
        account: Option<&StoredAccount>,
        record: Option<&StoredRateLimitSnapshot>,
        usage: Option<&StoredUsageSummary>,
    ) -> Vec<RtLine<'static>> {
        let mut lines: Vec<RtLine<'static>> = Vec::new();

        let account_type = account
            .map(|acc| match acc.mode {
                AuthMode::ChatGPT | AuthMode::ChatgptAuthTokens => "ChatGPT account",
                AuthMode::ApiKey => "API key",
            })
            .unwrap_or("Unknown account");

        let plan = record
            .and_then(|r| r.plan.as_deref())
            .or_else(|| usage.and_then(|u| u.plan.as_deref()))
            .unwrap_or("Unknown");

        let value_style = Style::default().fg(crate::colors::text_dim());
        let is_api_key = matches!(account.map(|acc| acc.mode), Some(AuthMode::ApiKey));
        let totals = usage
            .map(|u| u.totals.clone())
            .unwrap_or_default();
        let non_cached_input = totals
            .input_tokens
            .saturating_sub(totals.cached_input_tokens);
        let cached_input = totals.cached_input_tokens;
        let output_tokens = totals.output_tokens;
        let reasoning_tokens = totals.reasoning_output_tokens;
        let total_tokens = totals.total_tokens;

        let cost_usd = Self::usage_cost_usd_from_totals(&totals);
        let formatted_total = format_with_separators_u64(total_tokens);
        let formatted_cost = Self::format_usd(cost_usd);
        let cost_suffix = if is_api_key {
            format!("({formatted_cost})")
        } else {
            format!("(API would cost {formatted_cost})")
        };

        lines.push(RtLine::from(String::new()));

        lines.push(RtLine::from(vec![
            RtSpan::raw(status_field_prefix("Type")),
            RtSpan::styled(account_type.to_string(), value_style),
        ]));
        lines.push(RtLine::from(vec![
            RtSpan::raw(status_field_prefix("Plan")),
            RtSpan::styled(plan.to_string(), value_style),
        ]));
        let tokens_prefix = status_field_prefix("Tokens");
        let tokens_summary = format!("{formatted_total} total {cost_suffix}");
        lines.push(RtLine::from(vec![
            RtSpan::raw(tokens_prefix.clone()),
            RtSpan::styled(tokens_summary, value_style),
        ]));

        let indent = " ".repeat(tokens_prefix.len());
        let counts = [
            (format_with_separators_u64(cached_input), "cached"),
            (format_with_separators_u64(non_cached_input), "input"),
            (format_with_separators_u64(output_tokens), "output"),
            (format_with_separators_u64(reasoning_tokens), "reasoning"),
        ];
        let max_width = counts
            .iter()
            .map(|(count, _)| count.len())
            .max()
            .unwrap_or(0);
        for (count, label) in counts.iter() {
            let number = format!("{count:>max_width$}");
            lines.push(RtLine::from(vec![
                RtSpan::raw(indent.clone()),
                RtSpan::styled(number, value_style),
                RtSpan::styled(format!(" {label}"), value_style),
            ]));
        }
        lines
    }

    pub(in super::super) fn hourly_usage_lines(
        summary: Option<&StoredUsageSummary>,
        is_api_key_account: bool,
    ) -> Vec<RtLine<'static>> {
        const WIDTH: usize = 14;
        let now = Local::now();
        let anchor = now
            - ChronoDuration::minutes(now.minute() as i64)
            - ChronoDuration::seconds(now.second() as i64)
            - ChronoDuration::nanoseconds(now.nanosecond() as i64);

        let hourly_totals = Self::aggregate_hourly_totals(summary);
        let series: Vec<(DateTime<Local>, TokenTotals)> = (0..12)
            .map(|offset| anchor - ChronoDuration::hours(offset as i64))
            .map(|dt| {
                let utc_key = Self::truncate_utc_hour(dt.with_timezone(&Utc));
                let totals = hourly_totals
                    .get(&utc_key)
                    .cloned()
                    .unwrap_or_default();
                (dt, totals)
            })
            .collect();

        let max_total = series
            .iter()
            .map(|(_, totals)| totals.total_tokens)
            .max()
            .unwrap_or(0);

        let mut lines: Vec<RtLine<'static>> = Vec::new();
        lines.push(RtLine::from(vec![RtSpan::styled(
            "12 Hour History",
            Style::default().add_modifier(Modifier::BOLD),
        )]));

        let prefix = status_content_prefix();
        let tokens_width = series
            .iter()
            .map(|(_, totals)| format_with_separators_u64(totals.total_tokens).len())
            .max()
            .unwrap_or(0);
        let cached_width = series
            .iter()
            .map(|(_, totals)| format_with_separators_u64(totals.cached_input_tokens).len())
            .max()
            .unwrap_or(0);
        let cost_width = series
            .iter()
            .map(|(_, totals)| Self::format_usd(Self::usage_cost_usd_from_totals(totals)).len())
            .max()
            .unwrap_or(0);
        let column_divider = RtSpan::styled(
            " │ ",
            Style::default().fg(crate::colors::text_dim()),
        );
        for (dt, totals) in series.iter() {
            let label = Self::format_hour_label(*dt);
            let bar = Self::bar_segment(totals.total_tokens, max_total, WIDTH);
            let tokens = format_with_separators_u64(totals.total_tokens);
            let padding = tokens_width.saturating_sub(tokens.len());
            let formatted_tokens = format!("{space}{tokens}", space = " ".repeat(padding), tokens = tokens);
            let cached_tokens = format_with_separators_u64(totals.cached_input_tokens);
            let cached_padding = cached_width.saturating_sub(cached_tokens.len());
            let cached_display = format!(
                "{space}{cached_tokens}",
                space = " ".repeat(cached_padding),
                cached_tokens = cached_tokens
            );
            let cost_text = Self::format_usd(Self::usage_cost_usd_from_totals(totals));
            let cost_display = if is_api_key_account {
                format!(
                    "{space}{cost_text}",
                    space = " ".repeat(cost_width.saturating_sub(cost_text.len())),
                    cost_text = cost_text
                )
            } else {
                let saved = Self::format_usd(Self::usage_cost_usd_from_totals(totals));
                format!(
                    "{space}{saved}",
                    space = " ".repeat(cost_width.saturating_sub(saved.len())),
                    saved = saved
                )
            };
            lines.push(RtLine::from(vec![
                RtSpan::raw(prefix.clone()),
                RtSpan::styled(
                    format!("{label} "),
                    Style::default().fg(crate::colors::text_dim()),
                ),
                RtSpan::styled("│ ", Style::default().fg(crate::colors::text_dim())),
                RtSpan::styled(bar, Style::default().fg(crate::colors::primary())),
                RtSpan::raw(format!(" {formatted_tokens} tokens")),
                column_divider.clone(),
                RtSpan::styled(
                    format!("{cached_display} cached"),
                    Style::default().fg(crate::colors::text_dim()),
                ),
                column_divider.clone(),
                RtSpan::styled(
                    format!(
                        "{cost_display} {}",
                        if is_api_key_account { "cost" } else { "saved" }
                    ),
                    Style::default().fg(crate::colors::text_dim()),
                ),
            ]));
        }
        lines
    }

    pub(in super::super) fn daily_usage_lines(
        summary: Option<&StoredUsageSummary>,
        is_api_key_account: bool,
    ) -> Vec<RtLine<'static>> {
        const WIDTH: usize = 14;
        let today = Local::now().date_naive();
        let day_totals = Self::aggregate_daily_totals(summary);
        let daily: Vec<(chrono::NaiveDate, TokenTotals)> = (0..7)
            .map(|offset| today - ChronoDuration::days(offset as i64))
            .map(|day| {
                let totals = day_totals.get(&day).cloned().unwrap_or_default();
                (day, totals)
            })
            .collect();

        let max_total = daily
            .iter()
            .map(|(_, totals)| totals.total_tokens)
            .max()
            .unwrap_or(0);
        let mut lines: Vec<RtLine<'static>> = Vec::new();
        lines.push(Self::dim_line(String::new()));
        lines.push(RtLine::from(vec![RtSpan::styled(
            "7 Day History",
            Style::default().add_modifier(Modifier::BOLD),
        )]));
        let prefix = status_content_prefix();
        let tokens_width = daily
            .iter()
            .map(|(_, totals)| format_with_separators_u64(totals.total_tokens).len())
            .max()
            .unwrap_or(0);
        let cached_width = daily
            .iter()
            .map(|(_, totals)| format_with_separators_u64(totals.cached_input_tokens).len())
            .max()
            .unwrap_or(0);
        let cost_width = daily
            .iter()
            .map(|(_, totals)| Self::format_usd(Self::usage_cost_usd_from_totals(totals)).len())
            .max()
            .unwrap_or(0);
        let column_divider = RtSpan::styled(
            " │ ",
            Style::default().fg(crate::colors::text_dim()),
        );
        for (day, totals) in daily.iter() {
            let label = Self::format_daily_label(*day);
            let bar = Self::bar_segment(totals.total_tokens, max_total, WIDTH);
            let tokens = format_with_separators_u64(totals.total_tokens);
            let padding = tokens_width.saturating_sub(tokens.len());
            let formatted_tokens = format!("{space}{tokens}", space = " ".repeat(padding), tokens = tokens);
            let cached_tokens = format_with_separators_u64(totals.cached_input_tokens);
            let cached_padding = cached_width.saturating_sub(cached_tokens.len());
            let cached_display = format!(
                "{space}{cached_tokens}",
                space = " ".repeat(cached_padding),
                cached_tokens = cached_tokens
            );
            let daily_cost = Self::usage_cost_usd_from_totals(totals);
            let cost_text = Self::format_usd(daily_cost);
            let cost_display = if is_api_key_account {
                format!(
                    "{space}{cost_text}",
                    space = " ".repeat(cost_width.saturating_sub(cost_text.len())),
                    cost_text = cost_text
                )
            } else {
                let saved = Self::format_usd(Self::usage_cost_usd_from_totals(totals));
                format!(
                    "{space}{saved}",
                    space = " ".repeat(cost_width.saturating_sub(saved.len())),
                    saved = saved
                )
            };
            lines.push(RtLine::from(vec![
                RtSpan::raw(prefix.clone()),
                RtSpan::styled(
                    format!("{label} "),
                    Style::default().fg(crate::colors::text_dim()),
                ),
                RtSpan::styled("│ ", Style::default().fg(crate::colors::text_dim())),
                RtSpan::styled(bar, Style::default().fg(crate::colors::primary())),
                RtSpan::raw(format!(" {formatted_tokens} tokens")),
                column_divider.clone(),
                RtSpan::styled(
                    format!("{cached_display} cached"),
                    Style::default().fg(crate::colors::text_dim()),
                ),
                column_divider.clone(),
                RtSpan::styled(
                    format!(
                        "{cost_display} {}",
                        if is_api_key_account { "cost" } else { "saved" }
                    ),
                    Style::default().fg(crate::colors::text_dim()),
                ),
            ]));
        }
        lines
    }

    pub(in super::super) fn day_suffix(day: u32) -> &'static str {
        if (11..=13).contains(&(day % 100)) {
            return "th";
        }
        match day % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    }

    pub(in super::super) fn format_daily_label(date: chrono::NaiveDate) -> String {
        let suffix = Self::day_suffix(date.day());
        format!("{} {:>2}{}", date.format("%b"), date.day(), suffix)
    }

    pub(in super::super) fn format_hour_label(dt: DateTime<Local>) -> String {
        let (is_pm, hour) = dt.hour12();
        let meridiem = if is_pm { "pm" } else { "am" };
        format!("{} {:>2}{}", dt.format("%a"), hour, meridiem)
    }

    pub(in super::super) fn usage_history_lines(
        summary: Option<&StoredUsageSummary>,
        is_api_key_account: bool,
    ) -> Vec<RtLine<'static>> {
        let mut lines = Self::hourly_usage_lines(summary, is_api_key_account);
        lines.extend(Self::daily_usage_lines(summary, is_api_key_account));
        lines.extend(Self::six_month_usage_lines(summary, is_api_key_account));
        lines
    }

    pub(in super::super) fn six_month_usage_lines(
        summary: Option<&StoredUsageSummary>,
        is_api_key_account: bool,
    ) -> Vec<RtLine<'static>> {
        const WIDTH: usize = 14;
        const MONTHS: usize = 6;

        let today = Local::now().date_naive();
        let mut year = today.year();
        let mut month = today.month();

        let month_totals = Self::aggregate_monthly_totals(summary);
        let mut months: Vec<(chrono::NaiveDate, TokenTotals)> = Vec::with_capacity(MONTHS);
        for _ in 0..MONTHS {
            let Some(start) = chrono::NaiveDate::from_ymd_opt(year, month, 1) else {
                break;
            };
            let key = (start.year(), start.month());
            let totals = month_totals
                .get(&key)
                .cloned()
                .unwrap_or_default();
            months.push((start, totals));
            if month == 1 {
                month = 12;
                year -= 1;
            } else {
                month -= 1;
            }
        }

        let max_total = months
            .iter()
            .map(|(_, totals)| totals.total_tokens)
            .max()
            .unwrap_or(0);

        let mut lines: Vec<RtLine<'static>> = Vec::new();
        lines.push(Self::dim_line(String::new()));
        lines.push(RtLine::from(vec![RtSpan::styled(
            "6 Month History",
            Style::default().add_modifier(Modifier::BOLD),
        )]));

        let prefix = status_content_prefix();
        let tokens_width = months
            .iter()
            .map(|(_, totals)| format_with_separators_u64(totals.total_tokens).len())
            .max()
            .unwrap_or(0);
        let cached_width = months
            .iter()
            .map(|(_, totals)| format_with_separators_u64(totals.cached_input_tokens).len())
            .max()
            .unwrap_or(0);
        let cost_width = months
            .iter()
            .map(|(_, totals)| Self::format_usd(Self::usage_cost_usd_from_totals(totals)).len())
            .max()
            .unwrap_or(0);
        let column_divider = RtSpan::styled(
            " │ ",
            Style::default().fg(crate::colors::text_dim()),
        );
        for (start, totals) in months.iter() {
            let label = start.format("%b %Y").to_string();
            let bar = Self::bar_segment(totals.total_tokens, max_total, WIDTH);
            let tokens = format_with_separators_u64(totals.total_tokens);
            let padding = tokens_width.saturating_sub(tokens.len());
            let formatted_tokens = format!("{space}{tokens}", space = " ".repeat(padding), tokens = tokens);
            let cached_tokens = format_with_separators_u64(totals.cached_input_tokens);
            let cached_padding = cached_width.saturating_sub(cached_tokens.len());
            let cached_display = format!(
                "{space}{cached_tokens}",
                space = " ".repeat(cached_padding),
                cached_tokens = cached_tokens
            );
            let cost_text = Self::format_usd(Self::usage_cost_usd_from_totals(totals));
            let cost_display = if is_api_key_account {
                format!(
                    "{space}{cost_text}",
                    space = " ".repeat(cost_width.saturating_sub(cost_text.len())),
                    cost_text = cost_text
                )
            } else {
                let saved = Self::format_usd(Self::usage_cost_usd_from_totals(totals));
                format!(
                    "{space}{saved}",
                    space = " ".repeat(cost_width.saturating_sub(saved.len())),
                    saved = saved
                )
            };
            lines.push(RtLine::from(vec![
                RtSpan::raw(prefix.clone()),
                RtSpan::styled(
                    format!("{label} "),
                    Style::default().fg(crate::colors::text_dim()),
                ),
                RtSpan::styled("│ ", Style::default().fg(crate::colors::text_dim())),
                RtSpan::styled(bar, Style::default().fg(crate::colors::primary())),
                RtSpan::raw(format!(" {formatted_tokens} tokens")),
                column_divider.clone(),
                RtSpan::styled(
                    format!("{cached_display} cached"),
                    Style::default().fg(crate::colors::text_dim()),
                ),
                column_divider.clone(),
                RtSpan::styled(
                    format!(
                        "{cost_display} {}",
                        if is_api_key_account { "cost" } else { "saved" }
                    ),
                    Style::default().fg(crate::colors::text_dim()),
                ),
            ]));
        }
        lines
    }

    pub(in super::super) fn bar_segment(value: u64, max: u64, width: usize) -> String {
        const FILL: &str = "▇";
        if max == 0 {
            return format!("{}{}", FILL, " ".repeat(width.saturating_sub(1)));
        }
        if value == 0 {
            return format!("{}{}", FILL, " ".repeat(width.saturating_sub(1)));
        }
        let ratio = value as f64 / max as f64;
        let filled = (ratio * width as f64).ceil().clamp(1.0, width as f64) as usize;
        format!(
            "{}{}",
            FILL.repeat(filled),
            " ".repeat(width.saturating_sub(filled))
        )
    }

    pub(in super::super) fn dim_line(text: impl Into<String>) -> RtLine<'static> {
        RtLine::from(vec![RtSpan::styled(
            text.into(),
            Style::default().fg(crate::colors::text_dim()),
        )])
    }

    pub(in super::super) fn truncate_utc_hour(ts: DateTime<Utc>) -> DateTime<Utc> {
        let naive = ts.naive_utc();
        let Some(trimmed) = naive
            .with_minute(0)
            .and_then(|dt| dt.with_second(0))
            .and_then(|dt| dt.with_nanosecond(0))
        else {
            return ts;
        };
        Utc.from_utc_datetime(&trimmed)
    }

    pub(in super::super) fn aggregate_hourly_totals(
        summary: Option<&StoredUsageSummary>,
    ) -> HashMap<DateTime<Utc>, TokenTotals> {
        let mut totals = HashMap::new();
        if let Some(summary) = summary {
            for entry in &summary.hourly_entries {
                let key = Self::truncate_utc_hour(entry.timestamp);
                let slot = totals.entry(key).or_insert_with(TokenTotals::default);
                Self::accumulate_token_totals(slot, &entry.tokens);
            }
            for bucket in &summary.hourly_buckets {
                let slot = totals
                    .entry(bucket.period_start)
                    .or_insert_with(TokenTotals::default);
                Self::accumulate_token_totals(slot, &bucket.tokens);
            }
        }
        totals
    }

    pub(in super::super) fn aggregate_daily_totals(
        summary: Option<&StoredUsageSummary>,
    ) -> HashMap<chrono::NaiveDate, TokenTotals> {
        let mut totals = HashMap::new();
        if let Some(summary) = summary {
            for bucket in &summary.daily_buckets {
                let key = bucket.period_start.date_naive();
                let slot = totals.entry(key).or_insert_with(TokenTotals::default);
                Self::accumulate_token_totals(slot, &bucket.tokens);
            }
            for bucket in &summary.hourly_buckets {
                let key = bucket.period_start.date_naive();
                let slot = totals.entry(key).or_insert_with(TokenTotals::default);
                Self::accumulate_token_totals(slot, &bucket.tokens);
            }
            for entry in &summary.hourly_entries {
                let key = entry.timestamp.date_naive();
                let slot = totals.entry(key).or_insert_with(TokenTotals::default);
                Self::accumulate_token_totals(slot, &entry.tokens);
            }
        }
        totals
    }

    pub(in super::super) fn aggregate_monthly_totals(
        summary: Option<&StoredUsageSummary>,
    ) -> HashMap<(i32, u32), TokenTotals> {
        let mut totals = HashMap::new();
        if let Some(summary) = summary {
            let mut accumulate = |dt: DateTime<Utc>, tokens: &TokenTotals| {
                let date = dt.date_naive();
                let key = (date.year(), date.month());
                let slot = totals.entry(key).or_insert_with(TokenTotals::default);
                Self::accumulate_token_totals(slot, tokens);
            };

            for bucket in &summary.monthly_buckets {
                accumulate(bucket.period_start, &bucket.tokens);
            }
            for bucket in &summary.daily_buckets {
                accumulate(bucket.period_start, &bucket.tokens);
            }
            for bucket in &summary.hourly_buckets {
                accumulate(bucket.period_start, &bucket.tokens);
            }
            for entry in &summary.hourly_entries {
                accumulate(entry.timestamp, &entry.tokens);
            }
        }
        totals
    }
}
