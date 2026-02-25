use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use code_core::config::{
    set_shell_style_profile_mcp_servers,
    set_shell_style_profile_paths,
    set_shell_style_profile_skills,
};
use code_core::config_types::{ShellConfig, ShellScriptStyle, ShellStyleProfileConfig};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::components::form_text_field::FormTextField;
use crate::components::scroll_state::ScrollState;
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};
use crate::util::buffer::{fill_rect, write_line};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_panel::{panel_content_rect, render_panel, PanelFrameStyle};
use super::BottomPane;
use super::SettingsSection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    Style,
    References,
    SkillRoots,
    SkillsAllowlist,
    DisabledSkills,
    McpInclude,
    McpExclude,
    OpenSkills,
    Apply,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListTarget {
    References,
    SkillRoots,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickTarget {
    SkillsAllowlist,
    DisabledSkills,
    McpInclude,
    McpExclude,
}

#[derive(Clone, Debug)]
struct SkillOption {
    name: String,
    description: Option<String>,
}

#[derive(Debug)]
struct PickListItem {
    name: String,
    description: Option<String>,
    is_unknown: bool,
}

#[derive(Debug)]
struct PickListState {
    target: PickTarget,
    items: Vec<PickListItem>,
    checked: Vec<bool>,
    other_values: HashSet<String>,
    scroll: ScrollState,
}

#[derive(Debug)]
enum ViewMode {
    Main,
    EditList { target: ListTarget, before: String },
    PickList(PickListState),
}

pub(crate) struct ShellProfilesSettingsView {
    code_home: PathBuf,
    active_shell_path: Option<String>,
    active_style: Option<ShellScriptStyle>,
    selected_style: ShellScriptStyle,
    shell_style_profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig>,
    available_skills: Vec<SkillOption>,
    available_mcp_servers: Vec<String>,
    references_field: FormTextField,
    skill_roots_field: FormTextField,
    app_event_tx: AppEventSender,
    is_complete: bool,
    dirty: bool,
    status: Option<String>,
    mode: ViewMode,
    scroll: ScrollState,
    viewport_rows: Cell<usize>,
    pick_viewport_rows: Cell<usize>,
}

impl ShellProfilesSettingsView {
    pub(crate) fn new(
        code_home: PathBuf,
        current_shell: Option<&ShellConfig>,
        shell_style_profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig>,
        available_skills: Vec<(String, String)>,
        available_mcp_servers: Vec<String>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let active_shell_path = current_shell.map(|shell| shell.path.clone());
        let active_style = current_shell
            .and_then(|shell| {
                shell.script_style
                    .or_else(|| ShellScriptStyle::infer_from_shell_program(&shell.path))
            });
        let selected_style = active_style.unwrap_or(ShellScriptStyle::BashZshCompatible);

        let mut references_field = FormTextField::new_multi_line();
        references_field.set_placeholder("docs/shell/my-style.md");
        let mut skill_roots_field = FormTextField::new_multi_line();
        skill_roots_field.set_placeholder("skills/my-style");

        let mut available_skills: Vec<SkillOption> = available_skills
            .into_iter()
            .map(|(name, description)| SkillOption {
                name: name.trim().to_string(),
                description: {
                    let d = description.trim();
                    if d.is_empty() {
                        None
                    } else {
                        Some(d.to_string())
                    }
                },
            })
            .filter(|entry| !entry.name.is_empty())
            .collect();
        available_skills.sort_by(|a, b| {
            normalize_list_key(&a.name)
                .cmp(&normalize_list_key(&b.name))
                .then_with(|| a.name.cmp(&b.name))
        });
        let mut seen_skills: HashSet<String> = HashSet::new();
        available_skills.retain(|entry| seen_skills.insert(normalize_list_key(&entry.name)));

        let mut available_mcp_servers: Vec<String> = available_mcp_servers
            .into_iter()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect();
        available_mcp_servers.sort_by(|a, b| {
            normalize_list_key(a)
                .cmp(&normalize_list_key(b))
                .then_with(|| a.cmp(b))
        });
        let mut seen_servers: HashSet<String> = HashSet::new();
        available_mcp_servers.retain(|name| seen_servers.insert(normalize_list_key(name)));

        let mut view = Self {
            code_home,
            active_shell_path,
            active_style,
            selected_style,
            shell_style_profiles,
            available_skills,
            available_mcp_servers,
            references_field,
            skill_roots_field,
            app_event_tx,
            is_complete: false,
            dirty: false,
            status: None,
            mode: ViewMode::Main,
            scroll: ScrollState::new(),
            viewport_rows: Cell::new(0),
            pick_viewport_rows: Cell::new(0),
        };
        view.scroll.selected_idx = Some(0);
        view.load_fields_for_style(selected_style);
        view
    }

    pub(crate) fn set_current_shell(&mut self, current_shell: Option<&ShellConfig>) {
        self.active_shell_path = current_shell.map(|shell| shell.path.clone());
        let active_style = current_shell.and_then(|shell| {
            shell.script_style
                .or_else(|| ShellScriptStyle::infer_from_shell_program(&shell.path))
        });

        if self.active_style == active_style {
            return;
        }

        self.active_style = active_style;

        // Keep the panel "following" the active style when the user hasn't
        // staged edits or entered the editor. This avoids surprising resets
        // while still keeping defaults aligned with the selected shell.
        if matches!(self.mode, ViewMode::Main) && !self.dirty {
            if let Some(active) = self.active_style
                && self.selected_style != active {
                    self.selected_style = active;
                    self.load_fields_for_style(active);
                }
        }
    }

    fn rows() -> [RowKind; 10] {
        [
            RowKind::Style,
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

    fn selected_row(&self) -> RowKind {
        let rows = Self::rows();
        let idx = self.scroll.selected_idx.unwrap_or(0).min(rows.len().saturating_sub(1));
        rows[idx]
    }

    fn content_area(area: Rect) -> Rect {
        panel_content_rect(area, PanelFrameStyle::bottom_pane().with_margin(Margin::new(1, 0)))
    }

    fn layout_main(area: Rect) -> Option<(Rect, Rect, Rect)> {
        let content = Self::content_area(area);
        if content.width == 0 || content.height < 3 {
            return None;
        }
        let header = Rect::new(content.x, content.y, content.width, 1);
        let footer = Rect::new(
            content.x,
            content.y.saturating_add(content.height.saturating_sub(1)),
            content.width,
            1,
        );
        let list = Rect::new(
            content.x,
            content.y.saturating_add(1),
            content.width,
            content.height.saturating_sub(2),
        );
        Some((header, list, footer))
    }

    fn layout_picker(area: Rect) -> Option<(Rect, Rect, Rect)> {
        let content = Self::content_area(area);
        if content.width == 0 || content.height < 4 {
            return None;
        }
        let header = Rect::new(content.x, content.y, content.width, 1);
        let footer = Rect::new(
            content.x,
            content.y.saturating_add(content.height.saturating_sub(1)),
            content.width,
            1,
        );
        let list = Rect::new(
            content.x,
            content.y.saturating_add(1),
            content.width,
            content.height.saturating_sub(2),
        );
        Some((header, list, footer))
    }

    fn selection_index_at(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        let (_header, list, _footer) = Self::layout_main(area)?;
        if list.width == 0 || list.height == 0 {
            return None;
        }
        if x < list.x || x >= list.x.saturating_add(list.width) {
            return None;
        }
        if y < list.y || y >= list.y.saturating_add(list.height) {
            return None;
        }
        let rel = y.saturating_sub(list.y) as usize;
        let rows = Self::rows();
        let scroll_top = self.scroll.scroll_top.min(rows.len().saturating_sub(1));
        let actual = scroll_top.saturating_add(rel);
        if actual >= rows.len() {
            return None;
        }
        Some(actual)
    }

    fn cycle_style_next(&mut self) {
        self.stage_pending_profile_from_fields();
        self.selected_style = match self.selected_style {
            ShellScriptStyle::PosixSh => ShellScriptStyle::BashZshCompatible,
            ShellScriptStyle::BashZshCompatible => ShellScriptStyle::Zsh,
            ShellScriptStyle::Zsh => ShellScriptStyle::PosixSh,
        };
        self.load_fields_for_style(self.selected_style);
    }

    fn open_editor(&mut self, target: ListTarget) {
        let before = match target {
            ListTarget::References => self.references_field.text().to_string(),
            ListTarget::SkillRoots => self.skill_roots_field.text().to_string(),
        };
        self.mode = ViewMode::EditList { target, before };
    }

    fn picker_values_for_style(&self, target: PickTarget) -> (Vec<String>, Vec<String>) {
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

    fn open_picker(&mut self, target: PickTarget) {
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
            });
        }

        items.sort_by(|a, b| {
            normalize_list_key(&a.name)
                .cmp(&normalize_list_key(&b.name))
                .then_with(|| a.name.cmp(&b.name))
        });

        let checked: Vec<bool> = items
            .iter()
            .map(|item| current_set.contains(&normalize_list_key(&item.name)))
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

        self.mode = ViewMode::PickList(PickListState {
            target,
            items,
            checked,
            other_values: other_values_set,
            scroll,
        });
    }

    fn picker_conflict_label(target: PickTarget) -> &'static str {
        match target {
            PickTarget::SkillsAllowlist => "disabled",
            PickTarget::DisabledSkills => "allowlisted",
            PickTarget::McpInclude => "excluded",
            PickTarget::McpExclude => "included",
        }
    }

    fn picker_title(target: PickTarget) -> &'static str {
        match target {
            PickTarget::SkillsAllowlist => "Select skills allowlist (non-empty means only these run)",
            PickTarget::DisabledSkills => "Select disabled skills (always excluded for this style)",
            PickTarget::McpInclude => "Select MCP include list (non-empty means only these servers)",
            PickTarget::McpExclude => "Select MCP exclude list (always excluded for this style)",
        }
    }

    fn apply_picker_selection(&mut self, target: PickTarget, selection: Vec<String>) {
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
                profile.skills.retain(|v| !selected_set.contains(&normalize_list_key(v)));
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

    fn save_picker(&mut self, state: &PickListState) {
        let selection: Vec<String> = state
            .items
            .iter()
            .zip(state.checked.iter())
            .filter_map(|(item, checked)| (*checked).then_some(item.name.trim().to_string()))
            .filter(|value| !value.is_empty())
            .collect();

        self.apply_picker_selection(state.target, selection);
        self.dirty = true;
        self.status = Some("Changes staged. Select Apply to persist.".to_string());
    }

    fn toggle_picker_selection(state: &mut PickListState) -> bool {
        let Some(idx) = state.scroll.selected_idx else {
            return false;
        };
        let Some(is_checked) = state.checked.get_mut(idx) else {
            return false;
        };
        *is_checked = !*is_checked;
        true
    }

    fn open_skills_editor(&mut self) {
        self.app_event_tx.send(AppEvent::OpenSettings {
            section: Some(SettingsSection::Skills),
        });
        self.is_complete = true;
    }

    fn open_shell_selection(&mut self) {
        self.app_event_tx.send(AppEvent::OpenSettings {
            section: Some(SettingsSection::Shell),
        });
        self.is_complete = true;
    }

    fn apply_settings(&mut self) {
        self.stage_pending_profile_from_fields();

        let was_dirty = self.dirty;
        let mut changed_any = false;

        for style in [
            ShellScriptStyle::PosixSh,
            ShellScriptStyle::BashZshCompatible,
            ShellScriptStyle::Zsh,
        ] {
            let (references, skill_roots, skills, disabled_skills, include, exclude) =
                if let Some(profile) = self.shell_style_profiles.get(&style) {
                    (
                        profile.references.clone(),
                        profile.skill_roots.clone(),
                        profile.skills.clone(),
                        profile.disabled_skills.clone(),
                        profile.mcp_servers.include.clone(),
                        profile.mcp_servers.exclude.clone(),
                    )
                } else {
                    (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new())
                };

            match set_shell_style_profile_paths(&self.code_home, style, &references, &skill_roots) {
                Ok(changed) => changed_any |= changed,
                Err(err) => {
                    self.status = Some(format!("Failed to persist style paths: {err}"));
                    return;
                }
            }

            match set_shell_style_profile_skills(&self.code_home, style, &skills, &disabled_skills) {
                Ok(changed) => changed_any |= changed,
                Err(err) => {
                    self.status = Some(format!("Failed to persist style skills: {err}"));
                    return;
                }
            }

            match set_shell_style_profile_mcp_servers(&self.code_home, style, &include, &exclude) {
                Ok(changed) => changed_any |= changed,
                Err(err) => {
                    self.status = Some(format!("Failed to persist MCP filters: {err}"));
                    return;
                }
            }
        }

        if changed_any || was_dirty {
            self.app_event_tx.send(AppEvent::UpdateShellStyleProfiles {
                shell_style_profiles: self.shell_style_profiles.clone(),
            });
        }

        self.dirty = false;
        if changed_any {
            self.status = Some("Shell style profiles applied.".to_string());
        } else {
            self.status = Some("No changes to apply.".to_string());
        }
    }

    fn load_fields_for_style(&mut self, style: ShellScriptStyle) {
        let (references, skill_roots) =
            if let Some(profile) = self.shell_style_profiles.get(&style) {
                (
                    profile.references.clone(),
                    profile.skill_roots.clone(),
                )
            } else {
                (Vec::new(), Vec::new())
            };

        self.references_field.set_text(&format_path_list(&references));
        self.skill_roots_field.set_text(&format_path_list(&skill_roots));
    }

    fn stage_pending_profile_from_fields(&mut self) {
        let references = parse_path_list(self.references_field.text());
        let skill_roots = parse_path_list(self.skill_roots_field.text());

        if references.is_empty()
            && skill_roots.is_empty()
            && !self.shell_style_profiles.contains_key(&self.selected_style)
        {
            return;
        }

        let profile = self
            .shell_style_profiles
            .entry(self.selected_style)
            .or_default();
        profile.references = references;
        profile.skill_roots = skill_roots;
        if style_profile_is_empty(profile) {
            self.shell_style_profiles.remove(&self.selected_style);
        }
    }

    fn row_value(&self, row: RowKind) -> Option<String> {
        match row {
            RowKind::Style => {
                let selected = self.selected_style.to_string();
                match self.active_style {
                    Some(active) if active == self.selected_style => Some(format!("{selected} (active)")),
                    Some(active) => Some(format!("{selected} (active: {active})")),
                    None => Some(selected),
                }
            }
            RowKind::References => Some(format!("{} paths", parse_path_list(self.references_field.text()).len())),
            RowKind::SkillRoots => Some(format!("{} roots", parse_path_list(self.skill_roots_field.text()).len())),
            RowKind::SkillsAllowlist => Some(format!(
                "{} skills",
                self.shell_style_profiles
                    .get(&self.selected_style)
                    .map(|profile| profile.skills.len())
                    .unwrap_or(0)
            )),
            RowKind::DisabledSkills => Some(format!(
                "{} skills",
                self.shell_style_profiles
                    .get(&self.selected_style)
                    .map(|profile| profile.disabled_skills.len())
                    .unwrap_or(0)
            )),
            RowKind::McpInclude => Some(format!(
                "{} servers",
                self.shell_style_profiles
                    .get(&self.selected_style)
                    .map(|profile| profile.mcp_servers.include.len())
                    .unwrap_or(0)
            )),
            RowKind::McpExclude => Some(format!(
                "{} servers",
                self.shell_style_profiles
                    .get(&self.selected_style)
                    .map(|profile| profile.mcp_servers.exclude.len())
                    .unwrap_or(0)
            )),
            RowKind::OpenSkills => None,
            RowKind::Apply => None,
            RowKind::Close => None,
        }
    }

    fn row_label(row: RowKind) -> &'static str {
        match row {
            RowKind::Style => "Style",
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

    fn handle_mouse_event_main(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let rows = Self::rows();
        let total = rows.len();
        if total == 0 {
            return false;
        }

        if self.scroll.selected_idx.is_none() {
            self.scroll.selected_idx = Some(0);
        }
        self.scroll.clamp_selection(total);
        let mut selected = self.scroll.selected_idx.unwrap_or(0);
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            total,
            |x, y| self.selection_index_at(area, x, y),
            SelectableListMouseConfig {
                hover_select: true,
                require_pointer_hit_for_scroll: true,
                scroll_behavior: ScrollSelectionBehavior::Clamp,
                ..SelectableListMouseConfig::default()
            },
        );

        match result {
            SelectableListMouseResult::Ignored => false,
            SelectableListMouseResult::SelectionChanged => {
                self.scroll.selected_idx = Some(selected);
                let visible = self.viewport_rows.get().max(1);
                self.scroll.ensure_visible(total, visible);
                true
            }
            SelectableListMouseResult::Activated => {
                self.scroll.selected_idx = Some(selected);
                let visible = self.viewport_rows.get().max(1);
                self.scroll.ensure_visible(total, visible);
                self.activate_selected_row();
                true
            }
        }
    }

    fn editor_field_mut(&mut self, target: ListTarget) -> &mut FormTextField {
        match target {
            ListTarget::References => &mut self.references_field,
            ListTarget::SkillRoots => &mut self.skill_roots_field,
        }
    }

    fn activate_selected_row(&mut self) {
        match self.selected_row() {
            RowKind::Style => self.cycle_style_next(),
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

    fn render_main(&self, area: Rect, buf: &mut Buffer) {
        let Some((header_area, list_area, footer_area)) = Self::layout_main(area) else {
            return;
        };

        self.viewport_rows.set(list_area.height as usize);

        let title = "Shell profiles";
        let title_style = Style::default()
            .fg(crate::colors::text_bright())
            .add_modifier(Modifier::BOLD);
        let dim = Style::default().fg(crate::colors::text_dim());

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
            format!("active: {active_style}  editing: {selected_style}")
        };

        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled(title.to_string(), title_style));
        if self.dirty {
            spans.push(Span::styled(" (unsaved)".to_string(), dim));
        }
        spans.push(Span::styled("  •  ".to_string(), dim));
        spans.push(Span::styled("shell: ".to_string(), dim));
        spans.push(Span::styled(shell.to_string(), Style::default().fg(crate::colors::text())));
        spans.push(Span::styled("  •  ".to_string(), dim));
        spans.push(Span::styled(styles_summary, dim));

        let header_line = Line::from(spans);
        write_line(
            buf,
            header_area.x,
            header_area.y,
            header_area.width,
            &header_line,
            Style::default().bg(crate::colors::background()),
        );

        self.render_list(list_area, buf);

        let footer_text = self
            .status
            .as_deref()
            .unwrap_or("Enter: edit/apply  •  Ctrl+P: shell  •  Esc: close");
        let footer_line = Line::from(vec![Span::styled(
            footer_text.to_string(),
            Style::default().fg(crate::colors::text_dim()),
        )]);
        write_line(
            buf,
            footer_area.x,
            footer_area.y,
            footer_area.width,
            &footer_line,
            Style::default().bg(crate::colors::background()),
        );
    }

    fn render_list(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let rows = Self::rows();
        let total = rows.len();
        let visible = area.height as usize;
        let scroll_top = self.scroll.scroll_top.min(total.saturating_sub(1));
        let selected = self.scroll.selected_idx.unwrap_or(0).min(total.saturating_sub(1));

        for row_idx in 0..visible {
            let idx = scroll_top.saturating_add(row_idx);
            let y = area.y.saturating_add(row_idx as u16);
            let row_area = Rect::new(area.x, y, area.width, 1);

            if idx >= total {
                fill_rect(buf, row_area, Some(' '), Style::default().bg(crate::colors::background()));
                continue;
            }

            let row = rows[idx];
            let is_selected = idx == selected;
            let base = if is_selected {
                Style::default()
                    .bg(crate::colors::selection())
                    .fg(crate::colors::text_bright())
            } else {
                Style::default().bg(crate::colors::background()).fg(crate::colors::text())
            };
            fill_rect(buf, row_area, Some(' '), base);

            let prefix = if is_selected { "> " } else { "  " };
            let label = Self::row_label(row);
            let value = self.row_value(row).unwrap_or_default();
            let label_style = base.add_modifier(Modifier::BOLD);
            let value_style = if is_selected {
                base.add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text_dim())
            };
            let line = Line::from(vec![
                Span::styled(format!("{prefix}{label}: "), label_style),
                Span::styled(value, value_style),
            ]);
            write_line(buf, area.x, y, area.width, &line, base);
        }
    }

    fn render_editor(&self, area: Rect, buf: &mut Buffer, target: ListTarget) {
        let content = Self::content_area(area);
        if content.width == 0 || content.height < 4 {
            return;
        }

        let header_area = Rect::new(content.x, content.y, content.width, 1);
        let footer_area = Rect::new(
            content.x,
            content.y.saturating_add(content.height.saturating_sub(1)),
            content.width,
            1,
        );
        let field_area = Rect::new(
            content.x,
            content.y.saturating_add(1),
            content.width,
            content.height.saturating_sub(2),
        );

        let title = match target {
            ListTarget::References => "Edit references (one path per line)",
            ListTarget::SkillRoots => "Edit skill roots (one path per line)",
        };
        write_line(
            buf,
            header_area.x,
            header_area.y,
            header_area.width,
            &Line::from(vec![Span::styled(
                title.to_string(),
                Style::default()
                    .fg(crate::colors::text_bright())
                    .add_modifier(Modifier::BOLD),
            )]),
            Style::default().bg(crate::colors::background()),
        );

        let footer = Line::from(vec![Span::styled(
            "Ctrl+S save  •  Esc cancel".to_string(),
            Style::default().fg(crate::colors::text_dim()),
        )]);
        write_line(
            buf,
            footer_area.x,
            footer_area.y,
            footer_area.width,
            &footer,
            Style::default().bg(crate::colors::background()),
        );

        let focused = true;
        match target {
            ListTarget::References => self.references_field.render(field_area, buf, focused),
            ListTarget::SkillRoots => self.skill_roots_field.render(field_area, buf, focused),
        }
    }

    fn render_picker(&self, area: Rect, buf: &mut Buffer, state: &PickListState) {
        let Some((header_area, list_area, footer_area)) = Self::layout_picker(area) else {
            return;
        };

        self.pick_viewport_rows.set(list_area.height as usize);

        let title = Self::picker_title(state.target);
        let style = self.selected_style.to_string();
        let header_line = Line::from(vec![Span::styled(
            format!("{title}  •  style: {style}"),
            Style::default()
                .fg(crate::colors::text_bright())
                .add_modifier(Modifier::BOLD),
        )]);
        write_line(
            buf,
            header_area.x,
            header_area.y,
            header_area.width,
            &header_line,
            Style::default().bg(crate::colors::background()),
        );

        self.render_pick_list(list_area, buf, state);

        let footer = Line::from(vec![Span::styled(
            "↑↓ move  •  Space/Enter toggle  •  Ctrl+S save  •  Esc cancel".to_string(),
            Style::default().fg(crate::colors::text_dim()),
        )]);
        write_line(
            buf,
            footer_area.x,
            footer_area.y,
            footer_area.width,
            &footer,
            Style::default().bg(crate::colors::background()),
        );
    }

    fn render_pick_list(&self, area: Rect, buf: &mut Buffer, state: &PickListState) {
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
        let scroll_top = state.scroll.scroll_top.min(total.saturating_sub(1));
        let selected = state.scroll.selected_idx.unwrap_or(0).min(total.saturating_sub(1));
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
            let mut suffix: String = String::new();
            if item.is_unknown {
                suffix.push_str(" (unknown)");
            }
            let conflict_key = normalize_list_key(&item.name);
            if state.other_values.contains(&conflict_key) {
                suffix.push_str(" (");
                suffix.push_str(conflict_label);
                suffix.push(')');
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
                } else if state.other_values.contains(&conflict_key) {
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

    pub(crate) fn handle_key_event_direct(&mut self, key: KeyEvent) -> bool {
        if self.is_complete {
            return true;
        }

        let mode = std::mem::replace(&mut self.mode, ViewMode::Main);
        match mode {
            ViewMode::Main => match key {
                KeyEvent { code: KeyCode::Esc, .. } => {
                    self.is_complete = true;
                    true
                }
                KeyEvent { code: KeyCode::Char('p'), modifiers, .. }
                    if modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    self.open_shell_selection();
                    true
                }
                KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE, .. } => {
                    let rows = Self::rows();
                    let visible = self.viewport_rows.get().max(1);
                    self.scroll.move_up_wrap_visible(rows.len(), visible);
                    true
                }
                KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => {
                    let rows = Self::rows();
                    let visible = self.viewport_rows.get().max(1);
                    self.scroll.move_down_wrap_visible(rows.len(), visible);
                    true
                }
                KeyEvent { code: KeyCode::Left, modifiers: KeyModifiers::NONE, .. }
                    if self.selected_row() == RowKind::Style =>
                {
                    self.cycle_style_next();
                    true
                }
                KeyEvent { code: KeyCode::Right, modifiers: KeyModifiers::NONE, .. }
                    if self.selected_row() == RowKind::Style =>
                {
                    self.cycle_style_next();
                    true
                }
                KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                    self.activate_selected_row();
                    true
                }
                _ => false,
            },
            ViewMode::EditList { target, before } => match (key.code, key.modifiers) {
                (KeyCode::Esc, _) => {
                    match target {
                        ListTarget::References => self.references_field.set_text(&before),
                        ListTarget::SkillRoots => self.skill_roots_field.set_text(&before),
                    }
                    true
                }
                (KeyCode::Char('p'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    self.open_shell_selection();
                    true
                }
                (KeyCode::Char('s'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    self.stage_pending_profile_from_fields();
                    self.dirty = true;
                    self.status = Some("Changes staged. Select Apply to persist.".to_string());
                    true
                }
                _ => {
                    let handled = self.editor_field_mut(target).handle_key(key);
                    self.mode = ViewMode::EditList { target, before };
                    handled
                }
            },
            ViewMode::PickList(mut state) => match (key.code, key.modifiers) {
                (KeyCode::Esc, _) => true,
                (KeyCode::Char('s'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    self.save_picker(&state);
                    true
                }
                (KeyCode::Up, KeyModifiers::NONE) => {
                    let visible = self.pick_viewport_rows.get().max(1);
                    state.scroll.move_up_wrap_visible(state.items.len(), visible);
                    self.mode = ViewMode::PickList(state);
                    true
                }
                (KeyCode::Down, KeyModifiers::NONE) => {
                    let visible = self.pick_viewport_rows.get().max(1);
                    state.scroll.move_down_wrap_visible(state.items.len(), visible);
                    self.mode = ViewMode::PickList(state);
                    true
                }
                (KeyCode::Char(' '), KeyModifiers::NONE) | (KeyCode::Enter, KeyModifiers::NONE) => {
                    let handled = Self::toggle_picker_selection(&mut state);
                    self.mode = ViewMode::PickList(state);
                    handled
                }
                _ => {
                    self.mode = ViewMode::PickList(state);
                    false
                }
            },
        }
    }

    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        if self.is_complete {
            return true;
        }

        let target = match &self.mode {
            ViewMode::Main => return false,
            ViewMode::EditList { target, .. } => *target,
            ViewMode::PickList(_) => return false,
        };
        self.editor_field_mut(target).handle_paste(text);
        true
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        if self.is_complete {
            return true;
        }

        let mode = std::mem::replace(&mut self.mode, ViewMode::Main);
        match mode {
            ViewMode::Main => self.handle_mouse_event_main(mouse_event, area),
            ViewMode::EditList { target, before } => {
                let content = Self::content_area(area);
                if content.width == 0 || content.height < 4 {
                    self.mode = ViewMode::EditList { target, before };
                    return false;
                }
                let field_area = Rect::new(
                    content.x,
                    content.y.saturating_add(1),
                    content.width,
                    content.height.saturating_sub(2),
                );
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => self.editor_field_mut(target).handle_mouse_click(
                        mouse_event.column,
                        mouse_event.row,
                        field_area,
                    ),
                    MouseEventKind::ScrollDown => self.editor_field_mut(target).handle_mouse_scroll(true),
                    MouseEventKind::ScrollUp => self.editor_field_mut(target).handle_mouse_scroll(false),
                    _ => false,
                };
                self.mode = ViewMode::EditList { target, before };
                handled
            }
            ViewMode::PickList(mut state) => {
                let total = state.items.len();
                if total == 0 {
                    self.mode = ViewMode::PickList(state);
                    return false;
                }

                if state.scroll.selected_idx.is_none() {
                    state.scroll.selected_idx = Some(0);
                }
                state.scroll.clamp_selection(total);
                let mut selected = state.scroll.selected_idx.unwrap_or(0);

                let scroll_top = state.scroll.scroll_top.min(total.saturating_sub(1));
                let row_at_position = |x: u16, y: u16| -> Option<usize> {
                    let (_header, list, _footer) = Self::layout_picker(area)?;
                    if list.width == 0 || list.height == 0 {
                        return None;
                    }
                    if x < list.x || x >= list.x.saturating_add(list.width) {
                        return None;
                    }
                    if y < list.y || y >= list.y.saturating_add(list.height) {
                        return None;
                    }
                    let rel = y.saturating_sub(list.y) as usize;
                    let actual = scroll_top.saturating_add(rel);
                    if actual >= total {
                        None
                    } else {
                        Some(actual)
                    }
                };

                let result = route_selectable_list_mouse_with_config(
                    mouse_event,
                    &mut selected,
                    total,
                    row_at_position,
                    SelectableListMouseConfig {
                        hover_select: true,
                        require_pointer_hit_for_scroll: true,
                        scroll_behavior: ScrollSelectionBehavior::Clamp,
                        ..SelectableListMouseConfig::default()
                    },
                );

                let handled = match result {
                    SelectableListMouseResult::Ignored => false,
                    SelectableListMouseResult::SelectionChanged => {
                        state.scroll.selected_idx = Some(selected);
                        let visible = self.pick_viewport_rows.get().max(1);
                        state.scroll.ensure_visible(total, visible);
                        true
                    }
                    SelectableListMouseResult::Activated => {
                        state.scroll.selected_idx = Some(selected);
                        let visible = self.pick_viewport_rows.get().max(1);
                        state.scroll.ensure_visible(total, visible);
                        if let Some(entry) = state.checked.get_mut(selected) {
                            *entry = !*entry;
                        }
                        true
                    }
                };

                self.mode = ViewMode::PickList(state);
                handled
            }
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }
}

impl<'a> BottomPaneView<'a> for ShellProfilesSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_event_direct(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_key_event_direct(key_event))
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_mouse_event_direct(mouse_event, area))
    }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        redraw_if(self.handle_paste_direct(text))
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        14
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        render_panel(
            area,
            buf,
            "Shell Profiles",
            PanelFrameStyle::bottom_pane(),
            |content_area, buf| match &self.mode {
                ViewMode::Main => self.render_main(content_area, buf),
                ViewMode::EditList { target, .. } => self.render_editor(content_area, buf, *target),
                ViewMode::PickList(state) => self.render_picker(content_area, buf, state),
            },
        );
    }
}

fn format_path_list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_list_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn parse_path_list(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn style_profile_is_empty(profile: &ShellStyleProfileConfig) -> bool {
    profile.references.is_empty()
        && profile.prepend_developer_messages.is_empty()
        && profile.skills.is_empty()
        && profile.disabled_skills.is_empty()
        && profile.skill_roots.is_empty()
        && profile.mcp_servers.include.is_empty()
        && profile.mcp_servers.exclude.is_empty()
        && profile.command_safety == code_core::config_types::CommandSafetyProfileConfig::default()
        && profile.dangerous_command_detection.is_none()
}
