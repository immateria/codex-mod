use super::*;

mod create_spinner;
mod create_theme;
mod mouse;
mod overview;
mod spinner;
mod themes;

impl ThemeSelectionView {
    pub(super) fn process_key_event(&mut self, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Up | KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if let Mode::CreateSpinner(ref mut s) = self.mode {
                    Self::create_spinner_nav_prev(s);
                } else if let Mode::CreateTheme(ref mut s) = self.mode {
                    Self::create_theme_nav_prev(s);
                } else {
                    match self.mode {
                        Mode::Overview => self.overview_nav_prev(),
                        _ => self.move_selection_up(),
                    }
                }
            }
            KeyEvent {
                code: KeyCode::Down | KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if let Mode::CreateSpinner(ref mut s) = self.mode {
                    Self::create_spinner_nav_next(s);
                } else if let Mode::CreateTheme(ref mut s) = self.mode {
                    Self::create_theme_nav_next(s);
                } else {
                    match &self.mode {
                        Mode::Overview => self.overview_nav_next(),
                        _ => self.move_selection_down(),
                    }
                }
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                let current_mode = std::mem::replace(&mut self.mode, Mode::Overview);
                match current_mode {
                    Mode::Overview => self.overview_handle_enter(),
                    Mode::Themes => self.themes_handle_enter(),
                    Mode::Spinner => self.spinner_handle_enter(),
                    Mode::CreateSpinner(s) => self.create_spinner_handle_enter(s),
                    Mode::CreateTheme(s) => self.create_theme_handle_enter(s),
                }
            }
            KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                ..
            } => match self.mode {
                Mode::Overview => self.is_complete = true,
                Mode::CreateSpinner(_) => {
                    self.mode = Mode::Spinner;
                }
                Mode::CreateTheme(_) => {
                    self.app_event_tx
                        .send(AppEvent::PreviewTheme(self.revert_theme_on_back));
                    self.hovered_theme_index = None;
                    self.mode = Mode::Themes;
                    self.send_theme_split_preview();
                }
                _ => self.cancel_detail(),
            },
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                ..
            } => {
                if let Mode::CreateSpinner(ref mut s) = self.mode {
                    Self::create_spinner_handle_char(s, c, modifiers);
                } else if let Mode::CreateTheme(ref mut s) = self.mode {
                    Self::create_theme_handle_char(s, c, modifiers);
                }
            }
            KeyEvent {
                code: KeyCode::Backspace,
                ..
            } => {
                if let Mode::CreateSpinner(ref mut s) = self.mode {
                    Self::create_spinner_handle_backspace(s);
                } else if let Mode::CreateTheme(ref mut s) = self.mode {
                    Self::create_theme_handle_backspace(s);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        let handled = matches!(
            key_event,
            KeyEvent { code: KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right | KeyCode::Enter | KeyCode::Esc, .. }
                | KeyEvent { code: KeyCode::Backspace, .. }
        ) || matches!(
            key_event,
            KeyEvent { code: KeyCode::Char(_), modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT, .. }
        );
        self.process_key_event(key_event);
        handled
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_in_chrome(
            mouse_event,
            area,
            mouse::MouseChrome::Framed,
        )
    }

    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_in_chrome(
            mouse_event,
            area,
            mouse::MouseChrome::ContentOnly,
        )
    }
}

impl Drop for ThemeSelectionView {
    fn drop(&mut self) {
        self.clear_theme_split_preview();
    }
}

