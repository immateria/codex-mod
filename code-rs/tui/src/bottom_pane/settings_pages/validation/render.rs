use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

impl ValidationSettingsView {
    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let page = self.page();
        let selected_idx = self.state.selected_idx.unwrap_or(usize::MAX);
        let runs = self.build_runs(selected_idx);
        let Some(layout) = page
            .content_only()
            .render_runs(area, buf, self.state.scroll_top, &runs)
        else {
            return;
        };
        self.viewport_rows.set(layout.body.height as usize);
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let page = self.page();
        let selected_idx = self.state.selected_idx.unwrap_or(usize::MAX);
        let runs = self.build_runs(selected_idx);
        let Some(layout) = page
            .framed()
            .render_runs(area, buf, self.state.scroll_top, &runs)
        else {
            return;
        };
        let visible_slots = layout.body.height as usize;
        self.viewport_rows.set(visible_slots);
    }
}

