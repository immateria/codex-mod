use super::*;
use super::persistence::style_profile_is_empty;
use crate::text_formatting::parse_path_list;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::row_page::SettingsRowPage;
use crate::bottom_pane::settings_ui::rows::{KeyValueRow, StyledText, selection_index_at_over_text};
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test;

impl ShellProfilesSettingsView {
    pub(super) fn rows() -> [RowKind; 14] {
        [
            RowKind::Style,
            RowKind::ApplicableShells,
            RowKind::Summary,
            RowKind::References,
            RowKind::SkillRoots,
            RowKind::SkillsAllowlist,
            RowKind::DisabledSkills,
            RowKind::McpInclude,
            RowKind::McpExclude,
            RowKind::NewProfile,
            RowKind::DeleteProfile,
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
        let styles = relevant_styles_for_shell(self.active_shell_path.as_deref());
        let ids: Vec<String> = styles.iter().map(|s| s.to_string()).collect();
        self.selected_id = match ids.iter().position(|id| id == &self.selected_id) {
            Some(i) => ids[(i + 1) % ids.len()].clone(),
            None => ids[0].clone(),
        };
        let id = self.selected_id.clone();
        self.load_fields_for_style(&id);
        self.status = None;
    }

    pub(super) fn cycle_style_prev(&mut self) {
        self.stage_pending_profile_from_fields();
        let styles = relevant_styles_for_shell(self.active_shell_path.as_deref());
        let ids: Vec<String> = styles.iter().map(|s| s.to_string()).collect();
        self.selected_id = match ids.iter().position(|id| id == &self.selected_id) {
            Some(0) => ids[ids.len() - 1].clone(),
            Some(i) => ids[i - 1].clone(),
            None => ids[0].clone(),
        };
        let id = self.selected_id.clone();
        self.load_fields_for_style(&id);
        self.status = None;
    }

    pub(super) fn row_value(&self, row: RowKind) -> Option<String> {
        match row {
            RowKind::Style => {
                let selected = &self.selected_id;
                match &self.active_profile_id {
                    Some(active) if active == selected => {
                        Some(format!("{selected} (active)"))
                    }
                    Some(active) => Some(format!("{selected} (active: {active})")),
                    None => Some(selected.clone()),
                }
            }
            RowKind::Summary => {
                let summary = self.summary_field.text().trim();
                if summary.is_empty() {
                    Some("unset".to_owned())
                } else {
                    let first_line = summary.lines().next().unwrap_or(summary).trim();
                    Some(first_line.to_owned())
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
                    .get(&self.selected_id)
                    .map_or(0, |entry| entry.config.skills.len());
                if count == 0 {
                    Some("all (no filter)".to_owned())
                } else {
                    Some(format!("{count} selected"))
                }
            }
            RowKind::DisabledSkills => Some(format!(
                "{} disabled",
                self.shell_style_profiles
                    .get(&self.selected_id)
                    .map_or(0, |entry| entry.config.disabled_skills.len())
            )),
            RowKind::McpInclude => {
                let count = self
                    .shell_style_profiles
                    .get(&self.selected_id)
                    .map_or(0, |entry| entry.config.mcp_servers.include.len());
                if count == 0 {
                    Some("all (no filter)".to_owned())
                } else {
                    Some(format!("{count} selected"))
                }
            }
            RowKind::McpExclude => Some(format!(
                "{} excluded",
                self.shell_style_profiles
                    .get(&self.selected_id)
                    .map_or(0, |entry| entry.config.mcp_servers.exclude.len())
            )),
            RowKind::ApplicableShells => {
                let shells = self
                    .shell_style_profiles
                    .get(&self.selected_id)
                    .map(|e| e.applicable_shells.as_slice())
                    .unwrap_or(&[]);
                if shells.is_empty() {
                    Some("defaults".to_owned())
                } else {
                    Some(shells.join(", "))
                }
            }
            RowKind::NewProfile | RowKind::DeleteProfile => None,
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
            RowKind::ApplicableShells => "Applicable shells",
            RowKind::NewProfile => "New profile",
            RowKind::DeleteProfile => "Delete profile",
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
        let entry = self
            .shell_style_profiles
            .get(&self.selected_id)
            .cloned()
            .unwrap_or_default();
        let profile = entry.config;
        let style = entry
            .style
            .or_else(|| ShellScriptStyle::parse(&self.selected_id))
            .unwrap_or(ShellScriptStyle::BashZshCompatible);

        self.status = Some("Generating summary...".to_owned());
        self.app_event_tx
            .send(AppEvent::RequestGenerateShellStyleProfileSummary {
                style,
                profile,
            });
    }

    pub(crate) fn apply_generated_summary(&mut self, style: ShellScriptStyle, summary: String) {
        let normalized = summary.trim().to_owned();
        let id = style.to_string();
        if id == self.selected_id {
            self.summary_field.set_text(normalized.as_str());
        }

        let entry = self.shell_style_profiles.entry(id.clone()).or_default();
        entry.config.summary = if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        };
        if style_profile_is_empty(&entry.config) {
            self.shell_style_profiles.remove(&id);
        }

        self.dirty = true;
        self.status = Some("Summary staged. Select Apply to persist.".to_owned());
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
            RowKind::ApplicableShells => self.open_picker(PickTarget::ApplicableShells),
            RowKind::SkillsAllowlist => self.open_picker(PickTarget::SkillsAllowlist),
            RowKind::DisabledSkills => self.open_picker(PickTarget::DisabledSkills),
            RowKind::McpInclude => self.open_picker(PickTarget::McpInclude),
            RowKind::McpExclude => self.open_picker(PickTarget::McpExclude),
            RowKind::NewProfile => self.create_profile(),
            RowKind::DeleteProfile => self.delete_profile(),
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
            .active_profile_id
            .as_deref()
            .map_or_else(|| "auto".to_owned(), |id| id.to_owned());
        let selected_style = self.selected_id.clone();
        let styles_summary = if selected_style == active_style {
            format!("style: {active_style}")
        } else {
            format!("active: {active_style}  •  editing: {selected_style}")
        };
        let mut spans = vec![
            Span::styled("shell: ", crate::colors::style_text_dim()),
            Span::styled(
                shell.to_owned(),
                crate::colors::style_text(),
            ),
            Span::styled("  •  ", crate::colors::style_text_dim()),
            Span::styled(styles_summary, crate::colors::style_text_dim()),
        ];
        if self.dirty {
            spans.push(Span::styled(
                "  •  unsaved".to_owned(),
                Style::default()
                    .fg(crate::colors::warning())
                    .add_modifier(Modifier::BOLD),
            ));
        }
        vec![Line::from(spans)]
    }

    fn main_footer_lines(&self) -> Vec<Line<'static>> {
        use crate::bottom_pane::settings_ui::hints::{hint_enter, hint_esc, shortcut_line, KeyHint};

        let shortcut = shortcut_line(&[
            hint_enter(" edit/cycle/apply"),
            KeyHint::new(crate::icons::ctrl_combo("P"), " shell"),
            hint_esc(" close"),
        ]);

        let mut lines = Vec::with_capacity(2);

        if let Some(status) = self.status.as_deref()
            && !status.trim().is_empty()
        {
            lines.push(Line::from(Span::styled(
                status.trim().replace(['\r', '\n'], " "),
                crate::colors::style_text_dim(),
            )));
        } else if self.selected_row() == RowKind::Style {
            let summary = self
                .shell_style_profiles
                .get(&self.selected_id)
                .and_then(|entry| entry.config.summary.as_deref())
                .unwrap_or("")
                .trim();
            if summary.is_empty() {
                lines.push(Line::from(Span::styled(
                    "Summary: unset",
                    crate::colors::style_text_dim(),
                )));
            } else {
                let first_line = summary.lines().next().unwrap_or(summary).trim();
                lines.push(Line::from(Span::styled(
                    format!("Summary: {first_line}"),
                    crate::colors::style_text_dim(),
                )));
            }
        }

        lines.push(shortcut);
        lines
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
                        crate::colors::style_text_dim(),
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
            RowKind::ApplicableShells => Some("Enter to select applicable shells"),
            RowKind::Apply => Some("Persist staged changes"),
            RowKind::Close => Some("Close settings"),
            RowKind::OpenSkills => Some("Open skills settings"),
            RowKind::NewProfile => Some("Enter to create a new profile"),
            RowKind::DeleteProfile => Some("Enter to delete this profile"),
            _ => Some("Enter edit/open"),
        }
    }

