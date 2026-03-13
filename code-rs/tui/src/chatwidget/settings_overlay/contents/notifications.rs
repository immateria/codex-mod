use crate::bottom_pane::NotificationsSettingsView;

pub(crate) struct NotificationsSettingsContent {
    view: NotificationsSettingsView,
}

impl NotificationsSettingsContent {
    pub(crate) fn new(view: NotificationsSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content!(NotificationsSettingsContent);
