use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::app_event::AppEvent;
use crate::components::form_text_field::FormTextField;

use code_core::config_types::{McpDispatchMode, McpServerSchedulingToml, McpToolSchedulingOverrideToml};

use super::{McpSettingsMode, McpSettingsView};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServerRow {
    Dispatch,
    MaxConcurrent,
    MinInterval,
    QueueTimeout,
    MaxQueueDepth,
    Save,
    Cancel,
}

const SERVER_ROWS: [ServerRow; 7] = [
    ServerRow::Dispatch,
    ServerRow::MaxConcurrent,
    ServerRow::MinInterval,
    ServerRow::QueueTimeout,
    ServerRow::MaxQueueDepth,
    ServerRow::Save,
    ServerRow::Cancel,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolRow {
    MinInterval,
    MaxConcurrent,
    ClearOverride,
    Save,
    Cancel,
}

const TOOL_ROWS: [ToolRow; 5] = [
    ToolRow::MinInterval,
    ToolRow::MaxConcurrent,
    ToolRow::ClearOverride,
    ToolRow::Save,
    ToolRow::Cancel,
];

fn format_secs_compact(duration: Duration) -> String {
    let value = format_secs_for_edit(duration);
    format!("{value}s")
}

fn format_opt_secs_compact(duration: Option<Duration>) -> String {
    duration
        .map(format_secs_compact)
        .unwrap_or_else(|| "none".to_string())
}

fn format_secs_for_edit(duration: Duration) -> String {
    let secs = duration.as_secs_f64();
    if secs.fract() == 0.0 {
        format!("{}", duration.as_secs())
    } else {
        // Keep enough precision for sub-second limits without being noisy.
        format!("{secs:.3}").trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn parse_secs_field(label: &str, text: &str) -> Result<Option<Duration>, String> {
    let trimmed = text.trim();
    let trimmed = trimmed
        .strip_suffix('s')
        .or_else(|| trimmed.strip_suffix('S'))
        .unwrap_or(trimmed);
    if trimmed.is_empty() {
        return Ok(None);
    }
    let value: f64 = trimmed
        .parse()
        .map_err(|_| format!("{label} must be a number (seconds)"))?;
    if value < 0.0 {
        return Err(format!("{label} must be >= 0"));
    }
    Ok(Some(Duration::from_secs_f64(value)))
}

fn parse_u32_field(label: &str, text: &str) -> Result<u32, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} is required"));
    }
    let value: u32 = trimmed
        .parse()
        .map_err(|_| format!("{label} must be an integer"))?;
    if value == 0 {
        return Err(format!("{label} must be >= 1"));
    }
    Ok(value)
}

fn parse_optional_u32_field(label: &str, text: &str) -> Result<Option<u32>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let value: u32 = trimmed
        .parse()
        .map_err(|_| format!("{label} must be an integer"))?;
    if value == 0 {
        return Err(format!("{label} must be >= 1"));
    }
    Ok(Some(value))
}

fn centered_overlay_rect(area: Rect, max_w: u16, max_h: u16) -> Rect {
    let w = max_w.min(area.width.saturating_sub(2)).max(24);
    let h = max_h.min(area.height.saturating_sub(2)).max(8);
    let x = area.x.saturating_add((area.width.saturating_sub(w)) / 2);
    let y = area.y.saturating_add((area.height.saturating_sub(h)) / 2);
    Rect { x, y, width: w, height: h }
}

#[derive(Debug)]
pub(super) struct ServerSchedulingEditor {
    pub(super) server: String,
    pub(super) scheduling: McpServerSchedulingToml,
    selected_row: usize,
    editing: Option<ServerRow>,
    pub(super) error: Option<String>,
    max_concurrent_field: FormTextField,
    min_interval_field: FormTextField,
    queue_timeout_field: FormTextField,
    max_queue_depth_field: FormTextField,
}

impl ServerSchedulingEditor {
    pub(super) fn new(server: &str, scheduling: McpServerSchedulingToml) -> Self {
        let mut max_concurrent_field = FormTextField::new_single_line();
        max_concurrent_field.set_text(&scheduling.max_concurrent.to_string());

        let mut min_interval_field = FormTextField::new_single_line();
        min_interval_field.set_placeholder("none");
        if let Some(value) = scheduling.min_interval_sec {
            min_interval_field.set_text(&format_secs_for_edit(value));
        }

        let mut queue_timeout_field = FormTextField::new_single_line();
        queue_timeout_field.set_placeholder("none");
        if let Some(value) = scheduling.queue_timeout_sec {
            queue_timeout_field.set_text(&format_secs_for_edit(value));
        }

        let mut max_queue_depth_field = FormTextField::new_single_line();
        max_queue_depth_field.set_placeholder("none");
        if let Some(value) = scheduling.max_queue_depth {
            max_queue_depth_field.set_text(&value.to_string());
        }

        Self {
            server: server.to_string(),
            scheduling,
            selected_row: 0,
            editing: None,
            error: None,
            max_concurrent_field,
            min_interval_field,
            queue_timeout_field,
            max_queue_depth_field,
        }
    }

