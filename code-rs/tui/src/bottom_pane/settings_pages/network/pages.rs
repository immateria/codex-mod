use super::*;

use code_common::summarize_sandbox_policy;
use code_core::config::NetworkModeToml;
use code_core::protocol::SandboxPolicy;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::editor_page::SettingsEditorPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::row_page::SettingsRowPage;

impl NetworkSettingsView {
    pub(super) const HEADER_LINE_COUNT: usize = 6;

    pub(super) fn mode_label(mode: NetworkModeToml) -> &'static str {
        match mode {
            NetworkModeToml::Limited => "limited",
            NetworkModeToml::Full => "full",
        }
    }

    pub(super) fn row_count(&self, show_advanced: bool) -> usize {
        let mut count = 8;
        if show_advanced {
            count += 2;
            if self.settings.enable_socks5 {
                count += 1;
            }
            #[cfg(target_os = "macos")]
            {
                count += 1;
            }
        }
        count
    }

    pub(super) fn build_rows(&self, show_advanced: bool) -> Vec<RowKind> {
        let mut rows = Vec::with_capacity(self.row_count(show_advanced));
        rows.extend([
            RowKind::Enabled,
            RowKind::Mode,
            RowKind::AllowedDomains,
            RowKind::DeniedDomains,
            RowKind::AllowLocalBinding,
            RowKind::AdvancedToggle,
        ]);

        if show_advanced {
            rows.push(RowKind::Socks5Enabled);
            if self.settings.enable_socks5 {
                rows.push(RowKind::Socks5Udp);
            }
            rows.push(RowKind::AllowUpstreamProxyEnv);
            #[cfg(target_os = "macos")]
            rows.push(RowKind::AllowUnixSockets);
        }

        rows.push(RowKind::Apply);
        rows.push(RowKind::Close);
        debug_assert_eq!(rows.len(), self.row_count(show_advanced));
        rows
    }

    pub(super) fn render_header_lines(&self) -> Vec<Line<'static>> {
        let enabled = self.settings.enabled;
        let status_tag = if enabled { "ON" } else { "OFF" };
        let status_style = if enabled {
            Style::default()
                .fg(crate::colors::success())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(crate::colors::text_dim())
                .add_modifier(Modifier::BOLD)
        };
        let status_tail = if enabled {
            let mode = Self::mode_label(self.settings.mode);
            format!(": mediated (managed proxy) · mode {mode}")
        } else {
            let sandbox = Self::sandbox_policy_compact(&self.sandbox_policy);
            format!(": direct (sandbox policy decides) · sbx {sandbox}")
        };

        let enforcement = if !enabled {
            "Enforcement: n/a (mediation off)".to_string()
        } else if cfg!(target_os = "macos") {
            "Enforcement: fail-closed for sandboxed tools (seatbelt); best-effort otherwise"
                .to_string()
        } else if cfg!(windows) {
            "Enforcement: best-effort (no OS-level egress restriction yet)".to_string()
        } else {
            "Enforcement: best-effort (processes may ignore proxy env)".to_string()
        };

        vec![
            Line::from(vec![
                Span::styled(status_tag.to_string(), status_style),
                Span::styled(status_tail, Style::default().fg(crate::colors::text_dim())),
            ]),
            Line::from(Span::styled(
                "When ON: prompts only for allowlist misses. Deny/local/mode blocks require edits.",
                Style::default().fg(crate::colors::text_dim()),
            )),
            Line::from(Span::styled(
                "Coverage: exec, exec_command, web_fetch (browser partial)",
                Style::default().fg(crate::colors::text_dim()),
            )),
            Line::from(Span::styled(
                enforcement,
                Style::default().fg(crate::colors::text_dim()),
            )),
            Line::from(Span::styled(
                "Enter activate · Ctrl+S save lists · Esc close",
                Style::default().fg(crate::colors::text_dim()),
            )),
            Line::from(""),
        ]
    }

    pub(super) fn main_page(&self) -> SettingsRowPage<'static> {
        SettingsRowPage::new(" Network ", self.render_header_lines(), vec![])
    }

    fn sandbox_policy_compact(policy: &SandboxPolicy) -> String {
        let summary = summarize_sandbox_policy(policy);
        let base = summary.split_whitespace().next().unwrap_or("unknown");
        if summary.contains("(network access enabled)") {
            format!("{base} +net")
        } else {
            base.to_string()
        }
    }

    pub(super) fn visible_budget(&self, total: usize) -> usize {
        if total == 0 {
            return 1;
        }
        let hint = self.viewport_rows.get();
        let target = if hint == 0 {
            Self::DEFAULT_VISIBLE_ROWS
        } else {
            hint
        };
        target.clamp(1, total)
    }

    pub(super) fn reconcile_selection_state(&mut self, show_advanced: bool) {
        let total = self.row_count(show_advanced);
        if total == 0 {
            self.state.reset();
            return;
        }
        self.state.clamp_selection(total);
        let visible_budget = self.visible_budget(total);
        self.state.ensure_visible(total, visible_budget);
    }

    pub(super) fn edit_page(target: EditTarget) -> SettingsEditorPage<'static> {
        let (title, field_title) = match target {
            EditTarget::AllowedDomains => (" Network: Allowed Domains ", "Allowed domains"),
            EditTarget::DeniedDomains => (" Network: Denied Domains ", "Denied domains"),
            EditTarget::AllowUnixSockets => (" Network: Unix Sockets ", "Unix sockets"),
        };
        SettingsEditorPage::new(
            title,
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
        .with_wrap_lines(true)
    }
}
