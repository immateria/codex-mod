use super::*;
use super::persistence::{parse_path_list, style_profile_is_empty};

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::row_page::SettingsRowPage;
use crate::bottom_pane::settings_ui::rows::{KeyValueRow, StyledText, selection_index_at_over_text};
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test;

impl ShellProfilesSettingsView {
    pub(super) fn rows() -> [RowKind; 11] {
        [
            RowKind::Style,
            RowKind::Summary,
            RowKind::References,
            RowKind::SkillRoots,
            RowKind::SkillsAllowlist,
            RowKind::DisabledSkills,
            RowKind::McpInclude,
            RowKind::McpExclude,
            RowKind::OpenSkills,
            RowKind::Apply,
            RowKind::Close,
        ]
    }

    pub(super) fn selected_row(&self) -> RowKind {
        let rows = Self::rows();
        let idx = self
            .scroll
            .selected_idx
            .unwrap_or(0)
            .min(rows.len().saturating_sub(1));
        rows[idx]
    }

    pub(super) fn cycle_style_next(&mut self) {
        self.stage_pending_profile_from_fields();
        self.selected_style = match self.selected_style {
            ShellScriptStyle::PosixSh => ShellScriptStyle::BashZshCompatible,
            ShellScriptStyle::BashZshCompatible => ShellScriptStyle::Zsh,
            ShellScriptStyle::Zsh => ShellScriptStyle::PowerShell,
            ShellScriptStyle::PowerShell => ShellScriptStyle::Cmd,
            ShellScriptStyle::Cmd => ShellScriptStyle::Nushell,
            ShellScriptStyle::Nushell => ShellScriptStyle::Elvish,
            ShellScriptStyle::Elvish => ShellScriptStyle::PosixSh,
        };
        self.load_fields_for_style(self.selected_style);
        self.status = None;
    }

    pub(super) fn row_value(&self, row: RowKind) -> Option<String> {
        match row {
            RowKind::Style => {
                let selected = self.selected_style.to_string();
                match self.active_style {
                    Some(active) if active == self.selected_style => {
                        Some(format!("{selected} (active)"))
                    }
                    Some(active) => Some(format!("{selected} (active: {active})")),
                    None => Some(selected),
                }
            }
            RowKind::Summary => {
                let summary = self.summary_field.text().trim();
                if summary.is_empty() {
                    Some("unset".to_string())
                } else {
                    let first_line = summary.lines().next().unwrap_or(summary).trim();
                    Some(first_line.to_string())
                }
            }
            RowKind::References => {
                Some(format!("{} paths", parse_path_list(self.references_field.text()).len()))
            }
            RowKind::SkillRoots => {
                Some(format!("{} roots", parse_path_list(self.skill_roots_field.text()).len()))
            }
            RowKind::SkillsAllowlist => {
                let count = self
                    .shell_style_profiles
                    .get(&self.selected_style)
                    .map(|profile| profile.skills.len())
                    .unwrap_or(0);
                if count == 0 {
                    Some("all (no filter)".to_string())
                } else {
                    Some(format!("{count} selected"))
                }
            }
            RowKind::DisabledSkills => Some(format!(
                "{} disabled",
                self.shell_style_profiles
                    .get(&self.selected_style)
                    .map(|profile| profile.disabled_skills.len())
                    .unwrap_or(0)
            )),
            RowKind::McpInclude => {
                let count = self
                    .shell_style_profiles
                    .get(&self.selected_style)
                    .map(|profile| profile.mcp_servers.include.len())
                    .unwrap_or(0);
                if count == 0 {
                    Some("all (no filter)".to_string())
                } else {
                    Some(format!("{count} selected"))
                }
            }
            RowKind::McpExclude => Some(format!(
                "{} excluded",
                self.shell_style_profiles
                    .get(&self.selected_style)
                    .map(|profile| profile.mcp_servers.exclude.len())
                    .unwrap_or(0)
            )),
            RowKind::OpenSkills | RowKind::Apply | RowKind::Close => None,
        }
    }

