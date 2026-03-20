pub(super) fn handle_settings_paste(chat: &mut ChatWidget<'_>, text: String) -> bool {
    if chat.settings.overlay.is_none() {
        return false;
    }

    let overlay = match chat.settings.overlay.as_mut() {
        Some(overlay) => overlay,
        None => return false,
    };

    if overlay.is_menu_active() || overlay.is_help_visible() {
        return false;
    }

    if overlay.is_sidebar_focused() {
        return false;
    }

    if let Some(content) = overlay.active_content_mut()
        && content.handle_paste(text) {
            chat.request_redraw();
            return true;
        }

    false
}
