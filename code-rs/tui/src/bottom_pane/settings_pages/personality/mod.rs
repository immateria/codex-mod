use code_core::config_types::Personality;
use code_core::config_types::Tone;
use code_core::personality_traits::PersonalityTraits;

use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;

mod input;
pub(crate) mod model;
mod mouse;
mod pane_impl;
mod pages;
mod render;

const DEFAULT_VISIBLE_ROWS: usize = 6;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum PersonalityRow {
    Archetype,
    TonePreference,
    TraitSeparator,
    TraitConciseness,
    TraitThoroughness,
    TraitAutonomy,
    TraitPedagogy,
    TraitEnthusiasm,
    TraitFormality,
    TraitBoldness,
}

pub(crate) struct PersonalitySettingsView {
    personality: Option<Personality>,
    tone: Option<Tone>,
    traits: PersonalityTraits,
    app_event_tx: AppEventSender,
    state: ScrollState,
    viewport_rows: std::cell::Cell<usize>,
    is_complete: bool,
}

crate::bottom_pane::chrome_view::impl_chrome_view!(PersonalitySettingsView);

impl PersonalitySettingsView {
    pub fn new(
        personality: Option<Personality>,
        tone: Option<Tone>,
        traits: Option<PersonalityTraits>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let state = ScrollState::with_first_selected();
        Self {
            personality,
            tone,
            traits: traits.unwrap_or_default(),
            app_event_tx,
            state,
            viewport_rows: std::cell::Cell::new(DEFAULT_VISIBLE_ROWS),
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
