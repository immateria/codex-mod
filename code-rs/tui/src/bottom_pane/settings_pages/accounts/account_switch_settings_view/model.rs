use super::{AccountSwitchSettingsView, ViewMode};

use crate::app_event::AppEvent;
use code_core::config_types::AuthCredentialsStoreMode;

impl AccountSwitchSettingsView {
    pub(super) fn auth_store_mode_label(mode: AuthCredentialsStoreMode) -> &'static str {
        match mode {
            AuthCredentialsStoreMode::File => "file",
            AuthCredentialsStoreMode::Keyring => "keyring",
            AuthCredentialsStoreMode::Auto => "auto",
            AuthCredentialsStoreMode::Ephemeral => "ephemeral",
        }
    }

    fn next_auth_store_mode(mode: AuthCredentialsStoreMode) -> AuthCredentialsStoreMode {
        match mode {
            AuthCredentialsStoreMode::File => AuthCredentialsStoreMode::Keyring,
            AuthCredentialsStoreMode::Keyring => AuthCredentialsStoreMode::Auto,
            AuthCredentialsStoreMode::Auto => AuthCredentialsStoreMode::Ephemeral,
            AuthCredentialsStoreMode::Ephemeral => AuthCredentialsStoreMode::File,
        }
    }

    fn toggle_auto_switch(&mut self) {
        self.auto_switch_enabled = !self.auto_switch_enabled;
        self.app_event_tx
            .send(AppEvent::SetAutoSwitchAccountsOnRateLimit(
                self.auto_switch_enabled,
            ));
    }

    fn toggle_api_key_fallback(&mut self) {
        self.api_key_fallback_enabled = !self.api_key_fallback_enabled;
        self.app_event_tx
            .send(AppEvent::SetApiKeyFallbackOnAllAccountsLimited(
                self.api_key_fallback_enabled,
            ));
    }

    pub(super) fn close(&mut self) {
        self.is_complete = true;
    }

    fn show_login_accounts(&self) {
        self.app_event_tx.send(AppEvent::ShowLoginAccounts);
    }

    fn show_login_add_account(&self) {
        self.app_event_tx.send(AppEvent::ShowLoginAddAccount);
    }

    fn request_store_mode_change(&mut self, target: AuthCredentialsStoreMode, migrate_existing: bool) {
        self.app_event_tx.send(AppEvent::RequestSetAuthCredentialsStoreMode {
            mode: target,
            migrate_existing,
        });
    }

    fn open_store_mode_confirm(&mut self) {
        let target = Self::next_auth_store_mode(self.auth_credentials_store_mode);
        self.view_mode = ViewMode::ConfirmStoreChange { target };
        self.confirm_state.selected_idx = Some(0);
        self.confirm_state.scroll_top = 0;
    }

    pub(super) fn activate_selected_main(&mut self) {
        let selected = self
            .main_state
            .selected_idx
            .unwrap_or(0)
            .min(Self::MAIN_OPTION_COUNT.saturating_sub(1));
        match selected {
            0 => self.toggle_auto_switch(),
            1 => self.toggle_api_key_fallback(),
            2 => self.open_store_mode_confirm(),
            3 => self.show_login_accounts(),
            4 => self.show_login_add_account(),
            5 => self.close(),
            _ => {}
        }
    }

    pub(super) fn confirm_selected_index(&self) -> usize {
        self.confirm_state
            .selected_idx
            .unwrap_or(0)
            .min(Self::CONFIRM_OPTION_COUNT.saturating_sub(1))
    }

    pub(super) fn activate_selected_confirm(&mut self) {
        let ViewMode::ConfirmStoreChange { target } = self.view_mode else {
            return;
        };
        let selected = self.confirm_selected_index();

        match selected {
            0 => {
                self.request_store_mode_change(target, true);
                self.view_mode = ViewMode::Main;
            }
            1 => {
                self.request_store_mode_change(target, false);
                self.view_mode = ViewMode::Main;
            }
            2 => {
                self.view_mode = ViewMode::Main;
            }
            _ => {}
        }
    }
}
