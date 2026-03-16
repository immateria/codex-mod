use ratatui::style::Style;

use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow as SharedSettingsMenuRow;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::colors;

use super::SettingsOverviewView;

impl SettingsOverviewView {
    pub(super) fn selected_index(&self) -> usize {
        self.scroll.selected_idx.unwrap_or(0)
    }

    pub(super) fn selected_section(&self) -> Option<crate::bottom_pane::SettingsSection> {
        self.rows
            .get(self.selected_index())
            .map(|(section, _)| *section)
    }

    pub(super) fn menu_rows(&self) -> Vec<SharedSettingsMenuRow<'_, usize>> {
        self.rows
            .iter()
            .enumerate()
            .map(|(idx, (section, summary))| {
                let mut item = SharedSettingsMenuRow::new(idx, section.label());
                if let Some(summary) = summary.as_deref() {
                    item = item.with_detail(StyledText::new(
                        summary,
                        Style::new().fg(colors::text_dim()),
                    ));
                }
                item
            })
            .collect()
    }
}
