use crossterm::event::{KeyCode, KeyEvent};

use crate::app_event::AppEvent;
use crate::ui_interaction::{wrap_next, wrap_prev};

use super::UpdateSettingsView;

impl UpdateSettingsView {
    fn toggle_auto(&mut self) {
        self.auto_enabled = !self.auto_enabled;
        self.app_event_tx
            .send(AppEvent::SetAutoUpgradeEnabled(self.auto_enabled));
    }

    fn invoke_run_upgrade(&mut self) {
        let state = self.current_state();

        if self.command.is_none() {
            if let Some(instructions) = &self.manual_instructions {
                self.app_event_tx
                    .send_background_event_with_ticket(&self.ticket, instructions.clone());
            }
            return;
        }

        if state.checking {
            self.app_event_tx.send_background_event_with_ticket(
                &self.ticket,
                "Still checking for updates...".to_string(),
            );
            return;
        }
        if let Some(err) = &state.error {
            self.app_event_tx.send_background_event_with_ticket(
                &self.ticket,
                format!("/update failed: {err}"),
            );
            return;
        }
        let Some(latest) = state.latest_version else {
            self.app_event_tx.send_background_event_with_ticket(
                &self.ticket,
                "Code is already up to date.".to_string(),
            );
            return;
        };

        let Some(command) = self.command.clone() else {
            return;
        };
        let display = self
            .command_display
            .clone()
            .unwrap_or_else(|| command.join(" "));

        self.app_event_tx.send_background_event_with_ticket(
            &self.ticket,
            format!(
                "Update available: {} -> {}. Opening guided upgrade with `{display}`...",
                self.current_version, latest
            ),
        );
        self.app_event_tx.send(AppEvent::RunUpdateCommand {
            command,
            display: display.clone(),
            latest_version: Some(latest.clone()),
        });
        self.app_event_tx.send_background_event_with_ticket(
            &self.ticket,
            format!(
                "Complete the guided terminal steps for `{display}` then restart Code to finish upgrading to {latest}."
            ),
        );
        self.is_complete = true;
    }

    pub(super) fn activate_selected(&mut self) {
        match self.field {
            0 => self.invoke_run_upgrade(),
            1 => self.toggle_auto(),
            _ => self.is_complete = true,
        }
    }

    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        let handled = match key_event.code {
            KeyCode::Esc => {
                self.is_complete = true;
                true
            }
            KeyCode::Tab | KeyCode::Down => {
                self.field = wrap_next(self.field, Self::FIELD_COUNT);
                true
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.field = wrap_prev(self.field, Self::FIELD_COUNT);
                true
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if self.field == 1 => {
                self.toggle_auto();
                true
            }
            KeyCode::Enter => {
                self.activate_selected();
                true
            }
            _ => false,
        };
        if handled {
            self.app_event_tx.send(AppEvent::RequestRedraw);
        }
        handled
    }
}

