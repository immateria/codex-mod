use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;
use code_core::config_types::AuthCredentialsStoreMode;

mod input;
mod model;
mod mouse;
mod pane_impl;
mod pages;
mod render;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewMode {
    Main,
    ConfirmStoreChange {
        target: AuthCredentialsStoreMode,
    },
}

pub(crate) struct AccountSwitchSettingsView {
    app_event_tx: AppEventSender,
    main_state: ScrollState,
    confirm_state: ScrollState,
    auto_switch_enabled: bool,
    api_key_fallback_enabled: bool,
    auth_credentials_store_mode: AuthCredentialsStoreMode,
    view_mode: ViewMode,
    is_complete: bool,
}

pub(crate) type AccountSwitchSettingsViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, AccountSwitchSettingsView>;
pub(crate) type AccountSwitchSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, AccountSwitchSettingsView>;
pub(crate) type AccountSwitchSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, AccountSwitchSettingsView>;
pub(crate) type AccountSwitchSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, AccountSwitchSettingsView>;

impl AccountSwitchSettingsView {
    const MAIN_OPTION_COUNT: usize = 6;
    const CONFIRM_OPTION_COUNT: usize = 3;

    pub(crate) fn new(
        app_event_tx: AppEventSender,
        auto_switch_enabled: bool,
        api_key_fallback_enabled: bool,
        auth_credentials_store_mode: AuthCredentialsStoreMode,
    ) -> Self {
        Self {
            app_event_tx,
            main_state: ScrollState {
                selected_idx: Some(0),
                scroll_top: 0,
            },
            confirm_state: ScrollState::default(),
            auto_switch_enabled,
            api_key_fallback_enabled,
            auth_credentials_store_mode,
            view_mode: ViewMode::Main,
            is_complete: false,
        }
    }

    pub(crate) fn framed(&self) -> AccountSwitchSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> AccountSwitchSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> AccountSwitchSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> AccountSwitchSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(crate) fn is_view_complete(&self) -> bool {
        self.is_complete
    }
}
