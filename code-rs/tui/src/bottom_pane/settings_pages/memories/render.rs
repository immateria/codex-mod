use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::bottom_pane::settings_ui::rows::{KeyValueRow, StyledText};
use crate::bottom_pane::chrome::ChromeMode;
use crate::colors;

impl MemoriesSettingsView {
    pub(super) fn main_row_specs(&self, selected: usize) -> Vec<KeyValueRow<'_>> {
        let rows = Self::rows();
        rows.iter()
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
            .collect()
    }

    fn render_main_with(&self, area: Rect, buf: &mut Buffer, chrome: ChromeMode) {
        let rows = Self::rows();
        let total = rows.len();
        let mut state = self.state.get();
        state.clamp_selection(total);

        let selected = state.selected_idx.unwrap_or(0);
        let scroll_top = state.scroll_top;
        let row_specs = self.main_row_specs(selected);
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

    fn render_text_viewer_with(
        viewer: &TextViewerState,
        area: Rect,
        buf: &mut Buffer,
        chrome: ChromeMode,
    ) {
        let page = Self::text_viewer_page(viewer);
        if let Some(layout) = page.render_in_chrome(chrome, area, buf) {
            let visible = layout.body.height as usize;
            viewer.viewport_rows.set(visible.max(1));
        }
    }

    fn render_rollout_list_with(
        list: &RolloutListState,
        area: Rect,
        buf: &mut Buffer,
        chrome: ChromeMode,
    ) {
        let total = list.entries.len();
        let mut state = list.list_state.get();
        state.clamp_selection(total);
        let selected = state.selected_idx.unwrap_or(0);
        let scroll_top = state.scroll_top;
        let menu_rows = Self::rollout_list_menu_rows(list);
        let page = Self::rollout_list_page(list);
        let layout = page.render_menu_rows_in_chrome(
            chrome,
            area,
            buf,
            scroll_top,
            Some(selected),
            &menu_rows,
        );
        if let Some(layout) = layout {
            let visible = layout.body.height.max(1) as usize;
            state.ensure_visible(total, visible);
            list.viewport_rows.set(visible);
            list.list_state.set(state);
        }
    }

    fn render_search_input_with(
        title: &'static str,
        field: &FormTextField,
        area: Rect,
        buf: &mut Buffer,
        chrome: ChromeMode,
    ) {
        let page = Self::search_page(title);
        let _ = page.render_in_chrome(chrome, area, buf, field);
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main | ViewMode::Transition => {
                self.render_main_with(area, buf, ChromeMode::ContentOnly);
            }
            ViewMode::Edit { target, field, error } => {
                self.render_edit_with(
                    area,
                    buf,
                    *target,
                    field,
                    error.as_deref(),
                    ChromeMode::ContentOnly,
                );
            }
            ViewMode::TextViewer(viewer) => {
                Self::render_text_viewer_with(viewer, area, buf, ChromeMode::ContentOnly);
            }
            ViewMode::RolloutList(list) => {
                Self::render_rollout_list_with(list, area, buf, ChromeMode::ContentOnly);
            }
            ViewMode::SearchInput { viewer, field } => {
                Self::render_search_input_with(viewer.title, field, area, buf, ChromeMode::ContentOnly);
            }
        }
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main | ViewMode::Transition => self.render_main_with(area, buf, ChromeMode::Framed),
            ViewMode::Edit { target, field, error } => {
                self.render_edit_with(area, buf, *target, field, error.as_deref(), ChromeMode::Framed);
            }
            ViewMode::TextViewer(viewer) => {
                Self::render_text_viewer_with(viewer, area, buf, ChromeMode::Framed);
            }
            ViewMode::RolloutList(list) => {
                Self::render_rollout_list_with(list, area, buf, ChromeMode::Framed);
            }
            ViewMode::SearchInput { viewer, field } => {
                Self::render_search_input_with(viewer.title, field, area, buf, ChromeMode::Framed);
            }
        }
    }
}
