use crate::bottom_pane::settings_pages::network::NetworkSettingsView;

pub(crate) struct NetworkSettingsContent {
    view: NetworkSettingsView,
}

impl NetworkSettingsContent {
    pub(crate) fn new(view: NetworkSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content_with_paste!(NetworkSettingsContent);
