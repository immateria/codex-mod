mod input;
mod model;
mod mouse;
mod pages;
mod pane_impl;
mod render;
mod tool_detection;

#[cfg(test)]
mod tests;

use code_core::config_types::ValidationCategory;
use code_core::protocol::ValidationGroup;
use std::cell::Cell;
use unicode_width::UnicodeWidthStr;

use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;

pub(crate) use tool_detection::detect_tools;

#[derive(Clone, Debug)]
pub(crate) struct ToolStatus {
    pub name: &'static str,
    pub description: &'static str,
    pub installed: bool,
    pub install_hint: String,
    pub category: ValidationCategory,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupStatus {
    pub group: ValidationGroup,
    pub name: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct ToolRow {
    pub status: ToolStatus,
    pub enabled: bool,
    pub group_enabled: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SelectionKind {
    Group(usize),
    Tool(usize),
}

use crate::timing::DEFAULT_VISIBLE_ROWS;

#[derive(Clone, Debug)]
struct ValidationListModel {
    /// Selection index -> semantic kind.
    selection_kinds: Vec<SelectionKind>,
    /// Selection index -> absolute line index within the flattened run list.
    selection_line: Vec<usize>,
    /// Selection index -> inclusive (section_start_line, section_end_line).
    section_bounds: Vec<(usize, usize)>,
    /// Total line count across all runs.
    total_lines: usize,
}

pub(crate) struct ValidationSettingsView {
    groups: Vec<(GroupStatus, bool)>,
    tools: Vec<ToolRow>,
    app_event_tx: AppEventSender,
    state: ScrollState,
    is_complete: bool,
    tool_label_pad_cols: u16,
    viewport_rows: Cell<usize>,
    pending_notice: Option<String>,
}

crate::bottom_pane::chrome_view::impl_chrome_view!(ValidationSettingsView, framed);

impl ValidationSettingsView {
    pub fn new(
        groups: Vec<(GroupStatus, bool)>,
        tools: Vec<ToolRow>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let state = ScrollState::with_first_selected();
        let tool_label_pad_cols = tools.iter().map(|row| row.status.name.width()).max().unwrap_or(0);
        let tool_label_pad_cols = u16::try_from(tool_label_pad_cols).unwrap_or(u16::MAX);
        Self {
            groups,
            tools,
            app_event_tx,
            state,
            is_complete: false,
            tool_label_pad_cols,
            viewport_rows: Cell::new(0),
            pending_notice: None,
        }
    }

    pub fn is_view_complete(&self) -> bool {
        self.is_complete
    }
}

fn group_for_category(category: ValidationCategory) -> ValidationGroup {
    match category {
        ValidationCategory::Functional => ValidationGroup::Functional,
        ValidationCategory::Stylistic => ValidationGroup::Stylistic,
    }
}
