use super::PromptsSettingsView;

impl_settings_pane!(PromptsSettingsView, handle_key_event_direct,
    height = { Self::DEFAULT_HEIGHT },
    complete_fn = is_complete
);