    pub(super) fn create_profile(&mut self) {
        let base = "custom-profile";
        let mut id = base.to_owned();
        let mut n = 1u32;
        while self.shell_style_profiles.contains_key(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        self.stage_pending_profile_from_fields();
        let default_style = self
            .active_shell_path
            .as_deref()
            .and_then(|p| ShellScriptStyle::infer_from_shell_program(p))
            .unwrap_or(ShellScriptStyle::BashZshCompatible);
        self.shell_style_profiles.insert(
            id.clone(),
            ShellStyleProfileEntry {
                style: Some(default_style),
                applicable_shells: Vec::new(),
                config: Default::default(),
            },
        );
        self.selected_id = id.clone();
        self.load_fields_for_style(&id);
        self.dirty = true;
        self.status = Some(format!("Created profile '{id}'. Configure and select Apply to save."));
    }

    pub(super) fn delete_profile(&mut self) {
        let id = self.selected_id.clone();
        self.shell_style_profiles.remove(&id);
        let styles = relevant_styles_for_shell(self.active_shell_path.as_deref());
        let next_id = styles
            .first()
            .map(|s| s.to_string())
            .unwrap_or_else(|| ShellScriptStyle::BashZshCompatible.to_string());
        self.selected_id = next_id.clone();
        self.load_fields_for_style(&next_id);
        self.dirty = true;
        self.status = Some(format!("Deleted profile '{id}'. Select Apply to persist."));
    }

}

const ALL_STYLES: &[ShellScriptStyle] = &[
    ShellScriptStyle::PosixSh,
    ShellScriptStyle::BashZshCompatible,
    ShellScriptStyle::Zsh,
    ShellScriptStyle::PowerShell,
    ShellScriptStyle::Cmd,
    ShellScriptStyle::Nushell,
    ShellScriptStyle::Elvish,
    ShellScriptStyle::Fish,
    ShellScriptStyle::Xonsh,
    ShellScriptStyle::Oil,
];

/// Returns the script styles relevant to the given shell path.
///
/// Filters based on which styles could realistically be active for that shell
/// (e.g., zsh supports POSIX, bash-zsh-compat, and zsh-idiomatic; nu only supports nushell).
/// Unknown or absent shell paths fall back to the full list.
fn relevant_styles_for_shell(shell_path: Option<&str>) -> &'static [ShellScriptStyle] {
    static ZSH_STYLES: &[ShellScriptStyle] = &[
        ShellScriptStyle::PosixSh,
        ShellScriptStyle::BashZshCompatible,
        ShellScriptStyle::Zsh,
    ];
    static BASH_STYLES: &[ShellScriptStyle] = &[
        ShellScriptStyle::PosixSh,
        ShellScriptStyle::BashZshCompatible,
    ];
    static POSIX_STYLES: &[ShellScriptStyle] = &[ShellScriptStyle::PosixSh];
    static POWERSHELL_STYLES: &[ShellScriptStyle] = &[ShellScriptStyle::PowerShell];
    static CMD_STYLES: &[ShellScriptStyle] = &[ShellScriptStyle::Cmd];
    static NUSHELL_STYLES: &[ShellScriptStyle] = &[ShellScriptStyle::Nushell];
    static ELVISH_STYLES: &[ShellScriptStyle] = &[ShellScriptStyle::Elvish];
    static FISH_STYLES: &[ShellScriptStyle] = &[ShellScriptStyle::Fish];
    static XONSH_STYLES: &[ShellScriptStyle] = &[ShellScriptStyle::Xonsh];
    static OIL_STYLES: &[ShellScriptStyle] = &[ShellScriptStyle::Oil];

    let Some(path) = shell_path else {
        return ALL_STYLES;
    };
    match code_core::shell::shell_basename(path).as_str() {
        "zsh" => ZSH_STYLES,
        "bash" | "mksh" => BASH_STYLES,
        "sh" | "dash" | "ash" | "ksh" => POSIX_STYLES,
        "powershell" | "pwsh" => POWERSHELL_STYLES,
        "cmd" => CMD_STYLES,
        "nu" => NUSHELL_STYLES,
        "elvish" => ELVISH_STYLES,
        "fish" => FISH_STYLES,
        "xonsh" => XONSH_STYLES,
        "osh" | "oil" => OIL_STYLES,
        _ => ALL_STYLES,
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
