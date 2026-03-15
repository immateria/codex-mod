use super::*;

use crate::app_event::AppEvent;
use crate::ui_interaction::{route_selectable_list_mouse_with_config, SelectableListMouseConfig, SelectableListMouseResult};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::settings_ui::menu_rows::selection_id_at as selection_menu_id_at;

impl AutoDriveSettingsView {
    pub(super) fn update_hover_internal(&mut self, mouse_pos: (u16, u16), area: Rect) -> bool {
        match &self.mode {
            AutoDriveSettingsMode::Main => {
                let rows = self.main_menu_rows();
                let body = self.page().framed().layout(area).map(|layout| layout.body).unwrap_or(area);
                let hovered = selection_menu_id_at(body, mouse_pos.0, mouse_pos.1, 0, &rows)
                    .map(HoverTarget::MainOption);
                self.set_hovered(hovered)
            }
            AutoDriveSettingsMode::RoutingList => {
                let rows = self.routing_list_menu_rows();
                let body = self.page().framed().layout(area).map(|layout| layout.body).unwrap_or(area);
                let hovered = selection_menu_id_at(body, mouse_pos.0, mouse_pos.1, 0, &rows)
                    .map(HoverTarget::RoutingRow);
                self.set_hovered(hovered)
            }
            AutoDriveSettingsMode::RoutingEditor(editor) => {
                let editor = editor.clone();
                let page = self.routing_editor_page(&editor);
                let Some(layout) = page.framed().layout(area) else {
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
                let hovered = selection_menu_id_at(layout.body, mouse_pos.0, mouse_pos.1, 0, &rows)
                    .map(HoverTarget::RoutingEditor);
                self.set_hovered(hovered)
            }
        }
    }

    fn update_hover_internal_content_only(&mut self, mouse_pos: (u16, u16), area: Rect) -> bool {
        match &self.mode {
            AutoDriveSettingsMode::Main => {
                let rows = self.main_menu_rows();
                let hovered = selection_menu_id_at(area, mouse_pos.0, mouse_pos.1, 0, &rows)
                    .map(HoverTarget::MainOption);
                self.set_hovered(hovered)
            }
            AutoDriveSettingsMode::RoutingList => {
                let rows = self.routing_list_menu_rows();
                let hovered = selection_menu_id_at(area, mouse_pos.0, mouse_pos.1, 0, &rows)
                    .map(HoverTarget::RoutingRow);
                self.set_hovered(hovered)
            }
            AutoDriveSettingsMode::RoutingEditor(editor) => {
                let editor = editor.clone();
                let rows = self.routing_editor_menu_rows(&editor);
                let hovered = selection_menu_id_at(area, mouse_pos.0, mouse_pos.1, 0, &rows)
                    .map(HoverTarget::RoutingEditor);
                self.set_hovered(hovered)
            }
        }
    }

    pub(super) fn handle_mouse_event_internal(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        if area.width == 0 || area.height == 0 {
            return false;
        }

        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {}
            _ => return false,
        }

        match &self.mode {
            AutoDriveSettingsMode::Main => {
                let rows = self.main_menu_rows();
                let body = self.page().framed().layout(area).map(|layout| layout.body).unwrap_or(area);
                let config = SelectableListMouseConfig {
                    hover_select: false,
                    require_pointer_hit_for_scroll: true,
                    ..SelectableListMouseConfig::default()
                };
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut self.selected_index,
                    Self::option_count(),
                    |x, y| selection_menu_id_at(body, x, y, 0, &rows),
                    config,
                );

                match result {
                    SelectableListMouseResult::Activated => {
                        self.toggle_selected();
                        true
                    }
                    SelectableListMouseResult::SelectionChanged => true,
                    SelectableListMouseResult::Ignored => false,
                }
            }
            AutoDriveSettingsMode::RoutingList => {
                let total = self.routing_row_count();
                let rows = self.routing_list_menu_rows();
                let body = self.page().framed().layout(area).map(|layout| layout.body).unwrap_or(area);
                let config = SelectableListMouseConfig {
                    hover_select: false,
                    require_pointer_hit_for_scroll: true,
                    ..SelectableListMouseConfig::default()
                };
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut self.routing_selected_index,
                    total,
                    |x, y| selection_menu_id_at(body, x, y, 0, &rows),
                    config,
                );

                match result {
                    SelectableListMouseResult::Activated => {
                        let idx = self.routing_selected_index;
                        if idx >= self.model_routing_entries.len() {
                            self.open_routing_editor(None);
                        } else {
                            let checkbox_start = body.x.saturating_add(2);
                            let checkbox_end = body.x.saturating_add(5);
                            if mouse_event.column >= checkbox_start && mouse_event.column < checkbox_end
                            {
                                self.try_toggle_routing_entry_enabled(idx);
                            } else {
                                self.open_routing_editor(Some(idx));
                            }
                        }
                        true
                    }
                    SelectableListMouseResult::SelectionChanged => true,
                    SelectableListMouseResult::Ignored => false,
                }
            }
            AutoDriveSettingsMode::RoutingEditor(editor) => {
                if !matches!(mouse_event.kind, MouseEventKind::Down(MouseButton::Left)) {
                    return false;
                }
                let editor = editor.clone();
                let page = self.routing_editor_page(&editor);
                let Some(layout) = page.framed().layout(area) else {
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
                match selection_menu_id_at(
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

    fn handle_mouse_event_internal_content_only(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        if area.width == 0 || area.height == 0 {
            return false;
        }

        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {}
            _ => return false,
        }

        match &self.mode {
            AutoDriveSettingsMode::Main => {
                let rows = self.main_menu_rows();
                let config = SelectableListMouseConfig {
                    hover_select: false,
                    require_pointer_hit_for_scroll: true,
                    ..SelectableListMouseConfig::default()
                };
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut self.selected_index,
                    Self::option_count(),
                    |x, y| selection_menu_id_at(area, x, y, 0, &rows),
                    config,
                );

                match result {
                    SelectableListMouseResult::Activated => {
                        self.toggle_selected();
                        true
                    }
                    SelectableListMouseResult::SelectionChanged => true,
                    SelectableListMouseResult::Ignored => false,
                }
            }
            AutoDriveSettingsMode::RoutingList => {
                let total = self.routing_row_count();
                let rows = self.routing_list_menu_rows();
                let config = SelectableListMouseConfig {
                    hover_select: false,
                    require_pointer_hit_for_scroll: true,
                    ..SelectableListMouseConfig::default()
                };
                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut self.routing_selected_index,
                    total,
                    |x, y| selection_menu_id_at(area, x, y, 0, &rows),
                    config,
                );

                match result {
                    SelectableListMouseResult::Activated => {
                        let idx = self.routing_selected_index;
                        if idx >= self.model_routing_entries.len() {
                            self.open_routing_editor(None);
                        } else {
                            let checkbox_start = area.x.saturating_add(2);
                            let checkbox_end = area.x.saturating_add(5);
                            if mouse_event.column >= checkbox_start && mouse_event.column < checkbox_end
                            {
                                self.try_toggle_routing_entry_enabled(idx);
                            } else {
                                self.open_routing_editor(Some(idx));
                            }
                        }
                        true
                    }
                    SelectableListMouseResult::SelectionChanged => true,
                    SelectableListMouseResult::Ignored => false,
                }
            }
            AutoDriveSettingsMode::RoutingEditor(editor) => {
                if !matches!(mouse_event.kind, MouseEventKind::Down(MouseButton::Left)) {
                    return false;
                }
                let editor = editor.clone();
                let rows = self.routing_editor_menu_rows(&editor);
                match selection_menu_id_at(area, mouse_event.column, mouse_event.row, 0, &rows) {
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

    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let handled = match mouse_event.kind {
            MouseEventKind::Moved => {
                self.update_hover_internal_content_only((mouse_event.column, mouse_event.row), area)
            }
            _ => self.handle_mouse_event_internal_content_only(mouse_event, area),
        };

        if handled {
            self.app_event_tx.send(AppEvent::RequestRedraw);
        }
        handled
    }
}
