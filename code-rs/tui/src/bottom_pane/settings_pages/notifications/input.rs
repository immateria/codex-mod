use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;

use super::{NotificationsMode, NotificationsSettingsView};

impl NotificationsSettingsView {
    pub(super) fn toggle(&mut self) {
        match &mut self.mode {
            NotificationsMode::Toggle { enabled } => {
                *enabled = !*enabled;
                self.app_event_tx
                    .send(AppEvent::UpdateTuiNotifications(*enabled));
            }
            NotificationsMode::Custom { entries } => {
                let filters = if entries.is_empty() {
                    "(none)".to_string()
                } else {
                    entries.join(", ")
                };
                self.app_event_tx.send_background_event_with_ticket(
                    &self.ticket,
                    format!("TUI notifications are filtered in config: [{filters}]"),
                );
                self.app_event_tx.send_background_event_with_ticket(
                    &self.ticket,
                    "Edit ~/.code/config.toml [tui].notifications to change filters.".to_string(),
                );
            }
        }
    }

    fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.state.move_up_wrap(Self::ROW_COUNT);
                true
            }
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.state.move_down_wrap(Self::ROW_COUNT);
                true
            }
            KeyEvent {
                code: KeyCode::Left | KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if self.selected_row() == 0 {
                    self.toggle();
                }
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if self.selected_row() == 0 {
                    self.toggle();
                } else {
                    self.is_complete = true;
                }
                true
            }
            KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if self.selected_row() == 0 {
                    self.toggle();
                }
                true
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
                true
            }
            _ => false,
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.process_key_event(key_event)
    }
}
