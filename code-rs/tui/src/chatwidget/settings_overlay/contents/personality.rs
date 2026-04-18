use crate::bottom_pane::settings_pages::personality::PersonalitySettingsView;

pub(crate) struct PersonalitySettingsContent {
    view: PersonalitySettingsView,
}

impl PersonalitySettingsContent {
    pub(crate) fn new(view: PersonalitySettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content!(PersonalitySettingsContent);
