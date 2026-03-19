use std::collections::BTreeMap;
use std::cell::Cell;
use std::time::Duration;

use crate::app_event_sender::AppEventSender;
use code_core::config_types::McpServerSchedulingToml;
use code_core::config_types::McpToolSchedulingOverrideToml;
use code_core::protocol::McpAuthStatus;

use crate::bottom_pane::{ChromeMode, LastRenderContext};

mod input;
mod layout;
mod pane_impl;
mod policy_editor;
mod presentation;
mod selection;
mod state;
mod summary_scroll;
mod tool_state;
#[cfg(test)]
mod tests;
use layout::{McpPaneHit, McpViewLayout};

#[derive(Clone, Debug)]
pub(crate) struct McpServerRow {
    pub name: String,
    pub enabled: bool,
    pub transport: String,
    pub auth_status: McpAuthStatus,
    pub startup_timeout: Option<Duration>,
    pub tool_timeout: Option<Duration>,
    pub scheduling: McpServerSchedulingToml,
    pub tool_scheduling: BTreeMap<String, McpToolSchedulingOverrideToml>,
    pub tools: Vec<String>,
    pub disabled_tools: Vec<String>,
    pub resources: Vec<code_protocol::mcp::Resource>,
    pub resource_templates: Vec<code_protocol::mcp::ResourceTemplate>,
    pub tool_definitions: BTreeMap<String, mcp_types::Tool>,
    pub failure: Option<String>,
    pub status: String,
}

pub(crate) type McpServerRows = Vec<McpServerRow>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum McpSettingsFocus {
    Servers,
    Summary,
    Tools,
}

#[derive(Debug)]
enum McpSettingsMode {
    Main,
    EditServerScheduling(Box<policy_editor::ServerSchedulingEditor>),
    EditToolScheduling(Box<policy_editor::ToolSchedulingEditor>),
}

#[derive(Clone, Debug)]
enum McpSelectionKey {
    Server(String),
    Refresh,
    Add,
    Close,
}

#[derive(Clone, Debug)]
pub(crate) struct McpSettingsViewState {
    selection: McpSelectionKey,
    focus: McpSettingsFocus,
    stacked_scroll_top: usize,
    summary_scroll_top: usize,
    summary_hscroll: usize,
    summary_wrap: bool,
    tools_selected: usize,
    expanded_tool_by_server: BTreeMap<String, String>,
}

#[derive(Clone, Copy)]
struct McpToolEntry<'a> {
    name: &'a str,
    enabled: bool,
    definition: Option<&'a mcp_types::Tool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum McpToolHoverPart {
    Toggle,
    Expand,
    Label,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum McpScrollbarTarget {
    Stacked,
    Summary,
    Tools,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct McpScrollbarDragState {
    target: McpScrollbarTarget,
    offset_in_thumb: usize,
}

const SUMMARY_SCROLL_STEP: usize = 2;
const SUMMARY_PAGE_STEP: usize = 8;
const SUMMARY_HORIZONTAL_SCROLL_STEP: i32 = 2;

#[derive(Clone, Copy)]
struct SummaryMetrics {
    total_lines: usize,
    max_width: usize,
    visible_lines: usize,
}

pub(crate) struct McpSettingsView {
    rows: McpServerRows,
    selected: usize,
    focus: McpSettingsFocus,
    mode: McpSettingsMode,
    hovered_pane: McpPaneHit,
    hovered_list_index: Option<usize>,
    hovered_tool_index: Option<usize>,
    hovered_tool_part: Option<McpToolHoverPart>,
    armed_server_row_click: Option<usize>,
    stacked_scroll_top: usize,
    summary_scroll_top: usize,
    summary_last_max_scroll: Cell<usize>,
    summary_hscroll: usize,
    summary_wrap: bool,
    tools_selected: usize,
    expanded_tool_by_server: BTreeMap<String, String>,
    scrollbar_drag: Option<McpScrollbarDragState>,
    is_complete: bool,
    app_event_tx: AppEventSender,
    last_render: LastRenderContext,
}

impl McpSettingsView {
    pub fn new(rows: McpServerRows, app_event_tx: AppEventSender) -> Self {
        Self {
            rows,
            selected: 0,
            focus: McpSettingsFocus::Servers,
            mode: McpSettingsMode::Main,
            hovered_pane: McpPaneHit::Outside,
            hovered_list_index: None,
            hovered_tool_index: None,
            hovered_tool_part: None,
            armed_server_row_click: None,
            stacked_scroll_top: 0,
            summary_scroll_top: 0,
            summary_last_max_scroll: Cell::new(0),
            summary_hscroll: 0,
            summary_wrap: true,
            tools_selected: 0,
            expanded_tool_by_server: BTreeMap::new(),
            scrollbar_drag: None,
            is_complete: false,
            app_event_tx,
            last_render: LastRenderContext::new(ChromeMode::Framed),
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn framed(&self) -> McpSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> McpSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> McpSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> McpSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }
}

pub(crate) type McpSettingsViewFramed<'v> = crate::bottom_pane::chrome_view::Framed<'v, McpSettingsView>;
pub(crate) type McpSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, McpSettingsView>;
pub(crate) type McpSettingsViewFramedMut<'v> = crate::bottom_pane::chrome_view::FramedMut<'v, McpSettingsView>;
pub(crate) type McpSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, McpSettingsView>;
