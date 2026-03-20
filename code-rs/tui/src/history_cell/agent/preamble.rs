use super::card_style::{
    agent_card_style,
    ansi16_inverse_color,
    fill_card_background,
    hint_text_style,
    primary_text_style,
    rows_to_lines,
    secondary_text_style,
    title_text_style,
    truncate_with_ellipsis,
    CardRow,
    CardSegment,
    CardStyle,
    CARD_ACCENT_WIDTH,
};
use super::{HistoryCell, HistoryCellType, ToolCellStatus};
use crate::colors;
use crate::theme::{palette_mode, PaletteMode};
use code_common::elapsed::format_duration_digital;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::{Color, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use std::time::{Duration, Instant};

const BORDER_TOP: &str = "╭─";
const BORDER_BODY: &str = "│";
const BORDER_BOTTOM: &str = "╰─";
use unicode_width::UnicodeWidthChar;

const MAX_PLAN_LINES: usize = 4;
const MAX_SUMMARY_LINES: usize = 4;
const MAX_AGENT_DISPLAY: usize = 8;
const ACTION_TIME_COLUMN_MIN_WIDTH: usize = 2;
const ACTION_TIME_SEPARATOR_WIDTH: usize = 2;
const ACTION_TIME_INDENT: usize = 2;

#[derive(Clone, Default)]
pub(crate) struct AgentRunCell {
    agent_name: String,
    status_label: String,
    task: Option<String>,
    context: Option<String>,
    duration: Option<Duration>,
    plan: Vec<String>,
    agents: Vec<AgentStatusPreview>,
    summary_lines: Vec<String>,
    completed: bool,
    actions: Vec<ActionEntry>,
    cell_key: Option<String>,
    pub(crate) parent_call_id: Option<String>,
    batch_label: Option<String>,
    write_enabled: Option<bool>,
    first_action_at: Option<Instant>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AgentStatusPreview {
    pub id: String,
    pub name: String,
    pub status: String,
    pub model: Option<String>,
    pub details: Vec<AgentDetail>,
    pub status_kind: AgentStatusKind,
    pub step_progress: Option<StepProgress>,
    pub elapsed: Option<Duration>,
    pub last_update: Option<String>,
    pub elapsed_updated_at: Option<Instant>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct StepProgress {
    pub completed: u32,
    pub total: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AgentStatusKind {
    #[default]
    Running,
    Completed,
    Failed,
    Cancelled,
    Pending,
}

impl AgentStatusKind {
    fn glyph(self) -> &'static str {
        match self {
            AgentStatusKind::Running => "▶",
            AgentStatusKind::Completed => "✓",
            AgentStatusKind::Failed => "!",
            AgentStatusKind::Cancelled => "▮",
            AgentStatusKind::Pending => "…",
        }
    }

    fn label(self) -> &'static str {
        match self {
            AgentStatusKind::Running => "Running",
            AgentStatusKind::Completed => "Completed",
            AgentStatusKind::Failed => "Failed",
            AgentStatusKind::Cancelled => "Cancelled",
            AgentStatusKind::Pending => "Pending",
        }
    }

    fn color(self) -> Color {
        match self {
            AgentStatusKind::Running => colors::info(),
            AgentStatusKind::Completed => colors::success(),
            AgentStatusKind::Failed => colors::error(),
            AgentStatusKind::Cancelled => colors::text_dim(),
            AgentStatusKind::Pending => colors::text_dim(),
        }
    }
}

#[derive(Default, Clone, Copy)]
struct AgentCountSummary {
    total: usize,
    running: usize,
    completed: usize,
    failed: usize,
    cancelled: usize,
    pending: usize,
}

impl AgentCountSummary {
    fn observe(&mut self, kind: AgentStatusKind) {
        self.total += 1;
        match kind {
            AgentStatusKind::Running => self.running += 1,
            AgentStatusKind::Completed => self.completed += 1,
            AgentStatusKind::Failed => self.failed += 1,
            AgentStatusKind::Cancelled => self.cancelled += 1,
            AgentStatusKind::Pending => self.pending += 1,
        }
    }

    fn glyph_counts(&self) -> Vec<(AgentStatusKind, usize)> {
        let mut items = Vec::new();
        if self.completed > 0 {
            items.push((AgentStatusKind::Completed, self.completed));
        }
        if self.running > 0 {
            items.push((AgentStatusKind::Running, self.running));
        }
        if self.failed > 0 {
            items.push((AgentStatusKind::Failed, self.failed));
        }
        if self.cancelled > 0 {
            items.push((AgentStatusKind::Cancelled, self.cancelled));
        }
        if self.pending > 0 {
            items.push((AgentStatusKind::Pending, self.pending));
        }
        items
    }
}

#[derive(Clone, Debug)]
pub(crate) enum AgentDetail {
    Progress(String),
    Result(String),
    Error(String),
    Info(String),
}

#[derive(Clone)]
struct AgentRowData {
    name: String,
    status: String,
    meta: String,
    color: Color,
    name_width: usize,
    status_width: usize,
    meta_width: usize,
}

impl AgentRowData {
    fn new(name: String, status: String, meta: String, color: Color) -> Self {
        let name_width = string_width(name.as_str());
        let status_width = string_width(status.as_str());
        let meta_width = string_width(meta.as_str());
        Self {
            name,
            status,
            meta,
            color,
            name_width,
            status_width,
            meta_width,
        }
    }
}

#[derive(Clone, Debug)]
struct ActionEntry {
    label: String,
    elapsed: Duration,
}

