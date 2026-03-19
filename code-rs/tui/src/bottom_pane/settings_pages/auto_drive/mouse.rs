use super::*;

use crate::app_event::AppEvent;
use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test_no_ensure_visible;
use crate::ui_interaction::{SelectableListMouseConfig, SelectableListMouseResult};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

impl AutoDriveSettingsView {
    fn routing_checkbox_hit_test(body: Rect, x: u16) -> bool {
        // Kept consistent with pre-refactor behavior: clicking the "checkbox column" at the
        // start of a routing row toggles enabled, while the rest of the row opens the editor.
        let start = body.x.saturating_add(2);
        let end = body.x.saturating_add(5);
        x >= start && x < end
    }

    fn update_hover_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_pos: (u16, u16),
        area: Rect,
    ) -> bool {
        if area.width == 0 || area.height == 0 {
            return false;
        }

        match &self.mode {
            AutoDriveSettingsMode::Main => {
                let rows = self.main_menu_rows();
                let Some(layout) = self.page().layout_in_chrome(chrome, area) else {
                    return self.set_hovered(None);
                };
                let hovered = SettingsMenuPage::selection_menu_id_in_body(
                    layout.body,
                    mouse_pos.0,
                    mouse_pos.1,
                    0,
                    &rows,
                )
                .map(HoverTarget::MainOption);
                self.set_hovered(hovered)
            }
            AutoDriveSettingsMode::RoutingList => {
                let rows = self.routing_list_menu_rows();
                let Some(layout) = self.page().layout_in_chrome(chrome, area) else {
                    return self.set_hovered(None);
                };
                let scroll_top = self.routing_state.scroll_top.min(rows.len().saturating_sub(1));
                let hovered = SettingsMenuPage::selection_menu_id_in_body(
                    layout.body,
                    mouse_pos.0,
                    mouse_pos.1,
                    scroll_top,
                    &rows,
                )
                .map(HoverTarget::RoutingRow);
                self.set_hovered(hovered)
            }
            AutoDriveSettingsMode::RoutingEditor(editor) => {
                let editor = editor.clone();
                let page = self.routing_editor_page(&editor);
                let Some(layout) = page.layout_in_chrome(chrome, area) else {
                    return self.set_hovered(None);
                };

                let buttons = self.routing_editor_action_buttons(editor.selected_field);
                if let Some(action) = page.standard_action_at_end(
                    &layout,
                    mouse_pos.0,
                    mouse_pos.1,
                    &buttons,
                ) {
                    return self.set_hovered(Some(HoverTarget::RoutingEditor(action)));
                }

                let rows = self.routing_editor_menu_rows(&editor);
                let hovered = SettingsMenuPage::selection_menu_id_in_body(
                    layout.body,
                    mouse_pos.0,
                    mouse_pos.1,
                    0,
                    &rows,
                )
                .map(HoverTarget::RoutingEditor);
                self.set_hovered(hovered)
            }
        }
    }

    pub(super) fn update_hover_internal(&mut self, mouse_pos: (u16, u16), area: Rect) -> bool {
        self.update_hover_in_chrome(ChromeMode::Framed, mouse_pos, area)
    }

    fn handle_mouse_event_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        if area.width == 0 || area.height == 0 {
            return false;
        }

        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown => {}
            _ => return false,
        }

        match &self.mode {
            AutoDriveSettingsMode::Main => {
                let rows = self.main_menu_rows();
                let Some(layout) = self.page().layout_in_chrome(chrome, area) else {
                    return false;
                };

                // The main list is never scroll-rendered; keep scroll pinned.
                self.main_state.scroll_top = 0;
                let config = SelectableListMouseConfig {
                    hover_select: false,
                    require_pointer_hit_for_scroll: true,
                    ..SelectableListMouseConfig::default()
                };
                let outcome = route_scroll_state_mouse_with_hit_test_no_ensure_visible(
                    mouse_event,
                    &mut self.main_state,
                    rows.len(),
                    |x, y, scroll_top| {
                        SettingsMenuPage::selection_menu_id_in_body(layout.body, x, y, scroll_top, &rows)
                    },
                    config,
                );
                self.main_state.scroll_top = 0;

                let mut changed = outcome.changed;
                if matches!(outcome.result, SelectableListMouseResult::Activated) {
                    self.toggle_selected();
                    changed = true;
                }
                changed
            }
            AutoDriveSettingsMode::RoutingList => {
                let rows = self.routing_list_menu_rows();
                let total = rows.len();
                let Some(layout) = self.page().layout_in_chrome(chrome, area) else {
                    return false;
                };
                let visible_rows = layout.body.height.max(1) as usize;
                self.routing_viewport_rows.set(visible_rows);

                let config = SelectableListMouseConfig {
                    hover_select: false,
                    require_pointer_hit_for_scroll: true,
                    ..SelectableListMouseConfig::default()
                };
                let outcome = route_scroll_state_mouse_with_hit_test(
                    mouse_event,
                    &mut self.routing_state,
                    total,
                    visible_rows,
                    |x, y, scroll_top| {
                        SettingsMenuPage::selection_menu_id_in_body(layout.body, x, y, scroll_top, &rows)
                    },
                    config,
                );

                let mut changed = outcome.changed;
                if matches!(outcome.result, SelectableListMouseResult::Activated) {
                    let idx = self
                        .routing_state
                        .selected_idx
                        .unwrap_or(0);
                    if idx >= self.model_routing_entries.len() {
                        self.open_routing_editor(None);
                    } else {
                        if Self::routing_checkbox_hit_test(layout.body, mouse_event.column) {
                            self.try_toggle_routing_entry_enabled(idx);
                        } else {
                            self.open_routing_editor(Some(idx));
                        }
                    }
                    changed = true;
                }
                changed
            }
            AutoDriveSettingsMode::RoutingEditor(editor) => {
                if !matches!(mouse_event.kind, MouseEventKind::Down(MouseButton::Left)) {
                    return false;
                }

                let editor = editor.clone();
                let page = self.routing_editor_page(&editor);
                let Some(layout) = page.layout_in_chrome(chrome, area) else {
                    return false;
                };

                let buttons = self.routing_editor_action_buttons(editor.selected_field);
                if let Some(action) = page.standard_action_at_end(
                    &layout,
                    mouse_event.column,
                    mouse_event.row,
                    &buttons,
                ) {
                    return match action {
                        RoutingEditorField::Save => {
                            self.save_routing_editor();
                            true
                        }
                        RoutingEditorField::Cancel => {
                            self.close_routing_editor();
                            true
                        }
                        _ => false,
                    };
                }

                let rows = self.routing_editor_menu_rows(&editor);
                match SettingsMenuPage::selection_menu_id_in_body(
                    layout.body,
                    mouse_event.column,
                    mouse_event.row,
                    0,
                    &rows,
                ) {
                    Some(RoutingEditorField::Model) => {
                        let has_models = !self.routing_model_options.is_empty();
                        let model_options_len = self.routing_model_options.len();
                        self.update_routing_editor(|editor| {
                            editor.selected_field = RoutingEditorField::Model;
                            if has_models {
                                editor.model_cursor = (editor.model_cursor + 1) % model_options_len;
                            }
                        });
                        true
                    }
                    Some(RoutingEditorField::Enabled) => {
                        self.update_routing_editor(|editor| {
                            editor.selected_field = RoutingEditorField::Enabled;
                            editor.enabled = !editor.enabled;
                        });
                        true
                    }
                    Some(RoutingEditorField::Reasoning) => {
                        self.update_routing_editor(|editor| {
                            editor.selected_field = RoutingEditorField::Reasoning;
                            editor.toggle_reasoning_at_cursor();
                        });
                        true
                    }
                    Some(RoutingEditorField::Description) => {
                        let mut changed = false;
                        self.update_routing_editor(|editor| {
                            if editor.selected_field != RoutingEditorField::Description {
                                editor.selected_field = RoutingEditorField::Description;
                                changed = true;
                            }
                        });
                        changed
                    }
                    Some(RoutingEditorField::Save | RoutingEditorField::Cancel) => false,
                    None => false,
                }
            }
        }
    }

    pub(super) fn handle_mouse_event_internal(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.handle_mouse_event_in_chrome(ChromeMode::Framed, mouse_event, area)
    }

    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let handled = match mouse_event.kind {
            MouseEventKind::Moved => self.update_hover_in_chrome(
                ChromeMode::ContentOnly,
                (mouse_event.column, mouse_event.row),
                area,
            ),
            _ => self.handle_mouse_event_in_chrome(ChromeMode::ContentOnly, mouse_event, area),
        };

        if handled {
            self.app_event_tx.send(AppEvent::RequestRedraw);
        }
        handled
    }
}
