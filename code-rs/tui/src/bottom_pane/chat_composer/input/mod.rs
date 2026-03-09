use super::*;

mod editor;
mod file_popup;
mod paste;
mod slash_popup;

trait PopupMouseTarget {
    fn select_visible_index(&mut self, rel_y: usize) -> bool;
    fn move_up(&mut self);
    fn move_down(&mut self);
}

impl PopupMouseTarget for CommandPopup {
    fn select_visible_index(&mut self, rel_y: usize) -> bool {
        self.select_visible_index(rel_y)
    }

    fn move_up(&mut self) {
        self.move_up();
    }

    fn move_down(&mut self) {
        self.move_down();
    }
}

impl PopupMouseTarget for FileSearchPopup {
    fn select_visible_index(&mut self, rel_y: usize) -> bool {
        self.select_visible_index(rel_y)
    }

    fn move_up(&mut self) {
        self.move_up();
    }

    fn move_down(&mut self) {
        self.move_down();
    }
}

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
        let now = Instant::now();
        let burst_active = self.paste_burst.enter_should_insert_newline(now);
        let recent_plain_char = self.paste_burst.recent_plain_char(now);

        // Track rapid plain-character bursts (common when bracketed paste is
        // unavailable) so we can suppress Enter-based submissions and insert
        // literal newlines instead.
        if matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            match key_event.code {
                KeyCode::Char(_) => {
                    let unmodified = key_event.modifiers.is_empty()
                        || key_event.modifiers == KeyModifiers::SHIFT;
                    if unmodified {
                        self.paste_burst.record_plain_char_for_enter_window(now);
                    } else {
                        self.paste_burst.clear_enter_window();
                    }
                }
                KeyCode::Tab => {
                    // Tabs often appear in per-key pastes of code; treat as pastey input.
                    self.paste_burst.record_plain_char_for_enter_window(now);
                }
                KeyCode::Enter => {
                    // handled below
                }
                _ => self.paste_burst.clear_enter_window(),
            }
        } else if key_event.kind != KeyEventKind::Release {
            self.paste_burst.clear_enter_window();
        }

        let enter_should_newline = matches!(
            key_event,
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            }
        ) && (burst_active || recent_plain_char);

        if enter_should_newline {
            // Enter is a non-Down key, so clear the sticky scroll flag.
            self.next_down_scrolls_history = false;

            // Treat Enter as literal newline when a paste-like burst is active.
            self.insert_str("\n");
            self.history.reset_navigation();
            self.paste_burst.extend_enter_window(now);

            // Keep popups in sync just like the main path.
            self.resync_popups();

            return (InputResult::None, true);
        }

        // Treat Tab as literal input while we're inside a paste-like burst to
        // avoid launching file search or other Tab handlers mid-paste. This
        // keeps per-key pastes containing tabs (common in code blocks) intact.
        if matches!(
            key_event,
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            }
        ) && burst_active
        {
            self.insert_str("\t");
            self.history.reset_navigation();
            self.paste_burst.extend_enter_window(now);
            return (InputResult::None, true);
        }

        // Any non-Down key clears the sticky flag; handled before popup routing
        if !matches!(key_event.code, KeyCode::Down) {
            self.next_down_scrolls_history = false;
        }
        let result = match &mut self.active_popup {
            ActivePopup::Command(_) => self.handle_key_event_with_slash_popup(key_event),
            ActivePopup::File(_) => self.handle_key_event_with_file_popup(key_event),
            ActivePopup::None => self.handle_key_event_without_popup(key_event),
        };

        // Update (or hide/show) popup after processing the key.
        self.resync_popups();

        result
    }

    /// Handle a mouse event. Returns (InputResult, bool) matching handle_key_event.
    /// The `area` parameter is the full area where the composer is rendered.
    pub(crate) fn handle_mouse_event(&mut self, mouse_event: MouseEvent, area: Rect) -> (InputResult, bool) {
        let (mx, my) = (mouse_event.column, mouse_event.row);

        // Only handle left clicks and scroll
        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {}
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {}
            _ => return (InputResult::None, false),
        }

        // Calculate footer area (where popups live)
        let footer_height = self.footer_height();
        let footer_area = if footer_height > 0 {
            Some(Rect {
                x: area.x,
                y: area.y + area.height.saturating_sub(footer_height),
                width: area.width,
                height: footer_height,
            })
        } else {
            None
        };

        // Check if click/scroll is in footer area for popup handling
        let hit_footer = footer_area.filter(|fa| {
            mx >= fa.x && mx < fa.x + fa.width && my >= fa.y && my < fa.y + fa.height
        });

        // First, check if there's an active popup and handle events for it
        if let Some(footer_rect) = hit_footer {
            let rel_y = my.saturating_sub(footer_rect.y) as usize;
            match &mut self.active_popup {
                ActivePopup::Command(popup) => {
                    if Self::handle_popup_mouse(popup, mouse_event.kind, rel_y) {
                        return self.confirm_slash_popup_selection();
                    }
                    return (InputResult::None, false);
                }
                ActivePopup::File(popup) => {
                    if Self::handle_popup_mouse(popup, mouse_event.kind, rel_y) {
                        return self.confirm_file_popup_selection();
                    }
                    return (InputResult::None, false);
                }
                ActivePopup::None => {}
            }
        }

        // Not in popup area - check if click is on the textarea
        if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind
            && let Some(textarea_rect) = *self.last_textarea_rect.borrow() {
                let state = *self.textarea_state.borrow();
                if self.textarea.handle_mouse_click(mx, my, textarea_rect, state) {
                    return (InputResult::None, true);
                }
            }

        (InputResult::None, false)
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

    fn handle_popup_mouse(
        popup: &mut impl PopupMouseTarget,
        kind: MouseEventKind,
        rel_y: usize,
    ) -> bool {
        match kind {
            MouseEventKind::Down(MouseButton::Left) => popup.select_visible_index(rel_y),
            MouseEventKind::ScrollUp => {
                popup.move_up();
                false
            }
            MouseEventKind::ScrollDown => {
                popup.move_down();
                false
            }
            _ => false,
        }
    }

    fn apply_history_result(&mut self, text: Option<String>) -> bool {
        let Some(text) = text else {
            return false;
        };
        self.textarea.set_text(&text);
        self.textarea.set_cursor(0);
        self.resync_popups();
        true
    }

    // -------------------------------------------------------------
    // History navigation helpers (used by ChatWidget at scroll boundaries)
    // -------------------------------------------------------------
    pub(crate) fn try_history_up(&mut self) -> bool {
        if !self
            .history
            .should_handle_navigation(self.textarea.text(), self.textarea.cursor())
        {
            return false;
        }
        let text = self
            .history
            .navigate_up(self.textarea.text(), &self.app_event_tx);
        self.apply_history_result(text)
    }

    pub(crate) fn try_history_down(&mut self) -> bool {
        // Only meaningful when browsing or when original text is recorded
        if !self
            .history
            .should_handle_navigation(self.textarea.text(), self.textarea.cursor())
        {
            return false;
        }
        let text = self.history.navigate_down(&self.app_event_tx);
        self.apply_history_result(text)
    }

    pub(crate) fn history_is_browsing(&self) -> bool { self.history.is_browsing() }

    pub(crate) fn mark_next_down_scrolls_history(&mut self) {
        self.next_down_scrolls_history = true;
    }

}
