use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::NotificationsSettingsView;

impl NotificationsSettingsView {
    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        let page = self.page();
        let rows = self.menu_rows();
        let _ = page
            .content_only()
            .render_menu_rows(area, buf, 0, Some(self.selected_row), &rows);
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        let page = self.page();
        let rows = self.menu_rows();
        let _ = page
            .framed()
            .render_menu_rows(area, buf, 0, Some(self.selected_row), &rows);
    }
}

