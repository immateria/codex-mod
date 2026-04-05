use std::sync::{Arc, Mutex};

use code_secrets::SecretListEntry;

use crate::app_event::SecretsListSnapshot;

use super::ChatWidget;

#[derive(Debug, Clone, Default)]
pub(crate) enum SecretsListState {
    #[default]
    Uninitialized,
    Loading {
        env_id: String,
    },
    Ready {
        env_id: String,
        entries: Vec<SecretListEntry>,
    },
    Failed {
        env_id: String,
        error: String,
    },
}

impl SecretsListState {
    pub(crate) fn env_id(&self) -> Option<&str> {
        match self {
            SecretsListState::Uninitialized => None,
            SecretsListState::Loading { env_id } => Some(env_id),
            SecretsListState::Ready { env_id, .. } => Some(env_id),
            SecretsListState::Failed { env_id, .. } => Some(env_id),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum SecretsActionInProgress {
    FetchList,
    Delete {
        env_id: String,
        entry: SecretListEntry,
    },
}

#[derive(Debug, Default, Clone)]
pub(crate) struct SecretsSharedState {
    pub(crate) list: SecretsListState,
    pub(crate) action_in_progress: Option<SecretsActionInProgress>,
    pub(crate) action_error: Option<String>,
}

impl ChatWidget<'_> {
    pub(crate) fn secrets_shared_state(&self) -> Arc<Mutex<SecretsSharedState>> {
        self.secrets_shared_state.clone()
    }

    pub(crate) fn secrets_mark_list_loading(&mut self, env_id: String) {
        let mut state = self.secrets_shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.list = SecretsListState::Loading { env_id };
        state.action_error = None;
        state.action_in_progress = Some(SecretsActionInProgress::FetchList);
    }

    pub(crate) fn secrets_apply_list_loaded(
        &mut self,
        env_id: String,
        result: Result<SecretsListSnapshot, String>,
    ) {
        let mut state = self.secrets_shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(current_env_id) = state.list.env_id()
            && current_env_id != env_id
        {
            return;
        }

        match result {
            Ok(snapshot) => {
                state.list = SecretsListState::Ready {
                    env_id,
                    entries: snapshot.entries,
                };
            }
            Err(error) => {
                state.list = SecretsListState::Failed { env_id, error };
            }
        }

        if matches!(state.action_in_progress, Some(SecretsActionInProgress::FetchList)) {
            state.action_in_progress = None;
        }

        drop(state);
        self.refresh_settings_overview_rows();
    }

    pub(crate) fn secrets_mark_delete_in_progress(&mut self, env_id: String, entry: SecretListEntry) {
        let mut state = self.secrets_shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.action_error = None;
        state.action_in_progress = Some(SecretsActionInProgress::Delete { env_id, entry });
    }

    pub(crate) fn secrets_apply_delete_finished(
        &mut self,
        env_id: String,
        entry: SecretListEntry,
        result: Result<bool, String>,
    ) {
        let mut state = self.secrets_shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(current_env_id) = state.list.env_id()
            && current_env_id != env_id
        {
            return;
        }

        match result {
            Ok(_) => {
                state.action_error = None;
            }
            Err(error) => {
                state.action_error = Some(error);
            }
        }

        if matches!(
            state.action_in_progress,
            Some(SecretsActionInProgress::Delete {
                env_id: ref current_env_id,
                entry: ref current_entry,
            }) if current_env_id == &env_id && current_entry == &entry
        ) {
            state.action_in_progress = None;
        }

        drop(state);
        self.refresh_settings_overview_rows();
    }
}
