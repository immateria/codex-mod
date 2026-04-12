use super::PluginsSettingsView;

impl_settings_pane!(PluginsSettingsView, handle_key_event_direct,
    height = { 16 },
    complete_fn = is_complete,
    paste = handle_paste_direct
);
