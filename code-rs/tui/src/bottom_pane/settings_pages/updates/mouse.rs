use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::app_event::AppEvent;
use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::menu_rows::selection_id_at as selection_menu_id_at;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test;
use crate::ui_interaction::{
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

use super::UpdateSettingsView;

impl UpdateSettingsView {
    fn handle_mouse_event_direct_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let Some(layout) = self.page().layout_in_chrome(chrome, area) else {
            return false;
        };

        let rows = self.rows();
        let visible = layout.body.height.max(1) as usize;
        let outcome = route_scroll_state_mouse_with_hit_test(
            mouse_event,
            &mut self.state,
            Self::ROW_COUNT,
            visible,
            |x, y, scroll_top| selection_menu_id_at(layout.body, x, y, scroll_top, &rows),
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );

        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            self.activate_selected();
            self.app_event_tx.send(AppEvent::RequestRedraw);
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
