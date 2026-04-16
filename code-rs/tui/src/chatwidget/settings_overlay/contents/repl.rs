use crate::bottom_pane::settings_pages::repl::ReplSettingsView;

pub(crate) struct ReplSettingsContent {
    view: ReplSettingsView,
}

impl ReplSettingsContent {
    pub(crate) fn new(view: ReplSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content_with_paste!(ReplSettingsContent);
