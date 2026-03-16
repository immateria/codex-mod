use super::*;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::row_page::SettingsRowPage;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_in_body;
use crate::ui_interaction::{
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
                let Some(layout) = page.layout_in_chrome(chrome, area) else {
                    self.mode = ViewMode::Main;
                    return false;
                };
                let visible_slots = layout.visible_rows().max(1);
                self.viewport_rows.set(visible_slots);

                let mut state = self.state.get();
                let outcome = route_scroll_state_mouse_in_body(
                    mouse_event,
                    layout.body,
                    &mut state,
                    total,
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

                if matches!(self.mode, ViewMode::Transition) {
                    self.mode = ViewMode::Main;
                }
                outcome.changed
            }
            ViewMode::Edit { target, mut field, error } => {
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let Some(field_area) = Self::edit_page(target, error.as_deref())
                            .layout_in_chrome(chrome, area)
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

    pub(super) fn handle_mouse_event_direct_content(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.handle_mouse_event_direct_in_chrome(ChromeMode::ContentOnly, mouse_event, area)
    }

    pub(super) fn handle_mouse_event_direct_framed(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.handle_mouse_event_direct_in_chrome(ChromeMode::Framed, mouse_event, area)
    }
}
