use super::*;

use ratatui::style::{Style, Stylize};
use ratatui::text::Line;

use crate::bottom_pane::settings_ui::line_runs::SelectableLineRun;
use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::bottom_pane::settings_ui::toggle;
use crate::colors;

impl ValidationSettingsView {
    pub(super) fn build_model(&self) -> ValidationListModel {
        let mut selection_kinds = Vec::new();
        let mut selection_line = Vec::new();
        let mut section_bounds = Vec::new();

        let mut current_line = 0usize;

        for (group_idx, (status, enabled)) in self.groups.iter().enumerate() {
            let section_start = current_line;
            let section_selection_start = selection_kinds.len();

            selection_kinds.push(SelectionKind::Group(group_idx));
            selection_line.push(current_line);
            section_bounds.push((0, 0));
            current_line = current_line.saturating_add(1);

            for (idx, row) in self.tools.iter().enumerate() {
                if group_for_category(row.status.category) != status.group {
                    continue;
                }
                if *enabled {
                    selection_kinds.push(SelectionKind::Tool(idx));
                    selection_line.push(current_line);
                    section_bounds.push((0, 0));
                }
                current_line = current_line.saturating_add(1);
            }

            let section_end = current_line.saturating_sub(1);
            for bounds in &mut section_bounds[section_selection_start..] {
                *bounds = (section_start, section_end);
            }

            if group_idx + 1 < self.groups.len() {
                current_line = current_line.saturating_add(1);
            }
        }

        ValidationListModel {
            selection_kinds,
            selection_line,
            section_bounds,
            total_lines: current_line,
        }
    }

    pub(super) fn build_runs(&self, selected_idx: usize) -> Vec<SelectableLineRun<'_, usize>> {
        let mut runs = Vec::new();
        let mut selection_idx = 0usize;

        for (group_idx, (status, enabled)) in self.groups.iter().enumerate() {
            let group_sel_idx = selection_idx;
            selection_idx = selection_idx.saturating_add(1);
            let group_description = match status.group {
                ValidationGroup::Functional => "Compile & structural checks",
                ValidationGroup::Stylistic => "Formatting and style linting",
            };
            let group_hint = if *enabled {
                "(press Enter to disable)"
            } else {
                "(press Enter to enable)"
            };
            runs.push(
                SettingsMenuRow::new(group_sel_idx, status.name)
                    .with_value(toggle::enabled_word(*enabled))
                    .with_detail(StyledText::new(
                        group_description,
                        Style::new().fg(colors::text_dim()),
                    ))
                    .with_selected_hint(group_hint)
                    .into_run(Some(selected_idx)),
            );

            for row in &self.tools {
                if group_for_category(row.status.category) != status.group {
                    continue;
                }

                let tool_sel_idx = selection_idx;

                if *enabled {
                    selection_idx = selection_idx.saturating_add(1);

                    let mut tool_row = SettingsMenuRow::new(tool_sel_idx, row.status.name)
                        .with_indent_cols(2)
                        .with_label_pad_cols(self.tool_label_pad_cols)
                        .with_detail(StyledText::new(
                            row.status.description,
                            Style::new().fg(colors::text_dim()),
                        ));

                    tool_row = if !row.status.installed {
                        tool_row.with_value(StyledText::new(
                            "missing",
                            Style::new().fg(colors::warning()).bold(),
                        ))
                    } else {
                        tool_row.with_value(toggle::enabled_word_warning_off(row.enabled))
                    };

                    let tool_hint = if !row.status.installed {
                        "(press Enter to install)"
                    } else {
                        "(press Enter to toggle)"
                    };
                    tool_row = tool_row.with_selected_hint(tool_hint);

                    runs.push(tool_row.into_run(Some(selected_idx)));
                } else {
                    runs.push(
                        SettingsMenuRow::new(tool_sel_idx, row.status.name)
                            .with_indent_cols(2)
                            .with_label_pad_cols(self.tool_label_pad_cols)
                            .with_detail(StyledText::new(
                                row.status.description,
                                Style::new().fg(colors::text_dim()),
                            ))
                            .disabled()
                            .into_run(Some(selected_idx)),
                    );
                }
            }

            if group_idx + 1 < self.groups.len() {
                runs.push(SelectableLineRun::plain(vec![Line::from("")]));
            }
        }

        runs
    }
}

