use super::*;

mod editor;
mod file_popup;
mod history_nav;
mod key_router;
mod mouse;
mod paste;
mod slash_popup;

impl ChatComposer {
    fn resolve_builtin_slash_command(name: &str) -> Option<SlashCommand> {
        name.parse::<SlashCommand>().ok().filter(|cmd| cmd.is_available())
    }

    pub fn set_ctrl_c_quit_hint(&mut self, show: bool) {
        self.ctrl_c_quit_hint = show;
    }

    pub fn set_standard_terminal_hint(&mut self, hint: Option<String>) {
        self.standard_terminal_hint = hint;
    }

    pub fn standard_terminal_hint(&self) -> Option<&str> {
        self.standard_terminal_hint.as_deref()
    }

    /// Handle a key event coming from the main UI.
    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> (InputResult, bool) {
        key_router::handle_key_event_inner(self, key_event)
    }

    /// Handle a mouse event. Returns (InputResult, bool) matching handle_key_event.
    /// The `area` parameter is the full area where the composer is rendered.
    pub(crate) fn handle_mouse_event(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> (InputResult, bool) {
        mouse::handle_mouse_event_inner(self, mouse_event, area)
    }

    /// Refresh popup state after a text change that didn't flow through the
    /// main key-event path (e.g., history navigation or async fetches).
    fn resync_popups(&mut self) {
        self.sync_command_popup();
        if matches!(self.active_popup, ActivePopup::Command(_)) {
            self.dismissed_file_popup_token = None;
        } else {
            self.sync_file_search_popup();
        }
    }

    pub(crate) fn set_has_focus(&mut self, has_focus: bool) {
        self.has_focus = has_focus;
    }

    // -------------------------------------------------------------
    // History navigation helpers (used by ChatWidget at scroll boundaries)
    // -------------------------------------------------------------
    pub(crate) fn try_history_up(&mut self) -> bool {
        history_nav::try_history_up_inner(self)
    }

    pub(crate) fn try_history_down(&mut self) -> bool {
        history_nav::try_history_down_inner(self)
    }

    pub(crate) fn history_is_browsing(&self) -> bool {
        history_nav::history_is_browsing_inner(self)
    }

    pub(crate) fn mark_next_down_scrolls_history(&mut self) {
        history_nav::mark_next_down_scrolls_history_inner(self);
    }

}
