use crate::bottom_pane::ReviewSettingsView;
use code_core::config_types::ReasoningEffort;

pub(crate) struct ReviewSettingsContent {
    view: ReviewSettingsView,
}

impl ReviewSettingsContent {
    pub(crate) fn new(view: ReviewSettingsView) -> Self {
        Self { view }
    }

    pub(crate) fn update_review_model(&mut self, model: String, effort: ReasoningEffort) {
        self.view.set_review_model(model, effort);
    }

    pub(crate) fn set_review_use_chat_model(&mut self, use_chat: bool) {
        self.view.set_review_use_chat_model(use_chat);
    }

    pub(crate) fn update_review_resolve_model(&mut self, model: String, effort: ReasoningEffort) {
        self.view.set_review_resolve_model(model, effort);
    }

    pub(crate) fn set_review_resolve_use_chat_model(&mut self, use_chat: bool) {
        self.view.set_review_resolve_use_chat_model(use_chat);
    }

    pub(crate) fn update_auto_review_model(&mut self, model: String, effort: ReasoningEffort) {
        self.view.set_auto_review_model(model, effort);
    }

    pub(crate) fn set_auto_review_use_chat_model(&mut self, use_chat: bool) {
        self.view.set_auto_review_use_chat_model(use_chat);
    }

    pub(crate) fn update_auto_review_resolve_model(
        &mut self,
        model: String,
        effort: ReasoningEffort,
    ) {
        self.view.set_auto_review_resolve_model(model, effort);
    }

    pub(crate) fn set_auto_review_resolve_use_chat_model(&mut self, use_chat: bool) {
        self.view.set_auto_review_resolve_use_chat_model(use_chat);
    }

    pub(crate) fn set_review_followups(&mut self, attempts: u32) {
        self.view.set_review_followups(attempts);
    }

    pub(crate) fn set_auto_review_followups(&mut self, attempts: u32) {
        self.view.set_auto_review_followups(attempts);
    }
}

impl_settings_content!(ReviewSettingsContent);
