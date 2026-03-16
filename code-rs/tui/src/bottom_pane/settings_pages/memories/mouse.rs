use super::*;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_in_body;
use crate::ui_interaction::{
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

impl MemoriesSettingsView {
    fn handle_mouse_event_main_impl(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
        chrome: ChromeMode,
    ) -> bool {
        let rows = Self::rows();
        let total = rows.len();
        if total == 0 {
            return false;
        }

        let Some(layout) = self.main_page().layout_in_chrome(chrome, area) else {
            return false;
        };
        self.viewport_rows.set(layout.visible_rows());

        let mut state = self.state.get();
        let outcome = route_scroll_state_mouse_in_body(
            mouse_event,
            layout.body,
            &mut state,
            total,
            SelectableListMouseConfig {
                hover_select: false,
                activate_on_left_click: true,
                scroll_select: true,
                require_pointer_hit_for_scroll: false,
                scroll_behavior: ScrollSelectionBehavior::Wrap,
            },
        );
        self.state.set(state);

        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            self.activate_selected();
        }
        outcome.changed
    }

    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        if matches!(self.mode, ViewMode::Main) {
            return self.handle_mouse_event_main_impl(mouse_event, area, ChromeMode::ContentOnly);
        }

        match &mut self.mode {
            ViewMode::Edit { target, field, error } => match mouse_event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let field_area = Self::edit_page(self.scope, *target, error.as_deref())
                        .layout_in_chrome(ChromeMode::ContentOnly, area)
                        .map(|layout| layout.field);
                    if let Some(field_area) = field_area {
                        field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
                    } else {
                        false
                    }
                }
                MouseEventKind::ScrollDown => field.handle_mouse_scroll(true),
                MouseEventKind::ScrollUp => field.handle_mouse_scroll(false),
                _ => false,
            },
            ViewMode::Main | ViewMode::Transition => false,
        }
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        if matches!(self.mode, ViewMode::Main) {
            return self.handle_mouse_event_main_impl(mouse_event, area, ChromeMode::Framed);
        }

        match &mut self.mode {
            ViewMode::Edit { target, field, error } => match mouse_event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let field_area = Self::edit_page(self.scope, *target, error.as_deref())
                        .layout_in_chrome(ChromeMode::Framed, area)
                        .map(|layout| layout.field);
                    if let Some(field_area) = field_area {
                        field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
                    } else {
                        false
                    }
                }
                MouseEventKind::ScrollDown => field.handle_mouse_scroll(true),
                MouseEventKind::ScrollUp => field.handle_mouse_scroll(false),
                _ => false,
            },
            ViewMode::Main | ViewMode::Transition => false,
        }
    }
}
