impl ChatWidget<'_> {
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
