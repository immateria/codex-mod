use super::*;
use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::sectioned_panel::SettingsSectionedPanelLayout;
use super::model::PickListItem;
use super::persistence::{normalize_list_key, style_profile_is_empty};

/// Known shell basenames for the applicable-shells picker.
const ALL_KNOWN_SHELLS: &[(&str, &str)] = &[
    ("sh",         "POSIX sh"),
    ("dash",       "Dash (lightweight POSIX sh)"),
    ("ash",        "Ash (BusyBox sh)"),
    ("ksh",        "KornShell"),
    ("bash",       "Bash"),
    ("mksh",       "MirBSD KornShell"),
    ("zsh",        "Zsh"),
    ("powershell", "PowerShell"),
    ("pwsh",       "PowerShell (pwsh)"),
    ("cmd",        "Windows CMD"),
    ("nu",         "Nushell"),
    ("elvish",     "Elvish"),
    ("fish",       "Fish"),
    ("xonsh",      "Xonsh"),
    ("osh",        "Oil Shell (osh compat mode)"),
    ("oil",        "Oil Shell"),
];

impl ShellProfilesSettingsView {
    pub(super) fn picker_values_for_style(
        &self,
        target: PickTarget,
    ) -> (Vec<String>, Vec<String>) {
        let profile = self
            .shell_style_profiles
            .get(&self.selected_id)
            .map(|e| &e.config);
        match target {
            PickTarget::SkillsAllowlist => (
                profile.map(|p| p.skills.clone()).unwrap_or_default(),
                profile.map(|p| p.disabled_skills.clone()).unwrap_or_default(),
            ),
            PickTarget::DisabledSkills => (
                profile.map(|p| p.disabled_skills.clone()).unwrap_or_default(),
                profile.map(|p| p.skills.clone()).unwrap_or_default(),
            ),
            PickTarget::McpInclude => (
                profile
                    .map(|p| p.mcp_servers.include.clone())
                    .unwrap_or_default(),
                profile
                    .map(|p| p.mcp_servers.exclude.clone())
                    .unwrap_or_default(),
            ),
            PickTarget::McpExclude => (
                profile
                    .map(|p| p.mcp_servers.exclude.clone())
                    .unwrap_or_default(),
                profile
                    .map(|p| p.mcp_servers.include.clone())
                    .unwrap_or_default(),
            ),
            PickTarget::ApplicableShells => (
                self.shell_style_profiles
                    .get(&self.selected_id)
                    .map(|e| e.applicable_shells.clone())
                    .unwrap_or_default(),
                Vec::new(),
            ),
        }
    }

    pub(super) fn open_picker(&mut self, target: PickTarget) {
        self.stage_pending_profile_from_fields();

        let (current_values, other_values) = self.picker_values_for_style(target);
        let current_set: HashSet<String> = current_values
            .iter()
            .map(|value| normalize_list_key(value))
            .collect();

        let mut items: Vec<PickListItem> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        match target {
            PickTarget::SkillsAllowlist | PickTarget::DisabledSkills => {
                for skill in &self.available_skills {
                    let key = normalize_list_key(&skill.name);
                    if !seen.insert(key) {
                        continue;
                    }
                    items.push(PickListItem {
                        name: skill.name.clone(),
                        description: skill.description.clone(),
                        is_unknown: false,
                        is_no_filter_option: false,
                    });
                }
            }
            PickTarget::McpInclude | PickTarget::McpExclude => {
                for server in &self.available_mcp_servers {
                    let key = normalize_list_key(server);
                    if !seen.insert(key) {
                        continue;
                    }
                    items.push(PickListItem {
                        name: server.clone(),
                        description: None,
                        is_unknown: false,
                        is_no_filter_option: false,
                    });
                }
            }
            PickTarget::ApplicableShells => {
                for (name, description) in ALL_KNOWN_SHELLS {
                    let key = normalize_list_key(name);
                    if !seen.insert(key) {
                        continue;
                    }
                    items.push(PickListItem {
                        name: name.to_string(),
                        description: Some(description.to_string()),
                        is_unknown: false,
                        is_no_filter_option: false,
                    });
                }
            }
        }

        for value in current_values.iter().chain(other_values.iter()) {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                continue;
            }
            let key = normalize_list_key(trimmed);
            if !seen.insert(key) {
                continue;
            }
            items.push(PickListItem {
                name: trimmed.to_owned(),
                description: None,
                is_unknown: true,
                is_no_filter_option: false,
            });
        }

