use std::sync::{Arc, Mutex};

use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;
use crate::components::scroll_state::ScrollState;

mod input;
mod model;
mod mouse;
mod pane_impl;
mod pages;
mod render;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Default)]
pub struct UpdateSharedState {
    pub checking: bool,
    pub latest_version: Option<String>,
    pub error: Option<String>,
}

pub(crate) struct UpdateSettingsView {
    app_event_tx: AppEventSender,
    ticket: BackgroundOrderTicket,
    state: ScrollState,
    is_complete: bool,
    auto_enabled: bool,
    shared: Arc<Mutex<UpdateSharedState>>,
    current_version: String,
    command: Option<Vec<String>>,
    command_display: Option<String>,
    manual_instructions: Option<String>,
}

pub(crate) type UpdateSettingsViewFramed<'v> = crate::bottom_pane::chrome_view::Framed<'v, UpdateSettingsView>;
pub(crate) type UpdateSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, UpdateSettingsView>;
pub(crate) type UpdateSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, UpdateSettingsView>;
pub(crate) type UpdateSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, UpdateSettingsView>;

pub(crate) struct UpdateSettingsInit {
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) ticket: BackgroundOrderTicket,
    pub(crate) current_version: String,
    pub(crate) auto_enabled: bool,
    pub(crate) command: Option<Vec<String>>,
    pub(crate) command_display: Option<String>,
    pub(crate) manual_instructions: Option<String>,
    pub(crate) shared: Arc<Mutex<UpdateSharedState>>,
}
