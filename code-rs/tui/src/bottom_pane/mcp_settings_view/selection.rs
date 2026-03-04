use ratatui::layout::Rect;

use crate::ui_interaction::{centered_scroll_top, contains_point};

use super::McpSettingsView;

impl McpSettingsView {
    fn list_prefix_lines(&self) -> usize {
        // `list_lines()` renders a two-line empty-state header when there are no servers.
        if self.rows.is_empty() { 2 } else { 0 }
    }

    fn list_total_lines(&self) -> usize {
        let prefix = self.list_prefix_lines();
        prefix + self.rows.len() + 4
    }

    fn list_selection_line_index(&self) -> usize {
        let prefix = self.list_prefix_lines();
        let row_lines = self.rows.len();

        if self.selected < self.rows.len() {
            prefix + self.selected
        } else if self.selected == self.refresh_index() {
            prefix + row_lines + 1
        } else if self.selected == self.add_index() {
            prefix + row_lines + 2
        } else {
            debug_assert_eq!(self.selected, self.close_index());
            prefix + row_lines + 3
        }
    }

    fn list_selection_at_line_index(&self, line_index: usize) -> Option<usize> {
        let prefix = self.list_prefix_lines();
        let row_lines = self.rows.len();

        if line_index < prefix {
            return None;
        }
        let rel = line_index - prefix;
        if rel < row_lines {
            return Some(rel);
        }
        // `list_lines()` always inserts a blank separator line after the server rows.
        if rel == row_lines {
            return None;
        }
        if rel == row_lines + 1 {
            return Some(self.refresh_index());
        }
        if rel == row_lines + 2 {
            return Some(self.add_index());
        }
        if rel == row_lines + 3 {
            return Some(self.close_index());
        }
        None
    }

    pub(super) fn list_scroll_top(&self, viewport_height: u16) -> usize {
        centered_scroll_top(
            self.list_selection_line_index(),
            self.list_total_lines(),
            viewport_height as usize,
        )
    }

    pub(super) fn tools_scroll_top_for_entries_len(
        &self,
        viewport_height: u16,
        entries_len: usize,
    ) -> usize {
        centered_scroll_top(self.tools_selected, entries_len, viewport_height as usize)
    }

    pub(super) fn server_index_at_mouse_row(&self, list_inner: Rect, row: u16) -> Option<usize> {
        let y0 = list_inner.y;
        let y1 = list_inner.y.saturating_add(list_inner.height);
        if row < y0 || row >= y1 {
            return None;
        }
        let rel_y = row.saturating_sub(list_inner.y) as usize;
        let scroll_top = self.list_scroll_top(list_inner.height);
        let line = scroll_top.saturating_add(rel_y);
        self.list_selection_at_line_index(line)
    }

    pub(super) fn server_index_at_mouse_position(
        &self,
        list_inner: Rect,
        column: u16,
        row: u16,
    ) -> Option<usize> {
        if !contains_point(list_inner, column, row) {
            return None;
        }

        let rel_y = row.saturating_sub(list_inner.y) as usize;
        let scroll_top = self.list_scroll_top(list_inner.height);
        let line_idx = scroll_top.saturating_add(rel_y);
        self.list_selection_at_line_index(line_idx)
    }

    pub(super) fn tool_index_at_mouse_row(&self, tools_inner: Rect, row: u16) -> Option<usize> {
        let entries_len = self.tool_entries().len();
        if entries_len == 0 {
            return None;
        }
        let y0 = tools_inner.y;
        let y1 = tools_inner.y.saturating_add(tools_inner.height);
        if row < y0 || row >= y1 {
            return None;
        }
        let rel_y = row.saturating_sub(tools_inner.y) as usize;
        let scroll_top = self.tools_scroll_top_for_entries_len(tools_inner.height, entries_len);
        let idx = scroll_top.saturating_add(rel_y);
        if idx < entries_len {
            Some(idx)
        } else {
            None
        }
    }

    pub(super) fn tool_index_at_mouse_position(
        &self,
        tools_inner: Rect,
        column: u16,
        row: u16,
    ) -> Option<usize> {
        let entries_len = self.tool_entries().len();
        if entries_len == 0 {
            return None;
        }
        if !contains_point(tools_inner, column, row) {
            return None;
        }

        let rel_y = row.saturating_sub(tools_inner.y) as usize;
        let scroll_top = self.tools_scroll_top_for_entries_len(tools_inner.height, entries_len);
        let idx = scroll_top.saturating_add(rel_y);
        if idx >= entries_len {
            return None;
        }
        Some(idx)
    }
}
