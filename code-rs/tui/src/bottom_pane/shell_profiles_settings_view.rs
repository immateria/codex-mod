use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use code_core::config::set_shell_style_profile_mcp_servers;
use code_core::config::set_shell_style_profile_paths;
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
use std::collections::HashMap;
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
    McpInclude,
    McpExclude,
}

#[derive(Debug)]
enum ViewMode {
    Main,
    EditList { target: ListTarget, before: String },
}

pub(crate) struct ShellProfilesSettingsView {
    code_home: PathBuf,
    active_style: Option<ShellScriptStyle>,
    selected_style: ShellScriptStyle,
    shell_style_profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig>,
    references_field: FormTextField,
    skill_roots_field: FormTextField,
    mcp_include_field: FormTextField,
    mcp_exclude_field: FormTextField,
    app_event_tx: AppEventSender,
    is_complete: bool,
    dirty: bool,
    status: Option<String>,
    mode: ViewMode,
    scroll: ScrollState,
    viewport_rows: Cell<usize>,
}

impl ShellProfilesSettingsView {
    pub(crate) fn new(
        code_home: PathBuf,
        current_shell: Option<&ShellConfig>,
        shell_style_profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig>,
        app_event_tx: AppEventSender,
    ) -> Self {
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
        let mut mcp_include_field = FormTextField::new_multi_line();
        mcp_include_field.set_placeholder("brave\ncode");
        let mut mcp_exclude_field = FormTextField::new_multi_line();
        mcp_exclude_field.set_placeholder("github");

        let mut view = Self {
            code_home,
            active_style,
            selected_style,
            shell_style_profiles,
            references_field,
            skill_roots_field,
            mcp_include_field,
            mcp_exclude_field,
            app_event_tx,
            is_complete: false,
            dirty: false,
            status: None,
            mode: ViewMode::Main,
            scroll: ScrollState::new(),
            viewport_rows: Cell::new(0),
        };
        view.scroll.selected_idx = Some(0);
        view.load_fields_for_style(selected_style);
        view
    }

    fn rows() -> [RowKind; 8] {
        [
            RowKind::Style,
            RowKind::References,
            RowKind::SkillRoots,
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
            ListTarget::McpInclude => self.mcp_include_field.text().to_string(),
            ListTarget::McpExclude => self.mcp_exclude_field.text().to_string(),
        };
        self.mode = ViewMode::EditList { target, before };
    }

    fn cancel_editor(&mut self, target: ListTarget, before: String) {
        match target {
            ListTarget::References => self.references_field.set_text(&before),
            ListTarget::SkillRoots => self.skill_roots_field.set_text(&before),
            ListTarget::McpInclude => self.mcp_include_field.set_text(&before),
            ListTarget::McpExclude => self.mcp_exclude_field.set_text(&before),
        }
        self.mode = ViewMode::Main;
    }

    fn save_editor(&mut self) {
        self.stage_pending_profile_from_fields();
        self.dirty = true;
        self.status = Some("Changes staged. Select Apply to persist.".to_string());
        self.mode = ViewMode::Main;
    }

