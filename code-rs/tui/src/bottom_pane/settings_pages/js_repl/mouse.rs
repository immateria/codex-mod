use super::*;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_in_body;
use crate::components::mode_guard::ModeGuard;
use crate::ui_interaction::{
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

impl JsReplSettingsView {
    fn handle_mouse_event_direct_impl(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
        chrome: ChromeMode,
    ) -> bool {
        let mut mode_guard = ModeGuard::replace(&mut self.mode, ViewMode::Transition, |mode| {
            matches!(mode, ViewMode::Transition)
        });
        match mode_guard.mode_mut() {
            ViewMode::Main => {
                let rows = self.build_rows();
                let total = rows.len();
                if total == 0 {
                    return false;
                }

                let page = self.main_page();
                let layout = page.layout_in_chrome(chrome, area);
                let Some(layout) = layout else {
                    return false;
                };
                let visible_slots = layout.visible_rows().max(1);
                self.viewport_rows.set(visible_slots);

                self.reconcile_selection_state(total);
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

                if matches!(outcome.result, SelectableListMouseResult::Activated)
                    && let Some(selected) = self.state.selected_idx
                    && let Some(kind) = rows.get(selected).copied()
                {
                    self.activate_row(kind);
                }

                if matches!(self.mode, ViewMode::Transition) {
                    // Activation can add/remove optional rows; keep selection + scroll valid.
                    self.reconcile_selection_state(self.row_count());
                }
                outcome.changed
            }
            ViewMode::EditText { target, field } => {
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let page = Self::text_edit_page(*target);
                        let field_area = page.layout_in_chrome(chrome, area).map(|layout| layout.field);
                        if let Some(field_area) = field_area {
                            field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
                        } else {
                            false
                        }
                    }
                    MouseEventKind::ScrollDown => field.handle_mouse_scroll(true),
                    MouseEventKind::ScrollUp => field.handle_mouse_scroll(false),
                    _ => false,
                };
                handled
            }
            ViewMode::EditList { target, field } => {
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let page = Self::list_edit_page(*target);
                        let field_area = page.layout_in_chrome(chrome, area).map(|layout| layout.field);
                        if let Some(field_area) = field_area {
                            field.handle_mouse_click(mouse_event.column, mouse_event.row, field_area)
                        } else {
                            false
                        }
                    }
                    MouseEventKind::ScrollDown => field.handle_mouse_scroll(true),
                    MouseEventKind::ScrollUp => field.handle_mouse_scroll(false),
                    _ => false,
                };
                handled
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }

    pub(super) fn handle_mouse_event_direct_content(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_impl(mouse_event, area, ChromeMode::ContentOnly)
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_impl(mouse_event, area, ChromeMode::Framed)
    }
}
