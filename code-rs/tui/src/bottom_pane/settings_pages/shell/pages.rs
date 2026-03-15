use super::*;

use crate::bottom_pane::settings_ui::action_page::SettingsActionPage;
use crate::bottom_pane::settings_ui::buttons::{standard_button_specs, StandardButtonSpec};
use crate::bottom_pane::settings_ui::hints::{self, KeyHint};
use crate::bottom_pane::settings_ui::line_runs::SelectableLineRun;
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::colors;
use code_core::split_command_and_args;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

impl ShellSelectionView {
    pub(super) fn list_page(&self) -> SettingsMenuPage<'_> {
        let mut current_label = match self.current_shell.as_ref() {
            Some(current) => Self::display_shell(current),
            None => "auto-detected".to_string(),
        };
        if let Some(current) = self.current_shell.as_ref() {
            let style = current
                .script_style
                .or_else(|| ShellScriptStyle::infer_from_shell_program(&current.path))
                .map(|style| style.to_string())
                .unwrap_or_else(|| "auto".to_string());
            current_label.push_str(&format!(" (style: {style})"));
        }

        let header_lines = vec![
            Line::from(vec![
                Span::styled("Current: ", Style::new().fg(colors::text_dim())),
                Span::styled(
                    current_label,
                    Style::new().fg(colors::text_bright()).bold(),
                ),
            ]),
            Line::from(""),
        ];

        let footer_lines = vec![hints::shortcut_line(&[
            KeyHint::new("↑↓", " select"),
            KeyHint::new("Enter", " apply"),
            KeyHint::new("e/→", " edit"),
            KeyHint::new("p", " pin"),
            KeyHint::new("Ctrl+P", " profiles"),
            KeyHint::new("Esc", " close"),
        ])];

