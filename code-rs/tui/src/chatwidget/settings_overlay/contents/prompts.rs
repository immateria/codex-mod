use crate::bottom_pane::prompts_settings_view::PromptsSettingsView;

pub(crate) struct PromptsSettingsContent {
    view: PromptsSettingsView,
}

impl PromptsSettingsContent {
    pub(crate) fn new(view: PromptsSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content!(PromptsSettingsContent);
