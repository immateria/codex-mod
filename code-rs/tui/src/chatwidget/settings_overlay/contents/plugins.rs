use crate::bottom_pane::settings_pages::plugins::PluginsSettingsView;

pub(crate) struct PluginsSettingsContent {
    view: PluginsSettingsView,
}

impl PluginsSettingsContent {
    pub(crate) fn new(view: PluginsSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content!(PluginsSettingsContent);

