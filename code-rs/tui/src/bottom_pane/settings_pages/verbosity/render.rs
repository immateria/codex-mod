use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;

use super::VerbositySelectionView;

impl VerbositySelectionView {
    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        let page = self.page();
        let rows = self.menu_rows();
        let _layout =
            page.render_menu_rows_in_chrome(ChromeMode::Framed, area, buf, 0, self.state.selected_idx, &rows);
    }
}
