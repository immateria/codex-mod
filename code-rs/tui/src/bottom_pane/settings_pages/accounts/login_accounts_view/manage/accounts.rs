use std::cmp::Ordering;

use chrono::Utc;
use code_core::auth;
use code_core::auth_accounts;
use code_login::AuthMode;

use crate::account_label::account_mode_priority;
use crate::app_event::AppEvent;

use super::super::shared::Feedback;
use super::{AccountRow, LoginAccountsState};

fn cmp_ascii_case_insensitive(a: &str, b: &str) -> Ordering {
    let mut a_iter = a.bytes();
    let mut b_iter = b.bytes();
    loop {
        match (a_iter.next(), b_iter.next()) {
            (Some(a_byte), Some(b_byte)) => {
                let a_lower = a_byte.to_ascii_lowercase();
                let b_lower = b_byte.to_ascii_lowercase();
                match a_lower.cmp(&b_lower) {
                    Ordering::Equal => continue,
                    other => return other,
                }
            }
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (None, None) => return Ordering::Equal,
        }
    }
}

impl LoginAccountsState {
    pub(super) fn reload_accounts(&mut self) {
        let previously_selected_id = self
            .accounts
            .get(self.selected)
            .map(|row| row.id.clone());

        match auth_accounts::list_accounts(&self.code_home) {
            Ok(raw_accounts) => {
                let active_id = auth_accounts::get_active_account_id(&self.code_home).ok().flatten();
                self.active_account_id = active_id.clone();
                self.accounts = raw_accounts
                    .into_iter()
                    .map(|account| AccountRow::from_stored(account, active_id.as_deref()))
                    .collect();

                self.accounts.sort_by(|a, b| {
                    let priority = account_mode_priority;
                    let a_priority = priority(a.mode);
                    let b_priority = priority(b.mode);
                    a_priority
                        .cmp(&b_priority)
                        .then_with(|| cmp_ascii_case_insensitive(&a.label, &b.label))
                        .then_with(|| a.label.cmp(&b.label))
                        .then_with(|| a.id.cmp(&b.id))
                });

                let selected_idx = previously_selected_id
                    .as_deref()
                    .and_then(|id| self.accounts.iter().position(|row| row.id == id))
                    .or_else(|| {
                        active_id.as_deref().and_then(|id| {
                            self.accounts.iter().position(|row| row.id == id)
                        })
                    });

                if self.accounts.is_empty() {
                    self.selected = 0;
                } else {
                    self.selected = selected_idx.unwrap_or(0).min(self.accounts.len() - 1);
                }
            }
            Err(err) => {
                self.feedback = Some(Feedback {
                    message: format!("Failed to read accounts: {err}"),
                    is_error: true,
                });
                self.accounts.clear();
                self.selected = 0;
                self.active_account_id = None;
            }
        }
    }

    pub(super) fn sync_account_store_from_auth(&mut self) {
        let mut auth_json = match auth::load_auth_dot_json(&self.code_home, self.auth_credentials_store_mode)
        {
            Ok(Some(auth)) => auth,
            Ok(None) => return,
            Err(err) => {
                self.feedback = Some(Feedback {
                    message: format!("Failed to read current auth: {err}"),
                    is_error: true,
                });
                return;
            }
        };

        if let Some(tokens) = auth_json.tokens.take() {
            let last_refresh = auth_json.last_refresh.unwrap_or_else(Utc::now);
            let email = tokens.id_token.email.clone();
            if let Err(err) = auth_accounts::upsert_chatgpt_account(
                &self.code_home,
                tokens,
                last_refresh,
                email,
                true,
            ) {
                self.feedback = Some(Feedback {
                    message: format!("Failed to record ChatGPT login: {err}"),
                    is_error: true,
                });
            }
            return;
        }

        if let Some(api_key) = auth_json.openai_api_key.take() {
            if let Err(err) = auth_accounts::upsert_api_key_account(
                &self.code_home,
                api_key,
                None,
                true,
            ) {
                self.feedback = Some(Feedback {
                    message: format!("Failed to record API key login: {err}"),
                    is_error: true,
                });
            }
        }
    }

    pub(super) fn activate_account(&mut self, account_id: String, mode: AuthMode) -> bool {
        match auth::activate_account_with_store_mode(
            &self.code_home,
            &account_id,
            self.auth_credentials_store_mode,
        ) {
            Ok(()) => {
                self.feedback = Some(Feedback {
                    message: if mode.is_chatgpt() {
                        "ChatGPT account selected".to_string()
                    } else {
                        "API key selected".to_string()
                    },
                    is_error: false,
                });
                self.reload_accounts();
                self.app_event_tx.send(AppEvent::LoginUsingChatGptChanged {
                    using_chatgpt_auth: mode.is_chatgpt(),
                });
                true
            }
            Err(err) => {
                self.feedback = Some(Feedback {
                    message: format!("Failed to activate account: {err}"),
                    is_error: true,
                });
                false
            }
        }
    }

    pub(super) fn remove_account(&mut self, account_id: String) {
        match auth_accounts::remove_account(&self.code_home, &account_id) {
            Ok(Some(_)) => {
                let removed_active = self
                    .active_account_id
                    .as_ref()
                    .is_some_and(|id| id == &account_id);
                if removed_active {
                    let _ = auth::logout_with_store_mode(
                        &self.code_home,
                        self.auth_credentials_store_mode,
                    );
                }
                self.feedback = Some(Feedback {
                    message: "Account disconnected".to_string(),
                    is_error: false,
                });
                self.mode = super::ViewMode::List;
                self.reload_accounts();
                let using_chatgpt = self
                    .active_account_id
                    .as_ref()
                    .and_then(|id| auth_accounts::find_account(&self.code_home, id).ok().flatten())
                    .map(|acc| acc.mode.is_chatgpt())
                    .unwrap_or(false);
                self.app_event_tx.send(AppEvent::LoginUsingChatGptChanged {
                    using_chatgpt_auth: using_chatgpt,
                });
            }
            Ok(None) => {
                self.feedback = Some(Feedback {
                    message: "Account no longer exists".to_string(),
                    is_error: true,
                });
                self.mode = super::ViewMode::List;
                self.reload_accounts();
            }
            Err(err) => {
                self.feedback = Some(Feedback {
                    message: format!("Failed to remove account: {err}"),
                    is_error: true,
                });
                self.mode = super::ViewMode::List;
            }
        }
    }
}
