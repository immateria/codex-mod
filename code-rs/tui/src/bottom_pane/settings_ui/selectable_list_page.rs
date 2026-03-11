use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

use crate::colors;
use crate::util::buffer::fill_rect;

use super::frame::{SettingsFrame, SettingsFrameLayout};

const DEFAULT_VISIBLE_ROWS: usize = 8;

pub(crate) struct SettingsSelectableListPage<'a> {
    frame: SettingsFrame<'a>,
    default_visible_rows: usize,
}

impl<'a> SettingsSelectableListPage<'a> {
    pub(crate) fn new(
        title: impl Into<Cow<'a, str>>,
        header_lines: Vec<Line<'static>>,
        footer_lines: Vec<Line<'static>>,
    ) -> Self {
        Self {
            frame: SettingsFrame::new(title, header_lines, footer_lines),
            default_visible_rows: DEFAULT_VISIBLE_ROWS,
        }
    }

    pub(crate) fn with_default_visible_rows(mut self, default_visible_rows: usize) -> Self {
        self.default_visible_rows = default_visible_rows.max(1);
        self
    }

    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsFrameLayout> {
        self.frame.layout(area)
    }

    pub(crate) fn render_shell(&self, area: Rect, buf: &mut Buffer) -> Option<SettingsFrameLayout> {
        self.frame.render(area, buf)
    }

    pub(crate) fn visible_budget(&self, viewport_rows_hint: usize, total: usize) -> usize {
        if total == 0 {
            return 1;
        }
        let target = if viewport_rows_hint == 0 {
            self.default_visible_rows
        } else {
            viewport_rows_hint
        };
        target.clamp(1, total)
    }

    pub(crate) fn selection_index_at(
        body: Rect,
        x: u16,
        y: u16,
        start_row: usize,
        selection_rows: &[usize],
    ) -> Option<usize> {
        if !body.contains(Position { x, y }) {
            return None;
        }
        let row_index = start_row.saturating_add(y.saturating_sub(body.y) as usize);
        selection_rows.iter().position(|&row| row == row_index)
    }

    pub(crate) fn render_rows<F>(
        body: Rect,
        buf: &mut Buffer,
        start_row: usize,
        total_rows: usize,
        mut render_row: F,
    ) where
        F: FnMut(usize) -> Line<'static>,
    {
        let base_style = Style::new().bg(colors::background()).fg(colors::text());
        fill_rect(buf, body, Some(' '), base_style);

        let visible_slots = body.height as usize;
        let end_row = start_row.saturating_add(visible_slots).min(total_rows);
        let mut visible_lines = Vec::with_capacity(end_row.saturating_sub(start_row));
        for row_index in start_row..end_row {
            visible_lines.push(render_row(row_index));
        }
        Paragraph::new(visible_lines)
            .style(base_style)
            .render(body, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_index_uses_body_and_start_row() {
        let body = Rect::new(2, 4, 20, 3);
        let selection_rows = vec![1, 4, 6];
        assert_eq!(
            SettingsSelectableListPage::selection_index_at(body, 3, 4, 1, &selection_rows),
            Some(0)
        );
        assert_eq!(
            SettingsSelectableListPage::selection_index_at(body, 3, 5, 1, &selection_rows),
            None
        );
        assert_eq!(
            SettingsSelectableListPage::selection_index_at(body, 3, 6, 4, &selection_rows),
            Some(2)
        );
    }

    #[test]
    fn visible_budget_uses_default_then_hint() {
        let page = SettingsSelectableListPage::new("Test", vec![], vec![]);
        assert_eq!(page.visible_budget(0, 20), 8);
        assert_eq!(page.visible_budget(5, 20), 5);
        assert_eq!(page.visible_budget(0, 3), 3);
    }

    #[test]
    fn render_rows_clears_unused_body_lines() {
        let body = Rect::new(0, 0, 12, 3);
        let mut buf = Buffer::empty(body);
        fill_rect(
            &mut buf,
            body,
            Some('x'),
            Style::new().bg(colors::background()).fg(colors::text()),
        );

        SettingsSelectableListPage::render_rows(body, &mut buf, 0, 1, |_| Line::from("row"));

        assert_eq!(buf[(0, 0)].symbol(), "r");
        assert_eq!(buf[(0, 1)].symbol(), " ");
        assert_eq!(buf[(0, 2)].symbol(), " ");
    }
}
