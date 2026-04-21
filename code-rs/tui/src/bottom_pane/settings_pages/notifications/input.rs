use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;

use super::{NotificationsMode, NotificationsSettingsView};

impl NotificationsSettingsView {
    pub(super) fn toggle_notifications(&mut self) {
        match &mut self.mode {
            NotificationsMode::Toggle { enabled } => {
                *enabled = !*enabled;
                self.app_event_tx
                    .send(AppEvent::UpdateTuiNotifications(*enabled));
            }
            NotificationsMode::Custom { entries } => {
                let filters = if entries.is_empty() {
                    "(none)".to_owned()
                } else {
                    entries.join(", ")
                };
                self.app_event_tx.send_background_event_with_ticket(
                    &self.ticket,
                    format!("TUI notifications are filtered in config: [{filters}]"),
                );
                self.app_event_tx.send_background_event_with_ticket(
                    &self.ticket,
                    "Edit [tui].notifications in your config.toml to change filters.".to_owned(),
                );
            }
        }
    }

    pub(super) fn toggle_prevent_idle_sleep(&mut self) {
        self.prevent_idle_sleep = !self.prevent_idle_sleep;
        self.app_event_tx
            .send(AppEvent::UpdatePreventIdleSleep(self.prevent_idle_sleep));
    }

    fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent {
                code: KeyCode::Up | KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.state.move_up_wrap(Self::ROW_COUNT);
                true
            }
            KeyEvent {
                code: KeyCode::Down | KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.state.move_down_wrap(Self::ROW_COUNT);
                true
            }
            KeyEvent {
                code: KeyCode::Home,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.state.home(Self::ROW_COUNT);
                true
            }
            KeyEvent {
                code: KeyCode::End,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.state.end(Self::ROW_COUNT, Self::ROW_COUNT);
                true
            }
            KeyEvent {
                code: KeyCode::PageUp,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.state.page_up(Self::ROW_COUNT, Self::ROW_COUNT);
                true
            }
            KeyEvent {
                code: KeyCode::PageDown,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.state.page_down(Self::ROW_COUNT, Self::ROW_COUNT);
                true
            }
            KeyEvent {
                code: KeyCode::Left | KeyCode::Right | KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                match self.selected_row() {
                    0 => self.toggle_notifications(),
                    1 => self.toggle_prevent_idle_sleep(),
                    _ => {}
                }
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                match self.selected_row() {
                    0 => self.toggle_notifications(),
                    1 => self.toggle_prevent_idle_sleep(),
                    _ => {
                        self.is_complete = true;
                    }
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
