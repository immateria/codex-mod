use crate::app_event::AppEvent;
use crate::ui_interaction::clamp_index;

use super::{McpSelectionKey, McpSettingsFocus, McpSettingsView, McpSettingsViewState};

impl McpSettingsView {
    pub(super) fn len(&self) -> usize {
        self.rows.len().saturating_add(3)
    }

    pub(super) fn refresh_index(&self) -> usize {
        self.rows.len()
    }

    pub(super) fn add_index(&self) -> usize {
        self.rows.len().saturating_add(1)
    }

    pub(super) fn close_index(&self) -> usize {
        self.rows.len().saturating_add(2)
    }

    pub(super) fn selected_server(&self) -> Option<&super::McpServerRow> {
        self.rows.get(self.selected)
    }

    pub(super) fn selection_key(&self) -> McpSelectionKey {
        if self.selected < self.rows.len() {
            McpSelectionKey::Server(self.rows[self.selected].name.clone())
        } else if self.selected == self.refresh_index() {
            McpSelectionKey::Refresh
        } else if self.selected == self.add_index() {
            McpSelectionKey::Add
        } else {
            McpSelectionKey::Close
        }
    }

    pub(super) fn selection_index_from_key(&self, key: &McpSelectionKey) -> usize {
        match key {
            McpSelectionKey::Server(name) => self
                .rows
                .iter()
                .position(|row| row.name == *name)
                .unwrap_or(0),
            McpSelectionKey::Refresh => self.refresh_index(),
            McpSelectionKey::Add => self.add_index(),
            McpSelectionKey::Close => self.close_index(),
        }
    }

    pub(super) fn set_selected(&mut self, selected: usize) {
        let clamped = clamp_index(selected, self.len());
        if self.selected != clamped {
            self.selected = clamped;
            self.summary_scroll_top = 0;
            self.summary_hscroll = 0;
            self.tools_selected = 0;
            self.hovered_tool_index = None;
            self.hovered_tool_part = None;
            self.sync_selected_server_tool_state();
        }
    }

    pub(super) fn sync_selected_server_tool_state(&mut self) {
        let Some(server_name) = self.selected_server().map(|row| row.name.clone()) else {
            return;
        };

        let Some(expanded_tool_name) = self.expanded_tool_by_server.get(&server_name).cloned() else {
            return;
        };

        let entries = self.tool_entries();
        if let Some(idx) = entries.iter().position(|entry| entry.name == expanded_tool_name) {
            self.tools_selected = idx;
            self.summary_scroll_top = 0;
        } else {
            self.expanded_tool_by_server.remove(&server_name);
        }
    }

    pub(super) fn set_focus(&mut self, focus: McpSettingsFocus) {
        self.focus = focus;
        if self.focus == McpSettingsFocus::Tools {
            let max_idx = self.tool_entries().len().saturating_sub(1);
            self.tools_selected = self.tools_selected.min(max_idx);
        }
    }

    pub(crate) fn snapshot_state(&self) -> McpSettingsViewState {
        McpSettingsViewState {
            selection: self.selection_key(),
            focus: self.focus,
            stacked_scroll_top: self.stacked_scroll_top,
            summary_scroll_top: self.summary_scroll_top,
            summary_hscroll: self.summary_hscroll,
            summary_wrap: self.summary_wrap,
            tools_selected: self.tools_selected,
            expanded_tool_by_server: self.expanded_tool_by_server.clone(),
        }
    }

    pub(crate) fn restore_state(&mut self, state: &McpSettingsViewState) {
        let selected = self.selection_index_from_key(&state.selection);
        self.set_selected(selected);
        self.summary_wrap = state.summary_wrap;
        self.stacked_scroll_top = state.stacked_scroll_top;
        self.summary_scroll_top = state.summary_scroll_top;
        self.summary_hscroll = state.summary_hscroll;
        self.tools_selected = state.tools_selected;
        self.expanded_tool_by_server = state.expanded_tool_by_server.clone();
        self.set_focus(state.focus);
    }

    pub(super) fn cycle_focus(&mut self, reverse: bool) {
        let has_tools = !self.tool_entries().is_empty();
        let order = if has_tools {
            [
                McpSettingsFocus::Servers,
                McpSettingsFocus::Summary,
                McpSettingsFocus::Tools,
            ]
        } else {
            [McpSettingsFocus::Servers, McpSettingsFocus::Summary, McpSettingsFocus::Summary]
        };
        let current_idx = match self.focus {
            McpSettingsFocus::Servers => 0,
            McpSettingsFocus::Summary => 1,
            McpSettingsFocus::Tools => {
                if has_tools {
                    2
                } else {
                    1
                }
            }
        };
        let next_idx = if reverse {
            if current_idx == 0 {
                if has_tools { 2 } else { 1 }
            } else {
                current_idx - 1
            }
        } else if has_tools {
            (current_idx + 1) % 3
        } else if current_idx == 0 {
            1
        } else {
            0
        };
        self.set_focus(order[next_idx]);
    }

    pub(super) fn move_selection_up(&mut self) {
        if self.selected == 0 {
            self.set_selected(self.len().saturating_sub(1));
        } else {
            self.set_selected(self.selected - 1);
        }
    }

    pub(super) fn move_selection_down(&mut self) {
        self.set_selected((self.selected + 1) % self.len().max(1));
    }

    pub(super) fn on_toggle_server(&mut self) {
        if self.selected < self.rows.len() {
            let row = &mut self.rows[self.selected];
            let new_enabled = !row.enabled;
            row.enabled = new_enabled;
            row.status = if new_enabled {
                "Tools: pending".to_string()
            } else {
                "Tools: disabled".to_string()
            };
            if !new_enabled {
                row.failure = None;
                row.tools.clear();
            }
            self.app_event_tx.send(AppEvent::UpdateMcpServer {
                name: row.name.clone(),
                enable: new_enabled,
            });
        }
    }

    pub(super) fn on_enter_server_selection(&mut self) {
        match self.selected {
            idx if idx < self.rows.len() => self.on_toggle_server(),
            idx if idx == self.refresh_index() => self.request_refresh(),
            idx if idx == self.add_index() => {
                self.app_event_tx
                    .send(AppEvent::PrefillComposer("/mcp add ".to_string()));
                self.is_complete = true;
            }
            _ => {
                self.is_complete = true;
            }
        }
    }

    pub(super) fn request_refresh(&self) {
        self.app_event_tx
            .send(AppEvent::CodexOp(code_core::protocol::Op::RefreshMcpTools));
    }

    pub(super) fn queue_status_report(&self) {
        self.app_event_tx
            .send(AppEvent::PrefillComposer("/mcp status".to_string()));
    }

    pub(super) fn toggle_summary_wrap_mode(&mut self) {
        self.summary_wrap = !self.summary_wrap;
        self.summary_hscroll = 0;
    }

    pub(super) fn shift_summary_hscroll(&mut self, delta: i32) {
        if delta < 0 {
            self.summary_hscroll = self
                .summary_hscroll
                .saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.summary_hscroll = self.summary_hscroll.saturating_add(delta as usize);
        }
    }
}
