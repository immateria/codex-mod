impl ChatWidget<'_> {
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
}
