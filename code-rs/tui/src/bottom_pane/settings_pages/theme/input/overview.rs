use super::*;

impl ThemeSelectionView {
    pub(super) fn overview_nav_prev(&mut self) {
        self.overview_selected_index = self.overview_selected_index.saturating_sub(1) % 3;
    }

    pub(super) fn overview_nav_next(&mut self) {
        self.overview_selected_index = (self.overview_selected_index + 1) % 3;
    }

    pub(super) fn overview_handle_enter(&mut self) {
        match self.overview_selected_index {
            0 => {
                self.revert_theme_on_back = self.current_theme;
                self.hovered_theme_index = None;
                self.mode = Mode::Themes;
                self.just_entered_themes = true;
                self.send_theme_split_preview();
            }
            1 => {
                self.revert_spinner_on_back = self.current_spinner.clone();
                self.mode = Mode::Spinner;
                self.app_event_tx.send(AppEvent::ScheduleFrameIn(
                    std::time::Duration::from_millis(120),
                ));
                self.just_entered_spinner = true;
            }
            _ => {
                self.is_complete = true;
                self.mode = Mode::Overview;
            }
        }
    }

    pub(super) fn overview_handle_mouse_hover(&mut self, rel_y: usize) -> bool {
        let Some(next) = (match rel_y {
            0 => Some(0),
            1 => Some(1),
            3 => Some(2),
            _ => None,
        }) else {
            return false;
        };
        if self.overview_selected_index == next {
            return false;
        }
        self.overview_selected_index = next;
        true
    }
}

