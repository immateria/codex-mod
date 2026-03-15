use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use super::PlanningSettingsView;

impl PlanningSettingsView {
    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let Some(layout) = self.page().content_only().layout(area) else {
            return false;
        };
        let Some(row) =
            self.row_at_position(layout.body, mouse_event.column, mouse_event.row)
        else {
            return false;
        };

        self.state.selected_idx = Some(0);
        if matches!(
            mouse_event.kind,
            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
        ) {
            self.handle_enter(row);
        }
        true
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let Some(layout) = self.page().framed().layout(area) else {
            return false;
        };
        let Some(row) =
            self.row_at_position(layout.body, mouse_event.column, mouse_event.row)
        else {
            return false;
        };

        self.state.selected_idx = Some(0);
        if matches!(
            mouse_event.kind,
            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
        ) {
            self.handle_enter(row);
        }
        true
    }
}

