use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::row_page::SettingsRowPage;
use crate::bottom_pane::settings_ui::rows::{KeyValueRow, StyledText};
use crate::colors;

impl ExecLimitsSettingsView {
    fn render_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main_in_chrome(chrome, area, buf),
            ViewMode::Edit { target, field, error } => self.render_edit_in_chrome(
                chrome,
                area,
                buf,
                *target,
                field,
                error.as_deref(),
            ),
            ViewMode::Transition => self.render_main_in_chrome(chrome, area, buf),
        }
    }

    fn render_main_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        let rows = Self::build_rows();
        let total = rows.len();
        let mut state = self.state.get();
        state.clamp_selection(total);
        let selected_idx = state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));

        let is_dirty = self.settings != self.last_applied;
        let row_specs: Vec<KeyValueRow<'_>> = rows
            .iter()
            .copied()
            .map(|row| match row {
                RowKind::PidsMax => KeyValueRow::new("Process limit (pids.max)").with_value(
                    StyledText::new(
                        Self::format_limit_pids(self.settings.pids_max),
                        Style::default().fg(colors::success()),
                    ),
                ),
                RowKind::MemoryMax => KeyValueRow::new("Memory limit (memory.max)").with_value(
                    StyledText::new(
                        Self::format_limit_memory(self.settings.memory_max_mb),
                        Style::default().fg(colors::success()),
                    ),
                ),
                RowKind::ResetBothAuto => KeyValueRow::new("Reset both to Auto"),
                RowKind::DisableBoth => KeyValueRow::new("Disable both"),
                RowKind::Apply => KeyValueRow::new("Apply").with_value(StyledText::new(
                    if is_dirty { "Pending" } else { "Saved" },
                    Style::default().fg(colors::success()),
                )),
                RowKind::Close => KeyValueRow::new("Close"),
            })
            .collect();
        let page = SettingsRowPage::new(
            " Exec Limits ",
            self.render_header_lines(),
            self.render_footer_lines(),
        )
        ;
        let Some(layout) = page.render_in_chrome(
            chrome,
            area,
            buf,
            state.scroll_top,
            Some(selected_idx),
            &row_specs,
        ) else {
            return;
        };
        let visible_slots = layout.visible_rows();
        state.ensure_visible(total, visible_slots);
        self.state.set(state);
        self.viewport_rows.set(visible_slots);
    }

    fn render_edit_in_chrome(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        target: EditTarget,
        field: &FormTextField,
        error: Option<&str>,
    ) {
        // Layout is not currently used for hit-testing (mouse uses layout directly); keep it
        // in a named binding so ignoring it is a conscious choice.
        let _layout = Self::edit_page(target, error).render_in_chrome(chrome, area, buf, field);
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::ContentOnly, area, buf);
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::Framed, area, buf);
    }
}
