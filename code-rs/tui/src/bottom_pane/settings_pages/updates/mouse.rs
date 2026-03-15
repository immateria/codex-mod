use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::app_event::AppEvent;
use crate::bottom_pane::settings_ui::menu_rows::selection_id_at;
use crate::ui_interaction::{
    route_selectable_list_mouse_with_config,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

use super::UpdateSettingsView;

impl UpdateSettingsView {
    fn handle_mouse_event_in_body(&mut self, mouse_event: MouseEvent, body: Rect) -> bool {
        let rows = self.rows();
        let mut selected = self.field;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            rows.len(),
            |x, y| selection_id_at(body, x, y, 0, &rows),
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );
        self.field = selected;

        if matches!(result, SelectableListMouseResult::Activated) {
            self.activate_selected();
            self.app_event_tx.send(AppEvent::RequestRedraw);
        }

        result.handled()
    }

    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let Some(layout) = self.content_layout(area) else {
            return false;
        };
        self.handle_mouse_event_in_body(mouse_event, layout.body)
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.page()
            .framed()
            .layout(area)
            .map(|layout| self.handle_mouse_event_in_body(mouse_event, layout.body))
            .unwrap_or(false)
    }
}

