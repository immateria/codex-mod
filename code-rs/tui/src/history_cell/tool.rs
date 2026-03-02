//! Tool call history cells driven by structured argument/result state.

use super::*;
use crate::history::state::{
    ArgumentValue,
    HistoryId,
    RunningToolState,
    ToolArgument,
    ToolCallState,
    ToolStatus as HistoryToolStatus,
};
use crate::text_formatting::format_json_compact;
use serde_json::Value;
use std::cell::Cell;
use std::time::{Duration, Instant, SystemTime};

const TOOL_DETAILS_PREVIEW_ARGS: usize = 2;
const TOOL_DETAILS_PREVIEW_RESULT_LINES: usize = 1;

pub(crate) struct ToolCallCell {
    state: ToolCallState,
    pub(crate) parent_call_id: Option<String>,
    collapsed_details: Cell<bool>,
}

impl ToolCallCell {
    pub(crate) fn new(state: ToolCallState) -> Self {
        let mut state = state;
        // Successful tool calls are often noisy (arguments + result previews). Default
        // to collapsed details so history stays readable; failures remain expanded.
        let collapse_by_default = matches!(state.status, HistoryToolStatus::Success);
        state.id = HistoryId::ZERO;
        Self {
            state,
            parent_call_id: None,
            collapsed_details: Cell::new(collapse_by_default),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn from_state(state: ToolCallState) -> Self {
        let collapse_by_default = matches!(state.status, HistoryToolStatus::Success);
        Self {
            state,
            parent_call_id: None,
            collapsed_details: Cell::new(collapse_by_default),
        }
    }

    pub(crate) fn state(&self) -> &ToolCallState {
        &self.state
    }

    pub(crate) fn state_mut(&mut self) -> &mut ToolCallState {
        &mut self.state
    }

    pub(crate) fn details_collapsed(&self) -> bool {
        self.collapsed_details.get()
    }

    pub(crate) fn set_details_collapsed(&self, collapsed: bool) {
        self.collapsed_details.set(collapsed);
    }

    pub(crate) fn toggle_details_collapsed(&self) {
        self.collapsed_details.set(!self.collapsed_details.get());
    }

    pub(crate) fn retint(&mut self, _old: &crate::theme::Theme, _new: &crate::theme::Theme) {}

    fn header_line(&self) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut style = Style::default().add_modifier(Modifier::BOLD);
        style = match self.state.status {
            HistoryToolStatus::Running => style.fg(crate::colors::info()),
            HistoryToolStatus::Success => style.fg(crate::colors::success()),
            HistoryToolStatus::Failed => style.fg(crate::colors::error()),
        };
        spans.push(Span::styled(self.state.title.clone(), style));
        if let Some(duration) = self.state.duration {
            spans.push(Span::styled(
                format!(", duration: {}", format_duration(duration)),
                Style::default().fg(crate::colors::text_dim()),
            ));
        }

        // When collapsed, append a compact invocation hint so you rarely need to expand.
        if self.details_collapsed() {
            if let Some(hint) = self.compact_invocation_hint() {
                spans.push(Span::styled(
                    format!(" ({hint})"),
                    Style::default().fg(crate::colors::text_dim()),
                ));
            }
            let args_count = self.state.arguments.len();
            let result_count = self.state.result_preview
                .as_ref()
                .map(|r| r.lines.len())
                .unwrap_or(0);
            let total_hidden = args_count.saturating_add(result_count);
            if total_hidden > 0 {
                let mut parts: Vec<String> = Vec::new();
                if args_count > 0 {
                    parts.push(format!("{args_count} arg{}", if args_count == 1 { "" } else { "s" }));
                }
                if result_count > 0 {
                    parts.push(format!("{result_count} result line{}", if result_count == 1 { "" } else { "s" }));
                }
                spans.push(Span::styled(
                    format!(" • {}", parts.join(", ")),
                    Style::default()
                        .fg(crate::colors::text_dim())
                        .add_modifier(Modifier::DIM),
                ));
            }
        }

        Line::from(spans)
    }

    /// Extract a compact one-liner from the first argument value for collapsed display.
    fn compact_invocation_hint(&self) -> Option<String> {
        let arg = self.state.arguments.first()?;
        let raw = match &arg.value {
            ArgumentValue::Text(text) => text.clone(),
            ArgumentValue::Json(json) => {
                format_json_compact(&json.to_string()).unwrap_or_else(|| json.to_string())
            }
            ArgumentValue::Secret => return None,
        };
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() {
            return None;
        }
        // Truncate to ~60 chars for the header
        let first_line = trimmed.lines().next().unwrap_or(&trimmed);
        if first_line.len() > 60 {
            Some(format!("{}…", &first_line[..57]))
        } else if trimmed.lines().count() > 1 {
            Some(format!("{first_line}…"))
        } else {
            Some(first_line.to_string())
        }
    }

    fn result_preview_lines(&self) -> Vec<Line<'static>> {
        let Some(result) = &self.state.result_preview else {
            return Vec::new();
        };
        if result.lines.is_empty() {
            return Vec::new();
        }
        let dim = Style::default().fg(crate::colors::text_dim());
        let mut lines = result
            .lines
            .iter()
            .map(|line| Line::styled(line.clone(), dim))
            .collect::<Vec<_>>();
        if result.truncated {
            lines.push(Line::styled(
                "… truncated ",
                dim,
            ));
        }
        lines
    }

