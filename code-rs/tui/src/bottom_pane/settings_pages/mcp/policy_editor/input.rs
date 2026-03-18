use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;

use super::super::{McpSettingsFocus, McpSettingsMode, McpSettingsView};

use super::model::{ServerRow, ServerSchedulingEditor, ToolRow, ToolSchedulingEditor};

impl McpSettingsView {
    pub(in crate::bottom_pane::settings_pages::mcp) fn open_scheduling_editor_from_focus(
        &mut self,
    ) -> bool {
        if matches!(self.focus, McpSettingsFocus::Tools)
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
            self.mode = McpSettingsMode::EditServerScheduling(Box::new(
                ServerSchedulingEditor::new(&row.name, row.scheduling.clone()),
            ));
            return true;
        }

        false
    }

    pub(in crate::bottom_pane::settings_pages::mcp) fn handle_policy_editor_key(
        &mut self,
        key_event: KeyEvent,
    ) -> bool {
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
                (row @ ServerRow::MinInterval, KeyCode::Char(c)) if c.is_ascii_digit() || c == '.' => {
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
                (row @ ServerRow::QueueTimeout, KeyCode::Char(c)) if c.is_ascii_digit() || c == '.' => {
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
            KeyEvent { code: KeyCode::Tab, modifiers: KeyModifiers::NONE, .. } => {
                editor.move_selected(1);
                true
            }
            KeyEvent { code: KeyCode::BackTab, .. } => {
                editor.move_selected(-1);
                true
            }
            KeyEvent { code: KeyCode::Delete, .. } => {
                editor.clear_optional_field(selected);
                true
            }
            KeyEvent { code: KeyCode::Enter, .. } => match selected {
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
            },
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
                (row @ ToolRow::MinInterval, KeyCode::Char(c)) if c.is_ascii_digit() || c == '.' => {
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
            KeyEvent { code: KeyCode::Tab, modifiers: KeyModifiers::NONE, .. } => {
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
            KeyEvent { code: KeyCode::Enter, .. } => match selected {
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
}

