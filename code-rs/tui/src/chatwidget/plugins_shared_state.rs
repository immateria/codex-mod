use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use code_core::plugins::{ConfiguredMarketplace, PluginReadOutcome};
use code_utils_absolute_path::AbsolutePathBuf;

use crate::app_event::PluginListSnapshot;

use super::ChatWidget;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PluginDetailKey {
    pub(crate) marketplace_path: AbsolutePathBuf,
    pub(crate) plugin_name: String,
}

impl PluginDetailKey {
    pub(crate) fn new(marketplace_path: AbsolutePathBuf, plugin_name: String) -> Self {
        Self {
            marketplace_path,
            plugin_name,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PluginsListState {
    Uninitialized,
    Loading {
        roots: Vec<AbsolutePathBuf>,
        force_remote_sync: bool,
    },
    Ready {
        roots: Vec<AbsolutePathBuf>,
        marketplaces: Vec<ConfiguredMarketplace>,
        remote_sync_error: Option<String>,
        featured_plugin_ids: Vec<String>,
    },
    Failed {
        roots: Vec<AbsolutePathBuf>,
        error: String,
    },
}

impl Default for PluginsListState {
    fn default() -> Self {
        Self::Uninitialized
    }
}

impl PluginsListState {
    pub(crate) fn roots(&self) -> Option<&[AbsolutePathBuf]> {
        match self {
            PluginsListState::Uninitialized => None,
            PluginsListState::Loading { roots, .. } => Some(roots),
            PluginsListState::Ready { roots, .. } => Some(roots),
            PluginsListState::Failed { roots, .. } => Some(roots),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PluginsDetailState {
    Uninitialized,
    Loading,
    Ready(PluginReadOutcome),
    Failed(String),
}

impl Default for PluginsDetailState {
    fn default() -> Self {
        Self::Uninitialized
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PluginsActionInProgress {
    FetchList,
    FetchDetail(PluginDetailKey),
    Install {
        marketplace_path: AbsolutePathBuf,
        plugin_name: String,
        force_remote_sync: bool,
    },
    Uninstall {
        plugin_id_key: String,
        force_remote_sync: bool,
    },
    SetEnabled {
        plugin_id_key: String,
        enabled: bool,
    },
}

#[derive(Debug, Default, Clone)]
pub(crate) struct PluginsSharedState {
    pub(crate) list: PluginsListState,
    pub(crate) details: HashMap<PluginDetailKey, PluginsDetailState>,
    pub(crate) action_in_progress: Option<PluginsActionInProgress>,
    pub(crate) action_error: Option<String>,
}

impl ChatWidget<'_> {
    pub(crate) fn plugins_shared_state(&self) -> Arc<Mutex<PluginsSharedState>> {
        self.plugins_shared_state.clone()
    }

    pub(crate) fn plugins_set_action_in_progress(&mut self, action: PluginsActionInProgress) {
        let mut state = self.plugins_shared_state.lock().unwrap_or_else(|err| err.into_inner());
        state.action_in_progress = Some(action);
        state.action_error = None;
    }

    pub(crate) fn plugins_clear_action_in_progress(&mut self) {
        let mut state = self.plugins_shared_state.lock().unwrap_or_else(|err| err.into_inner());
        state.action_in_progress = None;
    }

    pub(crate) fn plugins_set_action_error(&mut self, error: Option<String>) {
        let mut state = self.plugins_shared_state.lock().unwrap_or_else(|err| err.into_inner());
        state.action_error = error;
    }

    pub(crate) fn plugins_mark_list_loading(
        &mut self,
        roots: Vec<AbsolutePathBuf>,
        force_remote_sync: bool,
    ) {
        let mut state = self.plugins_shared_state.lock().unwrap_or_else(|err| err.into_inner());
        state.list = PluginsListState::Loading {
            roots,
            force_remote_sync,
        };
        state.action_error = None;
        state.action_in_progress = Some(PluginsActionInProgress::FetchList);
    }

    pub(crate) fn plugins_apply_list_loaded(
        &mut self,
        roots: Vec<AbsolutePathBuf>,
        result: Result<PluginListSnapshot, String>,
    ) {
        let mut state = self.plugins_shared_state.lock().unwrap_or_else(|err| err.into_inner());

        // Ignore stale responses (roots changed since request).
        if let Some(current_roots) = state.list.roots()
            && *current_roots != roots
        {
            return;
        }

        match result {
            Ok(snapshot) => {
                state.list = PluginsListState::Ready {
                    roots,
                    marketplaces: snapshot.marketplaces,
                    remote_sync_error: snapshot.remote_sync_error,
                    featured_plugin_ids: snapshot.featured_plugin_ids,
                };
            }
            Err(error) => {
                state.list = PluginsListState::Failed { roots, error };
            }
        }

        if matches!(state.action_in_progress, Some(PluginsActionInProgress::FetchList)) {
            state.action_in_progress = None;
        }
    }

    pub(crate) fn plugins_mark_detail_loading(&mut self, key: PluginDetailKey) {
        let mut state = self.plugins_shared_state.lock().unwrap_or_else(|err| err.into_inner());
        state
            .details
            .insert(key.clone(), PluginsDetailState::Loading);
        state.action_error = None;
        state.action_in_progress = Some(PluginsActionInProgress::FetchDetail(key));
    }

    pub(crate) fn plugins_apply_detail_loaded(
        &mut self,
        key: PluginDetailKey,
        result: Result<PluginReadOutcome, String>,
    ) {
        let mut state = self.plugins_shared_state.lock().unwrap_or_else(|err| err.into_inner());

        match result {
            Ok(outcome) => {
                state.details.insert(key.clone(), PluginsDetailState::Ready(outcome));
            }
            Err(error) => {
                state.details.insert(key.clone(), PluginsDetailState::Failed(error));
            }
        }

        if matches!(
            state.action_in_progress,
            Some(PluginsActionInProgress::FetchDetail(ref current)) if *current == key
        ) {
            state.action_in_progress = None;
        }
    }
}
