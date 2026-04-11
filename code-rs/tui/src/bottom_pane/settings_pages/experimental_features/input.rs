use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl ExperimentalFeaturesSettingsView {
    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        if key_event.modifiers.contains(KeyModifiers::CONTROL)
            && !matches!(key_event.code, KeyCode::Char('s' | 'S'))
        {
            return false;
        }

        match key_event.code {
            KeyCode::Esc => {
                self.close();
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let total = self.feature_count();
                if total == 0 {
                    return false;
                }
                let mut state = self.list_state.get();
                state.move_up_wrap_visible(total, self.list_viewport_rows.get().max(1));
                self.list_state.set(state);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let total = self.feature_count();
                if total == 0 {
                    return false;
                }
                let mut state = self.list_state.get();
                state.move_down_wrap_visible(total, self.list_viewport_rows.get().max(1));
                self.list_state.set(state);
                true
            }
            KeyCode::Home => {
                let total = self.feature_count();
                let mut state = self.list_state.get();
                state.home(total);
                self.list_state.set(state);
                true
            }
            KeyCode::End => {
                let total = self.feature_count();
                let vis = self.list_viewport_rows.get().max(1);
                let mut state = self.list_state.get();
                state.end(total, vis);
                self.list_state.set(state);
                true
            }
            KeyCode::PageUp => {
                let total = self.feature_count();
                let vis = self.list_viewport_rows.get().max(1);
                let mut state = self.list_state.get();
                state.page_up(total, vis);
                self.list_state.set(state);
                true
            }
            KeyCode::PageDown => {
                let total = self.feature_count();
                let vis = self.list_viewport_rows.get().max(1);
                let mut state = self.list_state.get();
                state.page_down(total, vis);
                self.list_state.set(state);
                true
            }
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_selected(),
            KeyCode::Char('s') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.request_save()
            }
            _ => false,
        }
    }
}

