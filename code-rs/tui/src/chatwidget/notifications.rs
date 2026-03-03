use super::*;

impl ChatWidget<'_> {
    pub(super) fn clear_backgrounds_in(&self, buf: &mut Buffer, rect: Rect) {
        for y in rect.y..rect.y.saturating_add(rect.height) {
            for x in rect.x..rect.x.saturating_add(rect.width) {
                let cell = &mut buf[(x, y)];
                // Reset background; keep fg/content as-is
                cell.set_bg(ratatui::style::Color::Reset);
            }
        }
    }
    

    pub(crate) fn set_tui_notifications(&mut self, enabled: bool) {
        let new_state = Notifications::Enabled(enabled);
        self.config.tui.notifications = new_state.clone();
        self.config.tui_notifications = new_state.clone();

        match find_code_home() {
            Ok(home) => {
                match code_core::config::set_tui_notifications(&home, new_state) {
                    Ok(()) => {
                        let msg = format!(
                            "TUI notifications: {}",
                            if enabled { "enabled" } else { "disabled" }
                        );
                        self.push_background_tail(msg);
                    }
                    Err(err) => {
                        let msg = format!(
                            "WARN: Failed to persist TUI notifications setting: {err}"
                        );
                        self.history_push_plain_state(history_cell::new_error_event(msg));
                    }
                }
            }
            Err(_) => {
                let msg = format!(
                    "TUI notifications: {} (not persisted: CODE_HOME/CODEX_HOME not found)",
                    if enabled { "enabled" } else { "disabled" }
                );
                self.push_background_tail(msg);
            }
        }

        self.refresh_settings_overview_rows();
    }

    pub(super) fn emit_turn_complete_notification(&self, last_agent_message: Option<String>) {
        if !self.should_emit_tui_notification("agent-turn-complete") {
            return;
        }

        let snippet = last_agent_message
            .as_deref()
            .map(Self::notification_snippet)
            .filter(|text| !text.is_empty());

        self.app_event_tx.send(AppEvent::EmitTuiNotification {
            title: "Code".to_string(),
            body: snippet,
        });
    }

    pub(super) fn should_emit_tui_notification(&self, event: &str) -> bool {
        if self.replay_history_depth > 0 {
            return false;
        }
        self.tui_notification_filter_allows(event)
    }

    pub(super) fn tui_notification_filter_allows(&self, event: &str) -> bool {
        match &self.config.tui.notifications {
            Notifications::Enabled(enabled) => *enabled,
            Notifications::Custom(entries) => entries
                .iter()
                .any(|entry| entry.eq_ignore_ascii_case(event)),
        }
    }

    pub(super) fn notification_snippet(input: &str) -> String {
        let collapsed = input
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        const LIMIT: usize = 120;
        if collapsed.chars().count() <= LIMIT {
            return collapsed;
        }

        let mut truncated = String::new();
        for (count, ch) in collapsed.chars().enumerate() {
            if count >= LIMIT.saturating_sub(3) {
                break;
            }
            truncated.push(ch);
        }
        truncated.push_str("...");
        truncated
    }

    pub(crate) fn toggle_mcp_server(&mut self, name: &str, enable: bool) {
        match code_core::config::find_code_home() {
            Ok(home) => match code_core::config::set_mcp_server_enabled(&home, name, enable) {
                Ok(changed) => {
                    if changed {
                        if enable {
                            if let Ok((enabled, _)) = code_core::config::list_mcp_servers(&home)
                                && let Some((_, cfg)) = enabled.into_iter().find(|(n, _)| n == name)
                                {
                                    self.config.mcp_servers.insert(name.to_string(), cfg);
                                }
                        } else {
                            self.config.mcp_servers.remove(name);
                        }
                        let msg = format!(
                            "{} MCP server '{}'",
                            if enable { "Enabled" } else { "Disabled" },
                            name
                        );
                        self.push_background_tail(msg);
                    }
                }
                Err(e) => {
                    let msg = format!("Failed to update MCP server '{name}': {e}");
                    self.history_push_plain_state(history_cell::new_error_event(msg));
                }
            },
            Err(e) => {
                let msg = format!("Failed to locate CODEX_HOME: {e}");
                self.history_push_plain_state(history_cell::new_error_event(msg));
            }
        }
    }

    pub(crate) fn toggle_mcp_server_tool(
        &mut self,
        server_name: &str,
        tool_name: &str,
        enable: bool,
    ) {
        match code_core::config::find_code_home() {
            Ok(home) => match code_core::config::set_mcp_server_tool_enabled(
                &home,
                server_name,
                tool_name,
                enable,
            ) {
                Ok(changed) => {
                    if !changed {
                        return;
                    }

                    if let Some(server_cfg) = self.config.mcp_servers.get_mut(server_name) {
                        if enable {
                            server_cfg.disabled_tools.retain(|name| name != tool_name);
                        } else if !server_cfg
                            .disabled_tools
                            .iter()
                            .any(|name| name == tool_name)
                        {
                            server_cfg.disabled_tools.push(tool_name.to_string());
                            server_cfg.disabled_tools.sort();
                            server_cfg.disabled_tools.dedup();
                        }
                    }

                    self.submit_op(Op::SetMcpToolEnabled {
                        server: server_name.to_string(),
                        tool: tool_name.to_string(),
                        enable,
                    });

                    let msg = format!(
                        "{} MCP tool '{}::{}'",
                        if enable { "Enabled" } else { "Disabled" },
                        server_name,
                        tool_name
                    );
                    self.push_background_tail(msg);
                }
                Err(e) => {
                    let msg = format!(
                        "Failed to update MCP tool '{server_name}::{tool_name}': {e}"
                    );
                    self.history_push_plain_state(history_cell::new_error_event(msg));
                }
            },
            Err(e) => {
                let msg = format!("Failed to locate CODEX_HOME: {e}");
                self.history_push_plain_state(history_cell::new_error_event(msg));
            }
        }
    }

    pub(crate) fn set_mcp_server_scheduling(
        &mut self,
        server_name: &str,
        scheduling: code_core::config_types::McpServerSchedulingToml,
    ) {
        match code_core::config::find_code_home() {
            Ok(home) => match code_core::config::set_mcp_server_scheduling(
                &home,
                server_name,
                scheduling.clone(),
            ) {
                Ok(()) => {
                    if let Some(server_cfg) = self.config.mcp_servers.get_mut(server_name) {
                        server_cfg.scheduling = scheduling.clone();
                    }

                    self.submit_op(Op::SetMcpServerScheduling {
                        server: server_name.to_string(),
                        scheduling,
                    });

                    self.push_background_tail(format!(
                        "Updated MCP scheduling for '{server_name}'.",
                    ));
                }
                Err(err) => {
                    let msg = format!(
                        "Failed to update MCP scheduling for '{server_name}': {err}"
                    );
                    self.history_push_plain_state(history_cell::new_error_event(msg));
                }
            },
            Err(err) => {
                let msg = format!("Failed to locate CODEX_HOME: {err}");
                self.history_push_plain_state(history_cell::new_error_event(msg));
            }
        }
    }

    pub(crate) fn set_mcp_tool_scheduling_override(
        &mut self,
        server_name: &str,
        tool_name: &str,
        override_cfg: Option<code_core::config_types::McpToolSchedulingOverrideToml>,
    ) {
        match code_core::config::find_code_home() {
            Ok(home) => match code_core::config::set_mcp_tool_scheduling_override(
                &home,
                server_name,
                tool_name,
                override_cfg.clone(),
            ) {
                Ok(()) => {
                    if let Some(server_cfg) = self.config.mcp_servers.get_mut(server_name) {
                        let normalized_tool = tool_name.trim();
                        if !normalized_tool.is_empty() {
                            if let Some(cfg) = override_cfg.as_ref()
                                && (cfg.max_concurrent.is_some() || cfg.min_interval_sec.is_some())
                            {
                                server_cfg
                                    .tool_scheduling
                                    .insert(normalized_tool.to_string(), cfg.clone());
                            } else {
                                server_cfg.tool_scheduling.remove(normalized_tool);
                            }
                        }
                    }

                    self.submit_op(Op::SetMcpToolSchedulingOverride {
                        server: server_name.to_string(),
                        tool: tool_name.to_string(),
                        override_cfg,
                    });

                    self.push_background_tail(format!(
                        "Updated MCP tool scheduling for '{server_name}::{tool_name}'.",
                    ));
                }
                Err(err) => {
                    let msg = format!(
                        "Failed to update MCP tool scheduling for '{server_name}::{tool_name}': {err}"
                    );
                    self.history_push_plain_state(history_cell::new_error_event(msg));
                }
            },
            Err(err) => {
                let msg = format!("Failed to locate CODEX_HOME: {err}");
                self.history_push_plain_state(history_cell::new_error_event(msg));
            }
        }
    }
}
