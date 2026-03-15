use code_core::config_types::ReasoningEffort;

use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;

mod input;
mod model;
mod mouse;
mod pane_impl;
mod pages;
mod render;
#[cfg(test)]
mod tests;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PlanningRow {
    CustomModel,
}

pub(crate) struct PlanningSettingsView {
    use_chat_model: bool,
    planning_model: String,
    planning_reasoning: ReasoningEffort,
    app_event_tx: AppEventSender,
    state: ScrollState,
    is_complete: bool,
}

pub(crate) type PlanningSettingsViewFramed<'v> = crate::bottom_pane::chrome_view::Framed<'v, PlanningSettingsView>;
pub(crate) type PlanningSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, PlanningSettingsView>;
pub(crate) type PlanningSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, PlanningSettingsView>;
pub(crate) type PlanningSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, PlanningSettingsView>;

impl PlanningSettingsView {
    pub fn new(
        use_chat_model: bool,
        planning_model: String,
        planning_reasoning: ReasoningEffort,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        Self {
            use_chat_model,
            planning_model,
            planning_reasoning,
            app_event_tx,
            state,
            is_complete: false,
        }
    }

    pub fn set_planning_model(&mut self, model: String, effort: ReasoningEffort) {
        self.planning_model = model;
        self.planning_reasoning = effort;
    }

    pub fn set_use_chat_model(&mut self, use_chat: bool) {
        self.use_chat_model = use_chat;
    }

    pub(crate) fn framed(&self) -> PlanningSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> PlanningSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> PlanningSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> PlanningSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }
}
