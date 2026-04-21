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
                // Show contextual hint when selected (consistent with other settings pages).
                if is_selected {
                    let lr = crate::icons::nav_left_right();
                    let hint: std::borrow::Cow<'_, str> = match row {
                        RowKind::Scope => format!("({lr} to cycle)").into(),
                        RowKind::GenerateMemories
                        | RowKind::UseMemories
                        | RowKind::SkipMcpOrWebSearch => format!("({lr} to toggle)").into(),
                        RowKind::MaxRawMemories
                        | RowKind::MaxRolloutAgeDays
                        | RowKind::MaxRolloutsPerStartup
                        | RowKind::MinRolloutIdleHours => "Enter to edit".into(),
                        RowKind::ManageUserMemories => "Enter to manage".into(),
                        RowKind::BrowseTags => "Enter to browse".into(),
                        RowKind::BrowseEpochs => "Enter to browse".into(),
                        RowKind::ViewSummary
                        | RowKind::ViewRawMemories
                        | RowKind::ViewModelPrompt
                        | RowKind::ViewStatus => "Enter to view".into(),
                        RowKind::BrowseRollouts => "Enter to browse".into(),
                        RowKind::RefreshArtifacts
                        | RowKind::ClearArtifacts => "Enter to run".into(),
                        RowKind::OpenDirectory => "Enter to open".into(),
                        RowKind::Apply => {
                            if self.current_scope_dirty() {
                                "Enter to apply".into()
                            } else {
                                "No changes".into()
                            }
                        }
                        RowKind::Close => "Enter/Esc".into(),
                    };
                    spec = spec.with_selected_hint(hint);
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

    fn render_user_memory_list_with(
        list: &UserMemoryListState,
        area: Rect,
        buf: &mut Buffer,
        chrome: ChromeMode,
    ) {
        let total = list.entries.len() + 1; // +1 for "Add new" row
        let mut state = list.list_state.get();
        state.clamp_selection(total);
        let selected = state.selected_idx.unwrap_or(0);
        let scroll_top = state.scroll_top;
        let menu_rows = Self::user_memory_list_menu_rows(list);
        let page = Self::user_memory_list_page(list);
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

    fn render_user_memory_editor_with(
        editor: &UserMemoryEditorState,
        area: Rect,
        buf: &mut Buffer,
        chrome: ChromeMode,
    ) {
        let field = match editor.focus {
            UserMemoryEditorFocus::Content => &editor.content_field,
            UserMemoryEditorFocus::Tags => &editor.tags_field,
        };
        let page = Self::user_memory_editor_page(editor);
        let _ = page.render_in_chrome(chrome, area, buf, field);
    }

    fn render_tag_browser_with(
        browser: &TagBrowserState,
        area: Rect,
        buf: &mut Buffer,
        chrome: ChromeMode,
    ) {
        let total = browser.tags.len();
        let mut state = browser.list_state.get();
        state.clamp_selection(total);
        let selected = state.selected_idx.unwrap_or(0);
        let scroll_top = state.scroll_top;
        let menu_rows = Self::tag_browser_rows(browser);
        let page = Self::tag_browser_page(browser);
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
            browser.viewport_rows.set(visible);
            browser.list_state.set(state);
        }
    }

    fn render_epoch_browser_with(
        browser: &EpochBrowserState,
        area: Rect,
        buf: &mut Buffer,
        chrome: ChromeMode,
    ) {
        let total = browser.epochs.len();
        let mut state = browser.list_state.get();
        state.clamp_selection(total);
        let selected = state.selected_idx.unwrap_or(0);
        let scroll_top = state.scroll_top;
        let menu_rows = Self::epoch_browser_rows(browser);
        let page = Self::epoch_browser_page(browser);
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
            browser.viewport_rows.set(visible);
            browser.list_state.set(state);
        }
    }

    fn render_with_chrome(&self, area: Rect, buf: &mut Buffer, chrome: ChromeMode) {
        match &self.mode {
            ViewMode::Main | ViewMode::Transition => {
                self.render_main_with(area, buf, chrome);
            }
            ViewMode::Edit { target, field, error } => {
                self.render_edit_with(area, buf, *target, field, error.as_deref(), chrome);
            }
            ViewMode::TextViewer(viewer) => {
                Self::render_text_viewer_with(viewer, area, buf, chrome);
            }
            ViewMode::RolloutList(list) => {
                Self::render_rollout_list_with(list, area, buf, chrome);
            }
            ViewMode::UserMemoryList(list) => {
                Self::render_user_memory_list_with(list, area, buf, chrome);
            }
            ViewMode::UserMemoryEditor(editor) => {
                Self::render_user_memory_editor_with(editor, area, buf, chrome);
            }
            ViewMode::TagBrowser(browser) => {
                Self::render_tag_browser_with(browser, area, buf, chrome);
            }
            ViewMode::EpochBrowser(browser) => {
                Self::render_epoch_browser_with(browser, area, buf, chrome);
            }
            ViewMode::SearchInput { viewer, field } => {
                Self::render_search_input_with(viewer.title, field, area, buf, chrome);
            }
        }
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        self.render_with_chrome(area, buf, ChromeMode::ContentOnly);
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        self.render_with_chrome(area, buf, ChromeMode::Framed);
    }
}
