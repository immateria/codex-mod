use super::*;

use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::colors;
use crate::bottom_pane::settings_ui::line_runs::SelectableLineRun;
use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::bottom_pane::settings_ui::toggle;

impl ReviewSettingsView {
    pub(super) fn build_model(&self) -> ReviewListModel {
        let mut selection_kinds = Vec::new();
        let mut selection_line = Vec::new();
        let mut section_bounds = Vec::new();

        let mut current_line = 0usize;

        let review_section_start = current_line;
        current_line = current_line.saturating_add(1);

        selection_kinds.push(SelectionKind::ReviewEnabled);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        current_line = current_line.saturating_add(1);

        selection_kinds.push(SelectionKind::ReviewModel);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        current_line = current_line.saturating_add(1);

        selection_kinds.push(SelectionKind::ReviewResolveModel);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        current_line = current_line.saturating_add(1);

        selection_kinds.push(SelectionKind::ReviewAttempts);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        current_line = current_line.saturating_add(1);

        let review_section_end = current_line.saturating_sub(1);
        section_bounds.fill((review_section_start, review_section_end));

        let auto_review_section_start = current_line;
        current_line = current_line.saturating_add(1);

        let auto_review_selection_start = selection_kinds.len();
        selection_kinds.push(SelectionKind::AutoReviewEnabled);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        current_line = current_line.saturating_add(1);

        selection_kinds.push(SelectionKind::AutoReviewModel);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        current_line = current_line.saturating_add(1);

        selection_kinds.push(SelectionKind::AutoReviewResolveModel);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        current_line = current_line.saturating_add(1);

        selection_kinds.push(SelectionKind::AutoReviewAttempts);
        selection_line.push(current_line);
        section_bounds.push((0, 0));
        current_line = current_line.saturating_add(1);

        let auto_review_section_end = current_line.saturating_sub(1);
        section_bounds[auto_review_selection_start..]
            .fill((auto_review_section_start, auto_review_section_end));

        ReviewListModel {
            selection_kinds,
            selection_line,
            section_bounds,
            total_lines: current_line,
        }
    }

    pub(super) fn build_runs(&self, selected_idx: usize) -> Vec<SelectableLineRun<'static, usize>> {
        let mut runs = Vec::new();
        let mut selection_idx = 0usize;

        let review_label_pad_cols = u16::try_from(
            ["Enabled", "Review model", "Resolve model", "Max follow-up reviews"]
                .iter()
                .map(|label| label.width())
                .max()
                .unwrap_or(0),
        )
        .unwrap_or(u16::MAX);

