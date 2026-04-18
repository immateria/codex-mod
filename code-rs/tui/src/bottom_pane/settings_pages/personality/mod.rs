use code_core::config_types::Personality;
use code_core::config_types::Tone;

use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;

mod input;
pub(crate) mod model;
mod mouse;
mod pane_impl;
mod pages;
mod render;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum PersonalityRow {
    Archetype,
    TonePreference,
    TraitsInfo,
}

pub(crate) struct PersonalitySettingsView {
    personality: Option<Personality>,
    tone: Option<Tone>,
    has_traits: bool,
    app_event_tx: AppEventSender,
    state: ScrollState,
    is_complete: bool,
}

crate::bottom_pane::chrome_view::impl_chrome_view!(PersonalitySettingsView);

impl PersonalitySettingsView {
    pub fn new(
        personality: Option<Personality>,
        tone: Option<Tone>,
        has_traits: bool,
        app_event_tx: AppEventSender,
    ) -> Self {
        let state = ScrollState::with_first_selected();
        Self {
            personality,
            tone,
            has_traits,
            app_event_tx,
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
