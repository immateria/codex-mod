use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::StatusLineSetupView;

impl StatusLineSetupView {
    pub(super) fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
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
                ..
            } => {
                self.set_selected_index_for_active_lane(0);
                true
            }
            KeyEvent {
                code: KeyCode::End,
                ..
            } => {
                let max = self.choices_for_active_lane().len().saturating_sub(1);
                self.set_selected_index_for_active_lane(max);
                true
            }
            KeyEvent {
                code: KeyCode::PageUp,
                ..
            } => {
                let idx = self.selected_index_for_active_lane().saturating_sub(5);
                self.set_selected_index_for_active_lane(idx);
                true
            }
            KeyEvent {
                code: KeyCode::PageDown,
                ..
            } => {
                let max = self.choices_for_active_lane().len().saturating_sub(1);
                let idx = (self.selected_index_for_active_lane() + 5).min(max);
                self.set_selected_index_for_active_lane(idx);
                true
            }
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_selected_left();
                true
            }
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_selected_right();
                true
            }
            KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.toggle_selected();
                true
            }
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.switch_active_lane();
                true
            }
            KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.toggle_primary_lane();
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.confirm();
                true
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.cancel();
                true
            }
            _ => false,
        }
    }
}

