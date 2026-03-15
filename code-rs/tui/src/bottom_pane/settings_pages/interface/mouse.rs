use super::*;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::settings_ui::rows::selection_index_at;
use crate::ui_interaction::{
    route_selectable_list_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

impl InterfaceSettingsView {
    fn handle_mouse_event_main_impl(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
        chrome: UiChrome,
    ) -> bool {
        let total = self.build_rows().len();
        if total == 0 {
            return false;
        }

        let layout = match chrome {
            UiChrome::Framed => self.main_page().framed().layout(area),
            UiChrome::ContentOnly => self.main_page().content_only().layout(area),
        };
        let Some(layout) = layout else {
            return false;
        };

        let visible = layout.body.height.max(1) as usize;
        self.main_viewport_rows.set(visible);

        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        self.state.clamp_selection(total);
        self.state.scroll_top = self.state.scroll_top.min(total.saturating_sub(1));
        let scroll_top = self.state.scroll_top;

        let mut selected = self
            .state
            .selected_idx
            .expect("selected_idx should be Some after clamp_selection");
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            total,
            |x, y| selection_index_at(layout.body, x, y, scroll_top, total),
            SelectableListMouseConfig {
                hover_select: false,
                require_pointer_hit_for_scroll: true,
                scroll_behavior: ScrollSelectionBehavior::Clamp,
                ..SelectableListMouseConfig::default()
            },
        );

        match result {
            SelectableListMouseResult::Ignored => false,
            SelectableListMouseResult::SelectionChanged | SelectableListMouseResult::Activated => {
                self.state.selected_idx = Some(selected);
                let visible = self.main_viewport_rows.get().max(1);
                self.state.ensure_visible(total, visible);
                if matches!(result, SelectableListMouseResult::Activated) {
                    self.activate_selected_row();
                }
                true
            }
        }
    }

    fn handle_mouse_event_edit_impl(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
        chrome: UiChrome,
    ) -> bool {
        let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind else {
            return false;
        };

        let ViewMode::EditWidth { field, error } = &mut self.mode else {
            unreachable!("handle_mouse_event_edit_impl called outside EditWidth mode")
        };

        let page = Self::edit_width_page(error.as_deref());
        let field_area = match chrome {
            UiChrome::Framed => page.framed().layout(area).map(|layout| layout.field),
            UiChrome::ContentOnly => page.content_only().layout(area).map(|layout| layout.field),
        };
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
            ViewMode::Main => self.handle_mouse_event_main_impl(mouse_event, area, UiChrome::ContentOnly),
            ViewMode::EditWidth { .. } => {
                self.handle_mouse_event_edit_impl(mouse_event, area, UiChrome::ContentOnly)
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
            ViewMode::Main => self.handle_mouse_event_main_impl(mouse_event, area, UiChrome::Framed),
            ViewMode::EditWidth { .. } => {
                self.handle_mouse_event_edit_impl(mouse_event, area, UiChrome::Framed)
            }
            ViewMode::CaptureHotkey { .. } | ViewMode::Transition => false,
        }
    }
}
