use crate::bottom_pane::settings_pages::prompts::PromptsSettingsView;

pub(crate) struct PromptsSettingsContent {
    view: PromptsSettingsView,
}

impl PromptsSettingsContent {
    pub(crate) fn new(view: PromptsSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content!(PromptsSettingsContent);
