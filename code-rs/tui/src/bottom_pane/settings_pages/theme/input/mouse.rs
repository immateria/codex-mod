use super::*;

#[derive(Copy, Clone)]
pub(super) enum MouseChrome {
    Framed,
    ContentOnly,
}

impl ThemeSelectionView {
    pub(super) fn handle_mouse_event_direct_in_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
        chrome: MouseChrome,
    ) -> bool {
        if area.width == 0 || area.height == 0 {
            return false;
        }

        let Some(body_area) = Self::body_area_for_chrome(chrome, area) else {
            return false;
        };
        if body_area.width == 0 || body_area.height == 0 {
            return false;
        }

        let in_body = mouse_event.column >= body_area.x
            && mouse_event.column < body_area.x.saturating_add(body_area.width)
            && mouse_event.row >= body_area.y
            && mouse_event.row < body_area.y.saturating_add(body_area.height);

        match mouse_event.kind {
            MouseEventKind::Moved => {
                if !in_body {
                    if matches!(self.mode, Mode::Themes) {
                        return self.clear_hovered_theme_preview();
                    }
                    return false;
                }
                match self.mode {
                    Mode::Themes => self.handle_mouse_hover(mouse_event, body_area),
                    // Keep spinner list scrolling on wheel/click only.
                    Mode::Spinner => false,
                    Mode::Overview | Mode::CreateSpinner(_) | Mode::CreateTheme(_) => {
                        self.handle_mouse_hover(mouse_event, body_area)
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if !in_body {
                    return false;
                }
                self.handle_mouse_click(mouse_event, body_area)
            }
            MouseEventKind::ScrollUp => {
                if !in_body {
                    return false;
                }
                self.process_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
                true
            }
            MouseEventKind::ScrollDown => {
                if !in_body {
                    return false;
                }
                self.process_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                true
            }
            _ => false,
        }
    }

    fn body_area_for_chrome(chrome: MouseChrome, area: Rect) -> Option<Rect> {
        match chrome {
            MouseChrome::Framed => {
                let inner = Block::default().borders(Borders::ALL).inner(area);
                Some(inner.inner(Margin::new(1, 1)))
            }
            MouseChrome::ContentOnly => Some(Rect {
                x: area.x,
                y: area.y.saturating_add(1),
                width: area.width,
                height: area.height.saturating_sub(1),
            }),
        }
    }

    fn handle_mouse_hover(&mut self, mouse_event: MouseEvent, body_area: Rect) -> bool {
        let rel_y = mouse_event.row.saturating_sub(body_area.y) as usize;
        match self.mode {
            Mode::Overview => self.overview_handle_mouse_hover(rel_y),
            Mode::Themes => self.themes_handle_mouse_hover(mouse_event, body_area),
            Mode::Spinner => self.spinner_handle_mouse_hover(rel_y, body_area),
            Mode::CreateSpinner(_) | Mode::CreateTheme(_) => false,
        }
    }

    fn handle_mouse_click(&mut self, mouse_event: MouseEvent, body_area: Rect) -> bool {
        match self.mode {
            Mode::Overview | Mode::Spinner => {
                let changed = self.handle_mouse_hover(mouse_event, body_area);
                self.process_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                let _ = changed;
                true
            }
            Mode::Themes => self.themes_handle_mouse_click(mouse_event, body_area),
            Mode::CreateSpinner(_) | Mode::CreateTheme(_) => {
                self.handle_mouse_hover(mouse_event, body_area)
            }
        }
    }
}

