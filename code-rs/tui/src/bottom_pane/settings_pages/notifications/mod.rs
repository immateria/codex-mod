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

#[derive(Clone)]
pub(crate) enum NotificationsMode {
    Toggle { enabled: bool },
    Custom { entries: Vec<String> },
}

pub(crate) struct NotificationsSettingsView {
    mode: NotificationsMode,
    prevent_idle_sleep: bool,
    app_event_tx: AppEventSender,
    ticket: BackgroundOrderTicket,
    state: ScrollState,
    is_complete: bool,
}

crate::bottom_pane::chrome_view::impl_chrome_view!(NotificationsSettingsView);

impl NotificationsSettingsView {
    pub fn new(
        mode: NotificationsMode,
        prevent_idle_sleep: bool,
        app_event_tx: AppEventSender,
        ticket: BackgroundOrderTicket,
    ) -> Self {
        let mut state = ScrollState::new();
        state.clamp_selection(Self::ROW_COUNT);
        Self {
            mode,
            prevent_idle_sleep,
            app_event_tx,
            ticket,
            state,
            is_complete: false,
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn has_back_navigation(&self) -> bool {
        false
    }
}
