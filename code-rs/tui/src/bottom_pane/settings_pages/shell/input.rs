use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl ShellSelectionView {
    pub(super) fn activate_edit_action(&mut self, action: EditAction) {
        match action {
            EditAction::Apply => self.submit_custom_path(),
            EditAction::Pick => {
                let _ = self.pick_shell_binary_from_dialog();
            }
            EditAction::Show => {
                let _ = self.show_custom_shell_in_file_manager();
            }
            EditAction::Resolve => {
                let _ = self.resolve_custom_shell_path_in_place();
            }
            EditAction::Style => self.cycle_custom_style_override(),
            EditAction::Back => {
                self.custom_input_mode = false;
                self.custom_field.set_text("");
                self.custom_style_override = None;
                self.native_picker_notice = None;
                self.edit_focus = EditFocus::Field;
                self.hovered_action = None;
            }
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        if self.custom_input_mode {
            return match (key_event.code, key_event.modifiers) {
                (KeyCode::Esc, _) => {
                    self.custom_input_mode = false;
                    self.custom_field.set_text("");
                    self.custom_style_override = None;
                    self.native_picker_notice = None;
                    self.edit_focus = EditFocus::Field;
                    self.hovered_action = None;
                    true
                }
                (KeyCode::Enter, _) => {
                    match self.edit_focus {
                        EditFocus::Field => self.submit_custom_path(),
                        EditFocus::Actions => self.activate_edit_action(self.selected_action),
                    }
                    true
                }
                (KeyCode::Char('o'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    self.pick_shell_binary_from_dialog()
                }
                (KeyCode::Char('v'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    self.show_custom_shell_in_file_manager()
                }
                (KeyCode::Tab, _) => {
                    self.edit_focus = match self.edit_focus {
                        EditFocus::Field => EditFocus::Actions,
                        EditFocus::Actions => EditFocus::Field,
                    };
                    true
                }
                (KeyCode::Char('p'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    self.open_shell_profiles_settings();
                    true
                }
                (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                    self.resolve_custom_shell_path_in_place()
                }
                (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
                    self.cycle_custom_style_override();
                    true
                }
                (KeyCode::Left, _) if matches!(self.edit_focus, EditFocus::Actions) => {
                    let items = self.edit_action_items();
                    let len = items.len();
                    let idx = items
                        .iter()
                        .position(|(id, _)| *id == self.selected_action)
                        .unwrap_or(0);
                    let next = if idx == 0 { len.saturating_sub(1) } else { idx - 1 };
                    self.selected_action = items[next].0;
                    true
                }
                (KeyCode::Right, _) if matches!(self.edit_focus, EditFocus::Actions) => {
                    let items = self.edit_action_items();
                    let idx = items
                        .iter()
                        .position(|(id, _)| *id == self.selected_action)
                        .unwrap_or(0);
                    let next = (idx + 1) % items.len();
                    self.selected_action = items[next].0;
                    true
                }
                _ => match self.edit_focus {
                    EditFocus::Field => self.custom_field.handle_key(key_event),
                    EditFocus::Actions => false,
                },
            };
        }

        match (key_event.code, key_event.modifiers) {
            (KeyCode::Esc, _) | (KeyCode::Char('q'), KeyModifiers::NONE) => {
                self.send_closed(false);
                true
            }
            (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
                self.move_selection_up();
                true
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
                self.move_selection_down();
                true
            }
            (KeyCode::Char('p'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                self.open_shell_profiles_settings();
                true
            }
            (KeyCode::Char('e'), KeyModifiers::NONE) => {
                let (prefill, style) = self.prefill_for_selection(self.selected_index);
                self.open_custom_input_with_prefill(prefill, style);
                true
            }
            (KeyCode::Char('p'), KeyModifiers::NONE) => {
                self.pin_selected_shell_binary();
                true
            }
            (KeyCode::Right, _) => {
                let (prefill, style) = self.prefill_for_selection(self.selected_index);
                self.open_custom_input_with_prefill(prefill, style);
                true
            }
            (KeyCode::Enter, _) => {
                self.confirm_selection();
                true
            }
            _ => false,
        }
    }

    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        if !self.custom_input_mode {
            return false;
        }

        if text.is_empty() {
            return false;
        }

        self.edit_focus = EditFocus::Field;
        self.hovered_action = None;
        self.custom_field.handle_paste(text);
        true
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }
}
