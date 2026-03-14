use crate::bottom_pane::settings_pages::memories::MemoriesSettingsView;

pub(crate) struct MemoriesSettingsContent {
    view: MemoriesSettingsView,
}

impl MemoriesSettingsContent {
    pub(crate) fn new(view: MemoriesSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content_view_complete_key_always_true!(MemoriesSettingsContent);
