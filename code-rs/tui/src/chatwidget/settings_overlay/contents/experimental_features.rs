use crate::bottom_pane::settings_pages::experimental_features::ExperimentalFeaturesSettingsView;

pub(crate) struct ExperimentalFeaturesSettingsContent {
    view: ExperimentalFeaturesSettingsView,
}

impl ExperimentalFeaturesSettingsContent {
    pub(crate) fn new(view: ExperimentalFeaturesSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content!(ExperimentalFeaturesSettingsContent);

