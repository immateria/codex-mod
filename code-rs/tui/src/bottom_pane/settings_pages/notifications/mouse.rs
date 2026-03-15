use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::ui_interaction::{
    route_selectable_list_mouse_with_config,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

use super::NotificationsSettingsView;

impl NotificationsSettingsView {
    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let mut selected = self.selected_row;
        let rows = self.menu_rows();
        let Some(layout) = self.page().content_only().layout(area) else {
            return false;
        };
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            rows.len(),
            |x, y| crate::bottom_pane::settings_ui::menu_rows::selection_id_at(layout.body, x, y, 0, &rows),
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );
        self.selected_row = selected;

        if matches!(result, SelectableListMouseResult::Activated) {
            if self.selected_row == 0 {
                self.toggle();
            } else {
                self.is_complete = true;
            }
        }
        result.handled()
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let mut selected = self.selected_row;
        let rows = self.menu_rows();
        let Some(layout) = self.page().framed().layout(area) else {
            return false;
        };
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            rows.len(),
            |x, y| crate::bottom_pane::settings_ui::menu_rows::selection_id_at(layout.body, x, y, 0, &rows),
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );
        self.selected_row = selected;

        if matches!(result, SelectableListMouseResult::Activated) {
            if self.selected_row == 0 {
                self.toggle();
            } else {
                self.is_complete = true;
            }
        }
        result.handled()
    }
}