    fn selected(&self) -> ServerRow {
        debug_assert!(self.selected_row < SERVER_ROWS.len());
        SERVER_ROWS
            .get(self.selected_row)
            .copied()
            .unwrap_or(ServerRow::Dispatch)
    }

    fn set_selected_row(&mut self, idx: usize) {
        self.selected_row = idx.min(SERVER_ROWS.len().saturating_sub(1));
    }

    fn move_selected(&mut self, delta: isize) {
        let len = SERVER_ROWS.len();
        if len == 0 {
            return;
        }
        let cur = self.selected_row as isize;
        let next = (cur + delta).rem_euclid(len as isize) as usize;
        self.selected_row = next;
    }

    fn field_mut_for_row(&mut self, row: ServerRow) -> Option<&mut FormTextField> {
        match row {
            ServerRow::MaxConcurrent => Some(&mut self.max_concurrent_field),
            ServerRow::MinInterval => Some(&mut self.min_interval_field),
            ServerRow::QueueTimeout => Some(&mut self.queue_timeout_field),
            ServerRow::MaxQueueDepth => Some(&mut self.max_queue_depth_field),
            _ => None,
        }
    }

    fn toggle_dispatch(&mut self) {
        self.scheduling.dispatch = match self.scheduling.dispatch {
            McpDispatchMode::Exclusive => McpDispatchMode::Parallel,
            McpDispatchMode::Parallel => McpDispatchMode::Exclusive,
        };
    }

    fn clear_optional_field(&mut self, row: ServerRow) {
        match row {
            ServerRow::MinInterval => self.min_interval_field.set_text(""),
            ServerRow::QueueTimeout => self.queue_timeout_field.set_text(""),
            ServerRow::MaxQueueDepth => self.max_queue_depth_field.set_text(""),
            _ => {}
        }
    }

    fn commit(&mut self) -> Result<McpServerSchedulingToml, String> {
        let max_concurrent = parse_u32_field("Max concurrent", self.max_concurrent_field.text())?;
        let min_interval_sec =
            parse_secs_field("Min interval", self.min_interval_field.text())?;
        let queue_timeout_sec =
            parse_secs_field("Queue timeout", self.queue_timeout_field.text())?;
        let max_queue_depth =
            parse_optional_u32_field("Max queue depth", self.max_queue_depth_field.text())?;
        Ok(McpServerSchedulingToml {
            dispatch: self.scheduling.dispatch,
            max_concurrent,
            min_interval_sec,
            queue_timeout_sec,
            max_queue_depth,
        })
    }
}

#[derive(Debug)]
pub(super) struct ToolSchedulingEditor {
    pub(super) server: String,
    pub(super) tool: String,
    server_scheduling: McpServerSchedulingToml,
    selected_row: usize,
    editing: Option<ToolRow>,
    pub(super) error: Option<String>,
    override_min_interval: bool,
    override_max_concurrent: bool,
    min_interval_field: FormTextField,
    max_concurrent_field: FormTextField,
}

impl ToolSchedulingEditor {
    pub(super) fn new(
        server: &str,
        tool: &str,
        server_scheduling: McpServerSchedulingToml,
        override_cfg: Option<McpToolSchedulingOverrideToml>,
    ) -> Self {
        let mut min_interval_field = FormTextField::new_single_line();
        min_interval_field.set_placeholder("inherit");
        let mut max_concurrent_field = FormTextField::new_single_line();
        max_concurrent_field.set_placeholder("inherit");

        let mut override_min_interval = false;
        let mut override_max_concurrent = false;
        if let Some(cfg) = override_cfg {
            if let Some(value) = cfg.min_interval_sec {
                override_min_interval = true;
                min_interval_field.set_text(&format_secs_for_edit(value));
            }
            if let Some(value) = cfg.max_concurrent {
                override_max_concurrent = true;
                max_concurrent_field.set_text(&value.to_string());
            }
        }

        Self {
            server: server.to_string(),
            tool: tool.to_string(),
            server_scheduling,
            selected_row: 0,
            editing: None,
            error: None,
            override_min_interval,
            override_max_concurrent,
            min_interval_field,
            max_concurrent_field,
        }
    }

