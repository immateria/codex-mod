use super::*;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;

impl AppsSettingsView {
    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_in_chrome(ChromeMode::Framed, mouse_event, area)
    }

    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_in_chrome(ChromeMode::ContentOnly, mouse_event, area)
    }

    fn handle_mouse_event_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        match &self.mode {
            Mode::Overview => self.handle_mouse_overview(chrome, mouse_event, area),
            Mode::AccountDetail { .. } => false,
        }
    }

    fn handle_mouse_overview(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let snapshot = self.shared_snapshot();
        let rows = self.overview_rows(&snapshot);
        if rows.is_empty() {
            return false;
        }

        let page = self.overview_page(&snapshot);
        let Some(layout) = page.layout_in_chrome(chrome, area) else {
            return false;
        };

        let body = layout.body;
        let x = mouse_event.column;
        let y = mouse_event.row;
        if !body.contains(ratatui::layout::Position { x, y }) {
            return false;
        }

        let visible_rows = body.height.max(1) as usize;
        self.list_viewport_rows.set(visible_rows);

        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let state = self.list_state.get().clamped(rows.len());
                if let Some(id) = SettingsMenuPage::selection_menu_id_in_body(
                    body,
                    x,
                    y,
                    state.scroll_top,
                    &rows,
                ) {
                    let mut state = self.list_state.get();
                    state.selected_idx = Some(id);
                    self.list_state.set(state);
                    return true;
                }
                false
            }
            MouseEventKind::ScrollUp => {
                let mut state = self.list_state.get();
                state.move_up_wrap_visible(rows.len(), visible_rows);
                self.list_state.set(state);
                true
            }
            MouseEventKind::ScrollDown => {
                let mut state = self.list_state.get();
                state.move_down_wrap_visible(rows.len(), visible_rows);
                self.list_state.set(state);
                true
            }
            _ => false,
        }
    }
}

