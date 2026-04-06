use super::*;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::row_page::SettingsRowPage;
use crate::bottom_pane::settings_ui::rows::selection_index_at_over_text;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test;
use crate::components::mode_guard::ModeGuard;
use crate::ui_interaction::{
    contains_point,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

impl ExecLimitsSettingsView {
    fn handle_mouse_event_direct_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let mut mode_guard = ModeGuard::replace(&mut self.mode, ViewMode::Transition, |mode| {
            matches!(mode, ViewMode::Transition)
        });
        match mode_guard.mode_mut() {
            ViewMode::Main => {
                let rows = Self::build_rows();
                let total = rows.len();
                if total == 0 {
                    return false;
                }

                let page = SettingsRowPage::new(
                    " Exec Limits ",
                    self.render_header_lines(),
                    self.render_footer_lines(),
                );
                let Some(layout) = page.layout_in_chrome(chrome, area) else {
                    return false;
                };
                let visible_slots = layout.visible_rows().max(1);
                self.viewport_rows.set(visible_slots);

                let row_specs = self.main_row_specs(&rows);
                let mut state = self.state.get();
                let kind = mouse_event.kind;
                let outcome = route_scroll_state_mouse_with_hit_test(
                    mouse_event,
                    &mut state,
                    total,
                    visible_slots,
                    |x, y, scroll_top| {
                        if matches!(kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown) {
                            if !contains_point(layout.body, x, y) {
                                return None;
                            }
                            let rel = y.saturating_sub(layout.body.y) as usize;
                            Some(scroll_top.saturating_add(rel).min(total.saturating_sub(1)))
                        } else {
                            selection_index_at_over_text(layout.body, x, y, scroll_top, &row_specs)
                        }
                    },
                    SelectableListMouseConfig {
                        hover_select: false,
                        require_pointer_hit_for_scroll: true,
                        scroll_behavior: ScrollSelectionBehavior::Clamp,
                        ..SelectableListMouseConfig::default()
                    },
                );
                let selected = state.selected_idx.unwrap_or(0);
                self.state.set(state);

                if matches!(outcome.result, SelectableListMouseResult::Activated)
                    && let Some(kind) = rows.get(selected).copied()
                {
                    self.activate_row(kind);
                }

                outcome.changed
            }
            ViewMode::Edit { target, field, error } => {
                match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let Some(field_area) = Self::edit_page(*target, error.as_deref())
                            .layout_in_chrome(chrome, area)
                            .map(|layout| layout.field)
                        else {
                            return false;
                        };
                        field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
                    }
                    _ => false,
                }
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }

    pub(super) fn handle_mouse_event_direct_content(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.handle_mouse_event_direct_in_chrome(ChromeMode::ContentOnly, mouse_event, area)
    }

    pub(super) fn handle_mouse_event_direct_framed(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.handle_mouse_event_direct_in_chrome(ChromeMode::Framed, mouse_event, area)
    }
}
