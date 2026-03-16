use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_in_body;
use crate::ui_interaction::{
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

use super::NotificationsSettingsView;

impl NotificationsSettingsView {
    fn handle_mouse_event_direct_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let Some(layout) = self.page().layout_in_chrome(chrome, area) else {
            return false;
        };

        let outcome = route_scroll_state_mouse_in_body(
            mouse_event,
            layout.body,
            &mut self.state,
            Self::ROW_COUNT,
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );

        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            if self.selected_row() == 0 {
                self.toggle();
            } else {
                self.is_complete = true;
            }
        }
        outcome.changed
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
