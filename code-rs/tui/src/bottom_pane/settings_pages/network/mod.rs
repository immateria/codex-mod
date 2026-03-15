mod input;
mod mouse;
mod pages;
mod pane_impl;
mod render;

use std::cell::Cell;

use code_core::config::NetworkProxySettingsToml;
use code_core::protocol::SandboxPolicy;

use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;
use crate::components::form_text_field::FormTextField;
use crate::components::scroll_state::ScrollState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditTarget {
    AllowedDomains,
    DeniedDomains,
    AllowUnixSockets,
}

#[derive(Debug)]
enum ViewMode {
    Transition,
    Main { show_advanced: bool },
    EditList {
        target: EditTarget,
        field: FormTextField,
        show_advanced: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    Enabled,
    Mode,
    AllowedDomains,
    DeniedDomains,
    AllowLocalBinding,
    AdvancedToggle,
    Socks5Enabled,
    Socks5Udp,
    AllowUpstreamProxyEnv,
    AllowUnixSockets,
    Apply,
    Close,
}

pub(crate) struct NetworkSettingsView {
    settings: NetworkProxySettingsToml,
    sandbox_policy: SandboxPolicy,
    app_event_tx: AppEventSender,
    ticket: BackgroundOrderTicket,
    is_complete: bool,
    dirty: bool,
    mode: ViewMode,
    state: ScrollState,
    viewport_rows: Cell<usize>,
}

pub(crate) type NetworkSettingsViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, NetworkSettingsView>;
pub(crate) type NetworkSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, NetworkSettingsView>;
pub(crate) type NetworkSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, NetworkSettingsView>;
pub(crate) type NetworkSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, NetworkSettingsView>;

impl NetworkSettingsView {
    const DEFAULT_VISIBLE_ROWS: usize = 8;

    pub fn new(
        settings: Option<NetworkProxySettingsToml>,
        sandbox_policy: SandboxPolicy,
        app_event_tx: AppEventSender,
        ticket: BackgroundOrderTicket,
    ) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        Self {
            settings: settings.unwrap_or_default(),
            sandbox_policy,
            app_event_tx,
            ticket,
            is_complete: false,
            dirty: false,
            mode: ViewMode::Main { show_advanced: false },
            state,
            viewport_rows: Cell::new(0),
        }
    }

    pub(crate) fn framed(&self) -> NetworkSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> NetworkSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> NetworkSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> NetworkSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }
}

