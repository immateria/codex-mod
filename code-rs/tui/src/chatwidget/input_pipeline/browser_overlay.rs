use super::*;

impl ChatWidget<'_> {
    pub(in super::super) fn toggle_browser_overlay(&mut self) {
        let new_state = !self.browser_overlay_visible;
        self.browser_overlay_visible = new_state;
        if new_state {
            if self.agents_terminal.active {
                self.exit_agents_terminal_mode();
            }
            self.browser_overlay_state.reset();
            let session_key = self
                .tools_state
                .browser_last_key
                .clone()
                .or_else(|| self.tools_state.browser_sessions.keys().next().cloned());
            self.browser_overlay_state.set_session_key(session_key.clone());
            if let Some(key) = session_key
                && let Some(tracker) = self.tools_state.browser_sessions.get(&key) {
                    let history_len = tracker.cell.screenshot_history().len();
                    if history_len > 0 {
                        self
                            .browser_overlay_state
                            .set_screenshot_index(history_len.saturating_sub(1));
                    }
                }
        } else {
            self.browser_overlay_state.reset();
        }
        self.request_redraw();
    }

    pub(in super::super) fn handle_browser_overlay_key(&mut self, key_event: KeyEvent) -> bool {
        if !self.browser_overlay_visible {
            return false;
        }
        if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return true;
        }

        let shift = key_event.modifiers.contains(KeyModifiers::SHIFT);
        let ctrl = key_event.modifiers.contains(KeyModifiers::CONTROL);

        match key_event.code {
            KeyCode::Esc => {
                self.browser_overlay_visible = false;
                self.browser_overlay_state.reset();
                self.request_redraw();
            }
            KeyCode::Up if shift => {
                self.adjust_browser_overlay_action_scroll(-1);
                self.request_redraw();
            }
            KeyCode::Down if shift => {
                self.adjust_browser_overlay_action_scroll(1);
                self.request_redraw();
            }
            KeyCode::Up => {
                if self.move_browser_overlay_screenshot(-1) {
                    self.request_redraw();
                }
            }
            KeyCode::Down => {
                if self.move_browser_overlay_screenshot(1) {
                    self.request_redraw();
                }
            }
            KeyCode::Left => {
                if self.move_browser_overlay_screenshot(-1) {
                    self.request_redraw();
                }
            }
            KeyCode::Right => {
                if self.move_browser_overlay_screenshot(1) {
                    self.request_redraw();
                }
            }
            KeyCode::PageUp => {
                let step = self.browser_overlay_state.last_action_view_height().max(1) as i16;
                self.adjust_browser_overlay_action_scroll(-step);
                self.request_redraw();
            }
            KeyCode::PageDown => {
                let step = self.browser_overlay_state.last_action_view_height().max(1) as i16;
                self.adjust_browser_overlay_action_scroll(step);
                self.request_redraw();
            }
            KeyCode::Home => {
                if self.set_browser_overlay_screenshot_index(0) {
                    self.request_redraw();
                }
            }
            KeyCode::End => {
                if let Some((_, tracker)) = self.browser_overlay_tracker() {
                    let len = tracker.cell.screenshot_history().len();
                    if len > 0 && self.set_browser_overlay_screenshot_index(len - 1) {
                        self.request_redraw();
                    }
                }
            }
            KeyCode::Char('j') if key_event.modifiers.is_empty() => {
                self.adjust_browser_overlay_action_scroll(1);
                self.request_redraw();
            }
            KeyCode::Char('k') if key_event.modifiers.is_empty() => {
                self.adjust_browser_overlay_action_scroll(-1);
                self.request_redraw();
            }
            KeyCode::Char('g') if ctrl => {
                if self.set_browser_overlay_screenshot_index(0) {
                    self.request_redraw();
                }
            }
            KeyCode::Char('G') if key_event.modifiers.is_empty() => {
                if let Some((_, tracker)) = self.browser_overlay_tracker() {
                    let len = tracker.cell.screenshot_history().len();
                    if len > 0 && self.set_browser_overlay_screenshot_index(len - 1) {
                        self.request_redraw();
                    }
                }
            }
            _ => {}
        }

        true
    }

    pub(in super::super) fn browser_overlay_session_key(&self) -> Option<String> {
        if let Some(key) = self.browser_overlay_state.session_key()
            && self.tools_state.browser_sessions.contains_key(&key) {
                return Some(key);
            }
        if let Some(last) = self.tools_state.browser_last_key.clone()
            && self.tools_state.browser_sessions.contains_key(&last) {
                self.browser_overlay_state
                    .set_session_key(Some(last.clone()));
                return Some(last);
            }
        if let Some((key, _)) = self.tools_state.browser_sessions.iter().next() {
            let owned = key.clone();
            self.browser_overlay_state
                .set_session_key(Some(owned.clone()));
            return Some(owned);
        }
        None
    }

    pub(in super::super) fn browser_overlay_tracker(
        &self,
    ) -> Option<(String, &browser_sessions::BrowserSessionTracker)> {
        let key = self.browser_overlay_session_key()?;
        self.tools_state
            .browser_sessions
            .get(&key)
            .map(|tracker| (key, tracker))
    }

    pub(in super::super) fn set_browser_overlay_screenshot_index(&self, index: usize) -> bool {
        let Some((_, tracker)) = self.browser_overlay_tracker() else {
            return false;
        };
        let history = tracker.cell.screenshot_history();
        if history.is_empty() {
            return false;
        }
        let clamped = index.min(history.len().saturating_sub(1));
        if self.browser_overlay_state.screenshot_index() != clamped {
            self.browser_overlay_state.set_screenshot_index(clamped);
            return true;
        }
        false
    }

    pub(in super::super) fn move_browser_overlay_screenshot(&self, delta: isize) -> bool {
        let Some((_, tracker)) = self.browser_overlay_tracker() else {
            return false;
        };
        let history = tracker.cell.screenshot_history();
        if history.is_empty() {
            return false;
        }
        let last_index = history.len() as isize - 1;
        let mut current = self.browser_overlay_state.screenshot_index() as isize;
        if current > last_index {
            current = last_index;
        }
        let mut new_index = current + delta;
        if new_index < 0 {
            new_index = 0;
        }
        if new_index > last_index {
            new_index = last_index;
        }
        if new_index != current {
            self.browser_overlay_state
                .set_screenshot_index(new_index as usize);
            return true;
        }
        false
    }

    pub(in super::super) fn adjust_browser_overlay_action_scroll(&self, delta: i16) {
        let current = self.browser_overlay_state.action_scroll() as i32;
        let max = self.browser_overlay_state.max_action_scroll() as i32;
        let mut updated = current + delta as i32;
        if updated < 0 {
            updated = 0;
        } else if updated > max {
            updated = max;
        }
        self.browser_overlay_state
            .set_action_scroll(updated as u16);
    }
}
