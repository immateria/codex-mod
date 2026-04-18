use super::PersonalitySettingsView;

impl_settings_pane!(PersonalitySettingsView, handle_key_event_direct,
    height = { 10 },
    complete_field = is_complete
);