    fn error_lines(&self) -> Vec<Line<'static>> {
        let Some(error) = &self.state.error_message else {
            return Vec::new();
        };
        if error.is_empty() {
            return Vec::new();
        }
        vec![Line::styled(
            error.clone(),
            Style::default().fg(crate::colors::error()),
        )]
    }

    fn expanded_detail_lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.extend(render_arguments(&self.state.arguments));

        let result_lines = self.result_preview_lines();
        if !result_lines.is_empty() {
            lines.push(Line::from(""));
            lines.extend(result_lines);
        }

        let error_lines = self.error_lines();
        if !error_lines.is_empty() {
            lines.push(Line::from(""));
            lines.extend(error_lines);
        }

        lines
    }

    fn collapsed_detail_lines(&self) -> Vec<Line<'static>> {
        super::formatting::fold_sections(
            render_arguments(&self.state.arguments),
            self.result_preview_lines(),
            self.error_lines(),
            &super::formatting::FoldSectionLimits {
                args: TOOL_DETAILS_PREVIEW_ARGS,
                result: TOOL_DETAILS_PREVIEW_RESULT_LINES,
                error: 1,
            },
        )
    }
}

impl HistoryCell for ToolCallCell {
    impl_as_any!();

    fn is_fold_toggleable(&self) -> bool {
        true
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Tool {
            status: super::ToolCellStatus::from(self.state.status),
        }
    }

    fn call_id(&self) -> Option<&str> {
        self.state.call_id.as_deref()
    }

    fn parent_call_id(&self) -> Option<&str> {
        self.parent_call_id.as_deref()
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(self.header_line());
        if self.details_collapsed() {
            lines.extend(self.collapsed_detail_lines());
        } else {
            lines.extend(self.expanded_detail_lines());
        }

        lines.push(Line::from(""));
        lines
    }
}

pub(crate) struct RunningToolCallCell {
    state: RunningToolState,
    start_clock: Instant,
    pub(crate) parent_call_id: Option<String>,
    collapsed_details: Cell<bool>,
}

