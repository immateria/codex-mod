use std::cmp::max;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders};

use super::McpSettingsView;
use crate::ui_interaction::{clipped_vertical_rect_with_scroll, contains_point, inset_rect_right};

#[derive(Clone, Copy)]
pub(super) struct McpViewLayout {
    pub(super) list_rect: Rect,
    pub(super) summary_rect: Rect,
    pub(super) tools_rect: Rect,
    pub(super) list_inner: Rect,
    pub(super) summary_inner: Rect,
    pub(super) tools_inner: Rect,
    pub(super) hint_area: Option<Rect>,
    pub(super) stacked: bool,
    pub(super) stack_scroll_top: usize,
    pub(super) stack_max_scroll: usize,
    pub(super) stack_viewport: Rect,
    pub(super) stack_list_top: usize,
    pub(super) stack_summary_top: usize,
    pub(super) stack_tools_top: usize,
    pub(super) stack_list_h: usize,
    pub(super) stack_summary_h: usize,
    pub(super) stack_tools_h: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum McpPaneHit {
    Servers,
    Summary,
    Tools,
    Outside,
}

impl McpViewLayout {
    pub(super) fn from_area_with_scroll(area: Rect, stacked_scroll_top: usize) -> Option<Self> {
        let content = McpSettingsView::content_rect(area);
        if content.width == 0 || content.height == 0 {
            return None;
        }

        let show_hint_row = content.height >= 16 && content.width >= 80;
        let vertical = if show_hint_row {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(content)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1)])
                .split(content)
        };

        let main_area = vertical[0];
        let hint_area = if vertical.len() > 1 {
            Some(vertical[1])
        } else {
            None
        };

        if main_area.width >= 72 {
            let (list_rect, detail_rect) = split_content_wide(main_area);
            let (summary_rect, tools_rect) = split_details_wide(detail_rect);
            return Some(Self {
                list_rect,
                summary_rect,
                tools_rect,
                list_inner: Self::block_inner(list_rect),
                summary_inner: Self::block_inner(summary_rect),
                tools_inner: Self::block_inner(tools_rect),
                hint_area,
                stacked: false,
                stack_scroll_top: 0,
                stack_max_scroll: 0,
                stack_viewport: main_area,
                stack_list_top: 0,
                stack_summary_top: 0,
                stack_tools_top: 0,
                stack_list_h: 0,
                stack_summary_h: 0,
                stack_tools_h: 0,
            });
        }

        // In stacked mode we prefer keeping each pane readable (min-heights) and letting
        // the entire column scroll, instead of squeezing panes to fit the viewport.
        let base_list_h = 9usize;
        let base_summary_h = 8usize;
        let base_tools_h = 8usize;
        let base_total_h = base_list_h + base_summary_h + base_tools_h;
        let available_h = main_area.height as usize;

        let (list_h, summary_h, tools_h) = if available_h >= base_total_h {
            let extra = available_h - base_total_h;
            // Give additional space to details first, then tools, then servers.
            let summary_extra = extra / 2;
            let tools_extra = (extra.saturating_sub(summary_extra)) / 2;
            let list_extra = extra.saturating_sub(summary_extra + tools_extra);
            (
                base_list_h + list_extra,
                base_summary_h + summary_extra,
                base_tools_h + tools_extra,
            )
        } else {
            (base_list_h, base_summary_h, base_tools_h)
        };

        let stack_total_height = list_h + summary_h + tools_h;
        let stack_max_scroll = stack_total_height.saturating_sub(available_h);
        let stack_scroll_top = stacked_scroll_top.min(stack_max_scroll);
        let stack_list_top = 0usize;
        let stack_summary_top = list_h;
        let stack_tools_top = list_h + summary_h;

        let list_rect =
            clipped_vertical_rect_with_scroll(main_area, stack_list_top, list_h, stack_scroll_top);
        let summary_rect = clipped_vertical_rect_with_scroll(
            main_area,
            stack_summary_top,
            summary_h,
            stack_scroll_top,
        );
        let tools_rect =
            clipped_vertical_rect_with_scroll(main_area, stack_tools_top, tools_h, stack_scroll_top);

        Some(Self {
            list_rect,
            summary_rect,
            tools_rect,
            list_inner: Self::block_inner(list_rect),
            summary_inner: Self::block_inner(summary_rect),
            tools_inner: Self::block_inner(tools_rect),
            hint_area,
            stacked: true,
            stack_scroll_top,
            stack_max_scroll,
            stack_viewport: main_area,
            stack_list_top,
            stack_summary_top,
            stack_tools_top,
            stack_list_h: list_h,
            stack_summary_h: summary_h,
            stack_tools_h: tools_h,
        })
    }

    fn block_inner(rect: Rect) -> Rect {
        Block::default().borders(Borders::ALL).inner(rect)
    }

    pub(super) fn contains_list(self, x: u16, y: u16) -> bool {
        in_rect(self.list_rect, x, y)
    }

    pub(super) fn contains_summary(self, x: u16, y: u16) -> bool {
        in_rect(self.summary_rect, x, y)
    }

    pub(super) fn contains_tools(self, x: u16, y: u16) -> bool {
        in_rect(self.tools_rect, x, y)
    }

    pub(super) fn summary_scrollbar_area(self) -> Rect {
        inset_scrollbar_area(self.summary_inner)
    }

    pub(super) fn tools_scrollbar_area(self) -> Rect {
        inset_scrollbar_area(self.tools_inner)
    }
}

fn split_content_wide(content: Rect) -> (Rect, Rect) {
    let list_width = max(30u16, content.width / 3);
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(list_width), Constraint::Min(24)])
        .split(content);
    (chunks[0], chunks[1])
}

fn split_details_wide(content: Rect) -> (Rect, Rect) {
    let summary_h = if content.height <= 10 {
        content.height.saturating_sub(4).max(1)
    } else {
        max(8, content.height / 3)
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(summary_h), Constraint::Min(4)])
        .split(content);
    (chunks[0], chunks[1])
}

fn in_rect(rect: Rect, x: u16, y: u16) -> bool {
    contains_point(rect, x, y)
}

fn inset_scrollbar_area(area: Rect) -> Rect {
    inset_rect_right(area, 1)
}
