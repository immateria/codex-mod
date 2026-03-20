impl SettingsContent for LimitsSettingsContent {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            self.overlay.set_visible_rows(0);
            self.overlay.set_max_scroll(0);
            self.set_wide_active(false);
            return;
        }

        fill_rect(
            buf,
            area,
            Some(' '),
            Style::default().bg(crate::colors::background()),
        );

        let has_tabs = self.overlay.tab_count() > 1;
        let (hint_area, tabs_area, body_area) = Self::content_areas(area, has_tabs);

        self.render_hint_row(hint_area, buf, has_tabs);

        if let Some(tabs_rect) = tabs_area {
            self.render_tabs(tabs_rect, buf);
        }

        self.render_body(body_area, buf);
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        let scroll_target = self.active_scroll_target_for_keyboard();
        match key.code {
            KeyCode::Up => self.scroll_by(scroll_target, -1),
            KeyCode::Down => self.scroll_by(scroll_target, 1),
            KeyCode::PageUp => self.page_scroll(scroll_target, false),
            KeyCode::PageDown | KeyCode::Char(' ') => self.page_scroll(scroll_target, true),
            KeyCode::Home => self.jump_scroll(scroll_target, false),
            KeyCode::End => self.jump_scroll(scroll_target, true),
            KeyCode::Left | KeyCode::Char('[') => self.overlay.select_prev_tab(),
            KeyCode::Right | KeyCode::Char(']') => self.overlay.select_next_tab(),
            KeyCode::Char('v') | KeyCode::Char('V') => self.toggle_layout_mode(),
            KeyCode::Char('f') | KeyCode::Char('F') => self.cycle_focus_mode(),
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.overlay.select_prev_tab()
                } else {
                    self.overlay.select_next_tab()
                }
            }
            KeyCode::BackTab => self.overlay.select_prev_tab(),
            _ => false,
        }
    }

    fn is_complete(&self) -> bool {
        false
    }

    fn handle_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let has_tabs = self.overlay.tab_count() > 1;
        let (hint_area, tabs_area, body_area) = Self::content_areas(area, has_tabs);

        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(action) = self.hint_action_at(hint_area, has_tabs, mouse_event) {
                    return match action {
                        HintAction::ToggleLayout => self.toggle_layout_mode(),
                        HintAction::CycleFocus => self.cycle_focus_mode(),
                    };
                }

                if let Some(tabs_rect) = tabs_area
                    && let Some(tab_idx) = self.tab_at(tabs_rect, mouse_event)
                {
                    return self.overlay.select_tab(tab_idx);
                }

                if let Some(snapshot) = self.wide_snapshot_for_body(body_area)
                    && let Some(hit) = Self::pane_hit(&snapshot, mouse_event)
                {
                    self.update_wide_bounds(&snapshot.left_lines, &snapshot.right_lines, body_area.height);
                    return self.set_focus_from_pane_click(hit);
                }

                false
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let delta = if matches!(mouse_event.kind, MouseEventKind::ScrollUp) {
                    -1
                } else {
                    1
                };

                if let Some(snapshot) = self.wide_snapshot_for_body(body_area) {
                    self.update_wide_bounds(&snapshot.left_lines, &snapshot.right_lines, body_area.height);
                    let target = self.scroll_target_for_mouse(&snapshot, mouse_event);
                    return self.scroll_by(target, delta);
                }

                self.scroll_by(self.active_scroll_target_for_keyboard(), delta)
            }
            _ => false,
        }
    }
}
