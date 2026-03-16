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
    app_event_tx: AppEventSender,
    ticket: BackgroundOrderTicket,
    state: ScrollState,
    is_complete: bool,
}

pub(crate) type NotificationsSettingsViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, NotificationsSettingsView>;
pub(crate) type NotificationsSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, NotificationsSettingsView>;
pub(crate) type NotificationsSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, NotificationsSettingsView>;

impl NotificationsSettingsView {
    pub fn new(
        mode: NotificationsMode,
        app_event_tx: AppEventSender,
        ticket: BackgroundOrderTicket,
    ) -> Self {
        let mut state = ScrollState::new();
        state.clamp_selection(Self::ROW_COUNT);
        Self {
            mode,
            app_event_tx,
            ticket,
            state,
            is_complete: false,
        }
    }

    pub(crate) fn framed(&self) -> NotificationsSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> NotificationsSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> NotificationsSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }
}
