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

pub(crate) type NetworkSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, NetworkSettingsView>;
pub(crate) type NetworkSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, NetworkSettingsView>;

impl NetworkSettingsView {
    const DEFAULT_VISIBLE_ROWS: usize = 8;

    pub(super) fn desired_height_impl(&self, _width: u16) -> u16 {
        match &self.mode {
            ViewMode::Main { show_advanced } => {
                let header = u16::try_from(Self::HEADER_LINE_COUNT).unwrap_or(u16::MAX);
                let visible = u16::try_from(self.row_count(*show_advanced).clamp(1, 12))
                    .unwrap_or(u16::MAX);
                2u16.saturating_add(header).saturating_add(visible)
            }
            ViewMode::EditList { .. } => 18,
            ViewMode::Transition => {
                let header = u16::try_from(Self::HEADER_LINE_COUNT).unwrap_or(u16::MAX);
                2u16.saturating_add(header).saturating_add(8)
            }
        }
    }

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

    pub(crate) fn content_only(&self) -> NetworkSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> NetworkSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }
}

