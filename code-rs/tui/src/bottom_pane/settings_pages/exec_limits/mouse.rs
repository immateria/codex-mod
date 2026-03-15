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

impl ExecLimitsSettingsView {
    pub(super) fn handle_mouse_event_direct_content(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main => {
                let rows = Self::build_rows();
                let total = rows.len();
                if total == 0 {
                    self.mode = ViewMode::Main;
                    return false;
                }

                let page = SettingsRowPage::new(
                    " Exec Limits ",
                    self.render_header_lines(),
                    self.render_footer_lines(),
                );
                let Some(layout) = page.content_only().layout(area) else {
                    self.mode = ViewMode::Main;
                    return false;
                };
                let visible_slots = layout.visible_rows().max(1);
                self.viewport_rows.set(visible_slots);

                let mut state = self.state.get();
                state.clamp_selection(total);
                let scroll_top = state.scroll_top;
                let body = layout.body;
                let mut selected = state.selected_idx.unwrap_or(0);
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut selected,
                    total,
                    |x, y| SettingsRowPage::selection_index_at(body, x, y, scroll_top, total),
                    SelectableListMouseConfig {
                        hover_select: false,
                        require_pointer_hit_for_scroll: true,
                        scroll_behavior: ScrollSelectionBehavior::Clamp,
                        ..SelectableListMouseConfig::default()
                    },
                );
                state.selected_idx = Some(selected);
                state.ensure_visible(total, visible_slots);
                self.state.set(state);

                if matches!(result, SelectableListMouseResult::Activated)
                    && let Some(kind) = rows.get(selected).copied()
                {
                    self.activate_row(kind);
                }

                if matches!(self.mode, ViewMode::Transition) {
                    self.mode = ViewMode::Main;
                }
                result.handled()
            }
            ViewMode::Edit { target, mut field, error } => {
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let Some(field_area) = Self::edit_page(target, error.as_deref())
                            .content_only()
                            .layout(area)
                            .map(|layout| layout.field)
                        else {
                            return false;
                        };
                        field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
                    }
                    _ => false,
                };
                self.mode = ViewMode::Edit { target, field, error };
                handled
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }

    pub(super) fn handle_mouse_event_direct_framed(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main => {
                let rows = Self::build_rows();
                let total = rows.len();
                if total == 0 {
                    self.mode = ViewMode::Main;
                    return false;
                }

                let page = SettingsRowPage::new(
                    " Exec Limits ",
                    self.render_header_lines(),
                    self.render_footer_lines(),
                );
                let Some(layout) = page.framed().layout(area) else {
                    self.mode = ViewMode::Main;
                    return false;
                };
                let visible_slots = layout.visible_rows().max(1);
                self.viewport_rows.set(visible_slots);

                let mut state = self.state.get();
                state.clamp_selection(total);
                let scroll_top = state.scroll_top;
                let body = layout.body;
                let mut selected = state.selected_idx.unwrap_or(0);
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut selected,
                    total,
                    |x, y| SettingsRowPage::selection_index_at(body, x, y, scroll_top, total),
                    SelectableListMouseConfig {
                        hover_select: false,
                        require_pointer_hit_for_scroll: true,
                        scroll_behavior: ScrollSelectionBehavior::Clamp,
                        ..SelectableListMouseConfig::default()
                    },
                );
                state.selected_idx = Some(selected);
                state.ensure_visible(total, visible_slots);
                self.state.set(state);

                if matches!(result, SelectableListMouseResult::Activated)
                    && let Some(kind) = rows.get(selected).copied()
                {
                    self.activate_row(kind);
                }

                if matches!(self.mode, ViewMode::Transition) {
                    self.mode = ViewMode::Main;
                }
                result.handled()
            }
            ViewMode::Edit { target, mut field, error } => {
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let Some(field_area) = Self::edit_page(target, error.as_deref())
                            .framed()
                            .layout(area)
                            .map(|layout| layout.field)
                        else {
                            return false;
                        };
                        field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
                    }
                    _ => false,
                };
                self.mode = ViewMode::Edit { target, field, error };
                handled
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }
}

