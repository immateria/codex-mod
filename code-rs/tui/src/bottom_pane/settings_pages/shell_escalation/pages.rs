use super::*;

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::editor_page::SettingsEditorPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::row_page::SettingsRowPage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WrapperResolutionSource {
    Override,
    Sibling,
    Path,
}

impl WrapperResolutionSource {
    fn label(self) -> &'static str {
        match self {
            Self::Override => "override",
            Self::Sibling => "sibling",
            Self::Path => "PATH",
        }
    }
}

impl ShellEscalationSettingsView {
    pub(super) fn header_lines(&self) -> Vec<Line<'static>> {
        let profile = self
            .active_profile
            .as_deref()
            .map(|p| format!("Profile: {p}"))
            .unwrap_or_else(|| "Profile: (none)".to_string());

        vec![
            Line::from(Span::styled(
                "Configure zsh-fork escalation for sandboxed shell tool calls.",
                Style::default().fg(crate::colors::text_dim()),
            )),
            Line::from(Span::styled(
                profile,
                Style::default().fg(crate::colors::text_dim()),
            )),
            Line::from(Span::styled(
                "Enter activate · Ctrl+S apply · Esc close",
                Style::default().fg(crate::colors::text_dim()),
            )),
            Line::from(""),
        ]
    }

    fn shell_label(&self) -> String {
        let Some(shell) = &self.shell else {
            return "Shell: auto".to_string();
        };
        if shell.args.is_empty() {
            format!("Shell: {}", shell.path)
        } else {
            let args = shell.args.join(" ");
            format!("Shell: {} {args}", shell.path)
        }
    }

    fn shell_readiness_problem(&self) -> Option<String> {
        // Core zsh-fork execution requires `sess.user_shell()` to be `Shell::Zsh`,
        // which today only happens for the auto-detected login shell (no override).
        if let Some(shell) = &self.shell {
            return Some(format!(
                "Shell override is set (zsh-fork requires Shell: auto): {}",
                shell.path
            ));
        }

        #[cfg(unix)]
        {
            use libc::{getpwuid, getuid};
            use std::ffi::CStr;

            unsafe {
                let pw = getpwuid(getuid());
                if pw.is_null() {
                    return Some("Could not detect login shell (need zsh)".to_string());
                }

                let shell_ptr = (*pw).pw_shell;
                if shell_ptr.is_null() {
                    return Some("Could not detect login shell (need zsh)".to_string());
                }

                let shell_path = CStr::from_ptr(shell_ptr)
                    .to_string_lossy()
                    .into_owned();
                if shell_path.ends_with("/zsh") {
                    None
                } else {
                    Some(format!("Default user shell is not zsh: {shell_path}"))
                }
            }
        }

        #[cfg(not(unix))]
        {
            None
        }
    }

    fn resolve_wrapper(&self) -> (Option<std::path::PathBuf>, Option<WrapperResolutionSource>, Option<String>) {
        const WRAPPER_BASENAME: &str = "codex-execve-wrapper";

        let override_path = self
            .wrapper_override
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from);

        if let Some(override_path) = override_path {
            if override_path.is_file() {
                return (Some(override_path), Some(WrapperResolutionSource::Override), None);
            }
            return (
                None,
                None,
                Some(format!(
                    "Wrapper override does not exist: {}",
                    override_path.display()
                )),
            );
        }

        let current_exe = std::env::current_exe().ok();
        if let Some(current_exe) = current_exe
            && let Some(parent) = current_exe.parent()
        {
            let sibling = parent.join(WRAPPER_BASENAME);
            if sibling.is_file() {
                return (Some(sibling), Some(WrapperResolutionSource::Sibling), None);
            }
        }

        if let Some(path_env) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(path_env.as_os_str()) {
                let candidate = dir.join(WRAPPER_BASENAME);
                if candidate.is_file() {
                    return (Some(candidate), Some(WrapperResolutionSource::Path), None);
                }
            }
        }

        (None, None, Some("Wrapper binary not found (sibling or PATH)".to_string()))
    }

    fn zsh_path_problem(&self) -> Option<String> {
        let Some(zsh_path) = self
            .zsh_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Some("zsh_path is not set".to_string());
        };

        let zsh_path = std::path::Path::new(zsh_path);
        if !zsh_path.is_absolute() {
            return Some(format!("zsh_path must be absolute: {}", zsh_path.display()));
        }
        if !zsh_path.is_file() {
            return Some(format!("zsh_path does not exist: {}", zsh_path.display()));
        }
        None
    }

    pub(super) fn status_lines(&self) -> Vec<Line<'static>> {
        let mut reasons = Vec::<String>::new();

        if !cfg!(unix) {
            reasons.push("zsh-fork escalation is Unix-only".to_string());
        }
        if !self.enabled {
            reasons.push("Disabled (toggle is off)".to_string());
        }
        if let Some(reason) = self.shell_readiness_problem() {
            reasons.push(reason);
        }

        if let Some(reason) = self.zsh_path_problem() {
            reasons.push(reason);
        }

        let (wrapper_path, wrapper_source, wrapper_reason) = self.resolve_wrapper();
        if let Some(reason) = wrapper_reason {
            reasons.push(reason);
        }

        let ready = reasons.is_empty();

        let status_style = if ready {
            Style::default()
                .fg(crate::colors::success())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(crate::colors::warning())
                .add_modifier(Modifier::BOLD)
        };

        let wrapper_line = match (wrapper_path, wrapper_source) {
            (Some(path), Some(source)) => {
                format!("Wrapper: {} ({})", path.display(), source.label())
            }
            _ => "Wrapper: (not found)".to_string(),
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    if ready { "READY" } else { "NOT READY" },
                    status_style,
                ),
                Span::styled(
                    "  Status",
                    Style::default().fg(crate::colors::text_dim()),
                ),
            ]),
            Line::from(Span::styled(
                self.shell_label(),
                Style::default().fg(crate::colors::text_dim()),
            )),
            Line::from(Span::styled(
                wrapper_line,
                Style::default().fg(crate::colors::text_dim()),
            )),
        ];

        if !reasons.is_empty() {
            lines.push(Line::from(""));
            for reason in reasons.into_iter().take(4) {
                lines.push(Line::from(Span::styled(
                    format!("- {reason}"),
                    Style::default().fg(crate::colors::text_dim()),
                )));
            }
        }

        lines.extend([
            Line::from(""),
            Line::from(Span::styled(
                "Triggers only for sandboxed `shell` tool calls that invoke `zsh -lc/-c`.",
                Style::default().fg(crate::colors::text_dim()),
            )),
        ]);

        lines
    }

    pub(super) fn main_page(&self) -> SettingsRowPage<'static> {
        SettingsRowPage::new(" Shell escalation ", self.header_lines(), self.status_lines())
    }

    pub(super) fn edit_page(&self, target: EditTarget) -> SettingsEditorPage<'static> {
        let (title, field_title) = match target {
            EditTarget::ZshPath => (" Shell escalation: Zsh path ", "Patched zsh path"),
            EditTarget::WrapperOverride => (" Shell escalation: Wrapper override ", "Wrapper path override"),
        };

        let mut post = Vec::new();
        if let Some(notice) = self.editor_notice.as_ref() {
            post.push(Line::from(vec![Span::styled(
                notice.clone(),
                Style::default().fg(crate::colors::warning()),
            )]));
        }

        SettingsEditorPage::new(
            title,
            SettingsPanelStyle::bottom_pane(),
            field_title,
            vec![
                Line::from(vec![Span::styled(
                    "Enter accept · Ctrl+S accept+apply · Esc cancel · p pick path",
                    Style::default().fg(crate::colors::text_dim()),
                )]),
                Line::from(vec![Span::styled(
                    "Empty clears the value.",
                    Style::default().fg(crate::colors::text_dim()),
                )]),
                Line::from(""),
            ],
            post,
        )
    }
}
