use super::*;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::ui_interaction::{
    route_selectable_list_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};
use crate::bottom_pane::settings_ui::row_page::SettingsRowPage;

impl JsReplSettingsView {
    fn handle_mouse_event_direct_impl(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
        chrome: UiChrome,
    ) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main => {
                let rows = self.build_rows();
                let total = rows.len();
                if total == 0 {
                    self.mode = ViewMode::Main;
                    return false;
                }

                let page = self.main_page();
                let layout = match chrome {
                    UiChrome::Framed => page.framed().layout(area),
                    UiChrome::ContentOnly => page.content_only().layout(area),
                };
                let Some(layout) = layout else {
                    self.mode = ViewMode::Main;
                    return false;
                };
                let visible_slots = layout.visible_rows().max(1);
                self.viewport_rows.set(visible_slots);

                self.reconcile_selection_state(total);
                let scroll_top = self.state.scroll_top;
                let body = layout.body;
                let mut selected = self.state.selected_idx.unwrap_or(0);
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
                self.state.selected_idx = Some(selected);
                self.state.ensure_visible(total, visible_slots.min(total));

                if matches!(result, SelectableListMouseResult::Activated)
                    && let Some(kind) = rows.get(selected).copied()
                {
                    self.activate_row(kind);
                }

                if matches!(self.mode, ViewMode::Transition) {
                    // Activation can add/remove optional rows; keep selection + scroll valid.
                    self.reconcile_selection_state(self.row_count());
                    self.mode = ViewMode::Main;
                }
                result.handled()
            }
            ViewMode::EditText { target, mut field } => {
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let page = Self::text_edit_page(target);
                        let field_area = match chrome {
                            UiChrome::Framed => page.framed().layout(area).map(|layout| layout.field),
                            UiChrome::ContentOnly => page
                                .content_only()
                                .layout(area)
                                .map(|layout| layout.field),
                        };
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
                self.mode = ViewMode::EditText { target, field };
                handled
            }
            ViewMode::EditList { target, mut field } => {
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let page = Self::list_edit_page(target);
                        let field_area = match chrome {
                            UiChrome::Framed => page.framed().layout(area).map(|layout| layout.field),
                            UiChrome::ContentOnly => page
                                .content_only()
                                .layout(area)
                                .map(|layout| layout.field),
                        };
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
                self.mode = ViewMode::EditList { target, field };
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
        self.handle_mouse_event_direct_impl(mouse_event, area, UiChrome::ContentOnly)
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_impl(mouse_event, area, UiChrome::Framed)
    }
}
