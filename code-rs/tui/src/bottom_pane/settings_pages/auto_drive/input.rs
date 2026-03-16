use super::*;

use crate::app_event::AppEvent;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

impl AutoDriveSettingsView {
    fn handle_main_key(&mut self, key_event: KeyEvent) -> bool {
        match key_event.code {
            KeyCode::Esc => {
                self.close();
                true
            }
            KeyCode::Up => {
                self.main_state.move_up_wrap(Self::option_count());
                // The main list is never scroll-rendered; keep scroll pinned.
                self.main_state.scroll_top = 0;
                true
            }
            KeyCode::Down => {
                self.main_state.move_down_wrap(Self::option_count());
                // The main list is never scroll-rendered; keep scroll pinned.
                self.main_state.scroll_top = 0;
                true
            }
            KeyCode::Left => {
                if self.main_state.selected_idx == Some(5) {
                    self.cycle_continue_mode(false);
                    true
                } else {
                    false
                }
            }
            KeyCode::Right => {
                if self.main_state.selected_idx == Some(5) {
                    self.cycle_continue_mode(true);
                    true
                } else {
                    false
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.toggle_selected();
                true
            }
            _ => false,
        }
    }

    fn handle_routing_list_key(&mut self, key_event: KeyEvent) -> bool {
        match key_event.code {
            KeyCode::Esc => {
                self.mode = AutoDriveSettingsMode::Main;
                self.clear_status_message();
                self.clear_hovered();
                true
            }
            KeyCode::Up => {
                let total = self.routing_row_count();
                self.routing_state.move_up_wrap(total);
                let visible = self.routing_viewport_rows.get().max(1);
                self.routing_state.ensure_visible(total, visible);
                true
            }
            KeyCode::Down => {
                let total = self.routing_row_count();
                self.routing_state.move_down_wrap(total);
                let visible = self.routing_viewport_rows.get().max(1);
                self.routing_state.ensure_visible(total, visible);
                true
            }
            KeyCode::Enter => {
                let idx = self
                    .routing_state
                    .selected_idx
                    .unwrap_or(0)
                    .min(self.routing_row_count().saturating_sub(1));
                if idx >= self.model_routing_entries.len() {
                    self.open_routing_editor(None);
                } else {
                    self.open_routing_editor(Some(idx));
                }
                true
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.open_routing_editor(None);
                true
            }
            KeyCode::Char(' ') => {
                let idx = self
                    .routing_state
                    .selected_idx
                    .unwrap_or(0)
                    .min(self.routing_row_count().saturating_sub(1));
                if idx < self.model_routing_entries.len() {
                    self.try_toggle_routing_entry_enabled(idx);
                    true
                } else {
                    false
                }
            }
            KeyCode::Delete | KeyCode::Backspace | KeyCode::Char('d') | KeyCode::Char('D') => {
                let idx = self
                    .routing_state
                    .selected_idx
                    .unwrap_or(0)
                    .min(self.routing_row_count().saturating_sub(1));
                if idx < self.model_routing_entries.len() {
                    self.try_remove_routing_entry(idx);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub(super) fn save_routing_editor(&mut self) {
        let AutoDriveSettingsMode::RoutingEditor(editor) = &self.mode else {
            return;
        };
        let editor = editor.clone();

        let model = self
            .routing_model_options
            .get(editor.model_cursor)
            .cloned()
            .unwrap_or_else(Self::default_routing_model);
        let reasoning_levels = editor.selected_reasoning_levels();
        if reasoning_levels.is_empty() {
            self.set_status_message("Select at least one reasoning level.");
            return;
        }

        let entry = AutoDriveModelRoutingEntry {
            model,
            enabled: editor.enabled,
            reasoning_levels,
            description: editor.description.trim().to_string(),
        };

        let mut updated_entries = self.model_routing_entries.clone();
        if let Some(index) = editor.index {
            if let Some(slot) = updated_entries.get_mut(index) {
                *slot = entry;
            } else {
                updated_entries.push(entry);
            }
        } else {
            updated_entries.push(entry);
        }

        let sanitized = Self::sanitize_routing_entries(updated_entries);
        if sanitized.is_empty() {
            self.set_status_message("At least one valid gpt-* routing entry is required.");
            return;
        }
        if self.model_routing_enabled && !sanitized.iter().any(|entry| entry.enabled) {
            self.set_status_message("At least one routing entry must stay enabled.");
            return;
        }

        self.model_routing_entries = sanitized;
        let row_count = self.routing_row_count();
        self.routing_state.selected_idx = Some(
            editor
                .index
                .unwrap_or_else(|| self.model_routing_entries.len().saturating_sub(1))
                .min(row_count.saturating_sub(1)),
        );
        let visible = self.routing_viewport_rows.get().max(1);
        self.routing_state.ensure_visible(row_count, visible);
        self.mode = AutoDriveSettingsMode::RoutingList;
        self.send_update();
        self.clear_status_message();
    }

    pub(super) fn update_routing_editor<F>(&mut self, updater: F)
    where
        F: FnOnce(&mut RoutingEditorState),
    {
        if let AutoDriveSettingsMode::RoutingEditor(editor) = &mut self.mode {
            updater(editor);
        }
    }

    fn handle_routing_editor_key(&mut self, key_event: KeyEvent) -> bool {
        match key_event.code {
            KeyCode::Esc => {
                self.close_routing_editor();
                return true;
            }
            KeyCode::Tab => {
                self.update_routing_editor(|editor| {
                    editor.selected_field = editor.selected_field.next();
                });
                return true;
            }
            KeyCode::BackTab => {
                self.update_routing_editor(|editor| {
                    editor.selected_field = editor.selected_field.previous();
                });
                return true;
            }
            KeyCode::Up => {
                self.update_routing_editor(|editor| {
                    editor.selected_field = editor.selected_field.previous();
                });
                return true;
            }
            KeyCode::Down => {
                self.update_routing_editor(|editor| {
                    editor.selected_field = editor.selected_field.next();
                });
                return true;
            }
            _ => {}
        }

        let mut handled = false;
        let mut request_save = false;
        let mut request_cancel = false;
        let has_models = !self.routing_model_options.is_empty();
        let model_options_len = self.routing_model_options.len();

        self.update_routing_editor(|editor| match editor.selected_field {
            RoutingEditorField::Model => match key_event.code {
                KeyCode::Left => {
                    if has_models {
                        if editor.model_cursor == 0 {
                            editor.model_cursor = model_options_len.saturating_sub(1);
                        } else {
                            editor.model_cursor -= 1;
                        }
                        handled = true;
                    }
                }
                KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => {
                    if has_models {
                        editor.model_cursor = (editor.model_cursor + 1) % model_options_len;
                        handled = true;
                    }
                }
                _ => {}
            },
            RoutingEditorField::Enabled => {
                if matches!(
                    key_event.code,
                    KeyCode::Left | KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ')
                ) {
                    editor.enabled = !editor.enabled;
                    handled = true;
                }
            }
            RoutingEditorField::Reasoning => match key_event.code {
                KeyCode::Left => {
                    if editor.reasoning_cursor == 0 {
                        editor.reasoning_cursor = ROUTING_REASONING_LEVELS.len().saturating_sub(1);
                    } else {
                        editor.reasoning_cursor -= 1;
                    }
                    handled = true;
                }
                KeyCode::Right => {
                    editor.reasoning_cursor =
                        (editor.reasoning_cursor + 1) % ROUTING_REASONING_LEVELS.len();
                    handled = true;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    editor.toggle_reasoning_at_cursor();
                    handled = true;
                }
                _ => {}
            },
            RoutingEditorField::Description => match key_event.code {
                KeyCode::Backspace => {
                    editor.description.pop();
                    handled = true;
                }
                KeyCode::Char(c) => {
                    if !key_event.modifiers.contains(KeyModifiers::CONTROL)
                        && !key_event.modifiers.contains(KeyModifiers::ALT)
                        && editor.description.chars().count() < ROUTING_DESCRIPTION_MAX_CHARS
                    {
                        editor.description.push(c);
                        handled = true;
                    }
                }
                _ => {}
            },
            RoutingEditorField::Save => {
                if matches!(key_event.code, KeyCode::Enter | KeyCode::Char(' ')) {
                    request_save = true;
                    handled = true;
                }
            }
            RoutingEditorField::Cancel => {
                if matches!(key_event.code, KeyCode::Enter | KeyCode::Char(' ')) {
                    request_cancel = true;
                    handled = true;
                }
            }
        });

        if request_save {
            self.save_routing_editor();
            return true;
        }
        if request_cancel {
            self.close_routing_editor();
            return true;
        }

        handled
    }

    pub(super) fn handle_key_event_internal(&mut self, key_event: KeyEvent) -> bool {
        if key_event.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key_event.code, KeyCode::Char('s') | KeyCode::Char('S'))
        {
            self.close();
            return true;
        }

        let mode = self.mode.clone();
        match mode {
            AutoDriveSettingsMode::Main => self.handle_main_key(key_event),
            AutoDriveSettingsMode::RoutingList => self.handle_routing_list_key(key_event),
            AutoDriveSettingsMode::RoutingEditor(_) => self.handle_routing_editor_key(key_event),
        }
    }


    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return false;
        }

        let handled = self.handle_key_event_internal(key_event);
        if handled {
            self.app_event_tx.send(AppEvent::RequestRedraw);
        }
        handled
    }

    pub(super) fn handle_paste_internal(&mut self, text: &str) -> bool {
        let mut handled = false;
        self.update_routing_editor(|editor| {
            if editor.selected_field == RoutingEditorField::Description {
                let remaining =
                    ROUTING_DESCRIPTION_MAX_CHARS.saturating_sub(editor.description.chars().count());
                if remaining > 0 {
                    let sanitized = text.replace(['\r', '\n'], " ");
                    let insert: String = sanitized.chars().take(remaining).collect();
                    if !insert.is_empty() {
                        editor.description.push_str(&insert);
                        handled = true;
                    }
                }
            }
        });
        handled
    }
}