    fn open_skills_editor(&mut self) {
        self.app_event_tx.send(AppEvent::OpenSettings {
            section: Some(SettingsSection::Skills),
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
            let (references, skill_roots, include, exclude) =
                if let Some(profile) = self.shell_style_profiles.get(&style) {
                    (
                        profile.references.clone(),
                        profile.skill_roots.clone(),
                        profile.mcp_servers.include.clone(),
                        profile.mcp_servers.exclude.clone(),
                    )
                } else {
                    (Vec::new(), Vec::new(), Vec::new(), Vec::new())
                };

            match set_shell_style_profile_paths(&self.code_home, style, &references, &skill_roots) {
                Ok(changed) => changed_any |= changed,
                Err(err) => {
                    self.status = Some(format!("Failed to persist style paths: {err}"));
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
        let (references, skill_roots, mcp_include, mcp_exclude) =
            if let Some(profile) = self.shell_style_profiles.get(&style) {
                (
                    profile.references.clone(),
                    profile.skill_roots.clone(),
                    profile.mcp_servers.include.clone(),
                    profile.mcp_servers.exclude.clone(),
                )
            } else {
                (Vec::new(), Vec::new(), Vec::new(), Vec::new())
            };

        self.references_field.set_text(&format_path_list(&references));
        self.skill_roots_field.set_text(&format_path_list(&skill_roots));
        self.mcp_include_field.set_text(&format_string_list(&mcp_include));
        self.mcp_exclude_field.set_text(&format_string_list(&mcp_exclude));
    }

    fn stage_pending_profile_from_fields(&mut self) {
        let references = parse_path_list(self.references_field.text());
        let skill_roots = parse_path_list(self.skill_roots_field.text());
        let include = parse_string_list(self.mcp_include_field.text());
        let exclude = parse_string_list(self.mcp_exclude_field.text());

        if references.is_empty() && skill_roots.is_empty() && include.is_empty() && exclude.is_empty()
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
        profile.mcp_servers.include = include;
        profile.mcp_servers.exclude = exclude;
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
            RowKind::McpInclude => Some(format!("{} servers", parse_string_list(self.mcp_include_field.text()).len())),
            RowKind::McpExclude => Some(format!("{} servers", parse_string_list(self.mcp_exclude_field.text()).len())),
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
            RowKind::McpInclude => "MCP include",
            RowKind::McpExclude => "MCP exclude",
            RowKind::OpenSkills => "Edit skills",
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
            ListTarget::McpInclude => &mut self.mcp_include_field,
            ListTarget::McpExclude => &mut self.mcp_exclude_field,
        }
    }

    fn activate_selected_row(&mut self) {
        match self.selected_row() {
            RowKind::Style => self.cycle_style_next(),
            RowKind::References => self.open_editor(ListTarget::References),
            RowKind::SkillRoots => self.open_editor(ListTarget::SkillRoots),
            RowKind::McpInclude => self.open_editor(ListTarget::McpInclude),
            RowKind::McpExclude => self.open_editor(ListTarget::McpExclude),
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

        let header = if self.dirty {
            "Shell profiles (unsaved changes)"
        } else {
            "Shell profiles"
        };
        let header_line = Line::from(vec![Span::styled(
            header,
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

        self.render_list(list_area, buf);

        let footer_text = self
            .status
            .as_deref()
            .unwrap_or("Enter: edit/apply  •  Esc: close");
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
            ListTarget::McpInclude => "Edit MCP include list (one server per line)",
            ListTarget::McpExclude => "Edit MCP exclude list (one server per line)",
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
            ListTarget::McpInclude => self.mcp_include_field.render(field_area, buf, focused),
            ListTarget::McpExclude => self.mcp_exclude_field.render(field_area, buf, focused),
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key: KeyEvent) -> bool {
        if self.is_complete {
            return true;
        }

        let is_edit = matches!(self.mode, ViewMode::EditList { .. });
        if !is_edit {
            return match key {
                KeyEvent { code: KeyCode::Esc, .. } => {
                    self.is_complete = true;
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
            };
        }

        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                let ViewMode::EditList { target, before } = &mut self.mode else {
                    return false;
                };
                let target = *target;
                let before = std::mem::take(before);
                self.cancel_editor(target, before);
                true
            }
            (KeyCode::Char('s'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                self.save_editor();
                true
            }
            _ => {
                let target = match &self.mode {
                    ViewMode::EditList { target, .. } => *target,
                    ViewMode::Main => return false,
                };
                self.editor_field_mut(target).handle_key(key)
            }
        }
    }

    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        if self.is_complete {
            return true;
        }

        let target = match &self.mode {
            ViewMode::Main => return false,
            ViewMode::EditList { target, .. } => *target,
        };
        self.editor_field_mut(target).handle_paste(text);
        true
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        if self.is_complete {
            return true;
        }

        let target = match &self.mode {
            ViewMode::Main => return self.handle_mouse_event_main(mouse_event, area),
            ViewMode::EditList { target, .. } => *target,
        };

        let content = Self::content_area(area);
        if content.width == 0 || content.height < 4 {
            return false;
        }
        let field_area = Rect::new(
            content.x,
            content.y.saturating_add(1),
            content.width,
            content.height.saturating_sub(2),
        );
        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => self.editor_field_mut(target).handle_mouse_click(
                mouse_event.column,
                mouse_event.row,
                field_area,
            ),
            MouseEventKind::ScrollDown => self.editor_field_mut(target).handle_mouse_scroll(true),
            MouseEventKind::ScrollUp => self.editor_field_mut(target).handle_mouse_scroll(false),
            _ => false,
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

fn format_string_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_path_list(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn parse_string_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
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
