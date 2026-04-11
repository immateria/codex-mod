use super::*;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::rows::selection_index_at_over_text;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test;
use crate::bottom_pane::settings_ui::menu_rows::selection_id_at as selection_menu_id_at;
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
        let row_specs = self.main_row_specs(state.selected_idx.unwrap_or(0));
        let visible_rows = layout.visible_rows().max(1);
        let outcome = route_scroll_state_mouse_with_hit_test(
            mouse_event,
            &mut state,
            total,
            visible_rows,
            |x, y, scroll_top| selection_index_at_over_text(layout.body, x, y, scroll_top, &row_specs),
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

    fn handle_mouse_text_viewer(
        viewer: &TextViewerState,
        mouse_event: MouseEvent,
    ) -> bool {
        let total = viewer.lines.len();
        let visible = viewer.viewport_rows.get().max(1);
        let mut scroll = viewer.scroll_top.get();
        let changed = match mouse_event.kind {
            MouseEventKind::ScrollDown => {
                if scroll + visible < total {
                    scroll += 3;
                    scroll = scroll.min(total.saturating_sub(visible));
                    true
                } else {
                    false
                }
            }
            MouseEventKind::ScrollUp => {
                if scroll > 0 {
                    scroll = scroll.saturating_sub(3);
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        if changed {
            viewer.scroll_top.set(scroll);
        }
        changed
    }

    fn handle_mouse_rollout_list(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
        chrome: ChromeMode,
    ) -> bool {
        let ViewMode::RolloutList(ref list) = self.mode else {
            return false;
        };
        let total = list.entries.len();
        if total == 0 {
            return false;
        }

        let menu_rows = Self::rollout_list_menu_rows(list);
        let page = Self::rollout_list_page(list);
        let Some(layout) = page.layout_in_chrome(chrome, area) else {
            return false;
        };
        let visible = layout.body.height.max(1) as usize;
        list.viewport_rows.set(visible);

        let body = layout.body;
        let mut state = list.list_state.get();
        let outcome = route_scroll_state_mouse_with_hit_test(
            mouse_event,
            &mut state,
            total,
            visible,
            |x, y, scroll_top| selection_menu_id_at(body, x, y, scroll_top, &menu_rows),
            SelectableListMouseConfig {
                hover_select: false,
                activate_on_left_click: true,
                scroll_select: true,
                require_pointer_hit_for_scroll: false,
                scroll_behavior: ScrollSelectionBehavior::Wrap,
            },
        );
        list.list_state.set(state);

        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            let idx = state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));
            let slug = list.entries[idx].slug.clone();
            let ViewMode::RolloutList(list_state) = std::mem::replace(&mut self.mode, ViewMode::Transition) else {
                return false;
            };
            self.open_rollout_detail(list_state, &slug);
        }
        outcome.changed
    }

    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        match &self.mode {
            ViewMode::Main => {
                return self.handle_mouse_event_main_impl(mouse_event, area, ChromeMode::ContentOnly);
            }
            ViewMode::TextViewer(viewer) => {
                return Self::handle_mouse_text_viewer(viewer, mouse_event);
            }
            ViewMode::RolloutList(_) => {
                return self.handle_mouse_rollout_list(mouse_event, area, ChromeMode::ContentOnly);
            }
            _ => {}
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
            ViewMode::Main | ViewMode::Transition | ViewMode::TextViewer(_) | ViewMode::RolloutList(_) | ViewMode::SearchInput { .. } => false,
        }
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        match &self.mode {
            ViewMode::Main => {
                return self.handle_mouse_event_main_impl(mouse_event, area, ChromeMode::Framed);
            }
            ViewMode::TextViewer(viewer) => {
                return Self::handle_mouse_text_viewer(viewer, mouse_event);
            }
            ViewMode::RolloutList(_) => {
                return self.handle_mouse_rollout_list(mouse_event, area, ChromeMode::Framed);
            }
            _ => {}
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
            ViewMode::Main | ViewMode::Transition | ViewMode::TextViewer(_) | ViewMode::RolloutList(_) | ViewMode::SearchInput { .. } => false,
        }
    }
}