    fn selected(&self) -> ToolRow {
        debug_assert!(self.selected_row < TOOL_ROWS.len());
        TOOL_ROWS
            .get(self.selected_row)
            .copied()
            .unwrap_or(ToolRow::MinInterval)
    }

    fn set_selected_row(&mut self, idx: usize) {
        self.selected_row = idx.min(TOOL_ROWS.len().saturating_sub(1));
    }

    fn move_selected(&mut self, delta: isize) {
        let len = TOOL_ROWS.len();
        if len == 0 {
            return;
        }
        let cur = self.selected_row as isize;
        let next = (cur + delta).rem_euclid(len as isize) as usize;
        self.selected_row = next;
    }

    fn clear_all_overrides(&mut self) {
        self.override_min_interval = false;
        self.override_max_concurrent = false;
        self.min_interval_field.set_text("");
        self.max_concurrent_field.set_text("");
        self.editing = None;
    }

    fn clear_override_for_row(&mut self, row: ToolRow) {
        match row {
            ToolRow::MinInterval => {
                self.override_min_interval = false;
                self.min_interval_field.set_text("");
            }
            ToolRow::MaxConcurrent => {
                self.override_max_concurrent = false;
                self.max_concurrent_field.set_text("");
            }
            _ => {}
        }
    }

    fn ensure_override_defaults(&mut self, row: ToolRow) {
        match row {
            ToolRow::MinInterval => {
                if self.min_interval_field.text().trim().is_empty() {
                    if let Some(v) = self.server_scheduling.min_interval_sec {
                        self.min_interval_field.set_text(&format_secs_for_edit(v));
                    } else {
                        self.min_interval_field.set_text("1");
                    }
                }
            }
            ToolRow::MaxConcurrent => {
                if self.max_concurrent_field.text().trim().is_empty() {
                    self.max_concurrent_field
                        .set_text(&self.server_scheduling.max_concurrent.to_string());
                }
            }
            _ => {}
        }
    }

    fn toggle_inherit_override(&mut self, row: ToolRow) {
        match row {
            ToolRow::MinInterval => {
                if self.override_min_interval {
                    self.override_min_interval = false;
                    self.min_interval_field.set_text("");
                    self.editing = None;
                } else {
                    self.override_min_interval = true;
                    self.ensure_override_defaults(ToolRow::MinInterval);
                    self.editing = Some(ToolRow::MinInterval);
                }
            }
            ToolRow::MaxConcurrent => {
                if self.override_max_concurrent {
                    self.override_max_concurrent = false;
                    self.max_concurrent_field.set_text("");
                    self.editing = None;
                } else {
                    self.override_max_concurrent = true;
                    self.ensure_override_defaults(ToolRow::MaxConcurrent);
                    self.editing = Some(ToolRow::MaxConcurrent);
                }
            }
            _ => {}
        }
    }

    fn field_mut_for_row(&mut self, row: ToolRow) -> Option<&mut FormTextField> {
        match row {
            ToolRow::MinInterval => Some(&mut self.min_interval_field),
            ToolRow::MaxConcurrent => Some(&mut self.max_concurrent_field),
            _ => None,
        }
    }

    fn commit(&mut self) -> Result<Option<McpToolSchedulingOverrideToml>, String> {
        let min_interval_sec = if self.override_min_interval {
            let raw = self.min_interval_field.text().trim();
            if raw.is_empty() {
                return Err(
                    "Min interval: enter a value or toggle override off".to_string(),
                );
            }
            let parsed = parse_secs_field("Min interval", raw)?;
            let Some(value) = parsed else {
                return Err(
                    "Min interval: enter a value or toggle override off".to_string(),
                );
            };
            Some(value)
        } else {
            None
        };
        let max_concurrent = if self.override_max_concurrent {
            Some(parse_u32_field(
                "Max concurrent",
                self.max_concurrent_field.text(),
            )?)
        } else {
            None
        };

        let cfg = McpToolSchedulingOverrideToml {
            max_concurrent,
            min_interval_sec,
        };
        if cfg.max_concurrent.is_none() && cfg.min_interval_sec.is_none() {
            Ok(None)
        } else {
            Ok(Some(cfg))
        }
    }
}

