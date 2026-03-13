use crate::bottom_pane::AutoDriveSettingsView;
use code_core::config_types::ReasoningEffort;

pub(crate) struct AutoDriveSettingsContent {
    view: AutoDriveSettingsView,
}

impl AutoDriveSettingsContent {
    pub(crate) fn new(view: AutoDriveSettingsView) -> Self {
        Self { view }
    }

    pub(crate) fn update_model(&mut self, model: String, effort: ReasoningEffort) {
        self.view.set_model(model, effort);
    }

    pub(crate) fn set_use_chat_model(
        &mut self,
        use_chat: bool,
        model: String,
        effort: ReasoningEffort,
    ) {
        self.view.set_use_chat_model(use_chat, model, effort);
    }
}

impl_settings_content_view_complete!(AutoDriveSettingsContent);
