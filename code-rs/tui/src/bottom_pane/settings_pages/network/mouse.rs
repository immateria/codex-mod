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

impl NetworkSettingsView {
    pub(super) fn handle_mouse_event_direct_content(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main { mut show_advanced } => {
                let rows = self.build_rows(show_advanced);
                let total = rows.len();
                if total == 0 {
                    self.mode = ViewMode::Main { show_advanced };
                    return false;
                }

                if self.state.selected_idx.is_none() {
                    self.state.selected_idx = Some(0);
                }
                self.state.clamp_selection(total);

                let mut selected = self.state.selected_idx.unwrap_or(0);
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut selected,
                    total,
                    |x, y| self.selection_index_at_content(area, x, y, show_advanced),
                    SelectableListMouseConfig {
                        hover_select: false,
                        require_pointer_hit_for_scroll: true,
                        scroll_behavior: ScrollSelectionBehavior::Clamp,
                        ..SelectableListMouseConfig::default()
                    },
                );
                self.state.selected_idx = Some(selected);

                if matches!(result, SelectableListMouseResult::Activated)
                    && let Some(kind) = rows.get(selected).copied()
                {
                    self.activate_row(kind, &mut show_advanced);
                }

                if result.handled() {
                    self.reconcile_selection_state(show_advanced);
                }
                if matches!(self.mode, ViewMode::Transition) {
                    self.mode = ViewMode::Main { show_advanced };
                }
                result.handled()
            }
            ViewMode::EditList { target, mut field, show_advanced } => {
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let Some(field_area) = Self::edit_page(target)
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
                };
                self.mode = ViewMode::EditList {
                    target,
                    field,
                    show_advanced,
                };
                handled
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main { show_advanced: false };
                false
            }
        }
    }

    pub(super) fn handle_mouse_event_direct_framed(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main { mut show_advanced } => {
                let rows = self.build_rows(show_advanced);
                let total = rows.len();
                if total == 0 {
                    self.mode = ViewMode::Main { show_advanced };
                    return false;
                }

                if self.state.selected_idx.is_none() {
                    self.state.selected_idx = Some(0);
                }
                self.state.clamp_selection(total);

                let mut selected = self.state.selected_idx.unwrap_or(0);
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut selected,
                    total,
                    |x, y| self.selection_index_at(area, x, y, show_advanced),
                    SelectableListMouseConfig {
                        hover_select: false,
                        require_pointer_hit_for_scroll: true,
                        scroll_behavior: ScrollSelectionBehavior::Clamp,
                        ..SelectableListMouseConfig::default()
                    },
                );
                self.state.selected_idx = Some(selected);

                if matches!(result, SelectableListMouseResult::Activated)
                    && let Some(kind) = rows.get(selected).copied()
                {
                    self.activate_row(kind, &mut show_advanced);
                }

                if result.handled() {
                    self.reconcile_selection_state(show_advanced);
                }
                if matches!(self.mode, ViewMode::Transition) {
                    self.mode = ViewMode::Main { show_advanced };
                }
                result.handled()
            }
            ViewMode::EditList {
                target,
                mut field,
                show_advanced,
            } => {
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let Some(field_area) = Self::edit_page(target)
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
                };
                self.mode = ViewMode::EditList {
                    target,
                    field,
                    show_advanced,
                };
                handled
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main { show_advanced: false };
                false
            }
        }
    }

    fn selection_index_at(&self, area: Rect, x: u16, y: u16, show_advanced: bool) -> Option<usize> {
        let page = SettingsRowPage::new(" Network ", self.render_header_lines(), vec![]);
        let layout = page.framed().layout(area)?;
        SettingsRowPage::selection_index_at(
            layout.body,
            x,
            y,
            self.state.scroll_top,
            self.build_rows(show_advanced).len(),
        )
    }

    fn selection_index_at_content(
        &self,
        area: Rect,
        x: u16,
        y: u16,
        show_advanced: bool,
    ) -> Option<usize> {
        let page = SettingsRowPage::new(" Network ", self.render_header_lines(), vec![]);
        let layout = page.content_only().layout(area)?;
        SettingsRowPage::selection_index_at(
            layout.body,
            x,
            y,
            self.state.scroll_top,
            self.build_rows(show_advanced).len(),
        )
    }
}

