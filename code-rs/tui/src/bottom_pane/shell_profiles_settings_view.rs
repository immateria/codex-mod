use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use code_core::config::{
    set_shell_style_profile_mcp_servers,
    set_shell_style_profile_paths,
    set_shell_style_profile_summary,
    set_shell_style_profile_skills,
};
use code_core::config_types::{ShellConfig, ShellScriptStyle, ShellStyleProfileConfig};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::components::form_text_field::FormTextField;
use crate::components::scroll_state::ScrollState;
use crate::native_picker::{pick_path, NativePickerKind};
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};
use crate::util::buffer::{fill_rect, write_line};
use std::cell::Cell;
use unicode_width::UnicodeWidthStr;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_panel::{panel_content_rect, render_panel, PanelFrameStyle};
use super::BottomPane;
use super::SettingsSection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    Style,
    Summary,
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
    Summary,
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
    is_no_filter_option: bool,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorFooterAction {
    Save,
    Generate,
    Pick,
    Show,
    Cancel,
}

pub(crate) struct ShellProfilesSettingsView {
    code_home: PathBuf,
    active_shell_path: Option<String>,
    active_style: Option<ShellScriptStyle>,
    selected_style: ShellScriptStyle,
    shell_style_profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig>,
    available_skills: Vec<SkillOption>,
    available_mcp_servers: Vec<String>,
    summary_field: FormTextField,
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
        let mut summary_field = FormTextField::new_multi_line();
        summary_field.set_placeholder("Describe what this profile does (optional)");

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
            summary_field,
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
        if matches!(self.mode, ViewMode::Main)
            && !self.dirty
            && let Some(active) = self.active_style
            && self.selected_style != active
        {
            self.selected_style = active;
            self.load_fields_for_style(active);
        }
    }

    fn rows() -> [RowKind; 11] {
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
        self.status = None;
    }

    fn open_editor(&mut self, target: ListTarget) {
        let before = match target {
            ListTarget::Summary => self.summary_field.text().to_string(),
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

        let has_no_filter_option = matches!(target, PickTarget::SkillsAllowlist | PickTarget::McpInclude);
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
            PickTarget::SkillsAllowlist => "Skills allowlist",
            PickTarget::DisabledSkills => "Disabled skills",
            PickTarget::McpInclude => "MCP include",
            PickTarget::McpExclude => "MCP exclude",
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

    fn toggle_picker_selection(state: &mut PickListState) -> bool {
        let Some(idx) = state.scroll.selected_idx else {
            return false;
        };

        if idx == 0
            && matches!(state.target, PickTarget::SkillsAllowlist | PickTarget::McpInclude)
            && state
                .items
                .first()
                .is_some_and(|item| item.is_no_filter_option)
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
            && state
                .items
                .first()
                .is_some_and(|item| item.is_no_filter_option)
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

    fn request_summary_generation(&mut self) {
        self.stage_pending_profile_from_fields();
        let profile = self
            .shell_style_profiles
            .get(&self.selected_style)
            .cloned()
            .unwrap_or_default();

        self.status = Some("Generating summary...".to_string());
        self.app_event_tx.send(AppEvent::RequestGenerateShellStyleProfileSummary {
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

    pub(crate) fn set_summary_generation_error(&mut self, _style: ShellScriptStyle, error: String) {
        self.status = Some(format!("Summary generation failed: {error}"));
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
            let (summary, references, skill_roots, skills, disabled_skills, include, exclude) =
                if let Some(profile) = self.shell_style_profiles.get(&style) {
                    (
                        profile.summary.clone(),
                        profile.references.clone(),
                        profile.skill_roots.clone(),
                        profile.skills.clone(),
                        profile.disabled_skills.clone(),
                        profile.mcp_servers.include.clone(),
                        profile.mcp_servers.exclude.clone(),
                    )
                } else {
                    (None, Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new())
                };

            match set_shell_style_profile_paths(&self.code_home, style, &references, &skill_roots) {
                Ok(changed) => changed_any |= changed,
                Err(err) => {
                    self.status = Some(format!("Failed to persist style paths: {err}"));
                    return;
                }
            }

            match set_shell_style_profile_summary(&self.code_home, style, summary.as_deref()) {
                Ok(changed) => changed_any |= changed,
                Err(err) => {
                    self.status = Some(format!("Failed to persist summary: {err}"));
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
        let (summary, references, skill_roots) = if let Some(profile) = self.shell_style_profiles.get(&style) {
            (
                profile.summary.clone().unwrap_or_default(),
                profile.references.clone(),
                profile.skill_roots.clone(),
            )
        } else {
            (String::new(), Vec::new(), Vec::new())
        };

        self.summary_field.set_text(summary.as_str());
        self.references_field.set_text(&format_path_list(&references));
        self.skill_roots_field.set_text(&format_path_list(&skill_roots));
    }

    fn stage_pending_profile_from_fields(&mut self) {
        let references = parse_path_list(self.references_field.text());
        let skill_roots = parse_path_list(self.skill_roots_field.text());
        let summary = {
            let text = self.summary_field.text();
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };

        if references.is_empty()
            && skill_roots.is_empty()
            && summary.is_none()
            && !self.shell_style_profiles.contains_key(&self.selected_style)
        {
            return;
        }

        let profile = self
            .shell_style_profiles
            .entry(self.selected_style)
            .or_default();
        profile.summary = summary;
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
            RowKind::Summary => {
                let summary = self.summary_field.text().trim();
                if summary.is_empty() {
                    Some("unset".to_string())
                } else {
                    let first_line = summary.lines().next().unwrap_or(summary).trim();
                    Some(first_line.to_string())
                }
            }
            RowKind::References => Some(format!("{} paths", parse_path_list(self.references_field.text()).len())),
            RowKind::SkillRoots => Some(format!("{} roots", parse_path_list(self.skill_roots_field.text()).len())),
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
            RowKind::OpenSkills => None,
            RowKind::Apply => None,
            RowKind::Close => None,
        }
    }

    fn row_label(row: RowKind) -> &'static str {
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
            ListTarget::Summary => &mut self.summary_field,
            ListTarget::References => &mut self.references_field,
            ListTarget::SkillRoots => &mut self.skill_roots_field,
        }
    }

    fn editor_footer_line_and_hits(
        &self,
        footer_area: Rect,
        target: ListTarget,
    ) -> (Line<'static>, Vec<(EditorFooterAction, Rect)>) {
        let dim = Style::default().fg(crate::colors::text_dim());
        let key_style = Style::default().fg(crate::colors::function());
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut hits: Vec<(EditorFooterAction, Rect)> = Vec::new();

        let y = footer_area.y;
        let area_end = footer_area.x.saturating_add(footer_area.width);
        let mut cursor_x = footer_area.x;

        let push_sep = |cursor_x: &mut u16, spans: &mut Vec<Span<'static>>| {
            let text = "  •  ";
            spans.push(Span::styled(text.to_string(), dim));
            *cursor_x = cursor_x.saturating_add(UnicodeWidthStr::width(text) as u16);
        };

        let push_action = |key: &str,
                               label: &str,
                               action: EditorFooterAction,
                               cursor_x: &mut u16,
                               spans: &mut Vec<Span<'static>>,
                               hits: &mut Vec<(EditorFooterAction, Rect)>| {
            let start = *cursor_x;
            spans.push(Span::styled(key.to_string(), key_style));
            *cursor_x = cursor_x.saturating_add(UnicodeWidthStr::width(key) as u16);
            spans.push(Span::styled(label.to_string(), dim));
            *cursor_x = cursor_x.saturating_add(UnicodeWidthStr::width(label) as u16);

            let end = (*cursor_x).min(area_end);
            let visible_start = start.max(footer_area.x);
            if end > visible_start {
                hits.push((action, Rect::new(visible_start, y, end - visible_start, 1)));
            }
        };

        if let Some(status) = self.status.as_deref()
            && !status.trim().is_empty()
        {
            let status = status.trim().replace(['\r', '\n'], " ");
            cursor_x = cursor_x.saturating_add(UnicodeWidthStr::width(status.as_str()) as u16);
            spans.push(Span::styled(status, dim));
            push_sep(&mut cursor_x, &mut spans);
        }

        push_action("Ctrl+S", " save", EditorFooterAction::Save, &mut cursor_x, &mut spans, &mut hits);
        match target {
            ListTarget::Summary => {
                push_sep(&mut cursor_x, &mut spans);
                push_action(
                    "Ctrl+G",
                    " generate",
                    EditorFooterAction::Generate,
                    &mut cursor_x,
                    &mut spans,
                    &mut hits,
                );
                push_sep(&mut cursor_x, &mut spans);
                push_action(
                    "Esc",
                    " cancel",
                    EditorFooterAction::Cancel,
                    &mut cursor_x,
                    &mut spans,
                    &mut hits,
                );
            }
            ListTarget::References | ListTarget::SkillRoots => {
                push_sep(&mut cursor_x, &mut spans);
                push_action(
                    "Ctrl+O",
                    " pick",
                    EditorFooterAction::Pick,
                    &mut cursor_x,
                    &mut spans,
                    &mut hits,
                );
                push_sep(&mut cursor_x, &mut spans);
                push_action(
                    "Ctrl+V",
                    " show",
                    EditorFooterAction::Show,
                    &mut cursor_x,
                    &mut spans,
                    &mut hits,
                );
                push_sep(&mut cursor_x, &mut spans);
                push_action(
                    "Esc",
                    " cancel",
                    EditorFooterAction::Cancel,
                    &mut cursor_x,
                    &mut spans,
                    &mut hits,
                );
            }
        }

        hits.retain(|(_action, rect)| rect.x < area_end && rect.width > 0 && rect.y == y);
        (Line::from(spans), hits)
    }

    fn editor_append_picker_path(&mut self, target: ListTarget) {
        let kind = match target {
            ListTarget::Summary => {
                self.status = Some("Picker not available for summary".to_string());
                return;
            }
            ListTarget::References => NativePickerKind::File,
            ListTarget::SkillRoots => NativePickerKind::Folder,
        };
        let title = match target {
            ListTarget::Summary => "Select path",
            ListTarget::References => "Select reference file",
            ListTarget::SkillRoots => "Select skill root folder",
        };

        match pick_path(kind, title) {
            Ok(Some(path)) => {
                let entry = path.to_string_lossy();
                let entry = entry.trim();
                if !entry.is_empty() {
                    let field = self.editor_field_mut(target);
                    let mut next = field.text().to_string();
                    if !next.trim().is_empty() && !next.ends_with('\n') {
                        next.push('\n');
                    }
                    next.push_str(entry);
                    field.set_text(&next);
                    self.status = Some("Added path (not staged)".to_string());
                }
            }
            Ok(None) => {}
            Err(err) => {
                self.status = Some(format!("Native picker failed: {err:#}"));
            }
        }
    }

    fn editor_show_last_path(&mut self, target: ListTarget) {
        let text = self.editor_field_mut(target).text().to_string();
        let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
        let last = lines.next_back();

        match last {
            Some(path) => match crate::native_file_manager::reveal_path(std::path::Path::new(path)) {
                Ok(()) => self.status = Some("Opened in file manager".to_string()),
                Err(err) => self.status = Some(format!("Open failed: {err:#}")),
            },
            None => self.status = Some("No paths to show".to_string()),
        }
    }

    fn activate_selected_row(&mut self) {
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
        let footer_line = Line::from(vec![Span::styled(
            footer_text,
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
            let label_style = base.add_modifier(Modifier::BOLD);
            let line = match self.row_value(row) {
                Some(value) => {
                    let value_style = if is_selected {
                        base.add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .bg(crate::colors::background())
                            .fg(crate::colors::text_dim())
                    };
                    Line::from(vec![
                        Span::styled(format!("{prefix}{label}: "), label_style),
                        Span::styled(value, value_style),
                    ])
                }
                None => Line::from(vec![Span::styled(format!("{prefix}{label}"), label_style)]),
            };
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
            ListTarget::Summary => "Edit summary (optional)",
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

        let (footer, _hits) = self.editor_footer_line_and_hits(footer_area, target);
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
            ListTarget::Summary => self.summary_field.render(field_area, buf, focused),
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
        let selected_count: usize = state
            .items
            .iter()
            .zip(state.checked.iter())
            .filter(|(item, checked)| **checked && !item.is_no_filter_option)
            .count();
        let selection_summary = if matches!(state.target, PickTarget::SkillsAllowlist | PickTarget::McpInclude)
            && selected_count == 0
        {
            "all (no filter)".to_string()
        } else {
            format!("{selected_count} selected")
        };
        let style = self.selected_style.to_string();
        let header_line = Line::from(vec![Span::styled(
            format!("{title}  •  style: {style}  •  {selection_summary}"),
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
                    self.status = None;
                    let rows = Self::rows();
                    let visible = self.viewport_rows.get().max(1);
                    self.scroll.move_up_wrap_visible(rows.len(), visible);
                    true
                }
                KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => {
                    self.status = None;
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
                KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. } => {
                    self.status = None;
                    self.activate_selected_row();
                    true
                }
                KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                    self.status = None;
                    self.activate_selected_row();
                    true
                }
                _ => false,
            },
            ViewMode::EditList { target, before } => match (key.code, key.modifiers) {
                (KeyCode::Esc, _) => {
                    match target {
                        ListTarget::Summary => self.summary_field.set_text(&before),
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
                (KeyCode::Char('g'), mods)
                    if mods.contains(KeyModifiers::CONTROL) && matches!(target, ListTarget::Summary) =>
                {
                    self.request_summary_generation();
                    self.mode = ViewMode::EditList { target, before };
                    true
                }
                (KeyCode::Char('o'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    if matches!(target, ListTarget::References | ListTarget::SkillRoots) {
                        self.editor_append_picker_path(target);
                        self.mode = ViewMode::EditList { target, before };
                        true
                    } else {
                        self.mode = ViewMode::EditList { target, before };
                        false
                    }
                }
                (KeyCode::Char('v'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    if matches!(target, ListTarget::References | ListTarget::SkillRoots) {
                        self.editor_show_last_path(target);
                        self.mode = ViewMode::EditList { target, before };
                        true
                    } else {
                        self.mode = ViewMode::EditList { target, before };
                        false
                    }
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
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if mouse_event.row == footer_area.y
                            && mouse_event.column >= footer_area.x
                            && mouse_event.column < footer_area.x.saturating_add(footer_area.width)
                        {
                            let (_line, hits) = self.editor_footer_line_and_hits(footer_area, target);
                            for (action, rect) in hits {
                                if mouse_event.column >= rect.x
                                    && mouse_event.column < rect.x.saturating_add(rect.width)
                                    && mouse_event.row == rect.y
                                {
                                    match action {
                                        EditorFooterAction::Save => {
                                            self.stage_pending_profile_from_fields();
                                            self.dirty = true;
                                            self.status = Some(
                                                "Changes staged. Select Apply to persist.".to_string(),
                                            );
                                            self.mode = ViewMode::Main;
                                            return true;
                                        }
                                        EditorFooterAction::Generate => {
                                            self.request_summary_generation();
                                            self.mode = ViewMode::EditList { target, before };
                                            return true;
                                        }
                                        EditorFooterAction::Pick => {
                                            self.editor_append_picker_path(target);
                                            self.mode = ViewMode::EditList { target, before };
                                            return true;
                                        }
                                        EditorFooterAction::Show => {
                                            self.editor_show_last_path(target);
                                            self.mode = ViewMode::EditList { target, before };
                                            return true;
                                        }
                                        EditorFooterAction::Cancel => {
                                            match target {
                                                ListTarget::Summary => self.summary_field.set_text(&before),
                                                ListTarget::References => self.references_field.set_text(&before),
                                                ListTarget::SkillRoots => self.skill_roots_field.set_text(&before),
                                            }
                                            self.mode = ViewMode::Main;
                                            return true;
                                        }
                                    }
                                }
                            }
                        }

                        self.editor_field_mut(target).handle_mouse_click(
                            mouse_event.column,
                            mouse_event.row,
                            field_area,
                        )
                    }
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
                        let _ = Self::toggle_picker_selection(&mut state);
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

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
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
    profile
        .summary
        .as_ref()
        .map(|value| value.trim())
        .unwrap_or("")
        .is_empty()
        && profile.references.is_empty()
        && profile.prepend_developer_messages.is_empty()
        && profile.skills.is_empty()
        && profile.disabled_skills.is_empty()
        && profile.skill_roots.is_empty()
        && profile.mcp_servers.include.is_empty()
        && profile.mcp_servers.exclude.is_empty()
        && profile.command_safety == code_core::config_types::CommandSafetyProfileConfig::default()
        && profile.dangerous_command_detection.is_none()
}
