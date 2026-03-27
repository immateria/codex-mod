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

const DEFAULT_LIST_VIEWPORT_ROWS: usize = 10;

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

pub(crate) type AppsSettingsViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, AppsSettingsView>;
pub(crate) type AppsSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, AppsSettingsView>;
pub(crate) type AppsSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, AppsSettingsView>;
pub(crate) type AppsSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, AppsSettingsView>;

impl AppsSettingsView {
    pub(crate) fn new(
        shared_state: Arc<Mutex<AppsSharedState>>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let snapshot = shared_state.lock().unwrap_or_else(|err| err.into_inner()).clone();
        let baseline_sources = snapshot.sources_snapshot.clone();
        let draft_sources = baseline_sources.clone();

        let mut list_state = ScrollState::new();
        list_state.selected_idx = Some(0);

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

    pub(crate) fn framed(&self) -> AppsSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> AppsSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> AppsSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> AppsSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    fn sync_sources_snapshot_if_clean(&mut self) {
        let snapshot = self
            .shared_state
            .lock()
            .unwrap_or_else(|err| err.into_inner())
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
}
