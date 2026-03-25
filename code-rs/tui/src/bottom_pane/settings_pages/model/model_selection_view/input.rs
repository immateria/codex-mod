use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::components::mode_guard::ModeGuard;

use super::{EditTarget, ModelSelectionView, ViewMode};
use super::super::model_selection_state::EntryKind;

impl ModelSelectionView {
    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        let mut mode_guard = ModeGuard::replace(&mut self.mode, ViewMode::Transition, |mode| {
            matches!(mode, ViewMode::Transition)
        });

        match mode_guard.mode_mut() {
            ViewMode::Main => self.handle_key_event_main(key_event),
            ViewMode::Edit {
                target,
                field,
                error,
            } => match (key_event.code, key_event.modifiers) {
                (KeyCode::Esc, _) => {
                    self.mode = ViewMode::Main;
                    true
                }
                (KeyCode::Char('s'), KeyModifiers::CONTROL) | (KeyCode::Enter, _) => {
                    match self.save_edit_value(*target, field.text()) {
                        Ok(()) => {
                            self.mode = ViewMode::Main;
                        }
                        Err(message) => {
                            *error = Some(message);
                        }
                    }
                    true
                }
                _ => {
                    *error = None;
                    field.handle_key(key_event)
                }
            },
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }

    fn handle_key_event_main(&mut self, key_event: KeyEvent) -> bool {
        let selected_entry = self.data.entry_at(self.selected_index);
        match key_event {
            KeyEvent {
                code: KeyCode::Up | KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_selection_up();
                true
            }
            KeyEvent {
                code: KeyCode::Down | KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_selection_down();
                true
            }
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('-'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.adjust_selected_numeric_value(-1),
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('+') | KeyCode::Char('='),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } => self.adjust_selected_numeric_value(1),
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE,
                ..
            } if c.is_ascii_digit() => {
                let edit_target = match selected_entry {
                    Some(EntryKind::ContextWindow) => Some(EditTarget::ContextWindow),
                    Some(EntryKind::AutoCompact) => Some(EditTarget::AutoCompact),
                    _ => None,
                };
                if let Some(target) = edit_target {
                    self.open_edit_for(target, true);
                    if let ViewMode::Edit { field, .. } = &mut self.mode {
                        let _ = field.handle_key(key_event);
                    }
                    true
                } else {
                    false
                }
            }
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                let edit_target = match selected_entry {
                    Some(EntryKind::ContextWindow) => Some(EditTarget::ContextWindow),
                    Some(EntryKind::AutoCompact) => Some(EditTarget::AutoCompact),
                    _ => None,
                };
                if let Some(target) = edit_target {
                    self.open_edit_for(target, true);
                    true
                } else {
                    false
                }
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.confirm_selection();
                true
            }
            KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.send_closed(false);
                true
            }
            _ => false,
        }
    }
}
