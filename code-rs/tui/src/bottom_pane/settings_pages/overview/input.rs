use crossterm::event::{KeyCode, KeyEvent};

use crate::app_event::AppEvent;

use super::SettingsOverviewView;

impl SettingsOverviewView {
    fn move_up(&mut self, visible_rows: usize) {
        self.scroll
            .move_up_wrap_visible(self.rows.len(), visible_rows);
    }

    fn move_down(&mut self, visible_rows: usize) {
        self.scroll
            .move_down_wrap_visible(self.rows.len(), visible_rows);
    }

    pub(super) fn open_selected(&mut self) {
        let Some(section) = self.selected_section() else {
            return;
        };
        self.app_event_tx
            .send(AppEvent::OpenSettings { section: Some(section) });
        self.is_complete = true;
    }

    fn cancel(&mut self) {
        self.is_complete = true;
    }

    pub(super) fn process_key_event(&mut self, key_event: KeyEvent, visible_rows: usize) -> bool {
        match key_event.code {
            KeyCode::Esc => {
                self.cancel();
                true
            }
            KeyCode::Enter => {
                self.open_selected();
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up(visible_rows);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down(visible_rows);
                true
            }
            KeyCode::Home => {
                self.scroll.home(self.rows.len());
                true
            }
            KeyCode::End => {
                self.scroll.end(self.rows.len(), visible_rows.max(1));
                true
            }
            KeyCode::PageUp => {
                self.scroll.page_up(self.rows.len(), visible_rows.max(1));
                true
            }
            KeyCode::PageDown => {
                self.scroll.page_down(self.rows.len(), visible_rows.max(1));
                true
            }
            _ => false,
        }
    }
}
