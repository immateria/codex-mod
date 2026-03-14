use crate::bottom_pane::settings_pages::updates::UpdateSettingsView;

pub(crate) struct UpdatesSettingsContent {
    view: UpdateSettingsView,
}

impl UpdatesSettingsContent {
    pub(crate) fn new(view: UpdateSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content_view_complete!(UpdatesSettingsContent);
