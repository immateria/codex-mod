use super::*;

use crossterm::event::{KeyCode, KeyEvent};

impl SecretsSettingsView {
    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        match self.mode.clone() {
            Mode::List => self.handle_key_list(key_event),
            Mode::ConfirmDelete { entry } => self.handle_key_confirm_delete(key_event, entry),
        }
    }

    fn handle_key_list(&mut self, key_event: KeyEvent) -> bool {
        let snapshot = self.shared_snapshot();
        let entry_count = Self::list_entries(&snapshot)
            .map(<[_]>::len)
            .unwrap_or(0);
        let row_count = entry_count.max(1);

        match key_event.code {
            KeyCode::Esc => {
                self.is_complete = true;
                true
            }
            KeyCode::Up => {
                let mut state = self.list_state.get();
                state.move_up_wrap_visible(row_count, self.list_viewport_rows.get().max(1));
                self.list_state.set(state);
                true
            }
            KeyCode::Down => {
                let mut state = self.list_state.get();
                state.move_down_wrap_visible(row_count, self.list_viewport_rows.get().max(1));
                self.list_state.set(state);
                true
            }
            KeyCode::Char('r') => {
                self.request_secrets_list();
                true
            }
            KeyCode::Delete | KeyCode::Backspace | KeyCode::Char('d') => {
                if entry_count == 0 {
                    return false;
                }
                if snapshot.action_in_progress.is_some() {
                    return false;
                }
                let Some(entry) = self.selected_entry(&snapshot) else {
                    return false;
                };
                self.mode = Mode::ConfirmDelete { entry };
                self.focused_confirm_button = ConfirmAction::Cancel;
                self.hovered_confirm_button = None;
                true
            }
            _ => false,
        }
    }

    fn handle_key_confirm_delete(&mut self, key_event: KeyEvent, entry: SecretListEntry) -> bool {
        match key_event.code {
            KeyCode::Esc => {
                self.mode = Mode::List;
                true
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Left => {
                self.focused_confirm_button = match self.focused_confirm_button {
                    ConfirmAction::Delete => ConfirmAction::Cancel,
                    ConfirmAction::Cancel => ConfirmAction::Delete,
                };
                true
            }
            KeyCode::Enter | KeyCode::Char(' ') => match self.focused_confirm_button {
                ConfirmAction::Cancel => {
                    self.mode = Mode::List;
                    true
                }
                ConfirmAction::Delete => {
                    self.mode = Mode::List;
                    self.request_delete_secret(entry);
                    true
                }
            },
            _ => false,
        }
    }
}

