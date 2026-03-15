use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::VerbositySelectionView;

impl VerbositySelectionView {
    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        let page = self.page();
        let rows = self.menu_rows();
        let _ = page
            .framed()
            .render_menu_rows(area, buf, 0, Some(self.selected_idx), &rows);
    }
}

