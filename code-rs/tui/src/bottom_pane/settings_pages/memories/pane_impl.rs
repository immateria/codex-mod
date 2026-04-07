use super::MemoriesSettingsView;

impl_settings_pane!(MemoriesSettingsView, handle_key_event_direct,
    height = { 19 },
    complete_fn = is_view_complete
);
