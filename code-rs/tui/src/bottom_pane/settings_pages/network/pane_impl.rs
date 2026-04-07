use super::NetworkSettingsView;

impl_settings_pane!(NetworkSettingsView, handle_key_event_direct,
    height_fn = desired_height_impl,
    complete_field = is_complete,
    paste = handle_paste_direct
);
