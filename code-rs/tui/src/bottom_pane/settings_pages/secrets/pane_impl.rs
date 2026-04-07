use super::SecretsSettingsView;

impl_settings_pane!(SecretsSettingsView, handle_key_event_direct,
    height = { 16 },
    complete_fn = is_complete
);
