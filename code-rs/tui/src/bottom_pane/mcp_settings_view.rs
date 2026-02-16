use std::collections::BTreeMap;
use std::cell::Cell;
use std::time::Duration;

use crate::app_event_sender::AppEventSender;

mod input;
mod layout;
mod pane_impl;
mod presentation;
mod selection;
mod state;
mod summary_scroll;
mod tool_state;
use layout::{McpPaneHit, McpViewLayout};

#[derive(Clone, Debug)]
pub(crate) struct McpServerRow {
    pub name: String,
    pub enabled: bool,
    pub transport: String,
    pub startup_timeout: Option<Duration>,
    pub tool_timeout: Option<Duration>,
    pub tools: Vec<String>,
    pub disabled_tools: Vec<String>,
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
    last_render_area: Cell<Option<ratatui::layout::Rect>>,
}

impl McpSettingsView {
    pub fn new(rows: McpServerRows, app_event_tx: AppEventSender) -> Self {
        Self {
            rows,
            selected: 0,
            focus: McpSettingsFocus::Servers,
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
            last_render_area: Cell::new(None),
        }
    }

}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    use super::{
        McpPaneHit,
        McpServerRow,
        McpSettingsFocus,
        McpSettingsView,
        McpToolHoverPart,
        McpViewLayout,
    };
    use crate::app_event_sender::AppEventSender;

    fn make_server_row(name: &str) -> McpServerRow {
        McpServerRow {
            name: name.to_string(),
            enabled: true,
            transport: "npx -y test-server --transport stdio".to_string(),
            startup_timeout: Some(Duration::from_secs(30)),
            tool_timeout: Some(Duration::from_secs(30)),
            tools: vec!["tool_a".to_string(), "tool_b".to_string()],
            disabled_tools: Vec::new(),
            tool_definitions: BTreeMap::new(),
            failure: Some(
                "very long failure text that should wrap and produce vertical overflow ".repeat(12),
            ),
            status: "Failed to start".to_string(),
        }
    }

    fn make_view(rows: Vec<McpServerRow>) -> McpSettingsView {
        let (tx, _rx) = channel();
        McpSettingsView::new(rows, AppEventSender::new(tx))
    }

    fn mouse_event(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn key_routing_returns_false_for_unhandled_key() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let handled = view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(!handled);
    }

    #[test]
    fn wheel_over_summary_scrolls_details_and_sets_focus() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let area = Rect::new(0, 0, 100, 24);
        let layout =
            McpViewLayout::from_area_with_scroll(area, 0).expect("layout should exist");

        let event = mouse_event(
            MouseEventKind::ScrollDown,
            layout.summary_inner.x.saturating_add(1),
            layout.summary_inner.y.saturating_add(1),
        );

        assert!(view.handle_mouse_event_direct(event, area));
        assert_eq!(view.focus, McpSettingsFocus::Summary);
        assert!(view.summary_scroll_top > 0);
    }

    #[test]
    fn stacked_layout_scrolls_focused_pane_into_view() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let area = Rect::new(0, 0, 60, 14);
        view.last_render_area.set(Some(area));

        assert_eq!(view.focus, McpSettingsFocus::Servers);
        assert_eq!(view.stacked_scroll_top, 0);

        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
        assert_eq!(view.focus, McpSettingsFocus::Summary);
        assert!(view.stacked_scroll_top > 0);

        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
        assert_eq!(view.focus, McpSettingsFocus::Tools);
        assert!(view.stacked_scroll_top > 0);

        let layout =
            McpViewLayout::from_area_with_scroll(area, view.stacked_scroll_top).expect("layout");
        assert!(layout.stacked);
        assert!(layout.tools_rect.height > 0);
    }

    #[test]
    fn scrolling_up_from_bottom_sentinel_moves_summary_view() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let viewport = Rect::new(0, 0, 38, 6);
        let metrics = view.summary_metrics_for_viewport(viewport);
        let max_scroll = metrics.total_lines.saturating_sub(metrics.visible_lines);
        assert!(max_scroll > 0);

        view.summary_scroll_top = usize::MAX;
        view.scroll_summary_lines(-1);

        assert_eq!(view.summary_scroll_top, max_scroll.saturating_sub(1));
    }

    #[test]
    fn mouse_move_only_updates_hover_not_focus_or_selection() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let area = Rect::new(0, 0, 100, 24);
        let layout =
            McpViewLayout::from_area_with_scroll(area, 0).expect("layout should exist");
        let initial_focus = view.focus;
        let initial_selected = view.selected;

        let event = mouse_event(
            MouseEventKind::Moved,
            layout.summary_inner.x.saturating_add(1),
            layout.summary_inner.y.saturating_add(1),
        );

        assert!(view.handle_mouse_event_direct(event, area));
        assert_eq!(view.focus, initial_focus);
        assert_eq!(view.selected, initial_selected);
        assert_eq!(view.hovered_pane, McpPaneHit::Summary);
    }

    #[test]
    fn mouse_move_over_server_row_updates_list_hover() {
        let mut view = make_view(vec![make_server_row("server_a"), make_server_row("server_b")]);
        let area = Rect::new(0, 0, 100, 24);
        let layout =
            McpViewLayout::from_area_with_scroll(area, 0).expect("layout should exist");
        let initial_focus = view.focus;
        let initial_selected = view.selected;

        let hover_y = (layout.list_inner.y..layout.list_inner.y.saturating_add(layout.list_inner.height))
            .find(|row| view.server_index_at_mouse_row(layout.list_inner, *row) == Some(1))
            .expect("row hit for second server");
        let event = mouse_event(
            MouseEventKind::Moved,
            layout.list_inner.x.saturating_add(3),
            hover_y,
        );

        assert!(view.handle_mouse_event_direct(event, area));
        assert_eq!(view.focus, initial_focus);
        assert_eq!(view.selected, initial_selected);
        assert_eq!(view.hovered_pane, McpPaneHit::Servers);
        assert_eq!(view.hovered_list_index, Some(1));
    }

    #[test]
    fn server_list_is_single_line_per_server_without_summary_row() {
        let view = make_view(vec![make_server_row("server_a")]);
        let lines = view.list_lines(80);
        let line_text: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
        assert!(line_text.iter().any(|line| line.contains("[on ] server_a")));
        assert!(
            !line_text
                .iter()
                .any(|line| line.contains("test-server --transport stdio"))
        );
    }

    #[test]
    fn server_row_click_requires_second_click_to_toggle() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let area = Rect::new(0, 0, 100, 24);
        let layout =
            McpViewLayout::from_area_with_scroll(area, 0).expect("layout should exist");

        let click_y = (layout.list_inner.y..layout.list_inner.y.saturating_add(layout.list_inner.height))
            .find(|row| view.server_index_at_mouse_row(layout.list_inner, *row) == Some(0))
            .expect("row hit for first server");
        let click_event = mouse_event(
            MouseEventKind::Down(MouseButton::Left),
            layout.list_rect.x.saturating_add(1),
            click_y,
        );

        assert!(view.rows[0].enabled);
        assert!(view.handle_mouse_event_direct(click_event, area));
        assert!(view.rows[0].enabled);
        assert!(view.handle_mouse_event_direct(click_event, area));
        assert!(!view.rows[0].enabled);
    }

    #[test]
    fn tool_hover_distinguishes_toggle_and_expand_controls() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let area = Rect::new(0, 0, 100, 24);
        let layout =
            McpViewLayout::from_area_with_scroll(area, 0).expect("layout should exist");

        let tool_row_y =
            (layout.tools_inner.y..layout.tools_inner.y.saturating_add(layout.tools_inner.height))
                .find(|row| view.tool_index_at_mouse_row(layout.tools_inner, *row) == Some(0))
            .expect("row hit for first tool");

        let toggle_hover = mouse_event(
            MouseEventKind::Moved,
            layout.tools_inner.x.saturating_add(2),
            tool_row_y,
        );
        assert!(view.handle_mouse_event_direct(toggle_hover, area));
        assert_eq!(view.hovered_pane, McpPaneHit::Tools);
        assert_eq!(view.hovered_tool_index, Some(0));
        assert_eq!(view.hovered_tool_part, Some(McpToolHoverPart::Toggle));

        let expand_hover = mouse_event(
            MouseEventKind::Moved,
            layout.tools_inner.x.saturating_add(6),
            tool_row_y,
        );
        assert!(view.handle_mouse_event_direct(expand_hover, area));
        assert_eq!(view.hovered_tool_index, Some(0));
        assert_eq!(view.hovered_tool_part, Some(McpToolHoverPart::Expand));
    }

    #[test]
    fn wheel_outside_panes_does_not_mutate_state() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let area = Rect::new(0, 0, 100, 24);
        let initial_selected = view.selected;
        let initial_scroll = view.summary_scroll_top;

        let event = mouse_event(
            MouseEventKind::ScrollDown,
            area.x.saturating_add(area.width).saturating_add(1),
            area.y,
        );
        assert!(!view.handle_mouse_event_direct(event, area));
        assert_eq!(view.selected, initial_selected);
        assert_eq!(view.summary_scroll_top, initial_scroll);
    }
}