impl McpSettingsView {
    pub(super) fn open_scheduling_editor_from_focus(&mut self) -> bool {
        if matches!(self.focus, super::McpSettingsFocus::Tools)
            && let Some(row) = self.selected_server()
            && let Some(tool) = self.selected_tool_entry()
        {
            let tool_name = tool.name;
            let override_cfg = row.tool_scheduling.get(tool_name).cloned();
            self.mode = McpSettingsMode::EditToolScheduling(Box::new(ToolSchedulingEditor::new(
                &row.name,
                tool_name,
                row.scheduling.clone(),
                override_cfg,
            )));
            return true;
        }

        if let Some(row) = self.selected_server() {
            self.mode = McpSettingsMode::EditServerScheduling(Box::new(ServerSchedulingEditor::new(
                &row.name,
                row.scheduling.clone(),
            )));
            return true;
        }

        false
    }

    pub(super) fn handle_policy_editor_key(&mut self, key_event: KeyEvent) -> bool {
        let mode = std::mem::replace(&mut self.mode, McpSettingsMode::Main);
        match mode {
            McpSettingsMode::EditServerScheduling(mut editor) => {
                let (handled, exit) = self.step_server_editor_key(&mut editor, key_event);
                self.mode = if exit {
                    McpSettingsMode::Main
                } else {
                    McpSettingsMode::EditServerScheduling(editor)
                };
                handled
            }
            McpSettingsMode::EditToolScheduling(mut editor) => {
                let (handled, exit) = self.step_tool_editor_key(&mut editor, key_event);
                self.mode = if exit {
                    McpSettingsMode::Main
                } else {
                    McpSettingsMode::EditToolScheduling(editor)
                };
                handled
            }
            McpSettingsMode::Main => {
                self.mode = McpSettingsMode::Main;
                false
            }
        }
    }