impl RunningToolCallCell {
    pub(crate) fn new(state: RunningToolState) -> Self {
        let mut state = state;
        state.id = HistoryId::ZERO;
        Self {
            state,
            start_clock: Instant::now(),
            parent_call_id: None,
            collapsed_details: Cell::new(false),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn from_state(state: RunningToolState) -> Self {
        Self {
            state,
            start_clock: Instant::now(),
            parent_call_id: None,
            collapsed_details: Cell::new(false),
        }
    }

    pub(crate) fn state(&self) -> &RunningToolState {
        &self.state
    }

    pub(crate) fn state_mut(&mut self) -> &mut RunningToolState {
        &mut self.state
    }

    pub(crate) fn details_collapsed(&self) -> bool {
        self.collapsed_details.get()
    }

    pub(crate) fn set_details_collapsed(&self, collapsed: bool) {
        self.collapsed_details.set(collapsed);
    }

    pub(crate) fn toggle_details_collapsed(&self) {
        self.collapsed_details.set(!self.collapsed_details.get());
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub(crate) fn override_elapsed_for_testing(&mut self, duration: Duration) {
        if let Some(adjusted) = SystemTime::now().checked_sub(duration) {
            self.state.started_at = adjusted;
        } else {
            self.state.started_at = SystemTime::UNIX_EPOCH;
        }
        self.start_clock = Instant::now();
    }

    fn strip_zero_seconds_suffix(mut duration: String) -> String {
        if duration.ends_with(" 00s") {
            duration.truncate(duration.len().saturating_sub(4));
        }
        duration
    }

    fn compact_duration(duration: Duration) -> String {
        Self::strip_zero_seconds_suffix(format_duration(duration)).replace(' ', "")
    }

    fn spinner_frame(&self) -> &'static str {
        const FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];
        let idx = ((self.start_clock.elapsed().as_millis() / 100) as usize) % FRAMES.len();
        FRAMES[idx]
    }

    fn is_gh_run_wait(&self) -> bool {
        self.state.title == "Gh Run Wait..."
    }

    fn tool_argument_text(&self, name: &str) -> Option<String> {
        self.state
            .arguments
            .iter()
            .find(|arg| arg.name == name)
            .and_then(|arg| match &arg.value {
                ArgumentValue::Text(text) => Some(text.clone()),
                ArgumentValue::Json(json) => {
                    let raw = json.to_string();
                    Some(format_json_compact(&raw).unwrap_or(raw))
                }
                ArgumentValue::Secret => None,
            })
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
    }

    fn tool_argument_json(&self, name: &str) -> Option<Value> {
        self.state
            .arguments
            .iter()
            .find(|arg| arg.name == name)
            .and_then(|arg| match &arg.value {
                ArgumentValue::Json(json) => Some(json.clone()),
                _ => None,
            })
    }

    fn progress_bar(completed: usize, total: usize, width: usize) -> String {
        if total == 0 {
            return "[----------------]".to_string();
        }
        let clamped_width = width.max(1);
        let filled = (completed.saturating_mul(clamped_width)).saturating_add(total - 1) / total;
        let mut bar = String::with_capacity(clamped_width + 2);
        bar.push('[');
        for idx in 0..clamped_width {
            if idx < filled {
                bar.push('=');
            } else {
                bar.push('-');
            }
        }
        bar.push(']');
        bar
    }

    fn format_job_list(names: &[String], max_items: usize) -> String {
        if names.is_empty() {
            return String::new();
        }
        let shown = names.iter().take(max_items).cloned().collect::<Vec<_>>();
        let mut text = shown.join(", ");
        if names.len() > max_items {
            let remaining = names.len() - max_items;
            text.push_str(&format!(" +{remaining} more"));
        }
        text
    }

    fn render_gh_run_wait(&self, elapsed: Duration) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::styled(
            "Monitoring GitHub Workflow",
            Style::default()
                .fg(crate::colors::info())
                .add_modifier(Modifier::BOLD),
        ));

        let border = super::formatting::left_border_span();
        let dim = Style::default().fg(crate::colors::text_dim());
        let text = Style::default().fg(crate::colors::text());
        if let Some(url) = self.tool_argument_text("url") {
            lines.push(Line::from(vec![
                border.clone(),
                Span::styled("url ", dim),
                Span::styled(url, text),
            ]));
        }
        if let Some(branch) = self.tool_argument_text("branch") {
            lines.push(Line::from(vec![
                border.clone(),
                Span::styled("branch ", dim),
                Span::styled(branch, text),
            ]));
        }
        if let Some(run_id) = self.tool_argument_text("run_id") {
            lines.push(Line::from(vec![
                border.clone(),
                Span::styled("run ", dim),
                Span::styled(run_id, text),
            ]));
        }
        if let Some(workflow) = self.tool_argument_text("workflow") {
            lines.push(Line::from(vec![
                border.clone(),
                Span::styled("workflow ", dim),
                Span::styled(workflow, text),
            ]));
        }

        if let Some(jobs) = self.tool_argument_json("jobs") {
            let total = jobs.get("total").and_then(serde_json::Value::as_u64).unwrap_or(0) as usize;
            let completed = jobs
                .get("completed")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;
            let in_progress = jobs
                .get("in_progress")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;
            let queued = jobs.get("queued").and_then(serde_json::Value::as_u64).unwrap_or(0) as usize;
            let steps_total = jobs
                .get("steps_total")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;
            let steps_completed = jobs
                .get("steps_completed")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;
            let (progress_completed, progress_total, progress_label) = if steps_total > 0 {
                (steps_completed, steps_total, "progress (steps)")
            } else {
                (completed, total, "progress (jobs)")
            };
            let progress = Self::progress_bar(progress_completed, progress_total, 16);
            if progress_total > 0 {
                let percent = (progress_completed.saturating_mul(100)) / progress_total.max(1);
                lines.push(Line::from(vec![
                    border.clone(),
                    Span::styled(format!("{progress_label} "), dim),
                    Span::styled(
                        format!(
                            "{progress} {progress_completed}/{progress_total} ({percent}%)"
                        ),
                        text,
                    ),
                ]));
                lines.push(Line::from(vec![
                    border.clone(),
                    Span::styled("jobs ", dim),
                    Span::styled(
                        format!("{completed} completed • {in_progress} running • {queued} queued"),
                        text,
                    ),
                ]));
            }
            let running_names = jobs
                .get("running")
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(std::string::ToString::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let queued_names = jobs
                .get("queued_names")
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(std::string::ToString::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if !running_names.is_empty() {
                let list = Self::format_job_list(&running_names, 4);
                lines.push(Line::from(vec![
                    border.clone(),
                    Span::styled("running ", dim),
                    Span::styled(list, text),
                ]));
            }
            if !queued_names.is_empty() {
                let list = Self::format_job_list(&queued_names, 4);
                lines.push(Line::from(vec![
                    border,
                    Span::styled("queued ", dim),
                    Span::styled(list, text),
                ]));
            }
        }

        let elapsed_str = Self::compact_duration(elapsed);
        lines.push(Line::from(vec![
            Span::styled("└ ", dim),
            Span::styled("Waiting for ", dim),
            Span::styled(elapsed_str, text),
        ]));
        lines.push(Line::from(""));
        lines
    }

    pub(crate) fn has_title(&self, title: &str) -> bool {
        self.state.title == title
    }

    fn elapsed_duration(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.state.started_at)
            .unwrap_or_else(|_| self.start_clock.elapsed())
    }

