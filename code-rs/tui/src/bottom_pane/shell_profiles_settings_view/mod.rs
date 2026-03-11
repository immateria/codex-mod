use std::any::Any;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use code_core::config::{
    set_shell_style_profile_mcp_servers,
    set_shell_style_profile_paths,
    set_shell_style_profile_skills,
    set_shell_style_profile_summary,
};
use code_core::config_types::{ShellConfig, ShellScriptStyle, ShellStyleProfileConfig};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

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

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::BottomPane;
use super::SettingsSection;

mod editor;
mod main;
mod model;
mod persistence;
mod picker;
mod render;

use model::{
    EditorFooterAction, ListTarget, PickListState, PickTarget, RowKind, SkillOption, ViewMode,
};

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
        let active_style = current_shell.and_then(|shell| {
            shell
                .script_style
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
            persistence::normalize_list_key(&a.name)
                .cmp(&persistence::normalize_list_key(&b.name))
                .then_with(|| a.name.cmp(&b.name))
        });
        let mut seen_skills: HashSet<String> = HashSet::new();
        available_skills
            .retain(|entry| seen_skills.insert(persistence::normalize_list_key(&entry.name)));

        let mut available_mcp_servers: Vec<String> = available_mcp_servers
            .into_iter()
            .map(|name| name.trim().to_string())
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
            shell
                .script_style
                .or_else(|| ShellScriptStyle::infer_from_shell_program(&shell.path))
        });

        if self.active_style == active_style {
            return;
        }

        self.active_style = active_style;

        if matches!(self.mode, ViewMode::Main)
            && !self.dirty
            && let Some(active) = self.active_style
            && self.selected_style != active
        {
            self.selected_style = active;
            self.load_fields_for_style(active);
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
                KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. }
                | KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
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
                    self.mode = ViewMode::Main;
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
                    self.mode = ViewMode::Main;
                    true
                }
                (KeyCode::Char('g'), mods)
                    if mods.contains(KeyModifiers::CONTROL)
                        && matches!(target, ListTarget::Summary) =>
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
                (KeyCode::Esc, _) => {
                    self.mode = ViewMode::Main;
                    true
                }
                (KeyCode::Char('s'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    self.save_picker(&state);
                    self.mode = ViewMode::Main;
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
                (KeyCode::Char(' '), KeyModifiers::NONE)
                | (KeyCode::Enter, KeyModifiers::NONE) => {
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
            return false;
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
            return false;
        }

        let mode = std::mem::replace(&mut self.mode, ViewMode::Main);
        match mode {
            ViewMode::Main => self.handle_mouse_event_main(mouse_event, area),
            ViewMode::EditList { target, before } => {
                let Some(layout) = self.compute_editor_layout(area, target) else {
                    self.mode = ViewMode::EditList { target, before };
                    return false;
                };

                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if let Some(action) =
                            self.editor_footer_action_at(
                                target,
                                mouse_event.column,
                                mouse_event.row,
                                &layout,
                            )
                        {
                            match action {
                                EditorFooterAction::Save => {
                                    self.stage_pending_profile_from_fields();
                                    self.dirty = true;
                                    self.status =
                                        Some("Changes staged. Select Apply to persist.".to_string());
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

                        if layout.page.body.contains(ratatui::layout::Position {
                            x: mouse_event.column,
                            y: mouse_event.row,
                        }) {
                            self.editor_field_mut(target).handle_mouse_click(
                                mouse_event.column,
                                mouse_event.row,
                                layout.sections[0].inner,
                            )
                        } else {
                            false
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        if layout.page.body.contains(ratatui::layout::Position {
                            x: mouse_event.column,
                            y: mouse_event.row,
                        }) {
                            self.editor_field_mut(target).handle_mouse_scroll(true)
                        } else {
                            false
                        }
                    }
                    MouseEventKind::ScrollUp => {
                        if layout.page.body.contains(ratatui::layout::Position {
                            x: mouse_event.column,
                            y: mouse_event.row,
                        }) {
                            self.editor_field_mut(target).handle_mouse_scroll(false)
                        } else {
                            false
                        }
                    }
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
                    let layout = self.compute_picker_layout(area, &state)?;
                    crate::bottom_pane::settings_ui::rows::selection_index_at(
                        layout.body,
                        x,
                        y,
                        scroll_top,
                        total,
                    )
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
    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        self.handle_key_event_with_result(pane, key_event);
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
        self.is_complete()
    }

    fn desired_height(&self, _width: u16) -> u16 {
        14
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main(area, buf),
            ViewMode::EditList { target, .. } => self.render_editor(area, buf, *target),
            ViewMode::PickList(state) => self.render_picker(area, buf, state),
        }
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}