        items.sort_by(|a, b| {
            normalize_list_key(&a.name)
                .cmp(&normalize_list_key(&b.name))
                .then_with(|| a.name.cmp(&b.name))
        });

        let has_no_filter_option =
            matches!(target, PickTarget::SkillsAllowlist | PickTarget::McpInclude);
        if has_no_filter_option {
            let (label, description) = match target {
                PickTarget::SkillsAllowlist => (
                    "(all skills)".to_owned(),
                    Some("No allowlist filter (disabled skills still apply).".to_owned()),
                ),
                PickTarget::McpInclude => (
                    "(all MCP servers)".to_owned(),
                    Some("No include filter (excluded servers still apply).".to_owned()),
                ),
                _ => (String::new(), None),
            };
            if !label.is_empty() {
                items.insert(
                    0,
                    PickListItem {
                        name: label,
                        description,
                        is_unknown: false,
                        is_no_filter_option: true,
                    },
                );
            }
        }

        let checked: Vec<bool> = items
            .iter()
            .map(|item| {
                if item.is_no_filter_option {
                    current_set.is_empty()
                } else {
                    current_set.contains(&normalize_list_key(&item.name))
                }
            })
            .collect();
        let other_values_set: HashSet<String> = other_values
            .iter()
            .map(|value| normalize_list_key(value))
            .collect();

        let mut scroll = ScrollState::new();
        if items.is_empty() {
            scroll.selected_idx = None;
        } else if let Some(idx) = checked.iter().position(|is_checked| *is_checked) {
            scroll.selected_idx = Some(idx);
        } else {
            scroll.selected_idx = Some(0);
        }
        scroll.ensure_visible(items.len(), self.pick_viewport_rows.get().max(1));

