mod create_spinner;
mod create_theme;
mod options;
mod preview;
mod selection;

use super::*;

impl ThemeSelectionView {
    pub fn new(
        current_theme: ThemeName,
        app_event_tx: AppEventSender,
        tail_ticket: BackgroundOrderTicket,
        before_ticket: BackgroundOrderTicket,
    ) -> Self {
        let current_theme = map_theme_for_palette(current_theme, custom_theme_is_dark());
        let selected_theme_index = Self::theme_index_for(current_theme);

        // Initialize spinner selection from current runtime spinner
        let spinner_names = crate::spinner::spinner_names();
        let current_spinner_name = crate::spinner::current_spinner().name.clone();
        let selected_spinner_index = spinner_names
            .iter()
            .position(|n| *n == current_spinner_name)
            .unwrap_or(0);

        Self {
            original_theme: current_theme,
            current_theme,
            selected_theme_index,
            hovered_theme_index: None,
            _original_spinner: current_spinner_name.clone(),
            current_spinner: current_spinner_name.clone(),
            selected_spinner_index,
            mode: Mode::Overview,
            overview_selected_index: 0,
            revert_theme_on_back: current_theme,
            revert_spinner_on_back: current_spinner_name,
            just_entered_themes: false,
            just_entered_spinner: false,
            app_event_tx,
            tail_ticket,
            before_ticket,
            is_complete: false,
        }
    }
}

