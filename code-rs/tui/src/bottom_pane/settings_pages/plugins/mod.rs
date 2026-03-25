use std::cell::Cell;
use std::sync::{Arc, Mutex};

use code_core::plugins::PluginInstallRequest;
use code_core::plugins::PluginReadRequest;
use code_utils_absolute_path::AbsolutePathBuf;

use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;

use crate::chatwidget::PluginDetailKey;
use crate::chatwidget::PluginsSharedState;

mod input;
mod model;
mod mouse;
mod pages;
mod pane_impl;
mod render;
#[cfg(test)]
mod tests;

const DEFAULT_LIST_VIEWPORT_ROWS: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Mode {
    List,
    Detail {
        key: PluginDetailKey,
    },
    ConfirmUninstall {
        plugin_id_key: String,
        key: PluginDetailKey,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DetailAction {
    Install,
    Uninstall,
    Enable,
    Disable,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfirmAction {
    Uninstall,
    Cancel,
}

pub(crate) struct PluginsSettingsView {
    shared_state: Arc<Mutex<PluginsSharedState>>,
    roots: Vec<AbsolutePathBuf>,
    list_state: Cell<ScrollState>,
    list_viewport_rows: Cell<usize>,
    mode: Mode,
    hovered_detail_button: Option<DetailAction>,
    focused_detail_button: DetailAction,
    hovered_confirm_button: Option<ConfirmAction>,
    focused_confirm_button: ConfirmAction,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

pub(crate) type PluginsSettingsViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, PluginsSettingsView>;
pub(crate) type PluginsSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, PluginsSettingsView>;
pub(crate) type PluginsSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, PluginsSettingsView>;
pub(crate) type PluginsSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, PluginsSettingsView>;

impl PluginsSettingsView {
    fn selected_list_index(&self, plugin_count: usize) -> usize {
        self.list_state
            .get()
            .selected_idx
            .unwrap_or(0)
            .min(plugin_count.saturating_sub(1))
    }

    fn request_plugin_list(&self, force_remote_sync: bool) {
        self.app_event_tx.send(crate::app_event::AppEvent::FetchPluginsList {
            roots: self.roots.clone(),
            force_remote_sync,
        });
    }

    fn request_plugin_detail(&self, request: PluginReadRequest) {
        self.app_event_tx
            .send(crate::app_event::AppEvent::FetchPluginDetail { request });
    }

    fn request_install_plugin(&self, request: PluginInstallRequest, force_remote_sync: bool) {
        self.app_event_tx.send(crate::app_event::AppEvent::InstallPlugin {
            request,
            force_remote_sync,
        });
    }

    fn request_uninstall_plugin(&self, plugin_id_key: String, force_remote_sync: bool) {
        self.app_event_tx
            .send(crate::app_event::AppEvent::UninstallPlugin {
                plugin_id_key,
                force_remote_sync,
            });
    }

    fn request_set_plugin_enabled(&self, plugin_id_key: String, enabled: bool) {
        self.app_event_tx
            .send(crate::app_event::AppEvent::SetPluginEnabled {
                plugin_id_key,
                enabled,
            });
    }
}
