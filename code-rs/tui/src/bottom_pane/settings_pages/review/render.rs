use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;

impl ReviewSettingsView {
    fn render_in_chrome(&self, area: Rect, buf: &mut Buffer, chrome: ChromeMode) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let page = self.page();
        let runs = self.build_runs(self.state.selected_idx.unwrap_or(usize::MAX));
        let Some(layout) = page.render_runs_in_chrome(chrome, area, buf, self.state.scroll_top, &runs) else {
            return;
        };
        self.viewport_rows.set(layout.body.height as usize);
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(area, buf, ChromeMode::ContentOnly);
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(area, buf, ChromeMode::Framed);
    }
}