    fn step_server_editor_key(
        &mut self,
        editor: &mut ServerSchedulingEditor,
        key_event: KeyEvent,
    ) -> (bool, bool) {
        editor.error = None;

        if matches!(key_event, KeyEvent { code: KeyCode::Esc, .. }) {
            return (true, true);
        }

        if matches!(
            key_event,
            KeyEvent {
                code: KeyCode::Char('s' | 'S'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL)
        ) {
            return self.save_server_editor(editor);
        }

        let selected = editor.selected();
        if let Some(editing_row) = editor.editing {
            // While editing, allow navigation keys to exit edit mode.
            match key_event.code {
                KeyCode::Up | KeyCode::Down | KeyCode::Tab | KeyCode::BackTab => {
                    editor.editing = None;
                }
                _ => {
                    if let Some(field) = editor.field_mut_for_row(editing_row) {
                        return (field.handle_key(key_event), false);
                    }
                }
            }
        }

        if editor.editing.is_none() {
            match (selected, key_event.code) {
                (row @ ServerRow::MaxConcurrent, KeyCode::Char(c)) if c.is_ascii_digit() => {
                    editor.editing = Some(row);
                    editor.max_concurrent_field.set_text("");
                    let handled = editor.max_concurrent_field.handle_key(key_event);
                    return (handled, false);
                }
                (row @ ServerRow::MaxConcurrent, KeyCode::Backspace) => {
                    editor.editing = Some(row);
                    let handled = editor.max_concurrent_field.handle_key(key_event);
                    return (handled, false);
                }
                (row @ ServerRow::MaxQueueDepth, KeyCode::Char(c)) if c.is_ascii_digit() => {
                    editor.editing = Some(row);
                    editor.max_queue_depth_field.set_text("");
                    let handled = editor.max_queue_depth_field.handle_key(key_event);
                    return (handled, false);
                }
                (row @ ServerRow::MaxQueueDepth, KeyCode::Backspace) => {
                    editor.editing = Some(row);
                    let handled = editor.max_queue_depth_field.handle_key(key_event);
                    return (handled, false);
                }
                (row @ ServerRow::MinInterval, KeyCode::Char(c))
                    if c.is_ascii_digit() || c == '.' =>
                {
                    editor.editing = Some(row);
                    editor.min_interval_field.set_text("");
                    let handled = editor.min_interval_field.handle_key(key_event);
                    return (handled, false);
                }
                (row @ ServerRow::MinInterval, KeyCode::Backspace) => {
                    editor.editing = Some(row);
                    let handled = editor.min_interval_field.handle_key(key_event);
                    return (handled, false);
                }
                (row @ ServerRow::QueueTimeout, KeyCode::Char(c))
                    if c.is_ascii_digit() || c == '.' =>
                {
                    editor.editing = Some(row);
                    editor.queue_timeout_field.set_text("");
                    let handled = editor.queue_timeout_field.handle_key(key_event);
                    return (handled, false);
                }
                (row @ ServerRow::QueueTimeout, KeyCode::Backspace) => {
                    editor.editing = Some(row);
                    let handled = editor.queue_timeout_field.handle_key(key_event);
                    return (handled, false);
                }
                _ => {}
            }
        }

        let handled = match key_event {
            KeyEvent { code: KeyCode::Up, .. } => {
                editor.move_selected(-1);
                true
            }
            KeyEvent { code: KeyCode::Down, .. } => {
                editor.move_selected(1);
                true
            }
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                editor.move_selected(1);
                true
            }
            KeyEvent { code: KeyCode::BackTab, .. } => {
                editor.move_selected(-1);
                true
            }
            KeyEvent {
                code: KeyCode::Delete, ..
            } => {
                editor.clear_optional_field(selected);
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => {
                match selected {
                    ServerRow::Dispatch => {
                        editor.toggle_dispatch();
                        true
                    }
                    ServerRow::Save => {
                        return self.save_server_editor(editor);
                    }
                    ServerRow::Cancel => {
                        return (true, true);
                    }
                    row => {
                        // Toggle into edit mode for numeric fields.
                        if editor.editing == Some(row) {
                            editor.editing = None;
                        } else if editor.field_mut_for_row(row).is_some() {
                            editor.editing = Some(row);
                        }
                        true
                    }
                }
            }
            _ => false,
        };

        (handled, false)
    }

    fn save_server_editor(&mut self, editor: &mut ServerSchedulingEditor) -> (bool, bool) {
        match editor.commit() {
            Ok(cfg) => {
                self.app_event_tx.send(AppEvent::SetMcpServerScheduling {
                    server: editor.server.clone(),
                    scheduling: cfg.clone(),
                });
                if let Some(row) = self.rows.iter_mut().find(|r| r.name == editor.server) {
                    row.scheduling = cfg;
                }
                (true, true)
            }
            Err(err) => {
                editor.error = Some(err);
                (true, false)
            }
        }
    }

    fn step_tool_editor_key(
        &mut self,
        editor: &mut ToolSchedulingEditor,
        key_event: KeyEvent,
    ) -> (bool, bool) {
        editor.error = None;

        if matches!(key_event, KeyEvent { code: KeyCode::Esc, .. }) {
            return (true, true);
        }

        if matches!(
            key_event,
            KeyEvent {
                code: KeyCode::Char('s' | 'S'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL)
        ) {
            return self.save_tool_editor(editor);
        }

        let selected = editor.selected();
        if let Some(editing_row) = editor.editing {
            match key_event.code {
                KeyCode::Up | KeyCode::Down | KeyCode::Tab | KeyCode::BackTab => {
                    editor.editing = None;
                }
                _ => {
                    if let Some(field) = editor.field_mut_for_row(editing_row) {
                        return (field.handle_key(key_event), false);
                    }
                }
            }
        }

        if editor.editing.is_none() {
            match (selected, key_event.code) {
                (row @ ToolRow::MinInterval, KeyCode::Char(c))
                    if c.is_ascii_digit() || c == '.' =>
                {
                    if !editor.override_min_interval {
                        editor.override_min_interval = true;
                    }
                    editor.editing = Some(row);
                    editor.min_interval_field.set_text("");
                    let handled = editor.min_interval_field.handle_key(key_event);
                    return (handled, false);
                }
                (row @ ToolRow::MinInterval, KeyCode::Backspace) => {
                    if !editor.override_min_interval {
                        editor.override_min_interval = true;
                    }
                    editor.editing = Some(row);
                    let handled = editor.min_interval_field.handle_key(key_event);
                    return (handled, false);
                }
                (row @ ToolRow::MaxConcurrent, KeyCode::Char(c)) if c.is_ascii_digit() => {
                    if !editor.override_max_concurrent {
                        editor.override_max_concurrent = true;
                    }
                    editor.editing = Some(row);
                    editor.max_concurrent_field.set_text("");
                    let handled = editor.max_concurrent_field.handle_key(key_event);
                    return (handled, false);
                }
                (row @ ToolRow::MaxConcurrent, KeyCode::Backspace) => {
                    if !editor.override_max_concurrent {
                        editor.override_max_concurrent = true;
                    }
                    editor.editing = Some(row);
                    let handled = editor.max_concurrent_field.handle_key(key_event);
                    return (handled, false);
                }
                _ => {}
            }
        }

        let handled = match key_event {
            KeyEvent { code: KeyCode::Up, .. } => {
                editor.move_selected(-1);
                true
            }
            KeyEvent { code: KeyCode::Down, .. } => {
                editor.move_selected(1);
                true
            }
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                editor.move_selected(1);
                true
            }
            KeyEvent { code: KeyCode::BackTab, .. } => {
                editor.move_selected(-1);
                true
            }
            KeyEvent { code: KeyCode::Delete, .. } => {
                editor.clear_override_for_row(selected);
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => match selected {
                ToolRow::MinInterval | ToolRow::MaxConcurrent => {
                    editor.toggle_inherit_override(selected);
                    true
                }
                ToolRow::ClearOverride => {
                    editor.clear_all_overrides();
                    true
                }
                ToolRow::Save => {
                    return self.save_tool_editor(editor);
                }
                ToolRow::Cancel => {
                    return (true, true);
                }
            },
            _ => false,
        };

        (handled, false)
    }

    fn save_tool_editor(&mut self, editor: &mut ToolSchedulingEditor) -> (bool, bool) {
        match editor.commit() {
            Ok(cfg_opt) => {
                self.app_event_tx.send(AppEvent::SetMcpToolSchedulingOverride {
                    server: editor.server.clone(),
                    tool: editor.tool.clone(),
                    override_cfg: cfg_opt.clone(),
                });
                if let Some(row) = self.rows.iter_mut().find(|r| r.name == editor.server) {
                    let tool_key = editor.tool.trim();
                    if !tool_key.is_empty() {
                        if let Some(cfg) = cfg_opt.as_ref() {
                            row.tool_scheduling.insert(tool_key.to_string(), cfg.clone());
                        } else {
                            row.tool_scheduling.remove(tool_key);
                        }
                    }
                }
                (true, true)
            }
            Err(err) => {
                editor.error = Some(err);
                (true, false)
            }
        }
    }

    pub(super) fn handle_policy_editor_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {}
            _ => return false,
        }

        let outer = Block::default().borders(Borders::ALL).inner(area);
        let overlay = centered_overlay_rect(outer, 76, 14);
        let inner = Block::default().borders(Borders::ALL).inner(overlay);
        let (row_start_y, row_count) = match &self.mode {
            McpSettingsMode::EditToolScheduling(_) => (inner.y.saturating_add(2), TOOL_ROWS.len()),
            McpSettingsMode::EditServerScheduling(_) => {
                (inner.y.saturating_add(1), SERVER_ROWS.len())
            }
            McpSettingsMode::Main => return false,
        };
        let rows_end_y = inner.y.saturating_add(inner.height).saturating_sub(1);
        if mouse_event.row < row_start_y || mouse_event.row >= rows_end_y {
            return false;
        }
        let idx = mouse_event.row.saturating_sub(row_start_y) as usize;
        if idx >= row_count {
            return false;
        }

        let mut activate = false;
        let mode = std::mem::replace(&mut self.mode, McpSettingsMode::Main);
        let (handled, next_mode) = match mode {
            McpSettingsMode::EditServerScheduling(mut editor) => {
                let was_selected = editor.selected_row == idx;
                editor.set_selected_row(idx);
                activate = was_selected;
                (true, McpSettingsMode::EditServerScheduling(editor))
            }
            McpSettingsMode::EditToolScheduling(mut editor) => {
                let was_selected = editor.selected_row == idx;
                editor.set_selected_row(idx);
                activate = was_selected;
                (true, McpSettingsMode::EditToolScheduling(editor))
            }
            McpSettingsMode::Main => (false, McpSettingsMode::Main),
        };
        self.mode = next_mode;

        if activate {
            return self.handle_policy_editor_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        }

        handled
    }