        runs.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            " /review (manual) ",
            Style::new().fg(colors::primary()).bold().underlined(),
        )])]));

        let review_enabled_hint = if self.review_auto_resolve_enabled {
            "(press Enter to disable)"
        } else {
            "(press Enter to enable)"
        };
        runs.push(
            SettingsMenuRow::new(selection_idx, "Enabled")
                .with_label_pad_cols(review_label_pad_cols)
                .with_value(toggle::on_off_word(self.review_auto_resolve_enabled))
                .with_detail(StyledText::new(
                    "(auto-resolve /review)",
                    Style::new().fg(colors::text_dim()),
                ))
                .with_selected_hint(review_enabled_hint)
                .into_run(Some(selected_idx)),
        );
        selection_idx = selection_idx.saturating_add(1);

        let review_model_value = if self.review_use_chat_model {
            "Follow Chat Mode".to_string()
        } else {
            format!(
                "{} ({})",
                Self::format_model_label(&self.review_model),
                Self::reasoning_label(self.review_reasoning)
            )
        };
        runs.push(
            SettingsMenuRow::new(selection_idx, "Review model")
                .with_label_pad_cols(review_label_pad_cols)
                .with_value(StyledText::new(
                    review_model_value,
                    Style::new().fg(colors::function()),
                ))
                .with_selected_hint("Enter to change")
                .into_run(Some(selected_idx)),
        );
        selection_idx = selection_idx.saturating_add(1);

        let review_resolve_value = if self.review_resolve_use_chat_model {
            "Follow Chat Mode".to_string()
        } else {
            format!(
                "{} ({})",
                Self::format_model_label(&self.review_resolve_model),
                Self::reasoning_label(self.review_resolve_reasoning)
            )
        };
        runs.push(
            SettingsMenuRow::new(selection_idx, "Resolve model")
                .with_label_pad_cols(review_label_pad_cols)
                .with_value(StyledText::new(
                    review_resolve_value,
                    Style::new().fg(colors::function()),
                ))
                .with_selected_hint("Enter to change")
                .into_run(Some(selected_idx)),
        );
        selection_idx = selection_idx.saturating_add(1);

        let review_attempts_value = if self.review_followups == 0 {
            "0 (no re-reviews)".to_string()
        } else if self.review_followups == 1 {
            "1 re-review".to_string()
        } else {
            format!("{} re-reviews", self.review_followups)
        };
        runs.push(
            SettingsMenuRow::new(selection_idx, "Max follow-up reviews")
                .with_label_pad_cols(review_label_pad_cols)
                .with_value(StyledText::new(
                    review_attempts_value,
                    Style::new().fg(colors::function()),
                ))
                .with_selected_hint("(←/→ to adjust)")
                .into_run(Some(selected_idx)),
        );
        selection_idx = selection_idx.saturating_add(1);

        let auto_review_label_pad_cols = u16::try_from(
            ["Enabled", "Review model", "Resolve model", "Max follow-up reviews"]
                .iter()
                .map(|label| label.width())
                .max()
                .unwrap_or(0),
        )
        .unwrap_or(u16::MAX);

        runs.push(SelectableLineRun::plain(vec![Line::from(vec![Span::styled(
            " Auto Review (background) ",
            Style::new().fg(colors::primary()).bold().underlined(),
        )])]));

        let auto_review_enabled_hint = if self.auto_review_enabled {
            "(press Enter to disable)"
        } else {
            "(press Enter to enable)"
        };
        runs.push(
            SettingsMenuRow::new(selection_idx, "Enabled")
                .with_label_pad_cols(auto_review_label_pad_cols)
                .with_value(toggle::on_off_word(self.auto_review_enabled))
                .with_detail(StyledText::new(
                    "(background auto review)",
                    Style::new().fg(colors::text_dim()),
                ))
                .with_selected_hint(auto_review_enabled_hint)
                .into_run(Some(selected_idx)),
        );
        selection_idx = selection_idx.saturating_add(1);

        let auto_review_model_value = if self.auto_review_use_chat_model {
            "Follow Chat Mode".to_string()
        } else {
            format!(
                "{} ({})",
                Self::format_model_label(&self.auto_review_model),
                Self::reasoning_label(self.auto_review_reasoning)
            )
        };
        runs.push(
            SettingsMenuRow::new(selection_idx, "Review model")
                .with_label_pad_cols(auto_review_label_pad_cols)
                .with_value(StyledText::new(
                    auto_review_model_value,
                    Style::new().fg(colors::function()),
                ))
                .with_selected_hint("Enter to change")
                .into_run(Some(selected_idx)),
        );
        selection_idx = selection_idx.saturating_add(1);

        let auto_review_resolve_value = if self.auto_review_resolve_use_chat_model {
            "Follow Chat Mode".to_string()
        } else {
            format!(
                "{} ({})",
                Self::format_model_label(&self.auto_review_resolve_model),
                Self::reasoning_label(self.auto_review_resolve_reasoning)
            )
        };
        runs.push(
            SettingsMenuRow::new(selection_idx, "Resolve model")
                .with_label_pad_cols(auto_review_label_pad_cols)
                .with_value(StyledText::new(
                    auto_review_resolve_value,
                    Style::new().fg(colors::function()),
                ))
                .with_selected_hint("Enter to change")
                .into_run(Some(selected_idx)),
        );
        selection_idx = selection_idx.saturating_add(1);

        let auto_review_attempts_value = if self.auto_review_followups == 0 {
            "0 (no follow-ups)".to_string()
        } else if self.auto_review_followups == 1 {
            "1 follow-up".to_string()
        } else {
            format!("{} follow-ups", self.auto_review_followups)
        };
        runs.push(
            SettingsMenuRow::new(selection_idx, "Max follow-up reviews")
                .with_label_pad_cols(auto_review_label_pad_cols)
                .with_value(StyledText::new(
                    auto_review_attempts_value,
                    Style::new().fg(colors::function()),
                ))
                .with_selected_hint("(←/→ to adjust)")
                .into_run(Some(selected_idx)),
        );

        runs
    }

    fn reasoning_label(effort: ReasoningEffort) -> &'static str {
        match effort {
            ReasoningEffort::XHigh => "XHigh",
            ReasoningEffort::High => "High",
            ReasoningEffort::Medium => "Medium",
            ReasoningEffort::Low => "Low",
            ReasoningEffort::Minimal => "Minimal",
            ReasoningEffort::None => "None",
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
