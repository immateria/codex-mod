use super::ExperimentalFeaturesSettingsView;

impl_settings_pane!(ExperimentalFeaturesSettingsView, handle_key_event_direct,
    height = { 16 },
    complete_fn = is_complete
);