    pub(super) fn render_policy_editor(&self, area: Rect, buf: &mut Buffer) {
        let outer = Block::default().borders(Borders::ALL).inner(area);
        let overlay = centered_overlay_rect(outer, 76, 14);

        Clear.render(overlay, buf);

        match &self.mode {
            McpSettingsMode::EditServerScheduling(editor) => {
                self.render_server_editor(editor, overlay, buf);
            }
            McpSettingsMode::EditToolScheduling(editor) => {
                self.render_tool_editor(editor, overlay, buf);
            }
            McpSettingsMode::Main => {}
        }
    }

    fn render_server_editor(&self, editor: &ServerSchedulingEditor, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" MCP Scheduling: {} ", editor.server))
            .border_style(Style::default().fg(crate::colors::primary()))
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            );
        let inner = block.inner(area);
        block.render(area, buf);

        let dim = Style::default().fg(crate::colors::text_dim());
        let key_style = Style::default().fg(crate::colors::secondary());
        let value_style = Style::default().fg(crate::colors::text());
        let selected_style = Style::default()
            .bg(crate::colors::selection())
            .add_modifier(Modifier::BOLD);
        let err_style = Style::default().fg(crate::colors::error());

        let help = "Up/Down move · Enter edit/toggle · Del clear optional · Ctrl+S save · Esc cancel";
        Paragraph::new(Line::from(vec![Span::styled(help, dim)]))
            .render(Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 }, buf);

        let min_value_width = 14u16;
        let label_width = inner
            .width
            .saturating_sub(min_value_width)
            .clamp(12, 28);
        let value_width = inner.width.saturating_sub(label_width);

        let rows_end_y = inner.y.saturating_add(inner.height).saturating_sub(1);
        let mut y = inner.y.saturating_add(1);
        for (idx, row) in SERVER_ROWS.iter().enumerate() {
            if y >= rows_end_y {
                break;
            }
            let selected = idx == editor.selected_row;
            let row_style = if selected { selected_style } else { Style::default() };
            let prefix = if selected { "› " } else { "  " };

            let (label, value_text, field_opt): (&str, Option<String>, Option<(&FormTextField, bool)>) = match row {
                ServerRow::Dispatch => (
                    "Dispatch",
                    Some(editor.scheduling.dispatch.to_string()),
                    None,
                ),
                ServerRow::MaxConcurrent => (
                    "Max concurrent",
                    None,
                    Some((&editor.max_concurrent_field, editor.editing == Some(ServerRow::MaxConcurrent))),
                ),
                ServerRow::MinInterval => (
                    "Min interval (sec)",
                    None,
                    Some((&editor.min_interval_field, editor.editing == Some(ServerRow::MinInterval))),
                ),
                ServerRow::QueueTimeout => (
                    "Queue timeout (sec)",
                    None,
                    Some((&editor.queue_timeout_field, editor.editing == Some(ServerRow::QueueTimeout))),
                ),
                ServerRow::MaxQueueDepth => (
                    "Max queue depth",
                    None,
                    Some((&editor.max_queue_depth_field, editor.editing == Some(ServerRow::MaxQueueDepth))),
                ),
                ServerRow::Save => ("Save", Some("Ctrl+S".to_string()), None),
                ServerRow::Cancel => ("Cancel", Some("Esc".to_string()), None),
            };

            let label_rect = Rect {
                x: inner.x,
                y,
                width: label_width,
                height: 1,
            };
            let value_rect = Rect {
                x: inner.x.saturating_add(label_width),
                y,
                width: value_width,
                height: 1,
            };

            let label_line = Line::from(vec![
                Span::styled(prefix, row_style),
                Span::styled(
                    format!("{label}:"),
                    key_style.add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }),
                ),
            ]);
            Paragraph::new(label_line).render(label_rect, buf);

            if let Some((field, focused)) = field_opt {
                field.render(value_rect, buf, focused);
            } else {
                let value = value_text.unwrap_or_default();
                Paragraph::new(Line::from(vec![Span::styled(value, value_style)]))
                    .render(value_rect, buf);
            }
            y = y.saturating_add(1);
        }

        if let Some(err) = editor.error.as_deref() {
            let err_area = Rect {
                x: inner.x,
                y: rows_end_y,
                width: inner.width,
                height: 1,
            };
            Paragraph::new(Line::from(vec![Span::styled(err.to_string(), err_style)]))
                .render(err_area, buf);
        }
    }

    fn render_tool_editor(&self, editor: &ToolSchedulingEditor, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(
                " MCP Tool Scheduling: {}/{} ",
                editor.server, editor.tool
            ))
            .border_style(Style::default().fg(crate::colors::primary()))
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            );
        let inner = block.inner(area);
        block.render(area, buf);

        let dim = Style::default().fg(crate::colors::text_dim());
        let key_style = Style::default().fg(crate::colors::secondary());
        let value_style = Style::default().fg(crate::colors::text());
        let selected_style = Style::default()
            .bg(crate::colors::selection())
            .add_modifier(Modifier::BOLD);
        let err_style = Style::default().fg(crate::colors::error());

        let help = "Enter toggle override · Del clear · Ctrl+S save · Esc cancel";
        Paragraph::new(Line::from(vec![Span::styled(help, dim)]))
            .render(Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 }, buf);

        let override_min_interval_text = editor.min_interval_field.text();
        let (override_min_interval_value, override_min_interval_invalid) =
            if editor.override_min_interval {
                if override_min_interval_text.trim().is_empty() {
                    (None, true)
                } else {
                    match parse_secs_field("Min interval", override_min_interval_text) {
                        Ok(Some(v)) => (Some(v), false),
                        Ok(None) => (None, true),
                        Err(_) => (None, true),
                    }
                }
            } else {
                (None, false)
            };

        let (override_max_concurrent_value, override_max_concurrent_invalid) =
            if editor.override_max_concurrent {
                match parse_u32_field("Max concurrent", editor.max_concurrent_field.text()) {
                    Ok(v) => (Some(v), false),
                    Err(_) => (None, true),
                }
            } else {
                (None, false)
            };

        let effective_min_interval = if override_min_interval_invalid {
            None
        } else {
            match (editor.server_scheduling.min_interval_sec, override_min_interval_value) {
                (None, None) => None,
                (Some(s), None) => Some(s),
                (None, Some(o)) => Some(o),
                (Some(s), Some(o)) => Some(s.max(o)),
            }
        };

        let effective_max_concurrent = if override_max_concurrent_invalid {
            None
        } else {
            match override_max_concurrent_value {
                Some(o) => Some(editor.server_scheduling.max_concurrent.min(o)),
                None => Some(editor.server_scheduling.max_concurrent),
            }
        };

        let effective_line = {
            let max_conc = if override_max_concurrent_invalid {
                "?".to_string()
            } else {
                effective_max_concurrent
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "?".to_string())
            };

            let min_int = if override_min_interval_invalid {
                "?".to_string()
            } else {
                format_opt_secs_compact(effective_min_interval)
            };
            format!("Effective: max_concurrent={max_conc}, min_interval={min_int}")
        };
        Paragraph::new(Line::from(vec![Span::styled(effective_line, dim)]))
            .render(
                Rect {
                    x: inner.x,
                    y: inner.y.saturating_add(1),
                    width: inner.width,
                    height: 1,
                },
                buf,
            );

        let min_value_width = 14u16;
        let label_width = inner
            .width
            .saturating_sub(min_value_width)
            .clamp(12, 30);
        let value_width = inner.width.saturating_sub(label_width);

        let rows_end_y = inner.y.saturating_add(inner.height).saturating_sub(1);
        let mut y = inner.y.saturating_add(2);
        for (idx, row) in TOOL_ROWS.iter().enumerate() {
            if y >= rows_end_y {
                break;
            }
            let selected = idx == editor.selected_row;
            let row_style = if selected { selected_style } else { Style::default() };
            let prefix = if selected { "› " } else { "  " };

            let label_rect = Rect {
                x: inner.x,
                y,
                width: label_width,
                height: 1,
            };
            let value_rect = Rect {
                x: inner.x.saturating_add(label_width),
                y,
                width: value_width,
                height: 1,
            };

            let (label, value_text, field_opt): (String, Option<String>, Option<(&FormTextField, bool)>) = match row {
                ToolRow::MinInterval => {
                    if editor.override_min_interval {
                        (
                            "Min interval (override)".to_string(),
                            None,
                            Some((&editor.min_interval_field, editor.editing == Some(ToolRow::MinInterval))),
                        )
                    } else {
                        let server_v = format_opt_secs_compact(editor.server_scheduling.min_interval_sec);
                        (
                            "Min interval (inherit)".to_string(),
                            Some(format!("server {server_v}")),
                            None,
                        )
                    }
                }
                ToolRow::MaxConcurrent => {
                    if editor.override_max_concurrent {
                        (
                            "Max concurrent (override)".to_string(),
                            None,
                            Some((&editor.max_concurrent_field, editor.editing == Some(ToolRow::MaxConcurrent))),
                        )
                    } else {
                        (
                            "Max concurrent (inherit)".to_string(),
                            Some(format!("server {}", editor.server_scheduling.max_concurrent)),
                            None,
                        )
                    }
                }
                ToolRow::ClearOverride => (
                    "Clear override".to_string(),
                    Some("remove all tool limits".to_string()),
                    None,
                ),
                ToolRow::Save => ("Save".to_string(), Some("Ctrl+S".to_string()), None),
                ToolRow::Cancel => ("Cancel".to_string(), Some("Esc".to_string()), None),
            };

            let label_line = Line::from(vec![
                Span::styled(prefix, row_style),
                Span::styled(
                    format!("{label}:"),
                    key_style.add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }),
                ),
            ]);
            Paragraph::new(label_line).render(label_rect, buf);

            if let Some((field, focused)) = field_opt {
                field.render(value_rect, buf, focused);
            } else {
                let value = value_text.unwrap_or_default();
                Paragraph::new(Line::from(vec![Span::styled(value, value_style)]))
                    .render(value_rect, buf);
            }
            y = y.saturating_add(1);
        }

        if let Some(err) = editor.error.as_deref() {
            let err_area = Rect {
                x: inner.x,
                y: rows_end_y,
                width: inner.width,
                height: 1,
            };
            Paragraph::new(Line::from(vec![Span::styled(err.to_string(), err_style)]))
                .render(err_area, buf);
        }
    }
}
