use super::*;

use std::time::Duration;

use ratatui::layout::Margin;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::editor_page::SettingsEditorPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::colors;

impl ExecLimitsSettingsView {
    pub(super) fn render_header_lines(&self) -> Vec<Line<'static>> {
        let hint = Style::default().fg(colors::text_dim());
        let mut lines = vec![Line::from(Span::styled(
            "Limits for tool-spawned commands (shell + exec_command).",
            hint,
        ))];

        #[cfg(target_os = "linux")]
        {
            lines.push(Line::from(Span::styled(
                "Linux: enforced via cgroup v2 when available.",
                hint,
            )));

            // Make "Auto" less opaque by showing what it would currently resolve to.
            let auto_pids = code_core::config::exec_limits_auto_pids_max();
            let auto_mem_bytes = code_core::config::exec_limits_auto_memory_max_bytes();
            let auto_mem_mib =
                auto_mem_bytes.map(|b| (b.saturating_add(1024 * 1024 - 1)) / (1024 * 1024));
            let auto_line = match (auto_pids, auto_mem_mib) {
                (Some(pids), Some(mib)) => {
                    format!("Auto currently: pids_max={pids} · memory_max={mib} MiB")
                }
                (Some(pids), None) => format!("Auto currently: pids_max={pids}"),
                (None, Some(mib)) => format!("Auto currently: memory_max={mib} MiB"),
                (None, None) => "Auto currently: (not available)".to_string(),
            };
            lines.push(Line::from(Span::styled(auto_line, hint)));
        }
        #[cfg(not(target_os = "linux"))]
        lines.push(Line::from(Span::styled(
            "This platform: best-effort (no cgroup enforcement yet).",
            hint,
        )));

        let is_dirty = self.settings != self.last_applied;
        if is_dirty {
            lines.push(Line::from(Span::styled(
                "Pending changes: select Apply to save.",
                Style::default().fg(colors::warning()),
            )));
        } else if self
            .last_apply_at
            .is_some_and(|t| t.elapsed() < Duration::from_secs(2))
        {
            lines.push(Line::from(Span::styled(
                "Applied.",
                Style::default().fg(colors::success()),
            )));
        }

        lines.push(Line::from(""));
        lines
    }

    pub(super) fn render_footer_lines(&self) -> Vec<Line<'static>> {
        vec![Line::from(vec![
            Span::styled("↑↓", Style::default().fg(colors::function())),
            Span::styled(" move  ", Style::default().fg(colors::text_dim())),
            Span::styled("Enter", Style::default().fg(colors::success())),
            Span::styled(" edit/toggle  ", Style::default().fg(colors::text_dim())),
            Span::styled("a", Style::default().fg(colors::success())),
            Span::styled(" auto  ", Style::default().fg(colors::text_dim())),
            Span::styled("d", Style::default().fg(colors::success())),
            Span::styled(" disable  ", Style::default().fg(colors::text_dim())),
            Span::styled("Ctrl+S", Style::default().fg(colors::success())),
            Span::styled(" save  ", Style::default().fg(colors::text_dim())),
            Span::styled("Esc", Style::default().fg(colors::error())),
            Span::styled(" close", Style::default().fg(colors::text_dim())),
        ])]
    }

    pub(super) fn edit_page(target: EditTarget, error: Option<&str>) -> SettingsEditorPage<'static> {
        let (title, field_title) = match target {
            EditTarget::PidsMax => (" Edit Process Limit ", "Process limit"),
            EditTarget::MemoryMax => (" Edit Memory Limit (MiB) ", "Memory limit (MiB)"),
        };
        let hint = Style::default().fg(colors::text_dim());
        let mut post_field_lines = Vec::new();
        if let Some(err) = error {
            post_field_lines.push(Line::from(Span::styled(
                err.to_string(),
                Style::default().fg(colors::error()),
            )));
        } else {
            post_field_lines.push(Line::from(Span::styled(
                "Tip: type \"auto\" or \"disabled\".",
                hint,
            )));
        }
        SettingsEditorPage::new(
            title,
            SettingsPanelStyle::bottom_pane(),
            field_title,
            vec![
                Line::from(Span::styled("Enter/Ctrl+S save · Esc cancel", hint)),
                Line::from(""),
            ],
            post_field_lines,
        )
        .with_field_margin(Margin::new(2, 0))
    }
}