    fn collapsed_argument_lines(&self) -> Vec<Line<'static>> {
        let args_lines = render_arguments(&self.state.arguments);
        if args_lines.is_empty() {
            return Vec::new();
        }
        let mut shown = args_lines;
        super::formatting::fold_lines(
            &mut shown,
            true,
            &super::formatting::FoldConfig::with_threshold(TOOL_DETAILS_PREVIEW_ARGS),
        );
        shown
    }
}

impl HistoryCell for RunningToolCallCell {
    impl_as_any!();

    fn is_fold_toggleable(&self) -> bool {
        true
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Tool {
            status: super::ToolCellStatus::Running,
        }
    }

    fn call_id(&self) -> Option<&str> {
        self.state.call_id.as_deref()
    }

    fn parent_call_id(&self) -> Option<&str> {
        self.parent_call_id.as_deref()
    }

    fn gutter_symbol(&self) -> Option<&'static str> {
        if self.state.title == "Waiting" {
            if self.state.wait_has_call_id {
                None
            } else {
                Some(self.spinner_frame())
            }
        } else {
            Some("…")
        }
    }

    fn is_animating(&self) -> bool {
        true
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let elapsed = self.elapsed_duration();
        let mut lines: Vec<Line<'static>> = Vec::new();
        if self.state.title == "Waiting" {
            let show_elapsed = !self.state.wait_has_target;
            let mut spans = Vec::new();
            spans.push(
                Span::styled(
                    "Waiting...",
                    Style::default()
                        .fg(crate::colors::text())
                        .add_modifier(Modifier::BOLD),
                ),
            );
            let cap_ms = self.state.wait_cap_ms.unwrap_or(600_000);
            let cap_str = Self::strip_zero_seconds_suffix(
                format_duration(Duration::from_millis(cap_ms)),
            );
            let suffix = if show_elapsed {
                let elapsed_str = Self::strip_zero_seconds_suffix(format_duration(elapsed));
                format!(" ({elapsed_str} / up to {cap_str})")
            } else {
                format!(" (up to {cap_str})")
            };
            spans.push(Span::styled(
                suffix,
                Style::default().fg(crate::colors::text_dim()),
            ));
            lines.push(Line::from(spans));
        } else if self.is_gh_run_wait() {
            return self.render_gh_run_wait(elapsed);
        } else {
            lines.push(Line::styled(
                format!("{} ({})", self.state.title, format_duration(elapsed)),
                Style::default()
                    .fg(crate::colors::info())
                    .add_modifier(Modifier::BOLD),
            ));
        }
        if self.collapsed_details.get() {
            lines.extend(self.collapsed_argument_lines());
        } else {
            lines.extend(render_arguments(&self.state.arguments));
        }
        lines.push(Line::from(""));
        lines
    }
}

fn render_arguments(arguments: &[ToolArgument]) -> Vec<Line<'static>> {
    arguments.iter().map(render_argument).collect()
}

fn render_argument(arg: &ToolArgument) -> Line<'static> {
    let dim_style = Style::default().fg(crate::colors::text_dim());
    let mut spans = vec![Span::styled("└ ", dim_style)];
    spans.push(Span::styled(
        format!("{}: ", arg.name),
        dim_style,
    ));
    let value_span = match &arg.value {
        ArgumentValue::Text(text) => Span::styled(text.clone(), Style::default().fg(crate::colors::text())),
        ArgumentValue::Json(json) => {
            let compact = format_json_compact(&json.to_string()).unwrap_or_else(|| json.to_string());
            Span::styled(compact, Style::default().fg(crate::colors::text()))
        }
        ArgumentValue::Secret => Span::styled("(secret)".to_string(), Style::default().fg(crate::colors::text_dim())),
    };
    spans.push(value_span);
    Line::from(spans)
}
