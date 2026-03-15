use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::bottom_pane::settings_ui::menu_rows::render_menu_rows;
use crate::colors;

use super::PlanningSettingsView;

impl PlanningSettingsView {
    fn render_rows(&self, area: Rect, buf: &mut Buffer) {
        let rows = self.menu_rows();
        render_menu_rows(
            area,
            buf,
            0,
            self.selected_row(),
            &rows,
            Style::new().bg(colors::background()).fg(colors::text()),
        );
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        let Some(layout) = self.page().content_only().render_shell(area, buf) else {
            return;
        };
        self.render_rows(layout.body, buf);
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        let Some(layout) = self.page().framed().render_shell(area, buf) else {
            return;
        };
        self.render_rows(layout.body, buf);
    }
}

