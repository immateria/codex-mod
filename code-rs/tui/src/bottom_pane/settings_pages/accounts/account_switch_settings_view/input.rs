use super::{AccountSwitchSettingsView, ViewMode};
use crossterm::event::{KeyCode, KeyEvent};

impl AccountSwitchSettingsView {
    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        match self.view_mode {
            ViewMode::Main => match key_event.code {
                KeyCode::Esc => {
                    self.close();
                    true
                }
                KeyCode::Up => {
                    self.main_state.move_up_wrap(Self::MAIN_OPTION_COUNT);
                    true
                }
                KeyCode::Down | KeyCode::Tab => {
                    self.main_state.move_down_wrap(Self::MAIN_OPTION_COUNT);
                    true
                }
                KeyCode::BackTab => {
                    self.main_state.move_up_wrap(Self::MAIN_OPTION_COUNT);
                    true
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.activate_selected_main();
                    true
                }
                _ => false,
            },
            ViewMode::ConfirmStoreChange { .. } => match key_event.code {
                KeyCode::Esc => {
                    self.view_mode = ViewMode::Main;
                    true
                }
                KeyCode::Up => {
                    self.confirm_state.move_up_wrap(Self::CONFIRM_OPTION_COUNT);
                    true
                }
                KeyCode::Down | KeyCode::Tab => {
                    self.confirm_state.move_down_wrap(Self::CONFIRM_OPTION_COUNT);
                    true
                }
                KeyCode::BackTab => {
                    self.confirm_state.move_up_wrap(Self::CONFIRM_OPTION_COUNT);
                    true
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.activate_selected_confirm();
                    true
                }
                _ => false,
            },
        }
    }
}
