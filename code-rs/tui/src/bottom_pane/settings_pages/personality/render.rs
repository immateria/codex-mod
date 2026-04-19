use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;

use super::PersonalitySettingsView;

impl PersonalitySettingsView {
    fn render_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        let page = self.page();
        let rows = self.menu_rows();
        if let Some(layout) = page.render_menu_rows_in_chrome(
            chrome,
            area,
            buf,
            self.state.scroll_top,
            self.selected_row(),
            &rows,
        ) {
            self.viewport_rows.set(layout.body.height.max(1) as usize);
        }
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::ContentOnly, area, buf);
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::Framed, area, buf);
    }
}
