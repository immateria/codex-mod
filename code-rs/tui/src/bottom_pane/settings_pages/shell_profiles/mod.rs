use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use code_core::config::{
    set_shell_style_profile_mcp_servers,
    set_shell_style_profile_paths,
    set_shell_style_profile_skills,
    set_shell_style_profile_summary,
};
use code_core::config_types::{ShellConfig, ShellScriptStyle, ShellStyleProfileConfig, ShellStyleProfileEntry};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::components::form_text_field::FormTextField;
use crate::components::mode_guard::ModeGuard;
use crate::components::scroll_state::ScrollState;
use crate::native_picker::{pick_path, NativePickerKind};
use crate::ui_interaction::{
    SelectableListMouseResult,
    SETTINGS_LIST_MOUSE_CONFIG,
};
use crate::util::buffer::{fill_bg, fill_rect, write_line};

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::SettingsSection;

mod editor;
mod input;
mod main;
mod model;
mod mouse;
mod pane_impl;
mod persistence;
mod picker;
mod render;

use model::{
    EditorFooterAction, ListTarget, PickListState, PickTarget, RowKind, SkillOption, ViewMode,
};

pub(crate) struct ShellProfilesSettingsView {
    code_home: PathBuf,
    active_shell_path: Option<String>,
    active_profile_id: Option<String>,
    selected_id: String,
    shell_style_profiles: HashMap<String, ShellStyleProfileEntry>,
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

crate::bottom_pane::chrome_view::impl_chrome_view!(ShellProfilesSettingsView, framed);

impl ShellProfilesSettingsView {
    pub(crate) fn new(
        code_home: PathBuf,
        current_shell: Option<&ShellConfig>,
        shell_style_profiles: HashMap<String, ShellStyleProfileEntry>,
        available_skills: Vec<(String, String)>,
        available_mcp_servers: Vec<String>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let active_shell_path = current_shell.map(|shell| shell.path.clone());
        let active_style = current_shell.and_then(|shell| {
            shell
                .script_style
                .or_else(|| ShellScriptStyle::infer_from_shell_program(&shell.path))
        });
        let active_profile_id = active_style.map(|s| s.to_string());
        let selected_id = active_profile_id
            .clone()
            .unwrap_or_else(|| ShellScriptStyle::BashZshCompatible.to_string());

        let mut references_field = FormTextField::new_multi_line();
        references_field.set_placeholder("docs/shell/my-style.md");
        let mut skill_roots_field = FormTextField::new_multi_line();
        skill_roots_field.set_placeholder("skills/my-style");
        let mut summary_field = FormTextField::new_multi_line();
        summary_field.set_placeholder("Describe what this profile does (optional)");

        let mut available_skills: Vec<SkillOption> = available_skills
            .into_iter()
            .map(|(name, description)| SkillOption {
                name: name.trim().to_owned(),
                description: {
                    let d = description.trim();
                    if d.is_empty() {
                        None
                    } else {
                        Some(d.to_owned())
                    }
                },
            })
            .filter(|entry| !entry.name.is_empty())
            .collect();
        available_skills.sort_by(|a, b| {
            persistence::normalize_list_key(&a.name)
                .cmp(&persistence::normalize_list_key(&b.name))
                .then_with(|| a.name.cmp(&b.name))
        });
        let mut seen_skills: HashSet<String> = HashSet::new();
        available_skills
            .retain(|entry| seen_skills.insert(persistence::normalize_list_key(&entry.name)));

        let mut available_mcp_servers: Vec<String> = available_mcp_servers
            .into_iter()
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .collect();
        available_mcp_servers.sort_by(|a, b| {
            persistence::normalize_list_key(a)
                .cmp(&persistence::normalize_list_key(b))
                .then_with(|| a.cmp(b))
        });
        let mut seen_servers: HashSet<String> = HashSet::new();
        available_mcp_servers
            .retain(|name| seen_servers.insert(persistence::normalize_list_key(name)));

        let mut view = Self {
            code_home,
            active_shell_path,
            active_profile_id,
            selected_id: selected_id.clone(),
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
            viewport_rows: Cell::new(1),
            pick_viewport_rows: Cell::new(1),
        };
        view.scroll.selected_idx = Some(0);
        view.load_fields_for_style(&selected_id);
        view
    }

    pub(crate) fn set_current_shell(&mut self, current_shell: Option<&ShellConfig>) {
        self.active_shell_path = current_shell.map(|shell| shell.path.clone());
        let active_style = current_shell.and_then(|shell| {
            shell
                .script_style
                .or_else(|| ShellScriptStyle::infer_from_shell_program(&shell.path))
        });
        let active_profile_id = active_style.map(|s| s.to_string());

        if self.active_profile_id == active_profile_id {
            return;
        }

        self.active_profile_id = active_profile_id.clone();

        if matches!(self.mode, ViewMode::Main)
            && !self.dirty
            && let Some(ref active_id) = self.active_profile_id
            && self.selected_id != *active_id
        {
            let id = active_id.clone();
            self.selected_id = id.clone();
            self.load_fields_for_style(&id);
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key: KeyEvent) -> bool {
        input::handle_key_event_direct(self, key)
    }

    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        input::handle_paste_direct(self, text)
    }

    fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        render::render_in_chrome(self, ChromeMode::ContentOnly, area, buf);
    }

    fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        render::render_in_chrome(self, ChromeMode::Framed, area, buf);
    }

    fn handle_mouse_event_direct_content_only(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.handle_mouse_event_direct_in_chrome(mouse_event, area, ChromeMode::ContentOnly)
    }

    fn handle_mouse_event_direct_in_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
        chrome: ChromeMode,
    ) -> bool {
        mouse::handle_mouse_event_direct_in_chrome(self, chrome, mouse_event, area)
    }

    fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_in_chrome(mouse_event, area, ChromeMode::Framed)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn has_back_navigation(&self) -> bool {
        !matches!(self.mode, ViewMode::Main)
    }
}
