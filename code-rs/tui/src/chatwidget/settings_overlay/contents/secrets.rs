use crate::bottom_pane::settings_pages::secrets::SecretsSettingsView;

pub(crate) struct SecretsSettingsContent {
    view: SecretsSettingsView,
}

impl SecretsSettingsContent {
    pub(crate) fn new(view: SecretsSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content!(SecretsSettingsContent);

