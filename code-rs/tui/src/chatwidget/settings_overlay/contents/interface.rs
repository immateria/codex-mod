use crate::bottom_pane::settings_pages::interface::InterfaceSettingsView;

pub(crate) struct InterfaceSettingsContent {
    view: InterfaceSettingsView,
}

impl InterfaceSettingsContent {
    pub(crate) fn new(view: InterfaceSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content_with_paste!(InterfaceSettingsContent);
