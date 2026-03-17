use ratatui::style::{Style, Stylize};

use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::colors;

use super::{TextVerbosity, VerbositySelectionView, VERBOSITY_OPTIONS};

impl VerbositySelectionView {
    pub(super) fn selected_idx(&self) -> usize {
        // `VERBOSITY_OPTIONS` should always be non-empty, but keep this safe.
        let len = VERBOSITY_OPTIONS.len().max(1);
        self.state.selected_idx.unwrap_or(0).min(len.saturating_sub(1))
    }

    pub(super) fn selected_verbosity(&self) -> TextVerbosity {
        VERBOSITY_OPTIONS
            .get(self.selected_idx())
            .map(|(verbosity, _, _)| *verbosity)
            .unwrap_or(self.current_verbosity)
    }

    pub(super) fn move_selection_up(&mut self) {
        self.state.move_up_wrap(VERBOSITY_OPTIONS.len());
        self.state.scroll_top = 0;
    }

    pub(super) fn move_selection_down(&mut self) {
        self.state.move_down_wrap(VERBOSITY_OPTIONS.len());
        self.state.scroll_top = 0;
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
