use std::collections::BTreeMap;

use crate::app_event::AppEvent;

use super::{McpSettingsView, McpToolEntry};

impl McpSettingsView {
    pub(super) fn tool_entries(&self) -> Vec<McpToolEntry<'_>> {
        let Some(row) = self.selected_server() else {
            return Vec::new();
        };

        let mut map: BTreeMap<&str, bool> = BTreeMap::new();
        for tool in &row.tools {
            if !tool.trim().is_empty() {
                map.insert(tool.as_str(), true);
            }
        }
        for tool in &row.disabled_tools {
            if !tool.trim().is_empty() {
                map.insert(tool.as_str(), false);
            }
        }
        map.into_iter()
            .map(|(name, enabled)| McpToolEntry {
                name,
                enabled,
                definition: row.tool_definitions.get(name),
            })
            .collect()
    }

    pub(super) fn toggle_selected_tool(&mut self) {
        let Some(row) = self.selected_server() else {
            return;
        };
        let server_name = row.name.clone();
        let entries = self.tool_entries();
        let Some(entry) = entries.get(self.tools_selected).cloned() else {
            return;
        };
        let tool_name = entry.name.to_string();
        let enable = !entry.enabled;

        self.app_event_tx.send(AppEvent::UpdateMcpServerTool {
            server_name,
            tool_name: tool_name.clone(),
            enable,
        });

        if let Some(row_mut) = self.rows.get_mut(self.selected) {
            if enable {
                row_mut.disabled_tools.retain(|name| name != &tool_name);
                if !row_mut.tools.iter().any(|name| name == &tool_name) {
                    row_mut.tools.push(tool_name);
                    row_mut.tools.sort();
                    row_mut.tools.dedup();
                }
            } else {
                row_mut.tools.retain(|name| name != &tool_name);
                if !row_mut.disabled_tools.iter().any(|name| name == &tool_name) {
                    row_mut.disabled_tools.push(tool_name);
                    row_mut.disabled_tools.sort();
                    row_mut.disabled_tools.dedup();
                }
            }
        }
    }

    pub(super) fn expanded_tool_for_selected_server(&self) -> Option<&str> {
        let server = self.selected_server()?;
        self.expanded_tool_by_server
            .get(&server.name)
            .map(String::as_str)
    }

    pub(super) fn is_tool_expanded(&self, tool_name: &str) -> bool {
        self.expanded_tool_for_selected_server()
            .is_some_and(|current| current == tool_name)
    }

    pub(super) fn set_expanded_tool_for_selected_server(&mut self, tool_name: Option<String>) {
        let Some(server_name) = self.selected_server().map(|row| row.name.clone()) else {
            return;
        };
        match tool_name {
            Some(tool_name) => {
                self.expanded_tool_by_server.insert(server_name, tool_name);
            }
            None => {
                self.expanded_tool_by_server.remove(&server_name);
            }
        }
    }

    pub(super) fn toggle_selected_tool_expansion(&mut self) {
        let entries = self.tool_entries();
        let Some(entry) = entries.get(self.tools_selected) else {
            return;
        };
        if self.is_tool_expanded(entry.name) {
            self.set_expanded_tool_for_selected_server(None);
        } else {
            self.set_expanded_tool_for_selected_server(Some(entry.name.to_string()));
            // Show expanded details from the top of the details pane.
            self.summary_scroll_top = 0;
            self.summary_hscroll = 0;
        }
    }
}