        self.mode = ViewMode::PickList(PickListState {
            target,
            items,
            checked,
            other_values: other_values_set,
            scroll,
        });
    }

    pub(super) fn picker_conflict_label(target: PickTarget) -> &'static str {
        match target {
            PickTarget::ApplicableShells => "",
            PickTarget::SkillsAllowlist => "disabled",
            PickTarget::DisabledSkills => "allowlisted",
            PickTarget::McpInclude => "excluded",
            PickTarget::McpExclude => "included",
        }
    }

    pub(super) fn picker_title(target: PickTarget) -> &'static str {
        match target {
            PickTarget::ApplicableShells => "Applicable shells",
            PickTarget::SkillsAllowlist => "Skills allowlist",
            PickTarget::DisabledSkills => "Disabled skills",
            PickTarget::McpInclude => "MCP include",
            PickTarget::McpExclude => "MCP exclude",
        }
    }

    pub(super) fn apply_picker_selection(&mut self, target: PickTarget, selection: Vec<String>) {
        if selection.is_empty() && !self.shell_style_profiles.contains_key(&self.selected_id) {
            return;
        }

        let selected_id = self.selected_id.clone();
        let entry = self
            .shell_style_profiles
            .entry(selected_id.clone())
            .or_insert_with(Default::default);

        match target {
            PickTarget::ApplicableShells => {
                entry.applicable_shells = selection;
            }
            PickTarget::SkillsAllowlist => {
                entry.config.skills = selection;
                let selected_set: HashSet<String> =
                    entry.config.skills.iter().map(|v| normalize_list_key(v)).collect();
                entry
                    .config
                    .disabled_skills
                    .retain(|v| !selected_set.contains(&normalize_list_key(v)));
            }
            PickTarget::DisabledSkills => {
                entry.config.disabled_skills = selection;
                let selected_set: HashSet<String> = entry
                    .config
                    .disabled_skills
                    .iter()
                    .map(|v| normalize_list_key(v))
                    .collect();
                entry
                    .config
                    .skills
                    .retain(|v| !selected_set.contains(&normalize_list_key(v)));
            }
            PickTarget::McpInclude => {
                entry.config.mcp_servers.include = selection;
                let selected_set: HashSet<String> = entry
                    .config
                    .mcp_servers
                    .include
                    .iter()
                    .map(|v| normalize_list_key(v))
                    .collect();
                entry
                    .config
                    .mcp_servers
                    .exclude
                    .retain(|v| !selected_set.contains(&normalize_list_key(v)));
            }
            PickTarget::McpExclude => {
                entry.config.mcp_servers.exclude = selection;
                let selected_set: HashSet<String> = entry
                    .config
                    .mcp_servers
                    .exclude
                    .iter()
                    .map(|v| normalize_list_key(v))
                    .collect();
                entry
                    .config
                    .mcp_servers
                    .include
                    .retain(|v| !selected_set.contains(&normalize_list_key(v)));
            }
        }

        if let PickTarget::ApplicableShells = target {
            // applicable_shells is meaningful even when config is empty; only
            // remove the entry if both applicable_shells and config are empty
            // and this is a custom profile (style.is_some() means it was
            // explicitly created, style.is_none() means built-in key).
            let can_remove = style_profile_is_empty(&entry.config)
                && entry.applicable_shells.is_empty()
                && entry.style.is_none();
            if can_remove {
                self.shell_style_profiles.remove(&selected_id);
            }
            return;
        }
        let is_empty = style_profile_is_empty(&entry.config);
        if is_empty {
            self.shell_style_profiles.remove(&selected_id);
        }
    }

    pub(super) fn save_picker(&mut self, state: &PickListState) {
        let selection: Vec<String> = state
            .items
            .iter()
            .zip(state.checked.iter())
            .filter_map(|(item, checked)| {
                if item.is_no_filter_option {
                    return None;
                }
                (*checked).then_some(item.name.trim().to_owned())
            })
            .filter(|value| !value.is_empty())
            .collect();

        self.apply_picker_selection(state.target, selection);
        self.dirty = true;
        self.status = Some("Changes staged. Select Apply to persist.".to_owned());
    }

    pub(super) fn toggle_picker_selection(state: &mut PickListState) -> bool {
        let Some(idx) = state.scroll.selected_idx else {
            return false;
        };

        if idx == 0
            && matches!(state.target, PickTarget::SkillsAllowlist | PickTarget::McpInclude)
            && state.items.first().is_some_and(|item| item.is_no_filter_option)
        {
            if let Some(first) = state.checked.first_mut() {
                *first = true;
            }
            for entry in state.checked.iter_mut().skip(1) {
                *entry = false;
            }
            return true;
        }

        let Some(is_checked) = state.checked.get_mut(idx) else {
            return false;
        };
        *is_checked = !*is_checked;

        if matches!(state.target, PickTarget::SkillsAllowlist | PickTarget::McpInclude)
            && state.items.first().is_some_and(|item| item.is_no_filter_option)
        {
            let any_selected = state
                .checked
                .iter()
                .enumerate()
                .skip(1)
                .any(|(_idx, checked)| *checked);
            if let Some(first) = state.checked.first_mut() {
                *first = !any_selected;
            }
        }
        true
    }

    pub(super) fn picker_header_lines(&self, state: &PickListState) -> Vec<Line<'static>> {
        let selected_count: usize = state
            .items
            .iter()
            .zip(state.checked.iter())
            .filter(|(item, checked)| **checked && !item.is_no_filter_option)
            .count();
        let selection_summary =
            if matches!(state.target, PickTarget::SkillsAllowlist | PickTarget::McpInclude)
                && selected_count == 0
            {
                "all (no filter)".to_owned()
            } else {
                format!("{selected_count} selected")
            };
        let style = self.selected_id.clone();
        vec![Line::from(Span::styled(
            format!(
                "{}  •  style: {style}  •  {selection_summary}",
                Self::picker_title(state.target)
            ),
            Style::default()
                .fg(crate::colors::text_bright())
                .add_modifier(Modifier::BOLD),
        ))]
    }

    pub(super) fn picker_footer_lines() -> Vec<Line<'static>> {
        use crate::bottom_pane::settings_ui::hints::{hint_esc, hint_nav, shortcut_line, KeyHint};
        vec![shortcut_line(&[
            hint_nav(" navigate"),
            KeyHint::new("Space/Enter", " toggle"),
            KeyHint::new("Ctrl+S", " apply"),
            hint_esc(" cancel"),
        ])]
    }

    fn picker_page(&self, state: &PickListState) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Shell Profiles",
            SettingsPanelStyle::bottom_pane(),
            self.picker_header_lines(state),
            Self::picker_footer_lines(),
        )
    }

    pub(super) fn compute_picker_layout_in_chrome(
        &self,
        area: Rect,
        state: &PickListState,
        chrome: ChromeMode,
    ) -> Option<SettingsSectionedPanelLayout> {
        self.picker_page(state).layout_in_chrome(chrome, area)
    }

    pub(super) fn render_picker(&self, area: Rect, buf: &mut Buffer, state: &PickListState) {
        self.render_picker_in_chrome(area, buf, state, ChromeMode::Framed);
    }

    pub(super) fn render_picker_without_frame(
        &self,
        area: Rect,
        buf: &mut Buffer,
        state: &PickListState,
    ) {
        self.render_picker_in_chrome(area, buf, state, ChromeMode::ContentOnly);
    }

    fn render_picker_in_chrome(
        &self,
        area: Rect,
        buf: &mut Buffer,
        state: &PickListState,
        chrome: ChromeMode,
    ) {
        let Some(layout) = self.picker_page(state).render_shell_in_chrome(chrome, area, buf) else {
            return;
        };

        self.pick_viewport_rows
            .set((layout.body.height as usize).max(1));
        self.render_pick_list(layout.body, buf, state);
    }

    pub(super) fn render_pick_list(&self, area: Rect, buf: &mut Buffer, state: &PickListState) {
        if area.is_empty() {
            return;
        }

        let s_on_bg = crate::colors::style_on_background();
        let c_text_dim = crate::colors::text_dim();
        let c_selection = crate::colors::selection();

        let total = state.items.len();
        if total == 0 {
            fill_rect(
                buf,
                area,
                Some(' '),
                s_on_bg,
            );
            write_line(
                buf,
                area.x,
                area.y,
                area.width,
                &Line::from(vec![Span::styled(
                    "no options available".to_owned(),
                    Style::default()
                        .fg(c_text_dim)
                        .add_modifier(Modifier::ITALIC),
                )]),
                s_on_bg,
            );
            return;
        }
        let visible = area.height as usize;
        let scroll = state.scroll.clamped(total);
        let scroll_top = scroll.scroll_top;
        let selected = scroll.selected_idx.unwrap_or(0);
        let conflict_label = Self::picker_conflict_label(state.target);

        for row_idx in 0..visible {
            let idx = scroll_top.saturating_add(row_idx);
            let y = area.y.saturating_add(row_idx as u16);
            let row_area = Rect::new(area.x, y, area.width, 1);

            if idx >= total {
                fill_rect(
                    buf,
                    row_area,
                    Some(' '),
                    s_on_bg,
                );
                continue;
            }

            let item = &state.items[idx];
            let is_selected = idx == selected;
            let base = if is_selected {
                Style::default()
                    .bg(c_selection)
                    .fg(crate::colors::text_bright())
            } else {
                crate::colors::style_text_on_bg()
            };
            fill_bg(buf, row_area, base);

            let checked = state.checked.get(idx).copied().unwrap_or(false);
            let check = if checked {
                crate::icons::checkbox_on()
            } else {
                crate::icons::checkbox_off()
            };
            let mut suffix = String::new();
            let conflict_key = if item.is_no_filter_option {
                String::new()
            } else {
                normalize_list_key(&item.name)
            };
            if !item.is_no_filter_option {
                if item.is_unknown {
                    suffix.push_str(" (unknown)");
                }
                if state.other_values.contains(&conflict_key) {
                    suffix.push_str(" (");
                    suffix.push_str(conflict_label);
                    suffix.push(')');
                }
            }

            let mut spans = Vec::new();
            let prefix = if is_selected { "> " } else { "  " };
            spans.push(Span::styled(
                format!("{prefix}{check} "),
                base.add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!("{}{}", item.name, suffix),
                if is_selected {
                    base.add_modifier(Modifier::BOLD)
                } else if !item.is_no_filter_option && state.other_values.contains(&conflict_key) {
                    Style::default()
                        .bg(crate::colors::background())
                        .fg(c_text_dim)
                } else {
                    base
                },
            ));

            if is_selected
                && let Some(desc) = item.description.as_ref()
                && !desc.trim().is_empty()
            {
                spans.push(Span::styled(
                    format!("  {desc}"),
                    Style::default()
                        .bg(c_selection)
                        .fg(c_text_dim),
                ));
            }

            let line = Line::from(spans);
            write_line(buf, area.x, y, area.width, &line, base);
        }
    }
}
