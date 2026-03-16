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

impl InterfaceSettingsView {
    fn handle_mouse_event_main_impl(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
        chrome: ChromeMode,
    ) -> bool {
        let total = self.build_rows().len();
        if total == 0 {
            return false;
        }

        let page = self.main_page();
        let Some(layout) = page.layout_in_chrome(chrome, area) else {
            return false;
        };

        let visible = layout.body.height.max(1) as usize;
        self.main_viewport_rows.set(visible);

        let outcome = route_scroll_state_mouse_in_body(
            mouse_event,
            layout.body,
            &mut self.state,
            total,
            SelectableListMouseConfig {
                hover_select: false,
                require_pointer_hit_for_scroll: true,
                scroll_behavior: ScrollSelectionBehavior::Clamp,
                ..SelectableListMouseConfig::default()
            },
        );

        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            self.activate_selected_row();
        }
        outcome.changed
    }

    fn handle_mouse_event_edit_impl(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
        chrome: ChromeMode,
    ) -> bool {
        let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind else {
            return false;
        };

        let ViewMode::EditWidth { field, error } = &mut self.mode else {
            unreachable!("handle_mouse_event_edit_impl called outside EditWidth mode")
        };

        let page = Self::edit_width_page(error.as_deref());
        let field_area = page.layout_in_chrome(chrome, area).map(|layout| layout.field);
        let Some(field_area) = field_area else {
            return false;
        };

        field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
    }

    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        match &self.mode {
            ViewMode::Main => self.handle_mouse_event_main_impl(mouse_event, area, ChromeMode::ContentOnly),
            ViewMode::EditWidth { .. } => {
                self.handle_mouse_event_edit_impl(mouse_event, area, ChromeMode::ContentOnly)
            }
            ViewMode::CaptureHotkey { .. } | ViewMode::Transition => false,
        }
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        match &self.mode {
            ViewMode::Main => self.handle_mouse_event_main_impl(mouse_event, area, ChromeMode::Framed),
            ViewMode::EditWidth { .. } => {
                self.handle_mouse_event_edit_impl(mouse_event, area, ChromeMode::Framed)
            }
            ViewMode::CaptureHotkey { .. } | ViewMode::Transition => false,
        }
    }
}
