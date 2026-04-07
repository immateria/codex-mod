use super::JsReplSettingsView;

impl_settings_pane!(JsReplSettingsView, process_key_event,
    height_fn = desired_height_impl,
    complete_fn = is_complete,
    paste = handle_paste_direct
);
