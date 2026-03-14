use crate::bottom_pane::settings_pages::planning::PlanningSettingsView;
use code_core::config_types::ReasoningEffort;

pub(crate) struct PlanningSettingsContent {
    view: PlanningSettingsView,
}

impl PlanningSettingsContent {
    pub(crate) fn new(view: PlanningSettingsView) -> Self {
        Self { view }
    }

    pub(crate) fn update_planning_model(&mut self, model: String, effort: ReasoningEffort) {
        self.view.set_planning_model(model, effort);
    }

    pub(crate) fn set_use_chat_model(&mut self, use_chat: bool) {
        self.view.set_use_chat_model(use_chat);
    }
}

impl_settings_content!(PlanningSettingsContent);
