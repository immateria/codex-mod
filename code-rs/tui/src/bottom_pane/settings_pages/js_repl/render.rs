use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;

impl JsReplSettingsView {
    fn render_main_impl(&self, area: Rect, buf: &mut Buffer, chrome: ChromeMode) {
        let rows = self.build_rows();
        let total = rows.len();
        let selected_idx = self
            .state
            .selected_idx
            .unwrap_or(0)
            .min(total.saturating_sub(1));
        let scroll_top = self.state.scroll_top.min(total.saturating_sub(1));

        let row_specs = self.main_row_specs(&rows);
        let page = self.main_page();
        let layout = page.render_in_chrome(chrome, area, buf, scroll_top, Some(selected_idx), &row_specs);
        let Some(layout) = layout else {
            return;
        };
        self.viewport_rows.set(layout.visible_rows());
    }

    fn render_main(&self, area: Rect, buf: &mut Buffer) {
        self.render_main_impl(area, buf, ChromeMode::Framed);
    }

    fn render_main_without_frame(&self, area: Rect, buf: &mut Buffer) {
        self.render_main_impl(area, buf, ChromeMode::ContentOnly);
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main_without_frame(area, buf),
            ViewMode::EditText { target, field } => {
                // Layout is intentionally unused; `viewport_rows` is only relevant in main mode.
                let _layout = Self::text_edit_page(*target)
                    .render_in_chrome(ChromeMode::ContentOnly, area, buf, field);
            }
            ViewMode::EditList { target, field } => {
                // Layout is intentionally unused; `viewport_rows` is only relevant in main mode.
                let _layout = Self::list_edit_page(*target)
                    .render_in_chrome(ChromeMode::ContentOnly, area, buf, field);
            }
            ViewMode::Transition => self.render_main_without_frame(area, buf),
        }
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main(area, buf),
            ViewMode::EditText { target, field } => {
                // Layout is intentionally unused; `viewport_rows` is only relevant in main mode.
                let _layout =
                    Self::text_edit_page(*target).render_in_chrome(ChromeMode::Framed, area, buf, field);
            }
            ViewMode::EditList { target, field } => {
                // Layout is intentionally unused; `viewport_rows` is only relevant in main mode.
                let _layout =
                    Self::list_edit_page(*target).render_in_chrome(ChromeMode::Framed, area, buf, field);
            }
            ViewMode::Transition => self.render_main(area, buf),
        }
    }
}