        SettingsMenuPage::new(
            "Select Shell",
            SettingsPanelStyle::bottom_pane(),
            header_lines,
            footer_lines,
        )
    }

    pub(super) fn list_runs(&self) -> Vec<SelectableLineRun<'_, usize>> {
        let selected_id = Some(self.selected_index);
        let mut runs = Vec::new();

        let mut auto_row = SettingsMenuRow::new(0usize, "Auto-detect shell")
            .with_detail(StyledText::new(
                "use system default",
                Style::new().fg(colors::text_dim()),
            ));
        auto_row.selected_hint = Some("clears override".into());
        let mut run = auto_row.into_run(selected_id);
        if selected_id == Some(0) {
            run.lines.push(Line::from(Span::styled(
                "    Clears the override and follows your default shell.",
                Style::new().fg(colors::text_dim()),
            )));
        }
        runs.push(run);

        for (idx, shell) in self.shells.iter().enumerate() {
            let item_idx = idx.saturating_add(1);
            let status = if shell.available { "" } else { " (not found)" };
            let display_name = shell.preset.display_name.as_str();
            let label = format!("{display_name}{status}");
            let style_label = shell
                .preset
                .script_style
                .as_deref()
                .and_then(ShellScriptStyle::parse)
                .or_else(|| ShellScriptStyle::infer_from_shell_program(&shell.preset.command))
                .map(|style| style.to_string())
                .unwrap_or_else(|| "auto".to_string());

            let mut row = SettingsMenuRow::new(item_idx, label)
                .with_value(StyledText::new(
                    format!("[{style_label}]"),
                    Style::new().fg(colors::text_dim()),
                ))
                .with_detail(StyledText::new(
                    shell.preset.command.as_str(),
                    Style::new().fg(colors::text_dim()),
                ));
            if selected_id == Some(item_idx) {
                row = row.with_selected_hint("Enter to apply, e/→ to edit");
            }
            let mut run = row.into_run(selected_id);

            if selected_id == Some(item_idx) {
                let desc = shell.preset.description.trim();
                if !desc.is_empty() {
                    run.lines.push(Line::from(Span::styled(
                        format!("    {desc}"),
                        Style::new().fg(colors::text_dim()),
                    )));
                }

                let resolved = shell
                    .resolved_path
                    .as_deref()
                    .unwrap_or("not found in PATH");
                run.lines.push(Line::from(Span::styled(
                    format!("    Binary: {resolved}"),
                    Style::new().fg(colors::text_dim()),
                )));

                if !shell.available {
                    run.lines.push(Line::from(Span::styled(
                        "    Not found. Press Enter to edit an explicit command.",
                        Style::new().fg(colors::text_dim()),
                    )));
                }
            }

            runs.push(run);
        }

        runs.push(SelectableLineRun::plain(vec![Line::from("")]));

        let custom_idx = self.shells.len().saturating_add(1);
        let mut custom_run =
            SettingsMenuRow::new(custom_idx, "Custom / pinned path...").into_run(selected_id);
        if selected_id == Some(custom_idx) {
            custom_run.lines.push(Line::from(Span::styled(
                "    Edit a custom command (or pin a resolved binary path).",
                Style::new().fg(colors::text_dim()),
            )));
        }
        runs.push(custom_run);

        runs
    }

    pub(super) fn edit_buttons(&self) -> Vec<StandardButtonSpec<EditAction>> {
        let focused = match self.edit_focus {
            EditFocus::Field => None,
            EditFocus::Actions => Some(self.selected_action),
        };
        standard_button_specs(&EDIT_ACTION_ITEMS, focused, self.hovered_action)
    }

    pub(super) fn edit_page(&self) -> SettingsActionPage<'_> {
        let status_lines = vec![self.edit_status_line()];
        let footer_lines = vec![hints::shortcut_line(&[
            KeyHint::new("Tab", " focus"),
            KeyHint::new("Enter", " apply"),
            KeyHint::new("Ctrl+O", " pick"),
            KeyHint::new("Ctrl+V", " show"),
            KeyHint::new("Ctrl+R", " resolve"),
            KeyHint::new("Ctrl+T", " style"),
            KeyHint::new("Ctrl+P", " profiles"),
            KeyHint::new("Esc", " back"),
        ])];

        SettingsActionPage::new(
            "Edit Shell Command",
            SettingsPanelStyle::bottom_pane(),
            Vec::new(),
            footer_lines,
        )
        .with_status_lines(status_lines)
        .with_min_body_rows(6)
        .with_wrap_lines(true)
    }

    fn edit_status_line(&self) -> Line<'static> {
        let (status, status_style) = {
            let (path, _args) = split_command_and_args(self.custom_field.text());
            let trimmed = path.trim();
            if trimmed.is_empty() {
                (
                    "Enter a shell path or command".to_string(),
                    Style::new().fg(colors::text_dim()),
                )
            } else if trimmed.contains('/') || trimmed.contains('\\') {
                if std::path::Path::new(trimmed).exists() {
                    (
                        format!("OK ({trimmed})"),
                        Style::new().fg(colors::success()),
                    )
                } else {
                    (
                        format!("Not found ({trimmed})"),
                        Style::new().fg(colors::warning()),
                    )
                }
            } else {
                match which::which(trimmed) {
                    Ok(resolved) => {
                        let resolved = resolved.to_string_lossy();
                        (
                            format!("OK ({resolved})"),
                            Style::new().fg(colors::success()),
                        )
                    }
                    Err(_) => (
                        format!("Not found in PATH ({trimmed})"),
                        Style::new().fg(colors::warning()),
                    ),
                }
            }
        };

        let mut spans = vec![Span::styled(status, status_style)];
        if let Some(notice) = self.native_picker_notice.as_deref() {
            let notice = notice.trim();
            if !notice.is_empty() {
                spans.push(Span::styled(
                    format!("  •  {notice}"),
                    Style::new().fg(colors::warning()),
                ));
            }
        }
        Line::from(spans)
    }

}
