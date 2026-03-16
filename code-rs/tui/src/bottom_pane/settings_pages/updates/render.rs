use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;

use super::UpdateSettingsView;

impl UpdateSettingsView {
    fn render_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        let rows = self.rows();
        let _layout = self.page().render_menu_rows_in_chrome(
            chrome,
            area,
            buf,
            self.state.scroll_top,
            self.state.selected_idx,
            &rows,
        );
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::ContentOnly, area, buf);
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::Framed, area, buf);
    }
}
