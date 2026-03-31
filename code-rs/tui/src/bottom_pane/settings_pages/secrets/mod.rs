use std::cell::Cell;
use std::sync::{Arc, Mutex};

use code_secrets::SecretListEntry;

use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;

use crate::chatwidget::SecretsSharedState;

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
    List,
    ConfirmDelete { entry: SecretListEntry },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfirmAction {
    Delete,
    Cancel,
}

pub(crate) struct SecretsSettingsView {
    shared_state: Arc<Mutex<SecretsSharedState>>,
    env_id: String,
    list_state: Cell<ScrollState>,
    list_viewport_rows: Cell<usize>,
    mode: Mode,
    hovered_confirm_button: Option<ConfirmAction>,
    focused_confirm_button: ConfirmAction,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

pub(crate) type SecretsSettingsViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, SecretsSettingsView>;
pub(crate) type SecretsSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, SecretsSettingsView>;
pub(crate) type SecretsSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, SecretsSettingsView>;
pub(crate) type SecretsSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, SecretsSettingsView>;

impl SecretsSettingsView {
    pub(crate) fn new(
        shared_state: Arc<Mutex<SecretsSharedState>>,
        env_id: String,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut list_state = ScrollState::new();
        list_state.selected_idx = Some(0);

        let view = Self {
            shared_state,
            env_id,
            list_state: Cell::new(list_state),
            list_viewport_rows: Cell::new(DEFAULT_LIST_VIEWPORT_ROWS),
            mode: Mode::List,
            hovered_confirm_button: None,
            focused_confirm_button: ConfirmAction::Cancel,
            app_event_tx,
            is_complete: false,
        };

        let should_request_list = {
            let snapshot = view
                .shared_state
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            match &snapshot.list {
                crate::chatwidget::SecretsListState::Uninitialized => true,
                crate::chatwidget::SecretsListState::Loading { env_id }
                | crate::chatwidget::SecretsListState::Ready { env_id, .. }
                | crate::chatwidget::SecretsListState::Failed { env_id, .. } => env_id != &view.env_id,
            }
        };
        if should_request_list {
            view.request_secrets_list();
        }

        view
    }

    pub(crate) fn framed(&self) -> SecretsSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> SecretsSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> SecretsSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> SecretsSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn shared_snapshot(&self) -> SecretsSharedState {
        self.shared_state
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    fn list_entries(snapshot: &SecretsSharedState) -> Option<&[SecretListEntry]> {
        match &snapshot.list {
            crate::chatwidget::SecretsListState::Ready { entries, .. } => Some(entries),
            _ => None,
        }
    }

    fn selected_entry(&self, snapshot: &SecretsSharedState) -> Option<SecretListEntry> {
        let entries = Self::list_entries(snapshot)?;
        let idx = self
            .list_state
            .get()
            .selected_idx
            .unwrap_or(0)
            .min(entries.len().saturating_sub(1));
        entries.get(idx).cloned()
    }

    fn request_secrets_list(&self) {
        self.app_event_tx
            .send(crate::app_event::AppEvent::FetchSecretsList {
                env_id: self.env_id.clone(),
            });
    }

    fn request_delete_secret(&self, entry: SecretListEntry) {
        self.app_event_tx.send(crate::app_event::AppEvent::DeleteSecret {
            env_id: self.env_id.clone(),
            entry,
        });
    }
}
