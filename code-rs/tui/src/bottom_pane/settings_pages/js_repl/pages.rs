use super::*;

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::editor_page::SettingsEditorPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::row_page::SettingsRowPage;

impl JsReplSettingsView {
    pub(super) fn main_page(&self) -> SettingsRowPage<'static> {
        SettingsRowPage::new(" JS REPL ", self.render_header_lines(), vec![])
    }

    pub(super) fn render_header_lines(&self) -> Vec<Line<'static>> {
        let enabled = self.settings.enabled;
        let status_style = if enabled {
            Style::default()
                .fg(crate::colors::success())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(crate::colors::warning())
                .add_modifier(Modifier::BOLD)
        };

        let runtime = Self::runtime_label(self.settings.runtime);
        let runtime_style = Style::default()
            .fg(crate::colors::info())
            .add_modifier(Modifier::BOLD);

        let mediation = if self.network_enabled { "ON" } else { "OFF" };
        let mediation_style = if self.network_enabled {
            Style::default()
                .fg(crate::colors::success())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(crate::colors::text_dim())
        };

        let enabled_word = if enabled { "ON " } else { "OFF " };
        let mut lines = vec![Line::from(vec![
            Span::styled(enabled_word, status_style),
            Span::styled("js_repl", Style::default().fg(crate::colors::text_mid())),
            Span::styled("  |  runtime: ", Style::default().fg(crate::colors::text_dim())),
            Span::styled(runtime, runtime_style),
            Span::styled("  |  mediation: ", Style::default().fg(crate::colors::text_dim())),
            Span::styled(mediation, mediation_style),
        ])];

        let node_blocked = self.network_enabled
            && matches!(self.settings.runtime, JsReplRuntimeKindToml::Node)
            && !cfg!(target_os = "macos");
        if node_blocked {
            lines.push(Line::from(vec![Span::styled(
                "Note: Node is not enforceable with mediation on this platform; prefer Deno.",
                Style::default().fg(crate::colors::warning()),
            )]));
        } else {
            lines.push(Line::from(vec![Span::styled(
                "Enter edits. Ctrl+S saves in editors. Esc closes.",
                Style::default().fg(crate::colors::text_dim()),
            )]));
        }

        lines.push(Line::from(""));
        debug_assert_eq!(lines.len(), usize::from(Self::HEADER_ROWS));
        lines
    }

    fn text_edit_title(target: TextTarget) -> &'static str {
        match target {
            TextTarget::RuntimePath => " JS REPL: Runtime Path ",
        }
    }

    fn list_edit_title(target: ListTarget) -> &'static str {
        match target {
            ListTarget::RuntimeArgs => " JS REPL: Runtime Args ",
            ListTarget::NodeModuleDirs => " JS REPL: Node Module Dirs ",
        }
    }

    pub(super) fn text_edit_page(target: TextTarget) -> SettingsEditorPage<'static> {
        SettingsEditorPage::new(
            Self::text_edit_title(target),
            SettingsPanelStyle::bottom_pane(),
            "Runtime path",
            vec![
                Line::from(vec![Span::styled(
                    "Ctrl+S to save. Esc to cancel.",
                    Style::default().fg(crate::colors::text_dim()),
                )]),
                Line::from(""),
            ],
            vec![],
        )
    }

    pub(super) fn list_edit_page(target: ListTarget) -> SettingsEditorPage<'static> {
        let field_title = match target {
            ListTarget::RuntimeArgs => "Runtime args",
            ListTarget::NodeModuleDirs => "Node module dirs",
        };
        SettingsEditorPage::new(
            Self::list_edit_title(target),
            SettingsPanelStyle::bottom_pane(),
            field_title,
            vec![
                Line::from(vec![Span::styled(
                    "One entry per line. Ctrl+S to save. Esc to cancel.",
                    Style::default().fg(crate::colors::text_dim()),
                )]),
                Line::from(""),
            ],
            vec![],
        )
    }
}
