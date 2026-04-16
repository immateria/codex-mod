use super::ShellSelectionView;

impl_settings_pane!(
    ShellSelectionView, handle_key_event_direct,
    height_fn = desired_height_impl, complete_field = is_complete,
    paste = handle_paste_direct, hover = update_hover_delegate
);

