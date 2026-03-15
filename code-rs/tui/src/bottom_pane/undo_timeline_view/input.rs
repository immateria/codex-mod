use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::UndoTimelineView;

impl UndoTimelineView {
    pub(super) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        match key_event.code {
            KeyCode::Up => {
                self.move_up();
                true
            }
            KeyCode::Down => {
                self.move_down();
                true
            }
            KeyCode::PageUp => {
                self.page_up();
                true
            }
            KeyCode::PageDown => {
                self.page_down();
                true
            }
            KeyCode::Home => {
                self.go_home();
                true
            }
            KeyCode::End => {
                self.go_end();
                true
            }
            KeyCode::Enter => {
                self.confirm();
                true
            }
            KeyCode::Esc => {
                self.is_complete = true;
                true
            }
            KeyCode::Char(' ') => self.toggle_files(),
            KeyCode::Char('c') | KeyCode::Char('C')
                if !key_event.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.toggle_conversation()
            }
            KeyCode::Char('f') | KeyCode::Char('F')
                if !key_event.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.toggle_files()
            }
            KeyCode::Tab => {
                let Some(entry) = self.selected_entry() else {
                    return false;
                };
                if entry.conversation_available && !entry.files_available {
                    self.toggle_conversation()
                } else if entry.files_available && !entry.conversation_available {
                    self.toggle_files()
                } else if entry.files_available || entry.conversation_available {
                    if self.restore_files {
                        self.toggle_conversation()
                    } else {
                        self.toggle_files()
                    }
                } else {
                    false
                }
            }
            KeyCode::Right if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.toggle_conversation()
            }
            KeyCode::Left if key_event.modifiers.contains(KeyModifiers::CONTROL) => self.toggle_files(),
            _ => false,
        }
    }
}

