use crossterm::event::{KeyCode, KeyEvent};

use crate::app_event::AppEvent;

use super::{PlanningRow, PlanningSettingsView};

impl PlanningSettingsView {
    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.handle_key(key_event)
    }

    pub(super) fn handle_enter(&mut self, row: PlanningRow) {
        match row {
            PlanningRow::CustomModel => {
                self.app_event_tx.send(AppEvent::ShowPlanningModelSelector);
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        let rows = self.rows();
        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        let total = rows.len();
        self.state.ensure_visible(total, 4);

        match key.code {
            KeyCode::Up => {
                self.state.move_up_wrap(total);
                true
            }
            KeyCode::Down => {
                self.state.move_down_wrap(total);
                true
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some(sel) = self.state.selected_idx
                    && let Some(row) = rows.get(sel).copied()
                {
                    self.handle_enter(row);
                }
                true
            }
            KeyCode::Esc => {
                self.is_complete = true;
                true
            }
            _ => false,
        }
    }
}

