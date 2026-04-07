use super::NotificationsSettingsView;

impl_settings_pane!(NotificationsSettingsView, handle_key_event_direct,
    height = { 9 },
    complete_field = is_complete
);
