use super::AppsSettingsView;

impl_settings_pane!(AppsSettingsView, handle_key_event_direct,
    height = { 16 },
    complete_fn = is_complete
);
