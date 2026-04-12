use std::cell::Cell;
use std::sync::{Arc, Mutex};

use code_core::config_types::AppsSourcesModeToml;
use code_core::config_types::AppsSourcesToml;

use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;

use crate::chatwidget::AppsSharedState;

mod input;
mod mouse;
mod pages;
mod pane_impl;
mod render;
#[cfg(test)]
mod tests;

use crate::timing::DEFAULT_LIST_VIEWPORT_ROWS;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Mode {
    Overview,
    AccountDetail { account_id: String },
}

pub(crate) struct AppsSettingsView {
    shared_state: Arc<Mutex<AppsSharedState>>,
    list_state: Cell<ScrollState>,
    list_viewport_rows: Cell<usize>,
    mode: Mode,
    baseline_sources: AppsSourcesToml,
    draft_sources: AppsSourcesToml,
    sources_dirty: bool,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

crate::bottom_pane::chrome_view::impl_chrome_view!(AppsSettingsView);

impl AppsSettingsView {
    pub(crate) fn new(
        shared_state: Arc<Mutex<AppsSharedState>>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let snapshot = shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
        let baseline_sources = snapshot.sources_snapshot;
        let draft_sources = baseline_sources.clone();

        let list_state = ScrollState::with_first_selected();

        Self {
            shared_state,
            list_state: Cell::new(list_state),
            list_viewport_rows: Cell::new(DEFAULT_LIST_VIEWPORT_ROWS),
            mode: Mode::Overview,
            baseline_sources,
            draft_sources,
            sources_dirty: false,
            app_event_tx,
            is_complete: false,
        }
    }

    fn sync_sources_snapshot_if_clean(&mut self) {
        let snapshot = self
            .shared_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if self.sources_dirty {
            // If the shared snapshot caught up to our draft (save completed),
            // accept it as the new baseline and clear the dirty flag.
            if snapshot.sources_snapshot == self.draft_sources {
                self.baseline_sources = snapshot.sources_snapshot;
                self.sources_dirty = false;
            }
            return;
        }
        if snapshot.sources_snapshot != self.baseline_sources {
            self.baseline_sources = snapshot.sources_snapshot.clone();
            self.draft_sources = snapshot.sources_snapshot;
        }
    }

    fn cycle_mode(mode: AppsSourcesModeToml) -> AppsSourcesModeToml {
        match mode {
            AppsSourcesModeToml::ActiveOnly => AppsSourcesModeToml::ActivePlusPinned,
            AppsSourcesModeToml::ActivePlusPinned => AppsSourcesModeToml::PinnedOnly,
            AppsSourcesModeToml::PinnedOnly => AppsSourcesModeToml::ActiveOnly,
        }
    }

    fn request_save_sources(&self) {
        self.app_event_tx
            .send(crate::app_event::AppEvent::SetAppsSources {
                sources: self.draft_sources.clone(),
            });
    }

    fn request_refresh_status(&self, account_ids: Vec<String>, force_refresh_tools: bool) {
        self.app_event_tx
            .send(crate::app_event::AppEvent::FetchAppsStatus {
                account_ids,
                force_refresh_tools,
            });
    }

    fn request_open_accounts_settings(&self) {
        self.app_event_tx.send(crate::app_event::AppEvent::OpenSettings {
            section: Some(crate::bottom_pane::SettingsSection::Accounts),
        });
    }

    fn request_open_accounts_login(&self) {
        self.request_open_accounts_settings();
        self.app_event_tx.send(crate::app_event::AppEvent::ShowLoginAccounts);
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn close(&mut self) {
        self.is_complete = true;
    }

    pub(crate) fn has_back_navigation(&self) -> bool {
        !matches!(self.mode, Mode::Overview)
    }
}
