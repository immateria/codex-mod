use crate::bottom_pane::settings_pages::notifications::NotificationsSettingsView;

pub(crate) struct NotificationsSettingsContent {
    view: NotificationsSettingsView,
}

impl NotificationsSettingsContent {
    pub(crate) fn new(view: NotificationsSettingsView) -> Self {
        Self { view }
    }
}

impl_settings_content!(NotificationsSettingsContent);
