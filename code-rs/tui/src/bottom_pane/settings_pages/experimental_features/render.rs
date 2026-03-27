use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;

impl ExperimentalFeaturesSettingsView {
    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::Framed, area, buf);
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::ContentOnly, area, buf);
    }

    fn render_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let rows = self.overview_rows();
        let page = self.overview_page();
        let Some(layout) = page.layout_in_chrome(chrome, area) else {
            return;
        };

        let visible_rows = layout.body.height.max(1) as usize;
        self.list_viewport_rows.set(visible_rows);

        let mut state = self.list_state.get();
        state.clamp_selection(rows.len());
        state.ensure_visible(rows.len(), visible_rows);
        self.list_state.set(state);

        let _ = page.render_menu_rows_in_chrome(
            chrome,
            area,
            buf,
            state.scroll_top,
            state.selected_idx,
            &rows,
        );
    }
}

