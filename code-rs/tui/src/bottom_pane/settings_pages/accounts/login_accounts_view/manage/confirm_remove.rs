use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::{LoginAccountsState, ViewMode};

impl LoginAccountsState {
    pub(super) fn handle_confirm_remove_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(list_area) = self.list_hit_area_for_mouse(area) else {
                    return false;
                };
                if self
                    .list_selection_for_position(list_area, mouse_event.column, mouse_event.row)
                    .is_some()
                {
                    self.mode = ViewMode::List;
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    pub(super) fn handle_confirm_remove_key(&mut self, key_event: KeyEvent) -> bool {
        let account_id = if let ViewMode::ConfirmRemove { account_id } = &self.mode {
            account_id.clone()
        } else {
            return false;
        };

        match key_event.code {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.mode = ViewMode::List;
                true
            }
            KeyCode::Enter | KeyCode::Char('y') => {
                self.remove_account(account_id);
                true
            }
            _ => false,
        }
    }
}

