use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl AppsSettingsView {
    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.sync_sources_snapshot_if_clean();

        if key_event.modifiers.contains(KeyModifiers::CONTROL)
            && !matches!(key_event.code, KeyCode::Char('s') | KeyCode::Char('S'))
        {
            return false;
        }

        match self.mode.clone() {
            Mode::Overview => self.handle_key_overview(key_event),
            Mode::AccountDetail { account_id } => self.handle_key_account_detail(key_event, account_id),
        }
    }

    fn handle_key_overview(&mut self, key_event: KeyEvent) -> bool {
        let snapshot = self
            .shared_state
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone();
        let account_count = snapshot.accounts_snapshot.len();

        match key_event.code {
            KeyCode::Esc => {
                self.close();
                true
            }
            KeyCode::Up => {
                let mut state = self.list_state.get();
                state.move_up_wrap_visible(account_count, self.list_viewport_rows.get().max(1));
                self.list_state.set(state);
                true
            }
            KeyCode::Down => {
                let mut state = self.list_state.get();
                state.move_down_wrap_visible(account_count, self.list_viewport_rows.get().max(1));
                self.list_state.set(state);
                true
            }
            KeyCode::Char('m') => {
                self.draft_sources.mode = Self::cycle_mode(self.draft_sources.mode);
                self.sources_dirty = self.draft_sources != self.baseline_sources;
                true
            }
            KeyCode::Char(' ') => {
                let Some(idx) = self.list_state.get().selected_idx.or(Some(0)) else {
                    return false;
                };
                let Some(selected) = snapshot.accounts_snapshot.get(idx) else {
                    return false;
                };
                if !selected.is_chatgpt {
                    return false;
                }
                if let Some(pos) = self
                    .draft_sources
                    .pinned_account_ids
                    .iter()
                    .position(|id| id == &selected.id)
                {
                    self.draft_sources.pinned_account_ids.remove(pos);
                } else {
                    self.draft_sources.pinned_account_ids.push(selected.id.clone());
                }
                self.sources_dirty = self.draft_sources != self.baseline_sources;
                true
            }
            KeyCode::Enter => {
                let Some(idx) = self.list_state.get().selected_idx.or(Some(0)) else {
                    return false;
                };
                let Some(selected) = snapshot.accounts_snapshot.get(idx) else {
                    return false;
                };
                self.mode = Mode::AccountDetail {
                    account_id: selected.id.clone(),
                };
                true
            }
            KeyCode::Char('r') => {
                let active_id = snapshot
                    .accounts_snapshot
                    .iter()
                    .find(|acc| acc.is_active_model_account && acc.is_chatgpt)
                    .map(|acc| acc.id.as_str());
                let ids = code_core::apps_sources::effective_source_account_ids(
                    &self.baseline_sources,
                    active_id,
                );
                if ids.is_empty() {
                    return false;
                }
                self.request_refresh_status(ids, /*force_refresh_tools*/ true);
                true
            }
            KeyCode::Char('a') => {
                self.request_open_accounts_settings();
                true
            }
            KeyCode::Char('l') => {
                self.request_open_accounts_login();
                true
            }
            KeyCode::Char('s') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.request_save_sources();
                true
            }
            _ => false,
        }
    }

    fn handle_key_account_detail(&mut self, key_event: KeyEvent, account_id: String) -> bool {
        match key_event.code {
            KeyCode::Esc => {
                self.mode = Mode::Overview;
                true
            }
            KeyCode::Char('r') => {
                self.request_refresh_status(vec![account_id], /*force_refresh_tools*/ true);
                true
            }
            KeyCode::Char('a') => {
                self.request_open_accounts_settings();
                true
            }
            KeyCode::Char('l') => {
                // Only handle if status indicates auth is required.
                let snapshot = self
                    .shared_state
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .clone();
                let needs_login = match snapshot.status_by_account_id.get(&account_id) {
                    Some(crate::chatwidget::AppsAccountStatusState::Failed { needs_login, .. }) => *needs_login,
                    _ => false,
                };
                if !needs_login {
                    return false;
                }
                self.request_open_accounts_login();
                true
            }
            KeyCode::Char('s') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.request_save_sources();
                true
            }
            _ => false,
        }
    }
}
