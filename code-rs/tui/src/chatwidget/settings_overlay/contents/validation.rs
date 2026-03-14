use crate::bottom_pane::settings_pages::validation::ValidationSettingsView;

pub(crate) struct ValidationSettingsContent {
    view: ValidationSettingsView,
}

impl ValidationSettingsContent {
    pub(crate) fn new(view: ValidationSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content_view_complete!(ValidationSettingsContent);
