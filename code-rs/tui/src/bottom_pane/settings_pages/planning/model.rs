use ratatui::style::Style;

use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::colors;

use super::{PlanningRow, PlanningSettingsView};

impl PlanningSettingsView {
    pub(super) fn rows(&self) -> Vec<PlanningRow> {
        vec![PlanningRow::CustomModel]
    }

    pub(super) fn menu_rows(&self) -> Vec<SettingsMenuRow<'static, PlanningRow>> {
        let value_text = if self.use_chat_model {
            "Follow Chat Mode".to_string()
        } else {
            format!(
                "{} ({})",
                crate::text_formatting::format_model_label(&self.planning_model),
                crate::text_formatting::reasoning_effort_label(self.planning_reasoning)
            )
        };
        vec![SettingsMenuRow::new(PlanningRow::CustomModel, "Planning model")
            .with_value(StyledText::new(value_text, Style::new().fg(colors::function())))
            .with_selected_hint("Enter to change")]
    }

    pub(super) fn selected_row(&self) -> Option<PlanningRow> {
        self.rows().get(self.state.selected_idx.unwrap_or(0)).copied()
    }
}
