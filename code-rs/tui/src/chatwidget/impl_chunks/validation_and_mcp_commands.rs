use std::fmt::Write as _;

impl ChatWidget<'_> {
    fn validation_tool_flag_mut(
        &mut self,
        name: &str,
    ) -> Option<&mut Option<bool>> {
        let tools = &mut self.config.validation.tools;
        match name {
            "shellcheck" => Some(&mut tools.shellcheck),
            "markdownlint" => Some(&mut tools.markdownlint),
            "hadolint" => Some(&mut tools.hadolint),
            "yamllint" => Some(&mut tools.yamllint),
            "cargo-check" => Some(&mut tools.cargo_check),
            "shfmt" => Some(&mut tools.shfmt),
            "prettier" => Some(&mut tools.prettier),
            "tsc" => Some(&mut tools.tsc),
            "eslint" => Some(&mut tools.eslint),
            "phpstan" => Some(&mut tools.phpstan),
            "psalm" => Some(&mut tools.psalm),
            "mypy" => Some(&mut tools.mypy),
            "pyright" => Some(&mut tools.pyright),
            "golangci-lint" => Some(&mut tools.golangci_lint),
            _ => None,
        }
    }

    fn validation_group_label(group: ValidationGroup) -> &'static str {
        match group {
            ValidationGroup::Functional => "Functional checks",
            ValidationGroup::Stylistic => "Stylistic checks",
        }
    }

    fn validation_group_enabled(&self, group: ValidationGroup) -> bool {
        match group {
            ValidationGroup::Functional => self.config.validation.groups.functional,
            ValidationGroup::Stylistic => self.config.validation.groups.stylistic,
        }
    }

    fn validation_tool_requested(&self, name: &str) -> bool {
        let tools = &self.config.validation.tools;
        match name {
            "actionlint" => self.config.github.actionlint_on_patch,
            "shellcheck" => tools.shellcheck.unwrap_or(true),
            "markdownlint" => tools.markdownlint.unwrap_or(true),
            "hadolint" => tools.hadolint.unwrap_or(true),
            "yamllint" => tools.yamllint.unwrap_or(true),
            "cargo-check" => tools.cargo_check.unwrap_or(true),
            "shfmt" => tools.shfmt.unwrap_or(true),
            "prettier" => tools.prettier.unwrap_or(true),
            "tsc" => tools.tsc.unwrap_or(true),
            "eslint" => tools.eslint.unwrap_or(true),
            "phpstan" => tools.phpstan.unwrap_or(true),
            "psalm" => tools.psalm.unwrap_or(true),
            "mypy" => tools.mypy.unwrap_or(true),
            "pyright" => tools.pyright.unwrap_or(true),
            "golangci-lint" => tools.golangci_lint.unwrap_or(true),
            _ => true,
        }
    }

    fn validation_tool_enabled(&self, name: &str) -> bool {
        let requested = self.validation_tool_requested(name);
        let category = validation_tool_category(name);
        let group_enabled = match category {
            ValidationCategory::Functional => self.config.validation.groups.functional,
            ValidationCategory::Stylistic => self.config.validation.groups.stylistic,
        };
        requested && group_enabled
    }

    fn apply_validation_group_toggle(&mut self, group: ValidationGroup, enable: bool) {
        if self.validation_group_enabled(group) == enable {
            return;
        }

        match group {
            ValidationGroup::Functional => self.config.validation.groups.functional = enable,
            ValidationGroup::Stylistic => self.config.validation.groups.stylistic = enable,
        }

        if let Err(err) = self
            .code_op_tx
            .send(Op::UpdateValidationGroup { group, enable })
        {
            tracing::warn!("failed to send validation group update: {err}");
        }

        let result = match find_code_home() {
            Ok(home) => {
                let key = match group {
                    ValidationGroup::Functional => "functional",
                    ValidationGroup::Stylistic => "stylistic",
                };
                set_validation_group_enabled(&home, key, enable).map_err(|e| e.to_string())
            }
            Err(err) => Err(err.to_string()),
        };

        let label = Self::validation_group_label(group);
        if let Err(err) = result {
            self.push_background_tail(format!(
                "WARN: {} {} (persist failed: {err})",
                label,
                if enable { "enabled" } else { "disabled" }
            ));
        }

        self.refresh_settings_overview_rows();
    }

    fn apply_validation_tool_toggle(&mut self, name: &str, enable: bool) {
        if name == "actionlint" {
            if self.config.github.actionlint_on_patch == enable {
                return;
            }
            self.config.github.actionlint_on_patch = enable;
            if let Err(err) = self
                .code_op_tx
                .send(Op::UpdateValidationTool { name: name.to_string(), enable })
            {
                tracing::warn!("failed to send validation tool update: {err}");
            }
            let persist_result = match find_code_home() {
                Ok(home) => set_github_actionlint_on_patch(&home, enable)
                    .map_err(|e| e.to_string()),
                Err(err) => Err(err.to_string()),
            };
            if let Err(err) = persist_result {
                self.push_background_tail(format!(
                    "WARN: {}: {} (persist failed: {err})",
                    name,
                    if enable { "enabled" } else { "disabled" }
                ));
            }
            return;
        }

        let Some(flag) = self.validation_tool_flag_mut(name) else {
            self.push_background_tail(format!(
                "WARN: Unknown validation tool '{name}'"
            ));
            return;
        };

        if flag.unwrap_or(true) == enable {
            return;
        }

        *flag = Some(enable);
        if let Err(err) = self
            .code_op_tx
            .send(Op::UpdateValidationTool { name: name.to_string(), enable })
        {
            tracing::warn!("failed to send validation tool update: {err}");
        }
        let persist_result = match find_code_home() {
            Ok(home) => set_validation_tool_enabled(&home, name, enable)
                .map_err(|e| e.to_string()),
            Err(err) => Err(err.to_string()),
        };
        if let Err(err) = persist_result {
            self.push_background_tail(format!(
                "WARN: {}: {} (persist failed: {err})",
                name,
                if enable { "enabled" } else { "disabled" }
            ));
        }

        self.refresh_settings_overview_rows();
    }

    fn build_validation_status_message(&self) -> String {
        let mut lines = Vec::new();
        lines.push("Validation groups:".to_string());
        for group in [ValidationGroup::Functional, ValidationGroup::Stylistic] {
            let enabled = self.validation_group_enabled(group);
            lines.push(format!(
                "• {} — {}",
                Self::validation_group_label(group),
                if enabled { "enabled" } else { "disabled" }
            ));
        }
        lines.push(String::new());
        lines.push("Tools:".to_string());
        for status in crate::bottom_pane::settings_pages::validation::detect_tools() {
            let requested = self.validation_tool_requested(status.name);
            let effective = self.validation_tool_enabled(status.name);
            let mut state = if requested {
                if effective { "enabled".to_string() } else { "disabled (group off)".to_string() }
            } else {
                "disabled".to_string()
            };
            if !status.installed {
                state.push_str(" (not installed)");
            }
            lines.push(format!("• {} — {}", status.name, state));
        }
        lines.join("\n")
    }

    pub(crate) fn toggle_validation_tool(&mut self, name: &str, enable: bool) {
        self.apply_validation_tool_toggle(name, enable);
    }

    pub(crate) fn toggle_validation_group(&mut self, group: ValidationGroup, enable: bool) {
        self.apply_validation_group_toggle(group, enable);
    }

    pub(crate) fn handle_validation_command(&mut self, command_text: String) {
        let trimmed = command_text.trim();
        if trimmed.is_empty() {
            self.ensure_validation_settings_overlay();
            return;
        }

        let mut parts = trimmed.split_whitespace();
        match parts.next().unwrap_or("") {
            "status" => {
                let message = self.build_validation_status_message();
                self.push_background_tail(message);
            }
            "on" => {
                if !self.validation_group_enabled(ValidationGroup::Functional) {
                    self.apply_validation_group_toggle(ValidationGroup::Functional, true);
                }
            }
            "off" => {
                if self.validation_group_enabled(ValidationGroup::Functional) {
                    self.apply_validation_group_toggle(ValidationGroup::Functional, false);
                }
                if self.validation_group_enabled(ValidationGroup::Stylistic) {
                    self.apply_validation_group_toggle(ValidationGroup::Stylistic, false);
                }
            }
            group @ ("functional" | "stylistic") => {
                let Some(state) = parts.next() else {
                    self.push_background_tail("Usage: /validation <tool|group> on|off".to_string());
                    return;
                };
                let group = if group == "functional" {
                    ValidationGroup::Functional
                } else {
                    ValidationGroup::Stylistic
                };
                match state {
                    "on" | "enable" => self.apply_validation_group_toggle(group, true),
                    "off" | "disable" => self.apply_validation_group_toggle(group, false),
                    _ => self.push_background_tail(format!(
                        "WARN: Unknown validation command '{state}'. Use on|off."
                    )),
                }
            }
            tool => {
                let Some(state) = parts.next() else {
                    self.push_background_tail("Usage: /validation <tool|group> on|off".to_string());
                    return;
                };
                match state {
                    "on" | "enable" => self.apply_validation_tool_toggle(tool, true),
                    "off" | "disable" => self.apply_validation_tool_toggle(tool, false),
                    _ => self.push_background_tail(format!(
                        "WARN: Unknown validation command '{state}'. Use on|off."
                    )),
                }
            }
        }

        self.ensure_validation_settings_overlay();
    }

    fn format_mcp_summary(cfg: &code_core::config_types::McpServerConfig) -> String {
        code_core::mcp_snapshot::format_transport_summary(cfg)
    }

    fn format_mcp_status_report(rows: &[McpServerRow]) -> String {
        if rows.is_empty() {
            return "No MCP servers configured. Use /mcp add … to add one.".to_string();
        }

        let mut out = String::new();
        let enabled_count = rows.iter().filter(|row| row.enabled).count();
        let _ = writeln!(out, "Enabled ({enabled_count}):");
        for row in rows.iter().filter(|row| row.enabled) {
            let _ = writeln!(
                out,
                "• {} — {} · {} · Auth: {}",
                row.name, row.transport, row.status, row.auth_status
            );
            if let Some(timeout) = row.startup_timeout {
                let _ = writeln!(out, "  startup_timeout_sec: {:.3}", timeout.as_secs_f64());
            }
            if let Some(timeout) = row.tool_timeout {
                let _ = writeln!(out, "  tool_timeout_sec: {:.3}", timeout.as_secs_f64());
            }
            if !row.disabled_tools.is_empty() {
                let _ = writeln!(
                    out,
                    "  disabled_tools ({}): {}",
                    row.disabled_tools.len(),
                    row.disabled_tools.join(", ")
                );
            }
        }

        let disabled_count = rows.iter().filter(|row| !row.enabled).count();
        let _ = write!(out, "\nDisabled ({disabled_count}):\n");
        for row in rows.iter().filter(|row| !row.enabled) {
            let _ = writeln!(
                out,
                "• {} — {} · {} · Auth: {}",
                row.name, row.transport, row.status, row.auth_status
            );
        }

        out.trim_end().to_string()
    }

    fn format_mcp_tool_status(&self, name: &str, enabled: bool) -> String {
        if !enabled {
            return "Tools: disabled".to_string();
        }

        if let Some(failure) = self.mcp_server_failures.get(name) {
            return self.format_mcp_failure(failure);
        }

        if let Some(tools) = self.mcp_tools_by_server.get(name) {
            let list = Self::format_mcp_tool_list(tools);
            return format!("Tools: {list}");
        }

        "Tools: pending".to_string()
    }

    fn format_mcp_tool_list(tools: &[String]) -> String {
        const MAX_TOOLS: usize = 6;
        const MAX_CHARS: usize = 120;

        if tools.is_empty() {
            return "none".to_string();
        }

        let mut display = tools
            .iter()
            .take(MAX_TOOLS)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if tools.len() > MAX_TOOLS {
            let remaining = tools.len().saturating_sub(MAX_TOOLS);
            let _ = write!(display, ", +{remaining} more");
        }
        Self::truncate_with_ellipsis(&display, MAX_CHARS)
    }

    fn format_mcp_failure(&self, failure: &McpServerFailure) -> String {
        const MAX_CHARS: usize = 160;

        let summary = code_core::mcp_snapshot::format_failure_summary(failure);
        Self::truncate_with_ellipsis(&summary, MAX_CHARS)
    }

    /// Handle `/mcp` command: manage MCP servers (status/on/off/add).
    pub(crate) fn handle_mcp_command(&mut self, command_text: String) {
        let trimmed = command_text.trim();
        if trimmed.is_empty() {
            if !self.config.mcp_servers.is_empty() {
                self.submit_op(Op::ListMcpTools);
            }
            self.show_settings_overlay(Some(SettingsSection::Mcp));
            return;
        }

        let mut parts = trimmed.split_whitespace();
        let sub = parts.next().unwrap_or("");

        match sub {
            "status" => {
                if !self.config.mcp_servers.is_empty() {
                    self.submit_op(Op::ListMcpTools);
                }
                if let Some(rows) = self.build_mcp_server_rows() {
                    self.push_background_tail(Self::format_mcp_status_report(&rows));
                }
            }
            "on" | "off" => {
                let name = parts.next().unwrap_or("");
                if name.is_empty() {
                    let msg = format!("Usage: /mcp {sub} <name>");
                    self.history_push_plain_state(history_cell::new_error_event(msg));
                    return;
                }
                match find_code_home() {
                    Ok(home) => {
                        match code_core::config::set_mcp_server_enabled(&home, name, sub == "on") {
                            Ok(changed) => {
                                if changed {
                                    // Keep ChatWidget's in-memory config roughly in sync for new sessions.
                                    if sub == "off" {
                                        self.config.mcp_servers.remove(name);
                                    }
                                    if sub == "on" {
                                        // If enabling, try to load its config from disk and add to in-memory map.
                                        if let Ok((enabled, _)) =
                                            code_core::config::list_mcp_servers(&home)
                                            && let Some((_, cfg)) =
                                                enabled.into_iter().find(|(n, _)| n == name)
                                            {
                                                self.config
                                                    .mcp_servers
                                                    .insert(name.to_string(), cfg);
                                            }
                                    }
                                    let msg = format!(
                                        "{} MCP server '{}'",
                                        if sub == "on" { "Enabled" } else { "Disabled" },
                                        name
                                    );
                                    self.push_background_tail(msg);
                                } else {
                                    let msg = format!(
                                        "No change: server '{}' was already {}",
                                        name,
                                        if sub == "on" { "enabled" } else { "disabled" }
                                    );
                                    self.push_background_tail(msg);
                                }
                            }
                            Err(e) => {
                                let msg = format!("Failed to update MCP server '{name}': {e}");
                                self.history_push_plain_state(history_cell::new_error_event(msg));
                            }
                        }
                    }
                    Err(e) => {
                        let msg = format!("Failed to locate CODEX_HOME: {e}");
                        self.history_push_plain_state(history_cell::new_error_event(msg));
                    }
                }
            }
            "add" => {
                // Support two forms:
                //   1) /mcp add <name> <command> [args…] [ENV=VAL…]
                //   2) /mcp add <command> [args…] [ENV=VAL…]   (name derived)
                let tail_tokens: Vec<String> = parts.map(ToString::to_string).collect();
                if tail_tokens.is_empty() {
                    let msg = "Usage: /mcp add <name> <command> [args…] [ENV=VAL…]\n       or: /mcp add <command> [args…] [ENV=VAL…]".to_string();
                    self.history_push_plain_state(history_cell::new_error_event(msg));
                    return;
                }

                // Helper: derive a reasonable server name from command/args.
                fn derive_server_name(command: &str, tokens: &[String]) -> String {
                    // Prefer an npm-style package token if present.
                    let candidate = tokens
                        .iter()
                        .find(|t| {
                            !t.starts_with('-')
                                && !t.contains('=')
                                && (t.contains('/') || t.starts_with('@'))
                        })
                        .cloned();

                    let mut raw = match candidate {
                        Some(pkg) => {
                            // Strip scope, take the last path segment
                            let after_slash = pkg.rsplit('/').next().unwrap_or(pkg.as_str());
                            // Common convention: server-<name>
                            after_slash
                                .strip_prefix("server-")
                                .unwrap_or(after_slash)
                                .to_string()
                        }
                        None => command.to_string(),
                    };

                    // Sanitize: keep [a-zA-Z0-9_-], map others to '-'
                    raw = raw
                        .chars()
                        .map(|c| {
                            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                                c
                            } else {
                                '-'
                            }
                        })
                        .collect();
                    // Collapse multiple '-'
                    let mut out = String::with_capacity(raw.len());
                    let mut prev_dash = false;
                    for ch in raw.chars() {
                        if ch == '-' && prev_dash {
                            continue;
                        }
                        prev_dash = ch == '-';
                        out.push(ch);
                    }
                    // Ensure non-empty; fall back to "server"
                    if out.trim_matches('-').is_empty() {
                        "server".to_string()
                    } else {
                        out.trim_matches('-').to_string()
                    }
                }

                // Parse the two accepted forms
                let (name, command, rest_tokens) = if tail_tokens.len() >= 2 {
                    let first = &tail_tokens[0];
                    let second = &tail_tokens[1];
                    // If the presumed command looks like a flag, assume name was omitted.
                    if second.starts_with('-') {
                        let cmd = first.clone();
                        let name = derive_server_name(&cmd, &tail_tokens[1..]);
                        (name, cmd, tail_tokens[1..].to_vec())
                    } else {
                        (first.clone(), second.clone(), tail_tokens[2..].to_vec())
                    }
                } else {
                    // Only one token provided — treat it as a command and derive a name.
                    let cmd = tail_tokens[0].clone();
                    let name = derive_server_name(&cmd, &[]);
                    (name, cmd, Vec::new())
                };

                if command.is_empty() {
                    let msg = "Usage: /mcp add <name> <command> [args…] [ENV=VAL…]".to_string();
                    self.history_push_plain_state(history_cell::new_error_event(msg));
                    return;
                }

                // Separate args from ENV=VAL pairs
                let mut args: Vec<String> = Vec::new();
                let mut env: std::collections::HashMap<String, String> =
                    std::collections::HashMap::new();
                for tok in rest_tokens {
                    if let Some((k, v)) = tok.split_once('=') {
                        if !k.is_empty() {
                            env.insert(k.to_string(), v.to_string());
                        }
                    } else {
                        args.push(tok);
                    }
                }
                match find_code_home() {
                    Ok(home) => {
                        let transport = code_core::config_types::McpServerTransportConfig::Stdio {
                            command,
                            args: args.clone(),
                            env: if env.is_empty() { None } else { Some(env.clone()) },
                        };
                        let cfg = code_core::config_types::McpServerConfig {
                            transport,
                            startup_timeout_sec: None,
                            tool_timeout_sec: None,
                            scheduling: code_core::config_types::McpServerSchedulingToml::default(),
                            tool_scheduling: std::collections::BTreeMap::new(),
                            disabled_tools: Vec::new(),
                        };
                        match code_core::config::add_mcp_server(&home, &name, cfg.clone()) {
                            Ok(()) => {
                                let summary = Self::format_mcp_summary(&cfg);
                                // Update in-memory config for future sessions
                                self.config.mcp_servers.insert(name.clone(), cfg);
                                let msg = format!("Added MCP server '{name}': {summary}");
                                self.push_background_tail(msg);
                            }
                            Err(e) => {
                                let msg = format!("Failed to add MCP server '{name}': {e}");
                                self.history_push_plain_state(history_cell::new_error_event(msg));
                            }
                        }
                    }
                    Err(e) => {
                        let msg = format!("Failed to locate CODEX_HOME: {e}");
                        self.history_push_plain_state(history_cell::new_error_event(msg));
                    }
                }
            }
            _ => {
                let msg = format!(
                    "Unknown MCP command: '{sub}'\nUsage:\n  /mcp status\n  /mcp on <name>\n  /mcp off <name>\n  /mcp add <name> <command> [args…] [ENV=VAL…]"
                );
                self.history_push_plain_state(history_cell::new_error_event(msg));
            }
        }
    }

}
