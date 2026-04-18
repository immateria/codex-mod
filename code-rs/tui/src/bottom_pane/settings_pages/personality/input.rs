use crossterm::event::{KeyCode, KeyEvent};

use crate::app_event::AppEvent;

use super::{PersonalityRow, PersonalitySettingsView};

impl PersonalitySettingsView {
    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.handle_key(key_event)
    }

    fn emit_personality_change(&self) {
        self.app_event_tx.send(AppEvent::SetModelPersonality(self.personality));
    }

    fn emit_tone_change(&self) {
        self.app_event_tx.send(AppEvent::SetModelTone(self.tone));
    }

    fn emit_traits_change(&self) {
        let traits = self.current_traits();
        let value = if traits.is_neutral() { None } else { Some(traits) };
        self.app_event_tx.send(AppEvent::SetPersonalityTraits(value));
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        let rows = self.rows();
        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        let total = rows.len();
        self.state.ensure_visible(total, 6);

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.state.move_up_wrap(total);
                // Skip separator
                if self.selected_row() == Some(PersonalityRow::TraitSeparator) {
                    self.state.move_up_wrap(total);
                }
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.state.move_down_wrap(total);
                // Skip separator
                if self.selected_row() == Some(PersonalityRow::TraitSeparator) {
                    self.state.move_down_wrap(total);
                }
                true
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter | KeyCode::Char(' ') => {
                self.cycle_forward();
                self.emit_current_change();
                true
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.cycle_backward();
                self.emit_current_change();
                true
            }
            KeyCode::Esc => {
                self.is_complete = true;
                true
            }
            _ => false,
        }
    }

    pub(super) fn emit_current_change(&self) {
        match self.selected_row() {
            Some(PersonalityRow::Archetype) => self.emit_personality_change(),
            Some(PersonalityRow::TonePreference) => self.emit_tone_change(),
            Some(row) if self.is_trait_row(row) => self.emit_traits_change(),
            _ => {}
        }
    }
}
