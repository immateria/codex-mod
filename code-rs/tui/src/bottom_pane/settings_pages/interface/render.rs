use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;

impl InterfaceSettingsView {
    fn render_main_impl(&self, area: Rect, buf: &mut Buffer, chrome: ChromeMode) {
        let rows = self.build_rows();
        let total = rows.len();
        if total == 0 {
            return;
        }

        let selected_idx_raw = self.state.selected_idx.unwrap_or(0);
        debug_assert!(selected_idx_raw < total);
        debug_assert!(self.state.scroll_top < total);

        let state = self.state.clamped(total);
        let selected_idx = state.selected_idx.unwrap_or(0);
        let scroll_top = state.scroll_top;
        let selected_row = rows[selected_idx];

        let menu_rows = self.main_menu_rows(rows);
        let selected_id = Some(selected_idx);

        let page = self.main_page_for_selected_row(selected_row);
        let Some(layout) =
            page.render_menu_rows_in_chrome(chrome, area, buf, scroll_top, selected_id, &menu_rows)
        else {
            return;
        };

        self.main_viewport_rows
            .set(layout.body.height.max(1) as usize);
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main_without_frame(area, buf),
            ViewMode::EditWidth { field, error } => {
                // Layout is intentionally unused; `main_viewport_rows` is only relevant in main mode.
                let _layout = Self::edit_width_page(error.as_deref())
                    .content_only()
                    .render(area, buf, field);
            }
            ViewMode::CaptureHotkey { row, error } => {
                // Layout is intentionally unused; `main_viewport_rows` is only relevant in main mode.
                let _layout = self
                    .capture_hotkey_page(*row, error.as_deref())
                    .content_only()
                    .render(area, buf);
            }
            ViewMode::Transition => self.render_main_without_frame(area, buf),
        }
    }

    fn render_main(&self, area: Rect, buf: &mut Buffer) {
        self.render_main_impl(area, buf, ChromeMode::Framed);
    }

    fn render_main_without_frame(&self, area: Rect, buf: &mut Buffer) {
        self.render_main_impl(area, buf, ChromeMode::ContentOnly);
    }

    fn render_edit_width(
        area: Rect,
        buf: &mut Buffer,
        field: &FormTextField,
        error: Option<&str>,
    ) {
        // Layout is intentionally unused; `main_viewport_rows` is only relevant in main mode.
        let _layout = Self::edit_width_page(error).framed().render(area, buf, field);
    }

    fn render_capture_hotkey(&self, area: Rect, buf: &mut Buffer, row: RowKind, error: Option<&str>) {
        // Layout is intentionally unused; `main_viewport_rows` is only relevant in main mode.
        let _layout = self
            .capture_hotkey_page(row, error)
            .framed()
            .render(area, buf);
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main(area, buf),
            ViewMode::EditWidth { field, error } => {
                Self::render_edit_width(area, buf, field, error.as_deref())
            }
            ViewMode::CaptureHotkey { row, error } => {
                self.render_capture_hotkey(area, buf, *row, error.as_deref())
            }
            ViewMode::Transition => self.render_main(area, buf),
        }
    }
}
