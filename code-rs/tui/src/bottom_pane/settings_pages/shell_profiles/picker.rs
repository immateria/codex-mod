use super::*;
use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::sectioned_panel::SettingsSectionedPanelLayout;
use super::model::PickListItem;
use super::persistence::{normalize_list_key, style_profile_is_empty};

impl ShellProfilesSettingsView {
    pub(super) fn picker_values_for_style(
        &self,
        target: PickTarget,
    ) -> (Vec<String>, Vec<String>) {
        let profile = self.shell_style_profiles.get(&self.selected_style);
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
                name: trimmed.to_string(),
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
                    "(all skills)".to_string(),
                    Some("No allowlist filter (disabled skills still apply).".to_string()),
                ),
                PickTarget::McpInclude => (
                    "(all MCP servers)".to_string(),
                    Some("No include filter (excluded servers still apply).".to_string()),
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
            PickTarget::SkillsAllowlist => "disabled",
            PickTarget::DisabledSkills => "allowlisted",
            PickTarget::McpInclude => "excluded",
            PickTarget::McpExclude => "included",
        }
    }

    pub(super) fn picker_title(target: PickTarget) -> &'static str {
        match target {
            PickTarget::SkillsAllowlist => "Skills allowlist",
            PickTarget::DisabledSkills => "Disabled skills",
            PickTarget::McpInclude => "MCP include",
            PickTarget::McpExclude => "MCP exclude",
        }
    }

    pub(super) fn apply_picker_selection(&mut self, target: PickTarget, selection: Vec<String>) {
        if selection.is_empty() && !self.shell_style_profiles.contains_key(&self.selected_style) {
            return;
        }

        let profile = self
            .shell_style_profiles
            .entry(self.selected_style)
            .or_default();

        match target {
            PickTarget::SkillsAllowlist => {
                profile.skills = selection;
                let selected_set: HashSet<String> =
                    profile.skills.iter().map(|v| normalize_list_key(v)).collect();
                profile
                    .disabled_skills
                    .retain(|v| !selected_set.contains(&normalize_list_key(v)));
            }
            PickTarget::DisabledSkills => {
                profile.disabled_skills = selection;
                let selected_set: HashSet<String> = profile
                    .disabled_skills
                    .iter()
                    .map(|v| normalize_list_key(v))
                    .collect();
                profile
                    .skills
                    .retain(|v| !selected_set.contains(&normalize_list_key(v)));
            }
            PickTarget::McpInclude => {
                profile.mcp_servers.include = selection;
                let selected_set: HashSet<String> = profile
                    .mcp_servers
                    .include
                    .iter()
                    .map(|v| normalize_list_key(v))
                    .collect();
                profile
                    .mcp_servers
                    .exclude
                    .retain(|v| !selected_set.contains(&normalize_list_key(v)));
            }
            PickTarget::McpExclude => {
                profile.mcp_servers.exclude = selection;
                let selected_set: HashSet<String> = profile
                    .mcp_servers
                    .exclude
                    .iter()
                    .map(|v| normalize_list_key(v))
                    .collect();
                profile
                    .mcp_servers
                    .include
                    .retain(|v| !selected_set.contains(&normalize_list_key(v)));
            }
        }

        if style_profile_is_empty(profile) {
            self.shell_style_profiles.remove(&self.selected_style);
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
                (*checked).then_some(item.name.trim().to_string())
            })
            .filter(|value| !value.is_empty())
            .collect();

        self.apply_picker_selection(state.target, selection);
        self.dirty = true;
        self.status = Some("Changes staged. Select Apply to persist.".to_string());
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
                "all (no filter)".to_string()
            } else {
                format!("{selected_count} selected")
            };
        let style = self.selected_style.to_string();
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
        vec![Line::from(Span::styled(
            "↑↓ move  •  Space/Enter toggle  •  Ctrl+S save  •  Esc cancel",
            Style::default().fg(crate::colors::text_dim()),
        ))]
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
        if area.width == 0 || area.height == 0 {
            return;
        }

        let total = state.items.len();
        if total == 0 {
            fill_rect(
                buf,
                area,
                Some(' '),
                Style::default().bg(crate::colors::background()),
            );
            write_line(
                buf,
                area.x,
                area.y,
                area.width,
                &Line::from(vec![Span::styled(
                    "no options available".to_string(),
                    Style::default()
                        .fg(crate::colors::text_dim())
                        .add_modifier(Modifier::ITALIC),
                )]),
                Style::default().bg(crate::colors::background()),
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
                    Style::default().bg(crate::colors::background()),
                );
                continue;
            }

            let item = &state.items[idx];
            let is_selected = idx == selected;
            let base = if is_selected {
                Style::default()
                    .bg(crate::colors::selection())
                    .fg(crate::colors::text_bright())
            } else {
                Style::default().bg(crate::colors::background()).fg(crate::colors::text())
            };
            fill_rect(buf, row_area, Some(' '), base);

            let checked = state.checked.get(idx).copied().unwrap_or(false);
            let check = if checked { "[x]" } else { "[ ]" };
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
                        .fg(crate::colors::text_dim())
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
                        .bg(crate::colors::selection())
                        .fg(crate::colors::text_dim()),
                ));
            }

            let line = Line::from(spans);
            write_line(buf, area.x, y, area.width, &line, base);
        }
    }
}
