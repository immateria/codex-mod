use crate::bottom_pane::UpdateSettingsView;

pub(crate) struct UpdatesSettingsContent {
    view: UpdateSettingsView,
}

impl UpdatesSettingsContent {
    pub(crate) fn new(view: UpdateSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content_view_complete!(UpdatesSettingsContent);
