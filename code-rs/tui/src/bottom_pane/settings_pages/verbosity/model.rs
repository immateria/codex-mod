use ratatui::style::{Style, Stylize};

use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::colors;
use crate::ui_interaction::{wrap_next, wrap_prev};

use super::{TextVerbosity, VerbositySelectionView, VERBOSITY_OPTIONS};

impl VerbositySelectionView {
    pub(super) fn selected_verbosity(&self) -> TextVerbosity {
        VERBOSITY_OPTIONS
            .get(self.selected_idx)
            .map(|(verbosity, _, _)| *verbosity)
            .unwrap_or(self.current_verbosity)
    }

    pub(super) fn set_selected_index(&mut self, idx: usize) {
        let idx = idx.min(VERBOSITY_OPTIONS.len().saturating_sub(1));
        self.selected_idx = idx;
    }

    pub(super) fn move_selection_up(&mut self) {
        let idx = wrap_prev(self.selected_idx, VERBOSITY_OPTIONS.len());
        self.set_selected_index(idx);
    }

    pub(super) fn move_selection_down(&mut self) {
        let idx = wrap_next(self.selected_idx, VERBOSITY_OPTIONS.len());
        self.set_selected_index(idx);
    }

    pub(super) fn menu_rows(&self) -> Vec<SettingsMenuRow<'static, usize>> {
        VERBOSITY_OPTIONS
            .iter()
            .enumerate()
            .map(|(idx, (verbosity, name, description))| {
                let mut row = SettingsMenuRow::new(idx, *name).with_detail(StyledText::new(
                    *description,
                    Style::new().fg(colors::text_dim()),
                ));
                if *verbosity == self.current_verbosity {
                    row = row.with_value(StyledText::new(
                        "(current)",
                        Style::new().fg(colors::warning()).bold(),
                    ));
                }
                row
            })
            .collect()
    }
}

