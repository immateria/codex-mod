use super::*;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::settings_ui::row_page::SettingsRowPage;
use crate::ui_interaction::{
    route_selectable_list_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

impl MemoriesSettingsView {
    fn selection_index_at_framed(&self, x: u16, y: u16, area: Rect) -> Option<usize> {
        let layout = self.main_page().framed().layout(area)?;
        SettingsRowPage::selection_index_at(
            layout.body,
            x,
            y,
            self.state.get().scroll_top,
            Self::rows().len(),
        )
    }

    fn selection_index_at_content_only(&self, x: u16, y: u16, area: Rect) -> Option<usize> {
        let layout = self.main_page().content_only().layout(area)?;
        SettingsRowPage::selection_index_at(
            layout.body,
            x,
            y,
            self.state.get().scroll_top,
            Self::rows().len(),
        )
    }

    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        if matches!(self.mode, ViewMode::Main) {
            let rows = Self::rows();
            let mut selected = self.state.get().selected_idx.unwrap_or(0);
            let result = route_selectable_list_mouse_with_config(
                mouse_event,
                &mut selected,
                rows.len(),
                |x, y| self.selection_index_at_content_only(x, y, area),
                SelectableListMouseConfig {
                    hover_select: false,
                    activate_on_left_click: true,
                    scroll_select: true,
                    require_pointer_hit_for_scroll: false,
                    scroll_behavior: ScrollSelectionBehavior::Wrap,
                },
            );
            let mut state = self.state.get();
            state.selected_idx = Some(selected);
            state.ensure_visible(rows.len(), self.viewport_rows.get().max(1));
            self.state.set(state);
            if matches!(result, SelectableListMouseResult::Activated) {
                self.activate_selected();
            }
            return result.handled();
        }

        match &mut self.mode {
            ViewMode::Edit { target, field, error } => match mouse_event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let Some(field_area) = Self::edit_page(self.scope, *target, error.as_deref())
                        .content_only()
                        .layout(area)
                        .map(|layout| layout.field)
                    else {
                        return false;
                    };
                    field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
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
            let rows = Self::rows();
            let mut selected = self.state.get().selected_idx.unwrap_or(0);
            let result = route_selectable_list_mouse_with_config(
                mouse_event,
                &mut selected,
                rows.len(),
                |x, y| self.selection_index_at_framed(x, y, area),
                SelectableListMouseConfig {
                    hover_select: false,
                    activate_on_left_click: true,
                    scroll_select: true,
                    require_pointer_hit_for_scroll: false,
                    scroll_behavior: ScrollSelectionBehavior::Wrap,
                },
            );
            let mut state = self.state.get();
            state.selected_idx = Some(selected);
            state.ensure_visible(rows.len(), self.viewport_rows.get().max(1));
            self.state.set(state);
            if matches!(result, SelectableListMouseResult::Activated) {
                self.activate_selected();
            }
            return result.handled();
        }

        match &mut self.mode {
            ViewMode::Edit { target, field, error } => match mouse_event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let Some(field_area) = Self::edit_page(self.scope, *target, error.as_deref())
                        .framed()
                        .layout(area)
                        .map(|layout| layout.field)
                    else {
                        return false;
                    };
                    field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
                }
                MouseEventKind::ScrollDown => field.handle_mouse_scroll(true),
                MouseEventKind::ScrollUp => field.handle_mouse_scroll(false),
                _ => false,
            },
            ViewMode::Main | ViewMode::Transition => false,
        }
    }
}

