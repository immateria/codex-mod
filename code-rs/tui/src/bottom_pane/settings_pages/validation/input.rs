use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;
use crate::bottom_pane::settings_ui::line_runs::scroll_top_for_section;
use crate::bottom_pane::BottomPane;

impl ValidationSettingsView {
    pub(super) fn toggle_group(&mut self, idx: usize) {
        if idx >= self.groups.len() {
            return;
        }
        let group = self.groups[idx].0.group;
        let new_value;
        {
            let (_, enabled) = &mut self.groups[idx];
            new_value = !*enabled;
            *enabled = new_value;
        }
        self.apply_group_to_tools(group, new_value);
        self.app_event_tx
            .send(AppEvent::UpdateValidationGroup { group, enable: new_value });
    }

    fn toggle_tool(&mut self, idx: usize) {
        if let Some(row) = self.tools.get_mut(idx) {
            if !row.status.installed {
                return;
            }
            row.enabled = !row.enabled;
            self.app_event_tx.send(AppEvent::UpdateValidationTool {
                name: row.status.name.to_string(),
                enable: row.enabled,
            });
        }
    }

    fn apply_group_to_tools(&mut self, group: ValidationGroup, enabled: bool) {
        for tool in &mut self.tools {
            if group_for_category(tool.status.category) == group {
                tool.group_enabled = enabled;
            }
        }
    }

    pub(super) fn activate_selection(
        &mut self,
        pane: Option<&mut BottomPane<'_>>,
        selection: SelectionKind,
    ) {
        match selection {
            SelectionKind::Group(idx) => self.toggle_group(idx),
            SelectionKind::Tool(idx) => {
                if let Some(tool) = self.tools.get(idx) {
                    if !tool.status.installed {
                        let command = tool.status.install_hint.trim().to_string();
                        let tool_name = tool.status.name.to_string();
                        if command.is_empty() {
                            self.flash_notice(pane, format!("No install command available for {tool_name}"));
                        } else {
                            self.flash_notice(pane, format!("Opening terminal to install {tool_name}"));
                            self.is_complete = true;
                            self.app_event_tx.send(AppEvent::RequestValidationToolInstall {
                                name: tool_name,
                                command,
                            });
                        }
                    } else {
                        self.toggle_tool(idx);
                    }
                }
            }
        }
    }

    fn flash_notice(&mut self, pane: Option<&mut BottomPane<'_>>, text: String) {
        if let Some(pane) = pane {
            pane.flash_footer_notice(text.clone());
        }
        self.pending_notice = Some(text);
        self.app_event_tx.send(AppEvent::RequestRedraw);
    }

    pub(super) fn ensure_selected_visible(&mut self, model: &ValidationListModel, body_height: usize) {
        if body_height == 0 || model.total_lines == 0 || model.selection_kinds.is_empty() {
            self.state.scroll_top = 0;
            return;
        }

        let total = model.selection_kinds.len();
        self.state.clamp_selection(total);
        let Some(sel_idx) = self.state.selected_idx else {
            self.state.scroll_top = 0;
            return;
        };
        let sel_idx = sel_idx.min(total.saturating_sub(1));
        self.state.selected_idx = Some(sel_idx);

        let selected_line = model
            .selection_line
            .get(sel_idx)
            .copied()
            .unwrap_or(0)
            .min(model.total_lines.saturating_sub(1));
        let (section_start, section_end) = model
            .section_bounds
            .get(sel_idx)
            .copied()
            .unwrap_or((0, model.total_lines.saturating_sub(1)));
        let section_end = section_end.min(model.total_lines.saturating_sub(1));
        let section_start = section_start.min(section_end);

        self.state.scroll_top = scroll_top_for_section(
            self.state.scroll_top,
            body_height,
            selected_line,
            section_start,
            section_end,
        );
    }

    pub(super) fn handle_key_event_internal(
        &mut self,
        mut pane: Option<&mut BottomPane<'_>>,
        key_event: KeyEvent,
    ) -> bool {
        let body_height_hint = match self.viewport_rows.get() {
            0 => DEFAULT_VISIBLE_ROWS,
            other => other,
        };

        let mut model = self.build_model();
        let mut total = model.selection_kinds.len();
        if total == 0 {
            if matches!(key_event.code, KeyCode::Esc) {
                self.is_complete = true;
                return true;
            }
            return false;
        }

        self.ensure_selected_visible(&model, body_height_hint);

        let current_kind = self
            .state
            .selected_idx
            .and_then(|sel| model.selection_kinds.get(sel))
            .copied();

        let handled = match key_event {
            KeyEvent { code: KeyCode::Up, .. } => {
                self.state.move_up_wrap(total);
                self.ensure_selected_visible(&model, body_height_hint);
                true
            }
            KeyEvent { code: KeyCode::Down, .. } => {
                self.state.move_down_wrap(total);
                self.ensure_selected_visible(&model, body_height_hint);
                true
            }
            KeyEvent { code: KeyCode::Left, .. } | KeyEvent { code: KeyCode::Right, .. } => {
                if let Some(kind) = current_kind {
                    match kind {
                        SelectionKind::Group(idx) => self.toggle_group(idx),
                        SelectionKind::Tool(idx) => {
                            if let Some(tool) = self.tools.get(idx)
                                && tool.status.installed
                            {
                                self.toggle_tool(idx);
                            }
                        }
                    }
                    true
                } else {
                    false
                }
            }
            KeyEvent { code: KeyCode::Char(' '), .. } => {
                if let Some(kind) = current_kind {
                    self.activate_selection(pane.take(), kind);
                    true
                } else {
                    false
                }
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                if let Some(kind) = current_kind {
                    self.activate_selection(pane.take(), kind);
                    true
                } else {
                    false
                }
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
                true
            }
            _ => false,
        };

        if handled {
            model = self.build_model();
            total = model.selection_kinds.len();
            if total == 0 {
                self.state.selected_idx = None;
                self.state.scroll_top = 0;
            } else {
                self.ensure_selected_visible(&model, body_height_hint);
            }
        }
        handled
    }

    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.handle_key_event_internal(None, key_event)
    }
}

