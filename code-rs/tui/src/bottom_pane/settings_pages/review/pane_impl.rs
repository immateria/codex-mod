use super::ReviewSettingsView;

impl_settings_pane!(ReviewSettingsView, handle_key_event_impl,
    height = { 12 },
    complete_field = is_complete
);

