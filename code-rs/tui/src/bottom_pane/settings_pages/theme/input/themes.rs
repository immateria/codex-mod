use super::*;

impl ThemeSelectionView {
    pub(super) fn themes_handle_enter(&mut self) {
        let count = Self::theme_option_count();
        if Self::allow_custom_theme_generation() && self.selected_theme_index >= count {
            self.app_event_tx
                .send(AppEvent::PreviewTheme(self.revert_theme_on_back));
            self.clear_theme_split_preview();
            self.mode = Mode::CreateTheme(Box::new(CreateThemeState {
                step: std::cell::Cell::new(CreateStep::Prompt),
                prompt: String::new(),
                is_loading: std::cell::Cell::new(false),
                action_idx: 0,
                rx: None,
                thinking_lines: std::cell::RefCell::new(Vec::new()),
                thinking_current: std::cell::RefCell::new(String::new()),
                proposed_name: std::cell::RefCell::new(None),
                proposed_colors: std::cell::RefCell::new(None),
                preview_on: std::cell::Cell::new(true),
                review_focus_is_toggle: std::cell::Cell::new(true),
                last_raw_output: std::cell::RefCell::new(None),
                proposed_is_dark: std::cell::Cell::new(None),
            }));
        } else {
            self.confirm_theme()
        }
    }

    pub(super) fn themes_handle_mouse_hover(
        &mut self,
        mouse_event: MouseEvent,
        body_area: Rect,
    ) -> bool {
        let list_area = body_area;
        let option_count = Self::theme_option_count();
        let Some(next) =
            self.theme_index_at_mouse_position(mouse_event, list_area, option_count)
        else {
            return self.clear_hovered_theme_preview();
        };

        if next >= option_count {
            return self.clear_hovered_theme_preview();
        }

        if self.hovered_theme_index == Some(next) {
            return false;
        }

        self.hovered_theme_index = Some(next);
        self.send_theme_split_preview();
        true
    }

    pub(super) fn themes_handle_mouse_click(
        &mut self,
        mouse_event: MouseEvent,
        body_area: Rect,
    ) -> bool {
        let list_area = body_area;
        let option_count = Self::theme_option_count();
        let Some(next) =
            self.theme_index_at_mouse_position(mouse_event, list_area, option_count)
        else {
            return false;
        };

        self.selected_theme_index = next;
        if let Some(theme_name) = Self::theme_name_for_option_index(next) {
            self.current_theme = theme_name;
            self.hovered_theme_index = Some(next);
            self.send_theme_split_preview();
        } else {
            let _ = self.clear_hovered_theme_preview();
        }

        self.process_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        true
    }
}

