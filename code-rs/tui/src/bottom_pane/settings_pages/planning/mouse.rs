use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;

use super::PlanningSettingsView;

impl PlanningSettingsView {
    fn handle_mouse_event_direct_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind else {
            return false;
        };

        let rows = self.menu_rows();
        let Some(layout) = self.page().layout_in_chrome(chrome, area) else {
            return false;
        };
        let Some(row) = SettingsMenuPage::selection_menu_id_in_body(
            layout.body,
            mouse_event.column,
            mouse_event.row,
            0,
            &rows,
        ) else {
            return false;
        };

        self.state.selected_idx = Some(0);
        self.handle_enter(row);
        true
    }

    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_in_chrome(ChromeMode::ContentOnly, mouse_event, area)
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_in_chrome(ChromeMode::Framed, mouse_event, area)
    }
}
