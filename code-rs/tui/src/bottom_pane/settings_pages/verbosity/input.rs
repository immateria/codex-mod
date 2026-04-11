use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;

use super::{VerbositySelectionView, VERBOSITY_OPTIONS};

impl VerbositySelectionView {
    pub(super) fn confirm_selection(&mut self) {
        self.app_event_tx
            .send(AppEvent::UpdateTextVerbosity(self.selected_verbosity()));
        self.is_complete = true;
    }

    pub(super) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        let len = VERBOSITY_OPTIONS.len();
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
                code: KeyCode::Home,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.state.home(len);
                true
            }
            KeyEvent {
                code: KeyCode::End,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.state.end(len, len);
                true
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
                self.is_complete = true;
                true
            }
            _ => false,
        }
    }
}

