use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app_event::AppEvent;
use crate::bottom_pane::BottomPane;
use crate::bottom_pane::ChatComposer;

use super::*;

impl AutoCoordinatorView {
    fn normalize_status_message(message: &str) -> Option<String> {
        let mapped = ChatComposer::map_status_message(message);
        let trimmed = mapped.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    pub(super) fn update_status_message(&mut self, message: String) -> bool {
        let new_value = Self::normalize_status_message(&message);
        if self.status_message.as_deref() == new_value.as_deref() {
            return false;
        }
        self.status_message = new_value;
        true
    }

    pub(crate) fn handle_active_key_event(
        &mut self,
        _pane: &mut BottomPane<'_>,
        key_event: KeyEvent,
    ) -> bool {
        if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return false;
        }

        if key_event.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key_event.code, KeyCode::Char('s') | KeyCode::Char('S'))
        {
            self.app_event_tx.send(AppEvent::ShowAutoDriveSettings);
            return true;
        }

        let awaiting_without_input = matches!(
            &self.model,
            AutoCoordinatorViewModel::Active(model)
                if model.awaiting_submission && !model.show_composer
        );
        if awaiting_without_input {
            // Allow approval keys to bubble so ChatWidget handles them.
            let allow_passthrough = matches!(
                key_event.code,
                KeyCode::Esc
                    | KeyCode::Enter
                    | KeyCode::Char(' ')
                    | KeyCode::Char('e')
                    | KeyCode::Char('E')
            );
            if !allow_passthrough {
                return true;
            }
        }

        if matches!(key_event.code, KeyCode::Up | KeyCode::Down) {
            let hide_composer = match &self.model {
                AutoCoordinatorViewModel::Active(model) => !model.show_composer,
            };
            return hide_composer;
        }

        false
    }

    pub(super) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return false;
        }

        if key_event.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key_event.code, KeyCode::Char('s') | KeyCode::Char('S'))
        {
            self.app_event_tx.send(AppEvent::ShowAutoDriveSettings);
            return true;
        }
        false
    }
}

