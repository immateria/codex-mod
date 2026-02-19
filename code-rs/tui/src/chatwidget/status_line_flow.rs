use super::*;
use code_core::config_types::StatusLineLane;
use code_protocol::num_format::format_si_suffix;

impl ChatWidget<'_> {
    pub(crate) fn open_status_line_setup_from_args(&mut self, args: &str) -> Result<(), String> {
        let token = args
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();

        let primary_lane = self.status_line_primary_lane();
        let requested_lane = match token.as_str() {
            "" | "primary" => primary_lane,
            "secondary" => {
                let secondary = Self::opposite_status_line_lane(primary_lane);
                if !self.status_line_lane_enabled(secondary) {
                    self.bottom_pane.flash_footer_notice(format!(
                        "Secondary status line ({}) is currently empty. Add items to enable it.",
                        Self::status_line_lane_label(secondary),
                    ));
                }
                secondary
            }
            "top" => StatusLineLane::Top,
            "bottom" => StatusLineLane::Bottom,
            _ => {
                return Err(
                    "Usage: /statusline [primary|secondary|top|bottom]".to_string(),
                );
            }
        };

        self.open_status_line_setup(requested_lane);
        Ok(())
    }

    pub(crate) fn open_status_line_setup(&mut self, initial_lane: StatusLineLane) {
        let view = StatusLineSetupView::new(
            self.config
                .tui
                .status_line_top
                .as_deref()
                .or(self.config.tui.status_line.as_deref()),
            self.config.tui.status_line_bottom.as_deref(),
            self.status_line_primary_lane(),
            initial_lane,
            self.app_event_tx.clone(),
        );
        self.bottom_pane.show_status_line_setup(view);
        self.request_redraw();
    }

    pub(crate) fn setup_status_line(
        &mut self,
        top_items: Vec<StatusLineItem>,
        bottom_items: Vec<StatusLineItem>,
        primary: StatusLineLane,
    ) {
        self.bottom_pane
            .set_force_top_spacer(!bottom_items.is_empty());

        let top_ids = top_items.iter().map(ToString::to_string).collect::<Vec<_>>();
        let bottom_ids = bottom_items
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        self.config.tui.status_line_top = if top_ids.is_empty() {
            None
        } else {
            Some(top_ids)
        };
        self.config.tui.status_line_bottom = if bottom_ids.is_empty() {
            None
        } else {
            Some(bottom_ids)
        };
        self.config.tui.status_line_primary = primary;
        self.config.tui.status_line = self.config.tui.status_line_top.clone();

        if top_items.is_empty() && bottom_items.is_empty() {
            self.bottom_pane
                .flash_footer_notice("Status line reset to default layout".to_string());
        } else {
            self.bottom_pane.flash_footer_notice(format!(
                "Status line updated (top {}, bottom {}, primary {})",
                top_items.len(),
                bottom_items.len(),
                Self::status_line_lane_label(primary),
            ));
        }
        self.request_redraw();
    }

    pub(crate) fn cancel_status_line_setup(&mut self) {
        self.bottom_pane
            .flash_footer_notice("Status line setup cancelled".to_string());
        self.request_redraw();
    }

    pub(super) fn status_line_primary_lane(&self) -> StatusLineLane {
        self.config.tui.status_line_primary
    }

    fn status_line_lane_enabled(&self, lane: StatusLineLane) -> bool {
        match lane {
            StatusLineLane::Top => self.config.tui.header.show_top_line,
            StatusLineLane::Bottom => !self.status_line_bottom_items().is_empty(),
        }
    }

    pub(super) fn status_line_lane_label(lane: StatusLineLane) -> &'static str {
        match lane {
            StatusLineLane::Top => "top",
            StatusLineLane::Bottom => "bottom",
        }
    }

    fn opposite_status_line_lane(lane: StatusLineLane) -> StatusLineLane {
        match lane {
            StatusLineLane::Top => StatusLineLane::Bottom,
            StatusLineLane::Bottom => StatusLineLane::Top,
        }
    }

    pub(super) fn status_line_top_items(&self) -> Vec<StatusLineItem> {
        use std::collections::HashSet;

        let ids = self
            .config
            .tui
            .status_line_top
            .as_ref()
            .or(self.config.tui.status_line.as_ref());
        let Some(ids) = ids else {
            return Vec::new();
        };

        let mut seen = HashSet::<String>::new();
        let mut items = Vec::new();
        for id in ids {
            let Ok(item) = id.parse::<StatusLineItem>() else {
                continue;
            };
            let key = item.to_string();
            if !seen.insert(key) {
                continue;
            }
            items.push(item);
        }
        items
    }

    pub(super) fn status_line_bottom_items(&self) -> Vec<StatusLineItem> {
        use std::collections::HashSet;

        let Some(ids) = self.config.tui.status_line_bottom.as_ref() else {
            return Vec::new();
        };

        let mut seen = HashSet::<String>::new();
        let mut items = Vec::new();
        for id in ids {
            let Ok(item) = id.parse::<StatusLineItem>() else {
                continue;
            };
            let key = item.to_string();
            if !seen.insert(key) {
                continue;
            }
            items.push(item);
        }
        items
    }

    pub(super) fn status_line_value_for_item(&self, item: StatusLineItem) -> Option<String> {
        match item {
            StatusLineItem::ModelName => Some(self.format_model_name(&self.config.model)),
            StatusLineItem::ModelWithReasoning => Some(format!(
                "{} {}",
                self.format_model_name(&self.config.model),
                Self::format_reasoning_effort(self.config.model_reasoning_effort)
            )),
            StatusLineItem::CurrentDir => Some(Self::status_line_format_cwd(&self.config.cwd)),
            StatusLineItem::ProjectRoot => code_core::git_info::get_git_repo_root(&self.config.cwd)
                .map(|root| {
                    root.file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| root.display().to_string())
                }),
            StatusLineItem::GitBranch => self.get_git_branch(),
            StatusLineItem::ContextRemaining => self
                .status_line_context_remaining_percent()
                .map(|remaining| format!("{remaining}% left")),
            StatusLineItem::ContextUsed => self
                .status_line_context_used_percent()
                .map(|used| format!("{used}% used")),
            StatusLineItem::FiveHourLimit => self.rate_limit_snapshot.as_ref().map(|snapshot| {
                format!(
                    "{} {}%",
                    Self::format_window_minutes(snapshot.primary_window_minutes),
                    snapshot.primary_used_percent.clamp(0.0, 100.0).round() as i64
                )
            }),
            StatusLineItem::WeeklyLimit => self.rate_limit_snapshot.as_ref().map(|snapshot| {
                format!(
                    "weekly {}%",
                    snapshot.secondary_used_percent.clamp(0.0, 100.0).round() as i64
                )
            }),
            StatusLineItem::CodexVersion => Some(format!("v{}", code_version::version())),
            StatusLineItem::ContextWindowSize => self.config.model_context_window.map(|size| {
                format!("{} window", format_si_suffix(size.min(i64::MAX as u64) as i64))
            }),
            StatusLineItem::UsedTokens => {
                let total = self.total_token_usage.tokens_in_context_window();
                if total == 0 {
                    None
                } else {
                    Some(format!(
                        "{} used",
                        format_si_suffix(total.min(i64::MAX as u64) as i64)
                    ))
                }
            }
            StatusLineItem::TotalInputTokens => Some(format!(
                "{} in",
                format_si_suffix(self.total_token_usage.input_tokens.min(i64::MAX as u64) as i64)
            )),
            StatusLineItem::TotalOutputTokens => Some(format!(
                "{} out",
                format_si_suffix(self.total_token_usage.output_tokens.min(i64::MAX as u64) as i64)
            )),
            StatusLineItem::SessionId => self.session_id.map(|id| id.to_string()),
        }
    }

    fn status_line_context_remaining_percent(&self) -> Option<i64> {
        let context_window = self.config.model_context_window?;
        Some(
            self.last_token_usage
                .percent_of_context_window_remaining(context_window)
                .clamp(0, 100) as i64,
        )
    }

    fn status_line_context_used_percent(&self) -> Option<i64> {
        let remaining = self.status_line_context_remaining_percent()?;
        Some((100 - remaining).clamp(0, 100))
    }

    fn status_line_format_cwd(cwd: &Path) -> String {
        match crate::exec_command::relativize_to_home(cwd) {
            Some(rel) if !rel.as_os_str().is_empty() => format!("~/{}", rel.display()),
            Some(_) => "~".to_string(),
            None => cwd.display().to_string(),
        }
    }

    fn format_window_minutes(minutes: u64) -> String {
        if minutes == 0 {
            return "window".to_string();
        }
        if minutes.is_multiple_of(24 * 60) {
            let days = minutes / (24 * 60);
            return format!("{days}d");
        }
        if minutes.is_multiple_of(60) {
            return format!("{}h", minutes / 60);
        }
        if minutes < 60 {
            return format!("{minutes}m");
        }
        format!("{}h {}m", minutes / 60, minutes % 60)
    }
}