    pub(super) fn row_label(row: RowKind) -> &'static str {
        match row {
            RowKind::Style => "Style",
            RowKind::Summary => "Summary",
            RowKind::References => "References",
            RowKind::SkillRoots => "Skill roots",
            RowKind::SkillsAllowlist => "Skills allowlist",
            RowKind::DisabledSkills => "Disabled skills",
            RowKind::McpInclude => "MCP include",
            RowKind::McpExclude => "MCP exclude",
            RowKind::OpenSkills => "Manage skills",
            RowKind::Apply => "Apply",
            RowKind::Close => "Close",
        }
    }

    pub(super) fn open_skills_editor(&mut self) {
        self.app_event_tx.send(AppEvent::OpenSettings {
            section: Some(SettingsSection::Skills),
        });
        self.is_complete = true;
    }

    pub(super) fn open_shell_selection(&mut self) {
        self.app_event_tx.send(AppEvent::OpenSettings {
            section: Some(SettingsSection::Shell),
        });
        self.is_complete = true;
    }

    pub(super) fn request_summary_generation(&mut self) {
        self.stage_pending_profile_from_fields();
        let profile = self
            .shell_style_profiles
            .get(&self.selected_style)
            .cloned()
            .unwrap_or_default();

        self.status = Some("Generating summary...".to_string());
        self.app_event_tx
            .send(AppEvent::RequestGenerateShellStyleProfileSummary {
                style: self.selected_style,
                profile,
            });
    }

    pub(crate) fn apply_generated_summary(&mut self, style: ShellScriptStyle, summary: String) {
        let normalized = summary.trim().to_string();
        if style == self.selected_style {
            self.summary_field.set_text(normalized.as_str());
        }

        let profile = self.shell_style_profiles.entry(style).or_default();
        profile.summary = if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        };
        if style_profile_is_empty(profile) {
            self.shell_style_profiles.remove(&style);
        }

        self.dirty = true;
        self.status = Some("Summary staged. Select Apply to persist.".to_string());
    }

    pub(crate) fn set_summary_generation_error(
        &mut self,
        _style: ShellScriptStyle,
        error: String,
    ) {
        self.status = Some(format!("Summary generation failed: {error}"));
    }

    pub(super) fn activate_selected_row(&mut self) {
        match self.selected_row() {
            RowKind::Style => self.cycle_style_next(),
            RowKind::Summary => self.open_editor(ListTarget::Summary),
            RowKind::References => self.open_editor(ListTarget::References),
            RowKind::SkillRoots => self.open_editor(ListTarget::SkillRoots),
            RowKind::SkillsAllowlist => self.open_picker(PickTarget::SkillsAllowlist),
            RowKind::DisabledSkills => self.open_picker(PickTarget::DisabledSkills),
            RowKind::McpInclude => self.open_picker(PickTarget::McpInclude),
            RowKind::McpExclude => self.open_picker(PickTarget::McpExclude),
            RowKind::OpenSkills => self.open_skills_editor(),
            RowKind::Apply => self.apply_settings(),
            RowKind::Close => self.is_complete = true,
        }
    }

    fn main_header_lines(&self) -> Vec<Line<'static>> {
        let shell = self
            .active_shell_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .unwrap_or("auto");
        let active_style = self
            .active_style
            .map(|style| style.to_string())
            .unwrap_or_else(|| "auto".to_string());
        let selected_style = self.selected_style.to_string();
        let styles_summary = if selected_style == active_style {
            format!("style: {active_style}")
        } else {
            format!("active: {active_style}  •  editing: {selected_style}")
        };
        let mut spans = vec![
            Span::styled("shell: ", Style::default().fg(crate::colors::text_dim())),
            Span::styled(
                shell.to_string(),
                Style::default().fg(crate::colors::text()),
            ),
            Span::styled("  •  ".to_string(), Style::default().fg(crate::colors::text_dim())),
            Span::styled(styles_summary, Style::default().fg(crate::colors::text_dim())),
        ];
        if self.dirty {
            spans.push(Span::styled(
                "  •  unsaved".to_string(),
                Style::default()
                    .fg(crate::colors::warning())
                    .add_modifier(Modifier::BOLD),
            ));
        }
        vec![Line::from(spans)]
    }

    fn main_footer_lines(&self) -> Vec<Line<'static>> {
        let default_help = "Enter: edit/cycle/apply  •  Ctrl+P: shell  •  Esc: close";
        let footer_text = if let Some(status) = self.status.as_deref()
            && !status.trim().is_empty()
        {
            status.trim().replace(['\r', '\n'], " ")
        } else if self.selected_row() == RowKind::Style {
            let summary = self
                .shell_style_profiles
                .get(&self.selected_style)
                .and_then(|profile| profile.summary.as_deref())
                .unwrap_or("")
                .trim();
            if summary.is_empty() {
                format!("Summary: unset  •  {default_help}")
            } else {
                let first_line = summary.lines().next().unwrap_or(summary).trim();
                format!("Summary: {first_line}  •  {default_help}")
            }
        } else {
            default_help.to_string()
        };
        vec![Line::from(Span::styled(
            footer_text,
            Style::default().fg(crate::colors::text_dim()),
        ))]
    }

    fn main_page(&self) -> SettingsRowPage<'static> {
        SettingsRowPage::new(
            "Shell Profiles",
            self.main_header_lines(),
            self.main_footer_lines(),
        )
    }

    fn main_row_specs(&self) -> Vec<KeyValueRow<'static>> {
        Self::rows()
            .iter()
            .copied()
            .map(|row| {
                let mut spec = KeyValueRow::new(Self::row_label(row));
                if let Some(value) = self.row_value(row) {
                    spec = spec.with_value(StyledText::new(
                        value,
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
                if let Some(hint) = Self::selected_hint(row) {
                    spec = spec.with_selected_hint(hint);
                }
                spec
            })
            .collect()
    }

    fn handle_mouse_event_main_in_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
        chrome: ChromeMode,
    ) -> bool {
        let rows = Self::rows();
        let total = rows.len();
        if total == 0 {
            return false;
        }

        let Some(layout) = self.main_page().layout_in_chrome(chrome, area) else {
            return false;
        };
        let visible_rows = layout.visible_rows().max(1);
        self.viewport_rows.set(visible_rows);

        let row_specs = self.main_row_specs();
        let kind = mouse_event.kind;
        let outcome = route_scroll_state_mouse_with_hit_test(
            mouse_event,
            &mut self.scroll,
            total,
            visible_rows,
            |x, y, scroll_top| {
                if matches!(kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown) {
                    if !crate::ui_interaction::contains_point(layout.body, x, y) {
                        return None;
                    }
                    let rel = y.saturating_sub(layout.body.y) as usize;
                    Some(scroll_top.saturating_add(rel).min(total.saturating_sub(1)))
                } else {
                    selection_index_at_over_text(layout.body, x, y, scroll_top, &row_specs)
                }
            },
            SETTINGS_LIST_MOUSE_CONFIG,
        );

        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            self.activate_selected_row();
        }
        outcome.changed
    }

    pub(super) fn handle_mouse_event_main(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.handle_mouse_event_main_in_chrome(mouse_event, area, ChromeMode::Framed)
    }

    pub(super) fn handle_mouse_event_main_content(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_main_in_chrome(mouse_event, area, ChromeMode::ContentOnly)
    }

    fn render_main_in_chrome(&self, area: Rect, buf: &mut Buffer, chrome: ChromeMode) {
        let row_specs = self.main_row_specs();
        let rows = Self::rows();
        let total = rows.len();
        let scroll = self.scroll.clamped(total);
        let scroll_top = scroll.scroll_top;
        let selected = scroll.selected_idx;
        let Some(layout) = self
            .main_page()
            .render_in_chrome(chrome, area, buf, scroll_top, selected, &row_specs)
        else {
            return;
        };
        self.viewport_rows.set(layout.visible_rows().max(1));
    }

    pub(super) fn render_main(&self, area: Rect, buf: &mut Buffer) {
        self.render_main_in_chrome(area, buf, ChromeMode::Framed);
    }

    pub(super) fn render_main_without_frame(&self, area: Rect, buf: &mut Buffer) {
        self.render_main_in_chrome(area, buf, ChromeMode::ContentOnly);
    }

    fn selected_hint(row: RowKind) -> Option<&'static str> {
        match row {
            RowKind::Style => Some("Left/Right/Enter cycle"),
            RowKind::Apply => Some("Persist staged changes"),
            RowKind::Close => Some("Close settings"),
            RowKind::OpenSkills => Some("Open skills settings"),
            _ => Some("Enter edit/open"),
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::mpsc;

    #[test]
    fn content_hit_testing_differs_from_framed_layout() {
        let (tx, _rx) = mpsc::channel::<AppEvent>();
        let view = ShellProfilesSettingsView::new(
            PathBuf::from("code-home"),
            None,
            HashMap::new(),
            Vec::new(),
            Vec::new(),
            AppEventSender::new(tx),
        );

        let area = Rect::new(0, 0, 40, 12);
        let content_layout = view
            .main_page()
            .layout_in_chrome(ChromeMode::ContentOnly, area)
            .expect("layout");
        let framed_layout = view
            .main_page()
            .layout_in_chrome(ChromeMode::Framed, area)
            .expect("layout");
        let total = ShellProfilesSettingsView::rows().len();
        let scroll_top = view.scroll.clamped(total).scroll_top;
        let row_specs = view.main_row_specs();

        assert_eq!(
            crate::bottom_pane::settings_ui::rows::selection_index_at_over_text(
                content_layout.body,
                content_layout.body.x.saturating_add(2),
                content_layout.body.y,
                scroll_top,
                &row_specs,
            ),
            Some(0)
        );
        assert_eq!(
            crate::bottom_pane::settings_ui::rows::selection_index_at_over_text(
                framed_layout.body,
                content_layout.body.x.saturating_add(2),
                content_layout.body.y,
                scroll_top,
                &row_specs,
            ),
            None
        );
    }
}
