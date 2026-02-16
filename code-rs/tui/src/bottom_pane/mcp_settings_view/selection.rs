use ratatui::layout::Rect;

use crate::ui_interaction::{centered_scroll_top, contains_point};

use super::McpSettingsView;

impl McpSettingsView {
    fn list_total_lines(&self) -> usize {
        let prefix = if self.rows.is_empty() { 2 } else { 0 };
        prefix + self.rows.len() + 4
    }

    fn list_selection_line_index(&self) -> usize {
        let prefix = if self.rows.is_empty() { 2 } else { 0 };
        let row_lines = self.rows.len();

        if self.selected < self.rows.len() {
            prefix + self.selected
        } else if self.selected == self.refresh_index() {
            prefix + row_lines + 1
        } else if self.selected == self.add_index() {
            prefix + row_lines + 2
        } else {
            prefix + row_lines + 3
        }
    }

    fn list_selection_at_line_index(&self, line_index: usize) -> Option<usize> {
        let prefix = if self.rows.is_empty() { 2 } else { 0 };
        let row_lines = self.rows.len();

        if line_index < prefix {
            return None;
        }
        let rel = line_index - prefix;
        if rel < row_lines {
            return Some(rel);
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

    fn scroll_top_for_selected(selected: usize, total_lines: usize, viewport_height: usize) -> usize {
        centered_scroll_top(selected, total_lines, viewport_height)
    }

    pub(super) fn list_scroll_top(&self, viewport_height: u16) -> usize {
        Self::scroll_top_for_selected(
            self.list_selection_line_index(),
            self.list_total_lines(),
            viewport_height as usize,
        )
    }

    pub(super) fn tools_scroll_top(&self, viewport_height: u16) -> usize {
        Self::scroll_top_for_selected(
            self.tools_selected,
            self.tool_entries().len(),
            viewport_height as usize,
        )
    }

    pub(super) fn server_index_at_mouse_row(&self, list_inner: Rect, row: u16) -> Option<usize> {
        if !contains_point(list_inner, list_inner.x, row) {
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
        let selection = self.list_selection_at_line_index(line_idx)?;

        let rel_x = column.saturating_sub(list_inner.x) as usize;
        let lines = self.list_lines(list_inner.width as usize);
        let line_width = lines.get(line_idx).map(ratatui::text::Line::width)?;
        if rel_x < line_width { Some(selection) } else { None }
    }

    pub(super) fn tool_index_at_mouse_row(&self, tools_inner: Rect, row: u16) -> Option<usize> {
        let entries_len = self.tool_entries().len();
        if entries_len == 0 {
            return None;
        }
        if !contains_point(tools_inner, tools_inner.x, row) {
            return None;
        }
        let rel_y = row.saturating_sub(tools_inner.y) as usize;
        let scroll_top = self.tools_scroll_top(tools_inner.height);
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
        let scroll_top = self.tools_scroll_top(tools_inner.height);
        let idx = scroll_top.saturating_add(rel_y);
        if idx >= entries_len {
            return None;
        }

        let rel_x = column.saturating_sub(tools_inner.x) as usize;
        let lines = self.tools_lines(tools_inner.width as usize);
        let line_width = lines.get(idx).map(ratatui::text::Line::width)?;
        if rel_x < line_width { Some(idx) } else { None }
    }
}
