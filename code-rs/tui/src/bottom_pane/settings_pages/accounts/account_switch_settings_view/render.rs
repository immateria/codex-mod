use super::{AccountSwitchSettingsView, ViewMode};

use crate::bottom_pane::chrome::ChromeMode;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

impl AccountSwitchSettingsView {
    pub(super) fn render_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        match self.view_mode {
            ViewMode::Main => {
                let page = self.main_page();
                let selected = self
                    .main_state
                    .selected_idx
                    .unwrap_or(0)
                    .min(Self::MAIN_OPTION_COUNT.saturating_sub(1));
                let runs = self.main_runs(Some(selected));
                let _layout = page.render_runs_in_chrome(chrome, area, buf, 0, &runs);
            }
            ViewMode::ConfirmStoreChange { target } => {
                let page = self.confirm_page(target);
                let rows = self.confirm_rows();
                let selected = self.confirm_selected_index();
                let _layout =
                    page.render_menu_rows_in_chrome(chrome, area, buf, 0, Some(selected), &rows);
            }
        }
    }
}
