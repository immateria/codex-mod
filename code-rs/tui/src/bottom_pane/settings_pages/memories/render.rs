use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::bottom_pane::settings_ui::rows::{KeyValueRow, StyledText};
use crate::bottom_pane::chrome::ChromeMode;
use crate::colors;

impl MemoriesSettingsView {
    fn render_main_with(&self, area: Rect, buf: &mut Buffer, chrome: ChromeMode) {
        let rows = Self::rows();
        let total = rows.len();
        let mut state = self.state.get();
        state.clamp_selection(total);

        let selected = state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));
        let scroll_top = state.scroll_top.min(total.saturating_sub(1));
        let row_specs: Vec<KeyValueRow<'_>> = rows
            .iter()
            .enumerate()
            .map(|(idx, row)| {
                let is_selected = idx == selected;
                let mut spec = KeyValueRow::new(Self::row_label(*row));
                let value = self.row_value(*row);
                if !value.is_empty() {
                    spec = spec.with_value(StyledText::new(
                        value,
                        if is_selected {
                            Style::default()
                                .fg(colors::text_bright())
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(colors::text_dim())
                        },
                    ));
                }
                spec
            })
            .collect();
        let page = self.main_page();
        let Some(layout) =
            page.render_in_chrome(chrome, area, buf, scroll_top, Some(selected), &row_specs)
        else {
            return;
        };
        state.ensure_visible(total, layout.visible_rows());
        self.viewport_rows.set(layout.visible_rows());
        self.state.set(state);
    }

    fn render_edit_with(
        &self,
        area: Rect,
        buf: &mut Buffer,
        target: EditTarget,
        field: &FormTextField,
        error: Option<&str>,
        chrome: ChromeMode,
    ) {
        let page = Self::edit_page(self.scope, target, error);
        let _ = page.render_in_chrome(chrome, area, buf, field);
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main | ViewMode::Transition => {
                self.render_main_with(area, buf, ChromeMode::ContentOnly)
            }
            ViewMode::Edit { target, field, error } => {
                self.render_edit_with(
                    area,
                    buf,
                    *target,
                    field,
                    error.as_deref(),
                    ChromeMode::ContentOnly,
                )
            }
        }
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main | ViewMode::Transition => self.render_main_with(area, buf, ChromeMode::Framed),
            ViewMode::Edit { target, field, error } => {
                self.render_edit_with(area, buf, *target, field, error.as_deref(), ChromeMode::Framed)
            }
        }
    }
}
