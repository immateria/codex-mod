use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::prelude::Widget;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use code_core::config::{JsReplRuntimeKindToml, JsReplSettingsToml};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;
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
use std::cell::Cell;
use std::path::PathBuf;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::BottomPane;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextTarget {
    RuntimePath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListTarget {
    RuntimeArgs,
    NodeModuleDirs,
}

#[derive(Debug)]
enum ViewMode {
    Transition,
    Main,
    EditText { target: TextTarget, field: FormTextField },
    EditList { target: ListTarget, field: FormTextField },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    Enabled,
    RuntimeKind,
    RuntimePath,
    PickRuntimePath,
    ClearRuntimePath,
    RuntimeArgs,
    NodeModuleDirs,
    AddNodeModuleDir,
    Apply,
    Close,
}

pub(crate) struct JsReplSettingsView {
    settings: JsReplSettingsToml,
    network_enabled: bool,
    app_event_tx: AppEventSender,
    ticket: BackgroundOrderTicket,
    is_complete: bool,
    dirty: bool,
    mode: ViewMode,
    state: ScrollState,
    viewport_rows: Cell<usize>,
}

impl JsReplSettingsView {
    const DEFAULT_VISIBLE_ROWS: usize = 8;
    const HEADER_HEIGHT: usize = 3; // status + hint/note + blank line
    const EDIT_HEADER_HEIGHT: u16 = 2; // hint line + blank line

    pub(crate) fn new(
        settings: JsReplSettingsToml,
        network_enabled: bool,
        app_event_tx: AppEventSender,
        ticket: BackgroundOrderTicket,
    ) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        Self {
            settings,
            network_enabled,
            app_event_tx,
            ticket,
            is_complete: false,
            dirty: false,
            mode: ViewMode::Main,
            state,
            viewport_rows: Cell::new(0),
        }
    }

    fn runtime_label(kind: JsReplRuntimeKindToml) -> &'static str {
        match kind {
            JsReplRuntimeKindToml::Node => "node",
            JsReplRuntimeKindToml::Deno => "deno",
        }
    }

    fn build_rows(&self) -> Vec<RowKind> {
        let mut rows = vec![
            RowKind::Enabled,
            RowKind::RuntimeKind,
            RowKind::RuntimePath,
            RowKind::PickRuntimePath,
        ];
        if self.settings.runtime_path.is_some() {
            rows.push(RowKind::ClearRuntimePath);
        }

        rows.push(RowKind::RuntimeArgs);
        if matches!(self.settings.runtime, JsReplRuntimeKindToml::Node) {
            rows.push(RowKind::NodeModuleDirs);
            rows.push(RowKind::AddNodeModuleDir);
        }

        rows.push(RowKind::Apply);
        rows.push(RowKind::Close);
        rows
    }

    fn render_header_lines(&self) -> Vec<Line<'static>> {
        let enabled = self.settings.enabled;
        let status_style = if enabled {
            Style::default()
                .fg(crate::colors::success())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(crate::colors::warning())
                .add_modifier(Modifier::BOLD)
        };

        let runtime = Self::runtime_label(self.settings.runtime);
        let runtime_style = Style::default()
            .fg(crate::colors::info())
            .add_modifier(Modifier::BOLD);

        let mediation = if self.network_enabled { "ON" } else { "OFF" };
        let mediation_style = if self.network_enabled {
            Style::default()
                .fg(crate::colors::success())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(crate::colors::text_dim())
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    format!("{} ", if enabled { "ON" } else { "OFF" }),
                    status_style,
                ),
                Span::styled("js_repl", Style::default().fg(crate::colors::text_mid())),
                Span::styled("  |  runtime: ", Style::default().fg(crate::colors::text_dim())),
                Span::styled(runtime.to_string(), runtime_style),
                Span::styled("  |  mediation: ", Style::default().fg(crate::colors::text_dim())),
                Span::styled(mediation.to_string(), mediation_style),
            ]),
        ];

        let node_blocked = self.network_enabled
            && matches!(self.settings.runtime, JsReplRuntimeKindToml::Node)
            && !cfg!(target_os = "macos");
        if node_blocked {
            lines.push(Line::from(vec![Span::styled(
                "Note: Node is not enforceable with mediation on this platform; prefer Deno.",
                Style::default().fg(crate::colors::warning()),
            )]));
        } else {
            lines.push(Line::from(vec![Span::styled(
                "Enter edits. Ctrl+S saves in editors. Esc closes.",
                Style::default().fg(crate::colors::text_dim()),
            )]));
        }

        lines.push(Line::from(""));
        lines
    }

    fn visible_budget(&self, total: usize) -> usize {
        if total == 0 {
            return 0;
        }
        let raw = self.viewport_rows.get();
        let effective = if raw == 0 {
            Self::DEFAULT_VISIBLE_ROWS
        } else {
            raw
        };
        effective.max(1).min(total)
    }

    fn reconcile_selection_state(&mut self, total: usize) {
        if total == 0 {
            self.state.selected_idx = None;
            self.state.scroll_top = 0;
            return;
        }
        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        self.state.clamp_selection(total);
        self.state.scroll_top = self.state.scroll_top.min(total.saturating_sub(1));
        let visible_budget = self.visible_budget(total);
        self.state.ensure_visible(total, visible_budget);
    }

    fn toggle_enabled(&mut self) {
        self.settings.enabled = !self.settings.enabled;
        self.dirty = true;
    }

    fn cycle_runtime(&mut self) {
        self.settings.runtime = match self.settings.runtime {
            JsReplRuntimeKindToml::Node => JsReplRuntimeKindToml::Deno,
            JsReplRuntimeKindToml::Deno => JsReplRuntimeKindToml::Node,
        };
        self.dirty = true;
    }

    fn open_text_editor(&mut self, target: TextTarget) {
        let mut field = FormTextField::new_single_line();
        match target {
            TextTarget::RuntimePath => {
                field.set_placeholder("node (or /path/to/node)");
                field.set_text(
                    self.settings
                        .runtime_path
                        .as_ref()
                        .map(|path| path.to_string_lossy().to_string())
                        .unwrap_or_default()
                        .as_str(),
                );
            }
        }
        self.mode = ViewMode::EditText { target, field };
    }

    fn open_list_editor(&mut self, target: ListTarget) {
        let mut field = FormTextField::new_multi_line();
        match target {
            ListTarget::RuntimeArgs => {
                field.set_placeholder("--flag (one per line)");
                field.set_text(&self.settings.runtime_args.join("\n"));
            }
            ListTarget::NodeModuleDirs => {
                field.set_placeholder("/path/to/node_modules (one per line)");
                let lines = self
                    .settings
                    .node_module_dirs
                    .iter()
                    .map(|path| path.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                field.set_text(&lines);
            }
        }
        self.mode = ViewMode::EditList { target, field };
    }

    fn save_text_editor(&mut self, target: TextTarget, field: &FormTextField) -> Result<(), String> {
        match target {
            TextTarget::RuntimePath => {
                let raw = field.text().trim();
                if raw.is_empty() {
                    self.settings.runtime_path = None;
                } else {
                    self.settings.runtime_path = Some(PathBuf::from(raw));
                }
            }
        }
        self.dirty = true;
        Ok(())
    }

    fn save_list_editor(&mut self, target: ListTarget, field: &FormTextField) -> Result<(), String> {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut lines: Vec<String> = Vec::new();
        for line in field.text().lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if seen.insert(trimmed.to_string()) {
                lines.push(trimmed.to_string());
            }
        }

        match target {
            ListTarget::RuntimeArgs => {
                self.settings.runtime_args = lines;
            }
            ListTarget::NodeModuleDirs => {
                self.settings.node_module_dirs = lines.into_iter().map(PathBuf::from).collect();
            }
        }
        self.dirty = true;
        Ok(())
    }

    fn pick_runtime_path(&mut self) {
        let result = pick_path(NativePickerKind::File, "Select js_repl runtime executable");
        match result {
            Ok(Some(path)) => {
                self.settings.runtime_path = Some(path);
                self.dirty = true;
            }
            Ok(None) => {}
            Err(err) => {
                self.app_event_tx.send_background_event_with_ticket(
                    &self.ticket,
                    format!("JS REPL picker failed: {err:#}"),
                );
            }
        }
    }

    fn clear_runtime_path(&mut self) {
        self.settings.runtime_path = None;
        self.dirty = true;
    }

    fn add_node_module_dir(&mut self) {
        let result = pick_path(NativePickerKind::Folder, "Select node_modules folder");
        match result {
            Ok(Some(path)) => {
                let rendered = path.to_string_lossy().to_string();
                if !self
                    .settings
                    .node_module_dirs
                    .iter()
                    .any(|existing| existing.to_string_lossy() == rendered)
                {
                    self.settings.node_module_dirs.push(path);
                    self.dirty = true;
                }
            }
            Ok(None) => {}
            Err(err) => {
                self.app_event_tx.send_background_event_with_ticket(
                    &self.ticket,
                    format!("JS REPL picker failed: {err:#}"),
                );
            }
        }
    }

    fn apply_settings(&mut self) {
        self.app_event_tx
            .send(AppEvent::SetJsReplSettings(self.settings.clone()));
        self.app_event_tx.send_background_event_with_ticket(
            &self.ticket,
            "JS REPL: applying…".to_string(),
        );
        self.dirty = false;
    }

    fn activate_row(&mut self, kind: RowKind) {
        match kind {
            RowKind::Enabled => self.toggle_enabled(),
            RowKind::RuntimeKind => self.cycle_runtime(),
            RowKind::RuntimePath => self.open_text_editor(TextTarget::RuntimePath),
            RowKind::PickRuntimePath => self.pick_runtime_path(),
            RowKind::ClearRuntimePath => self.clear_runtime_path(),
            RowKind::RuntimeArgs => self.open_list_editor(ListTarget::RuntimeArgs),
            RowKind::NodeModuleDirs => self.open_list_editor(ListTarget::NodeModuleDirs),
            RowKind::AddNodeModuleDir => self.add_node_module_dir(),
            RowKind::Apply => self.apply_settings(),
            RowKind::Close => self.is_complete = true,
        }
    }

    fn selection_index_at(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        if area.width == 0 || area.height == 0 {
            return None;
        }
        let inner = Block::default().borders(Borders::ALL).inner(area);
        if inner.width == 0 || inner.height == 0 {
            return None;
        }
        if x < inner.x
            || x >= inner.x.saturating_add(inner.width)
            || y < inner.y
            || y >= inner.y.saturating_add(inner.height)
        {
            return None;
        }

        let available_height = inner.height as usize;
        let header_height = Self::HEADER_HEIGHT.min(available_height);
        let list_height = available_height.saturating_sub(header_height);
        if list_height == 0 {
            return None;
        }

        let rel_y = y.saturating_sub(inner.y) as usize;
        if rel_y < header_height || rel_y >= header_height + list_height {
            return None;
        }
        let line_offset = rel_y - header_height;

        let scroll_top = self.state.scroll_top;
        Some(scroll_top.saturating_add(line_offset))
    }

    fn edit_textarea_rect(area: Rect) -> Rect {
        let inner = Block::default().borders(Borders::ALL).inner(area);
        Rect {
            x: inner.x,
            y: inner.y.saturating_add(Self::EDIT_HEADER_HEIGHT),
            width: inner.width,
            height: inner.height.saturating_sub(Self::EDIT_HEADER_HEIGHT),
        }
    }

    fn list_visible_slots_for_area(&self, area: Rect) -> usize {
        let inner = Block::default().borders(Borders::ALL).inner(area);
        if inner.width == 0 || inner.height == 0 {
            return 0;
        }
        let available_height = inner.height as usize;
        let header_height = Self::HEADER_HEIGHT.min(available_height);
        available_height.saturating_sub(header_height).max(1)
    }

    fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main => {
                let rows = self.build_rows();
                let total = rows.len();
                if total == 0 {
                    if matches!(key_event.code, KeyCode::Esc) {
                        self.is_complete = true;
                        self.mode = ViewMode::Main;
                        return true;
                    }
                    self.mode = ViewMode::Main;
                    return false;
                }

                self.reconcile_selection_state(total);
                let selected = self.state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));

                let handled = match key_event.code {
                    KeyCode::Esc => {
                        self.is_complete = true;
                        true
                    }
                    KeyCode::Enter => {
                        if let Some(kind) = rows.get(selected).copied() {
                            self.activate_row(kind);
                            true
                        } else {
                            false
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.state.move_up_wrap_visible(total, self.visible_budget(total));
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.state.move_down_wrap_visible(total, self.visible_budget(total));
                        true
                    }
                    KeyCode::Home => {
                        self.state.selected_idx = Some(0);
                        self.state.scroll_top = 0;
                        true
                    }
                    KeyCode::End => {
                        if total > 0 {
                            self.state.selected_idx = Some(total - 1);
                            self.state.ensure_visible(total, self.visible_budget(total));
                        }
                        true
                    }
                    _ => false,
                };

                self.mode = ViewMode::Main;
                handled
            }
            ViewMode::EditText { target, mut field } => {
                match key_event {
                    KeyEvent {
                        code: KeyCode::Char('s'),
                        modifiers,
                        ..
                    } if modifiers.contains(KeyModifiers::CONTROL) => {
                        match self.save_text_editor(target, &field) {
                            Ok(()) => {
                                self.mode = ViewMode::Main;
                                true
                            }
                            Err(err) => {
                                self.app_event_tx.send_background_event_with_ticket(
                                    &self.ticket,
                                    format!("JS REPL: {err}"),
                                );
                                self.mode = ViewMode::EditText { target, field };
                                true
                            }
                        }
                    }
                    KeyEvent { code: KeyCode::Esc, .. } => {
                        self.mode = ViewMode::Main;
                        true
                    }
                    _ => {
                        let handled = field.handle_key(key_event);
                        self.mode = ViewMode::EditText { target, field };
                        handled
                    }
                }
            }
            ViewMode::EditList { target, mut field } => {
                match key_event {
                    KeyEvent {
                        code: KeyCode::Char('s'),
                        modifiers,
                        ..
                    } if modifiers.contains(KeyModifiers::CONTROL) => {
                        match self.save_list_editor(target, &field) {
                            Ok(()) => {
                                self.mode = ViewMode::Main;
                                true
                            }
                            Err(err) => {
                                self.app_event_tx.send_background_event_with_ticket(
                                    &self.ticket,
                                    format!("JS REPL: {err}"),
                                );
                                self.mode = ViewMode::EditList { target, field };
                                true
                            }
                        }
                    }
                    KeyEvent { code: KeyCode::Esc, .. } => {
                        self.mode = ViewMode::Main;
                        true
                    }
                    _ => {
                        let handled = field.handle_key(key_event);
                        self.mode = ViewMode::EditList { target, field };
                        handled
                    }
                }
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.process_key_event(key_event)
    }

    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        match &mut self.mode {
            ViewMode::EditText { field, .. } | ViewMode::EditList { field, .. } => {
                field.handle_paste(text);
                true
            }
            ViewMode::Main | ViewMode::Transition => false,
        }
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        match mode {
            ViewMode::Main => {
                let rows = self.build_rows();
                let total = rows.len();
                if total == 0 {
                    self.mode = ViewMode::Main;
                    return false;
                }

                let visible_slots = self.list_visible_slots_for_area(area);
                self.viewport_rows.set(visible_slots);

                self.reconcile_selection_state(total);
                let mut selected = self.state.selected_idx.unwrap_or(0);
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
                self.state.selected_idx = Some(selected);
                self.state.ensure_visible(total, visible_slots.min(total));

                if matches!(result, SelectableListMouseResult::Activated)
                    && let Some(kind) = rows.get(selected).copied()
                {
                    self.activate_row(kind);
                }

                self.mode = ViewMode::Main;
                result.handled()
            }
            ViewMode::EditText { target, mut field } => {
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let textarea = Self::edit_textarea_rect(area);
                        field.handle_mouse_click(mouse_event.column, mouse_event.row, textarea)
                    }
                    MouseEventKind::ScrollDown => field.handle_mouse_scroll(true),
                    MouseEventKind::ScrollUp => field.handle_mouse_scroll(false),
                    _ => false,
                };
                self.mode = ViewMode::EditText { target, field };
                handled
            }
            ViewMode::EditList { target, mut field } => {
                let handled = match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let textarea = Self::edit_textarea_rect(area);
                        field.handle_mouse_click(mouse_event.column, mouse_event.row, textarea)
                    }
                    MouseEventKind::ScrollDown => field.handle_mouse_scroll(true),
                    MouseEventKind::ScrollUp => field.handle_mouse_scroll(false),
                    _ => false,
                };
                self.mode = ViewMode::EditList { target, field };
                handled
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn render_main(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" JS REPL ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let row_style = |selected: bool| {
            if selected {
                Style::default()
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            }
        };
        let arrow_style = |selected: bool| {
            if selected {
                Style::default().fg(crate::colors::primary())
            } else {
                Style::default().fg(crate::colors::text_dim())
            }
        };

        let header_lines = self.render_header_lines();
        let available_height = inner.height as usize;
        let header_height = header_lines.len().min(available_height);
        let list_height = available_height.saturating_sub(header_height);
        let visible_slots = list_height.max(1);
        self.viewport_rows.set(visible_slots);

        let rows = self.build_rows();
        let total = rows.len();
        let selected_idx = self
            .state
            .selected_idx
            .unwrap_or(0)
            .min(total.saturating_sub(1));
        let scroll_top = self.state.scroll_top.min(total.saturating_sub(1));

        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.extend(header_lines);

        let enabled_label = if self.settings.enabled { "Enabled" } else { "Disabled" };
        let enabled_color = if self.settings.enabled {
            crate::colors::success()
        } else {
            crate::colors::warning()
        };
        let runtime_label = Self::runtime_label(self.settings.runtime);
        let runtime_path = self
            .settings
            .runtime_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| "auto (PATH)".to_string());
        let runtime_args = if self.settings.runtime_args.is_empty() {
            "(none)".to_string()
        } else {
            format!("{} entries", self.settings.runtime_args.len())
        };
        let module_dirs = if self.settings.node_module_dirs.is_empty() {
            "(none)".to_string()
        } else {
            format!("{} entries", self.settings.node_module_dirs.len())
        };
        let apply_suffix = if self.dirty { " *" } else { "" };

        let mut remaining = visible_slots;
        let mut row_index = scroll_top;
        while remaining > 0 && row_index < rows.len() {
            let kind = rows[row_index];
            let selected = row_index == selected_idx;
            let arrow = if selected { "› " } else { "  " };
            let mut spans = vec![Span::styled(arrow, arrow_style(selected))];
            match kind {
                RowKind::Enabled => {
                    spans.push(Span::raw("Enabled: "));
                    spans.push(Span::styled(
                        enabled_label,
                        Style::default()
                            .fg(enabled_color)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                RowKind::RuntimeKind => {
                    spans.push(Span::raw("Runtime: "));
                    spans.push(Span::styled(
                        runtime_label.to_string(),
                        Style::default().fg(crate::colors::info()),
                    ));
                }
                RowKind::RuntimePath => {
                    spans.push(Span::raw("Runtime path: "));
                    spans.push(Span::styled(
                        runtime_path.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
                RowKind::PickRuntimePath => {
                    spans.push(Span::raw("Pick runtime path (file picker)"));
                }
                RowKind::ClearRuntimePath => {
                    spans.push(Span::raw("Clear runtime path (use PATH)"));
                }
                RowKind::RuntimeArgs => {
                    spans.push(Span::raw("Runtime args: "));
                    spans.push(Span::styled(
                        runtime_args.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
                RowKind::NodeModuleDirs => {
                    spans.push(Span::raw("Node module dirs: "));
                    spans.push(Span::styled(
                        module_dirs.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
                RowKind::AddNodeModuleDir => {
                    spans.push(Span::raw("Add node module dir (folder picker)"));
                }
                RowKind::Apply => {
                    spans.push(Span::raw("Apply changes"));
                    spans.push(Span::styled(
                        apply_suffix,
                        Style::default().fg(crate::colors::warning()),
                    ));
                }
                RowKind::Close => spans.push(Span::raw("Close")),
            }
            lines.push(Line::from(spans).style(row_style(selected)));
            remaining = remaining.saturating_sub(1);
            row_index += 1;
        }

        Paragraph::new(lines).render(inner, buf);
    }

    fn render_edit(
        &self,
        area: Rect,
        buf: &mut Buffer,
        title: &'static str,
        field: &FormTextField,
        hint: &'static str,
    ) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(title)
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let header = vec![
            Line::from(vec![Span::styled(
                hint,
                Style::default().fg(crate::colors::text_dim()),
            )]),
            Line::from(""),
        ];
        Paragraph::new(header).render(inner, buf);

        let textarea = Self::edit_textarea_rect(area);
        if textarea.width == 0 || textarea.height == 0 {
            return;
        }
        field.render(textarea, buf, true);
    }
}

impl<'a> BottomPaneView<'a> for JsReplSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.process_key_event(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.process_key_event(key_event))
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
        match &self.mode {
            ViewMode::Main => {
                let header = self.render_header_lines().len() as u16;
                let total_rows = self.build_rows().len();
                let visible = total_rows.clamp(1, 12) as u16;
                2 + header + visible
            }
            ViewMode::EditText { .. } | ViewMode::EditList { .. } => 18,
            ViewMode::Transition => 2 + self.render_header_lines().len() as u16 + 8,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main => self.render_main(area, buf),
            ViewMode::EditText { target, field } => {
                let title = match target {
                    TextTarget::RuntimePath => " JS REPL: Runtime Path ",
                };
                self.render_edit(area, buf, title, field, "Ctrl+S to save. Esc to cancel.");
            }
            ViewMode::EditList { target, field } => {
                let title = match target {
                    ListTarget::RuntimeArgs => " JS REPL: Runtime Args ",
                    ListTarget::NodeModuleDirs => " JS REPL: Node Module Dirs ",
                };
                self.render_edit(
                    area,
                    buf,
                    title,
                    field,
                    "One entry per line. Ctrl+S to save. Esc to cancel.",
                );
            }
            ViewMode::Transition => self.render_main(area, buf),
        }
    }
}
