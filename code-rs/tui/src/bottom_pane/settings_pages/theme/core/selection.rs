use super::super::*;

impl ThemeSelectionView {
    pub(in crate::bottom_pane::settings_pages::theme) fn move_selection_up(&mut self) {
        if matches!(self.mode, Mode::Themes) {
            self.hovered_theme_index = None;
            if self.selected_theme_index > 0 {
                self.selected_theme_index -= 1;
                if let Some(theme) = Self::theme_name_for_option_index(self.selected_theme_index) {
                    self.current_theme = theme;
                }
            }
            self.send_theme_split_preview();
        } else {
            let names = crate::spinner::spinner_names();
            if self.selected_spinner_index > 0 {
                self.selected_spinner_index -= 1;
                if let Some(name) = names.get(self.selected_spinner_index) {
                    self.current_spinner = name.clone();
                    self.app_event_tx
                        .send(AppEvent::PreviewSpinner(self.current_spinner.clone()));
                }
            }
        }
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn move_selection_down(&mut self) {
        if matches!(self.mode, Mode::Themes) {
            self.hovered_theme_index = None;
            let options_len = Self::theme_option_count();
            let allow_extra_row = Self::allow_custom_theme_generation();
            let limit = if allow_extra_row {
                options_len
            } else {
                options_len.saturating_sub(1)
            };
            if self.selected_theme_index < limit {
                self.selected_theme_index += 1;
                if self.selected_theme_index < options_len
                    && let Some(theme) = Self::theme_name_for_option_index(self.selected_theme_index)
                {
                    self.current_theme = theme;
                }
            }
            self.send_theme_split_preview();
        } else {
            let names = crate::spinner::spinner_names();
            // Allow moving onto the extra pseudo-row (Generate your own…)
            if self.selected_spinner_index < names.len() {
                self.selected_spinner_index += 1;
                if self.selected_spinner_index < names.len() {
                    if let Some(name) = names.get(self.selected_spinner_index) {
                        self.current_spinner = name.clone();
                        self.app_event_tx
                            .send(AppEvent::PreviewSpinner(self.current_spinner.clone()));
                    }
                } else {
                    // On the pseudo-row: do not change current spinner preview
                }
            }
        }
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn confirm_theme(&mut self) {
        self.hovered_theme_index = None;
        self.app_event_tx
            .send(AppEvent::UpdateTheme(self.current_theme));
        self.clear_theme_split_preview();
        self.revert_theme_on_back = self.current_theme;
        self.mode = Mode::Overview;
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn confirm_spinner(&mut self) {
        self.app_event_tx
            .send(AppEvent::UpdateSpinner(self.current_spinner.clone()));
        self.revert_spinner_on_back = self.current_spinner.clone();
        self.mode = Mode::Overview;
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn cancel_detail(&mut self) {
        match self.mode {
            Mode::Themes => {
                self.hovered_theme_index = None;
                if self.current_theme != self.revert_theme_on_back {
                    self.current_theme = self.revert_theme_on_back;
                    self.app_event_tx
                        .send(AppEvent::PreviewTheme(self.current_theme));
                }
                self.clear_theme_split_preview();
            }
            Mode::Spinner => {
                if self.current_spinner != self.revert_spinner_on_back {
                    self.current_spinner = self.revert_spinner_on_back.clone();
                    self.app_event_tx
                        .send(AppEvent::PreviewSpinner(self.current_spinner.clone()));
                }
            }
            Mode::Overview => {}
            Mode::CreateSpinner(_) => {}
            Mode::CreateTheme(_) => {}
        }
        self.mode = Mode::Overview;
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn send_tail(&self, message: impl Into<String>) {
        self.app_event_tx
            .send_background_event_with_ticket(&self.tail_ticket, message);
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn send_before_next_output(&self, message: impl Into<String>) {
        self.app_event_tx.send_background_before_next_output_with_ticket(
            &self.before_ticket,
            message,
        );
    }
}
