use crate::bottom_pane::settings_pages::apps::AppsSettingsView;

pub(crate) struct AppsSettingsContent {
    view: AppsSettingsView,
}

impl AppsSettingsContent {
    pub(crate) fn new(view: AppsSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content!(AppsSettingsContent);

