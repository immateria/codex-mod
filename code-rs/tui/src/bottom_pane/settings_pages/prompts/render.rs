use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;

impl PromptsSettingsView {
    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::ContentOnly, area, buf);
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::Framed, area, buf);
    }

    fn render_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        match self.mode {
            Mode::List => self.render_list_in_chrome(chrome, area, buf),
            Mode::Edit => self.render_form_in_chrome(chrome, area, buf),
        }
    }

    fn render_list_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        let rows = self.list_rows();
        let Some(layout) = self.list_page().render_menu_rows_in_chrome(
            chrome,
            area,
            buf,
            self.list_state.scroll_top,
            self.list_state.selected_idx,
            &rows,
        ) else {
            return;
        };
        self.list_viewport_rows
            .set(layout.body.height.max(1) as usize);
    }

    fn render_form_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        let page = self.edit_form_page();
        let buttons = self.edit_button_specs();
        let _layout = page.render_with_standard_actions_end_in_chrome(
            chrome,
            area,
            buf,
            &[&self.name_field, &self.body_field],
            &buttons,
        );
    }
}
