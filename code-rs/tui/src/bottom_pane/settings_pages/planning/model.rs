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
                Self::format_model_label(&self.planning_model),
                Self::reasoning_label(self.planning_reasoning)
            )
        };
        vec![SettingsMenuRow::new(PlanningRow::CustomModel, "Planning model")
            .with_value(StyledText::new(value_text, Style::new().fg(colors::function())))
            .with_selected_hint("Enter to change")]
    }

    pub(super) fn selected_row(&self) -> Option<PlanningRow> {
        self.rows().get(self.state.selected_idx.unwrap_or(0)).copied()
    }

    fn reasoning_label(effort: code_core::config_types::ReasoningEffort) -> &'static str {
        match effort {
            code_core::config_types::ReasoningEffort::XHigh => "XHigh",
            code_core::config_types::ReasoningEffort::High => "High",
            code_core::config_types::ReasoningEffort::Medium => "Medium",
            code_core::config_types::ReasoningEffort::Low => "Low",
            code_core::config_types::ReasoningEffort::Minimal => "Minimal",
            code_core::config_types::ReasoningEffort::None => "None",
        }
    }

    fn format_model_label(model: &str) -> String {
        let mut parts = Vec::new();
        for (idx, part) in model.split('-').enumerate() {
            if idx == 0 {
                parts.push(part.to_ascii_uppercase());
                continue;
            }
            let mut chars = part.chars();
            let formatted = match chars.next() {
                Some(first) if first.is_ascii_alphabetic() => {
                    let mut s = String::new();
                    s.push(first.to_ascii_uppercase());
                    s.push_str(chars.as_str());
                    s
                }
                Some(first) => {
                    let mut s = String::new();
                    s.push(first);
                    s.push_str(chars.as_str());
                    s
                }
                None => String::new(),
            };
            parts.push(formatted);
        }
        parts.join("-")
    }
}
