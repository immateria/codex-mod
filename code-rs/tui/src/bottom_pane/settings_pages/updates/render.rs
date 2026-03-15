use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::bottom_pane::settings_ui::menu_rows::render_menu_rows;
use crate::colors;
use crate::util::buffer::{fill_rect, write_line};

use super::UpdateSettingsView;

impl UpdateSettingsView {
    fn render_body_without_frame(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let base = Style::new().bg(colors::background()).fg(colors::text());
        fill_rect(buf, area, Some(' '), base);

        let Some(layout) = self.content_layout(area) else {
            let rows = self.rows();
            render_menu_rows(area, buf, 0, Some(self.field), &rows, base);
            return;
        };

        let header_lines = self.header_lines();
        for (idx, line) in header_lines
            .iter()
            .enumerate()
            .take(layout.header.height as usize)
        {
            let y = layout.header.y.saturating_add(idx as u16);
            write_line(buf, layout.header.x, y, layout.header.width, line, base);
        }

        let rows = self.rows();
        render_menu_rows(layout.body, buf, 0, Some(self.field), &rows, base);

        let footer_lines = Self::footer_lines();
        for (idx, line) in footer_lines
            .iter()
            .enumerate()
            .take(layout.footer.height as usize)
        {
            let y = layout.footer.y.saturating_add(idx as u16);
            write_line(buf, layout.footer.x, y, layout.footer.width, line, base);
        }
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        self.render_body_without_frame(area, buf);
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        let rows = self.rows();
        let _layout = self
            .page()
            .framed()
            .render_menu_rows(area, buf, 0, Some(self.field), &rows);
    }
}

