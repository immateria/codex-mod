use super::ShellProfilesSettingsView;

impl_settings_pane!(
    ShellProfilesSettingsView, handle_key_event_direct,
    height = { 14 }, complete_field = is_complete,
    paste = handle_paste_direct, as_any = yes
);

