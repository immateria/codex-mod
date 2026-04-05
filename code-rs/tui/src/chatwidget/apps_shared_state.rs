use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use code_core::config_types::AppsSourcesToml;

use crate::app_event::AppsStatusSnapshot;

use super::ChatWidget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppsAccountSnapshot {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) is_chatgpt: bool,
    pub(crate) is_active_model_account: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConnectedAppSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) tool_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) enum AppsAccountStatusState {
    Uninitialized,
    Loading,
    Ready {
        connected_apps: Vec<ConnectedAppSummary>,
        last_refresh: DateTime<Utc>,
    },
    Failed {
        error: String,
        needs_login: bool,
    },
}

impl Default for AppsAccountStatusState {
    fn default() -> Self {
        Self::Uninitialized
    }
}

#[derive(Debug, Clone)]
pub(crate) enum AppsActionInProgress {
    SaveSources,
    RefreshStatus { account_ids: Vec<String> },
}

#[derive(Debug, Default, Clone)]
pub(crate) struct AppsSharedState {
    pub(crate) active_profile: Option<String>,
    pub(crate) sources_snapshot: AppsSourcesToml,
    pub(crate) accounts_snapshot: Vec<AppsAccountSnapshot>,
    pub(crate) status_by_account_id: HashMap<String, AppsAccountStatusState>,
    pub(crate) pending_status_refresh_account_ids: Option<Vec<String>>,
    pub(crate) action_in_progress: Option<AppsActionInProgress>,
    pub(crate) action_error: Option<String>,
}

impl ChatWidget<'_> {
    pub(crate) fn apps_shared_state(&self) -> Arc<Mutex<AppsSharedState>> {
        self.apps_shared_state.clone()
    }

    pub(crate) fn apps_set_sources_snapshot(
        &mut self,
        active_profile: Option<String>,
        sources: AppsSourcesToml,
    ) {
        let mut state = self.apps_shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.active_profile = active_profile;
        state.sources_snapshot = sources;
    }

    pub(crate) fn apps_set_accounts_snapshot(&mut self, accounts: Vec<AppsAccountSnapshot>) {
        let mut state = self.apps_shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.accounts_snapshot = accounts;
    }

    pub(crate) fn apps_mark_status_loading(
        &mut self,
        account_ids: &[String],
        force_refresh_tools: bool,
    ) {
        let mut state = self.apps_shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for id in account_ids {
            state
                .status_by_account_id
                .insert(id.clone(), AppsAccountStatusState::Loading);
        }
        state.action_error = None;
        state.action_in_progress = Some(AppsActionInProgress::RefreshStatus {
            account_ids: account_ids.to_vec(),
        });
        if force_refresh_tools {
            state.pending_status_refresh_account_ids = Some(account_ids.to_vec());
        }
    }

    pub(crate) fn apps_take_pending_status_refresh_account_ids(&mut self) -> Option<Vec<String>> {
        let mut state = self.apps_shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.pending_status_refresh_account_ids.take()
    }

    pub(crate) fn apps_apply_status_loaded(
        &mut self,
        account_id: String,
        result: Result<AppsStatusSnapshot, String>,
        needs_login: bool,
    ) {
        let mut state = self.apps_shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match result {
            Ok(snapshot) => {
                state.status_by_account_id.insert(
                    account_id,
                    AppsAccountStatusState::Ready {
                        connected_apps: snapshot.connected_apps,
                        last_refresh: snapshot.last_refresh,
                    },
                );
            }
            Err(error) => {
                state.status_by_account_id.insert(
                    account_id,
                    AppsAccountStatusState::Failed { error, needs_login },
                );
            }
        }

        if let Some(AppsActionInProgress::RefreshStatus { account_ids }) = &state.action_in_progress {
            let any_loading = account_ids.iter().any(|id| {
                matches!(
                    state.status_by_account_id.get(id),
                    Some(AppsAccountStatusState::Loading)
                )
            });
            if !any_loading {
                state.action_in_progress = None;
            }
        }
        drop(state);
        self.refresh_settings_overview_rows();
    }

    pub(crate) fn apps_set_action_in_progress(&mut self, action: AppsActionInProgress) {
        let mut state = self.apps_shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.action_in_progress = Some(action);
        state.action_error = None;
    }

    pub(crate) fn apps_clear_action_in_progress(&mut self) {
        let mut state = self.apps_shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.action_in_progress = None;
    }

    pub(crate) fn apps_set_action_error(&mut self, error: Option<String>) {
        let mut state = self.apps_shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.action_error = error;
    }

    pub(crate) fn apps_connected_apps_from_mcp_snapshot(
        &self,
        source_account_id: &str,
    ) -> Vec<ConnectedAppSummary> {
        let server_name =
            code_core::apps_sources::codex_apps_server_name_for_source_account_id(source_account_id);
        let prefix = format!("{server_name}__");

        let mut by_id: HashMap<String, ConnectedAppSummary> = HashMap::new();
        for (qualified_tool_id, tool) in &self.mcp_tool_catalog_protocol_by_id {
            if !qualified_tool_id.starts_with(&prefix) {
                continue;
            }
            let Some(meta) = tool
                .annotations
                .as_ref()
                .or(tool.meta.as_ref())
                .and_then(serde_json::Value::as_object)
            else {
                continue;
            };
            let Some(connector_id) = meta
                .get("connector_id")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
            else {
                continue;
            };
            let connector_name = meta
                .get("connector_name")
                .or_else(|| meta.get("connector_display_name"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(connector_id);
            let connector_description = meta
                .get("connector_description")
                .or_else(|| meta.get("connectorDescription"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|description| !description.is_empty())
                .map(str::to_string);

            let entry = by_id.entry(connector_id.to_string()).or_insert_with(|| {
                ConnectedAppSummary {
                    id: connector_id.to_string(),
                    name: connector_name.to_string(),
                    description: connector_description.clone(),
                    tool_count: 0,
                }
            });
            entry.tool_count = entry.tool_count.saturating_add(1);
        }

        let mut apps = by_id.into_values().collect::<Vec<_>>();
        apps.sort_by(|left, right| left.name.cmp(&right.name).then_with(|| left.id.cmp(&right.id)));
        apps
    }

    pub(crate) fn apps_status_snapshot_for_account_id(
        &self,
        source_account_id: &str,
    ) -> (Result<AppsStatusSnapshot, String>, bool) {
        let server_name =
            code_core::apps_sources::codex_apps_server_name_for_source_account_id(source_account_id);
        let needs_login = matches!(
            self.mcp_auth_statuses.get(&server_name).copied(),
            Some(code_core::protocol::McpAuthStatus::NotLoggedIn),
        );
        if needs_login {
            return (Err("Not logged in".to_string()), true);
        }

        fn message_looks_like_auth_issue(message: &str) -> bool {
            let lower = message.to_ascii_lowercase();
            lower.contains("not logged")
                || lower.contains("unauthorized")
                || lower.contains("forbidden")
                || lower.contains("auth")
                || lower.contains("token")
                || lower.contains("401")
                || lower.contains("403")
        }

        if let Some(failure) = self.mcp_server_failures.get(&server_name) {
            let needs_login = message_looks_like_auth_issue(&failure.message);
            return (Err(failure.message.clone()), needs_login);
        }

        let connected_apps = self.apps_connected_apps_from_mcp_snapshot(source_account_id);
        (
            Ok(AppsStatusSnapshot {
                connected_apps,
                last_refresh: Utc::now(),
            }),
            false,
        )
    }
}
