impl ChatWidget<'_> {
    pub(super) fn build_model_settings_view(&self) -> ModelSelectionView {
        let presets = self.available_model_presets();
        let current_model = self.config.model.clone();
        let current_effort = self.config.model_reasoning_effort;
        ModelSelectionView::new(
            ModelSelectionViewParams {
                presets,
                current_model,
                current_effort,
                current_service_tier: self.config.service_tier,
                current_context_mode: self.config.context_mode,
                current_context_window: self.config.model_context_window,
                current_auto_compact_token_limit: self.config.model_auto_compact_token_limit,
                use_chat_model: false,
                target: ModelSelectionTarget::Session,
            },
            self.app_event_tx.clone(),
        )
    }

    pub(super) fn build_model_settings_content(&self) -> ModelSettingsContent {
        ModelSettingsContent::new(self.build_model_settings_view())
    }

    pub(super) fn build_theme_settings_content(&mut self) -> ThemeSettingsContent {
        let tail_ticket = self.make_background_tail_ticket();
        let before_ticket = self.make_background_before_next_output_ticket();
        let view = ThemeSelectionView::new(
            crate::theme::current_theme_name(),
            self.app_event_tx.clone(),
            tail_ticket,
            before_ticket,
        );
        ThemeSettingsContent::new(view)
    }

    pub(super) fn build_interface_settings_view(&mut self) -> InterfaceSettingsView {
        InterfaceSettingsView::new(
            self.config.code_home.clone(),
            self.config.tui.settings_menu.clone(),
            self.config.tui.hotkeys.clone(),
            self.app_event_tx.clone(),
        )
    }

    pub(super) fn build_interface_settings_content(&mut self) -> InterfaceSettingsContent {
        InterfaceSettingsContent::new(self.build_interface_settings_view())
    }

    pub(super) fn build_shell_settings_content(&mut self) -> ShellSettingsContent {
        let presets = self.available_shell_presets();
        let view = ShellSelectionView::new(self.config.shell.clone(), presets, self.app_event_tx.clone());
        ShellSettingsContent::new(view)
    }

    pub(super) fn build_shell_profiles_settings_content(&mut self) -> ShellProfilesSettingsContent {
        let skills = self
            .bottom_pane
            .skills()
            .iter()
            .map(|skill| (skill.name.clone(), skill.description.clone()))
            .collect::<Vec<_>>();
        let mcp_servers = self
            .config
            .mcp_servers
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let view = ShellProfilesSettingsView::new(
            self.config.code_home.clone(),
            self.config.shell.as_ref(),
            self.config.shell_style_profiles.clone(),
            skills,
            mcp_servers,
            self.app_event_tx.clone(),
        );
        ShellProfilesSettingsContent::new(view)
    }

    pub(super) fn build_notifications_settings_view(&mut self) -> NotificationsSettingsView {
        let mode = match &self.config.tui.notifications {
            Notifications::Enabled(enabled) => NotificationsMode::Toggle { enabled: *enabled },
            Notifications::Custom(entries) => NotificationsMode::Custom { entries: entries.clone() },
        };
        let ticket = self.make_background_tail_ticket();
        NotificationsSettingsView::new(mode, self.app_event_tx.clone(), ticket)
    }

    pub(super) fn build_notifications_settings_content(&mut self) -> NotificationsSettingsContent {
        NotificationsSettingsContent::new(self.build_notifications_settings_view())
    }

    pub(super) fn build_memories_settings_view(&self) -> MemoriesSettingsView {
        self.app_event_tx.send(crate::app_event::AppEvent::RunMemoriesStatusLoad {
            target: crate::app_event::MemoriesStatusLoadTarget::SettingsView,
        });
        MemoriesSettingsView::new(
            self.config.code_home.clone(),
            self.config.cwd.clone(),
            self.config.active_profile.clone(),
            self.config.global_memories.clone(),
            self.config.active_profile_memories.clone(),
            self.config.project_memories.clone(),
            self.app_event_tx.clone(),
        )
    }

    pub(super) fn build_memories_settings_content(&self) -> MemoriesSettingsContent {
        MemoriesSettingsContent::new(self.build_memories_settings_view())
    }

    pub(super) fn build_network_settings_view(&mut self) -> NetworkSettingsView {
        let ticket = self.make_background_tail_ticket();
        NetworkSettingsView::new(
            self.config.network.clone(),
            self.config.sandbox_policy.clone(),
            self.app_event_tx.clone(),
            ticket,
        )
    }

    pub(super) fn build_network_settings_content(&mut self) -> NetworkSettingsContent {
        NetworkSettingsContent::new(self.build_network_settings_view())
    }

    pub(super) fn build_exec_limits_settings_view(&mut self) -> ExecLimitsSettingsView {
        ExecLimitsSettingsView::new(self.config.exec_limits.clone(), self.app_event_tx.clone())
    }

    pub(super) fn build_exec_limits_settings_content(&mut self) -> ExecLimitsSettingsContent {
        ExecLimitsSettingsContent::new(self.build_exec_limits_settings_view())
    }

    pub(super) fn build_js_repl_settings_view(&mut self) -> JsReplSettingsView {
        let ticket = self.make_background_tail_ticket();
        let settings = code_core::config::JsReplSettingsToml {
            enabled: self.config.tools_js_repl,
            runtime: self.config.js_repl_runtime,
            runtime_path: self.config.js_repl_runtime_path.clone(),
            runtime_args: self.config.js_repl_runtime_args.clone(),
            node_module_dirs: self.config.js_repl_node_module_dirs.clone(),
        };
        let network_enabled = self
            .config
            .network
            .as_ref()
            .is_some_and(|net| net.enabled);
        JsReplSettingsView::new(settings, network_enabled, self.app_event_tx.clone(), ticket)
    }

    pub(super) fn build_js_repl_settings_content(&mut self) -> JsReplSettingsContent {
        JsReplSettingsContent::new(self.build_js_repl_settings_view())
    }

    pub(super) fn build_prompts_settings_view(&mut self) -> PromptsSettingsView {
        let prompts = self.bottom_pane.custom_prompts().to_vec();
        PromptsSettingsView::new(prompts, self.app_event_tx.clone())
    }

    pub(super) fn build_prompts_settings_content(&mut self) -> PromptsSettingsContent {
        PromptsSettingsContent::new(self.build_prompts_settings_view())
    }

    pub(super) fn build_skills_settings_view(&mut self) -> SkillsSettingsView {
        let skills = self.bottom_pane.skills().to_vec();
        SkillsSettingsView::new(
            skills,
            self.config.shell_style_profiles.clone(),
            self.app_event_tx.clone(),
        )
    }

    pub(super) fn build_skills_settings_content(&mut self) -> SkillsSettingsContent {
        SkillsSettingsContent::new(self.build_skills_settings_view())
    }

    pub(super) fn build_apps_settings_view(&mut self) -> AppsSettingsView {
        self.apps_set_sources_snapshot(
            self.config.active_profile.clone(),
            self.config.apps_sources.clone(),
        );

        let code_home = self.config.code_home.clone();
        let active_account_id =
            code_core::auth_accounts::get_active_account_id(&code_home).unwrap_or_default();
        let mut accounts = code_core::auth_accounts::list_accounts(&code_home).unwrap_or_default();
        accounts.sort_by(|left, right| {
            left.label
                .as_deref()
                .unwrap_or(left.id.as_str())
                .cmp(right.label.as_deref().unwrap_or(right.id.as_str()))
                .then_with(|| left.id.cmp(&right.id))
        });
        let mut snapshot_accounts = accounts
            .into_iter()
            .map(|account| {
                let label = account
                    .label
                    .clone()
                    .unwrap_or_else(|| account.id.clone());
                crate::chatwidget::AppsAccountSnapshot {
                    id: account.id.clone(),
                    label,
                    is_chatgpt: account.mode.is_chatgpt(),
                    is_active_model_account: active_account_id.as_deref() == Some(account.id.as_str()),
                }
            })
            .collect::<Vec<_>>();
        snapshot_accounts.sort_by(|left, right| {
            right
                .is_active_model_account
                .cmp(&left.is_active_model_account)
                .then_with(|| left.label.cmp(&right.label))
                .then_with(|| left.id.cmp(&right.id))
        });
        self.apps_set_accounts_snapshot(snapshot_accounts);

        let shared_state = self.apps_shared_state();
        AppsSettingsView::new(shared_state, self.app_event_tx.clone())
    }

    pub(super) fn build_apps_settings_content(&mut self) -> AppsSettingsContent {
        AppsSettingsContent::new(self.build_apps_settings_view())
    }

    pub(super) fn build_plugins_settings_view(&mut self) -> PluginsSettingsView {
        self.plugins_set_sources_snapshot(self.config.plugins.clone());
        let shared_state = self.plugins_shared_state();
        let roots = code_utils_absolute_path::AbsolutePathBuf::try_from(self.config.cwd.clone())
            .ok()
            .into_iter()
            .collect::<Vec<_>>();
        PluginsSettingsView::new(shared_state, roots, self.app_event_tx.clone())
    }

    pub(super) fn build_plugins_settings_content(&mut self) -> PluginsSettingsContent {
        PluginsSettingsContent::new(self.build_plugins_settings_view())
    }

    pub(super) fn build_chrome_settings_content(&self, port: Option<u16>) -> ChromeSettingsContent {
        ChromeSettingsContent::new(self.app_event_tx.clone(), port)
    }

    pub(super) fn build_mcp_server_rows(&mut self) -> Option<McpServerRows> {
        let home = match code_core::config::find_code_home() {
            Ok(home) => home,
            Err(e) => {
                let msg = format!("Failed to locate CODE_HOME: {e}");
                self.history_push_plain_state(history_cell::new_error_event(msg));
                return None;
            }
        };

        let runtime_snapshot = code_core::mcp_snapshot::McpRuntimeSnapshot {
            tools_by_server: self.mcp_tools_by_server.clone(),
            disabled_tools_by_server: self.mcp_disabled_tools_by_server.clone(),
            auth_statuses: self.mcp_auth_statuses.clone(),
            failures: self.mcp_server_failures.clone(),
        };
        let resources_by_server = &self.mcp_resources_by_server;
        let resource_templates_by_server = &self.mcp_resource_templates_by_server;

        let merged_servers = match code_core::mcp_snapshot::merge_servers(&home, &runtime_snapshot) {
            Ok(result) => result,
            Err(e) => {
                let msg = format!("Failed to read MCP config: {e}");
                self.history_push_plain_state(history_cell::new_error_event(msg));
                return None;
            }
        };

        let mut rows: McpServerRows = Vec::new();
        for server in merged_servers {
            let server_name = server.name.clone();
            let transport = Self::format_mcp_summary(&server.config);
            let status = self.format_mcp_tool_status(&server_name, server.enabled);
            let failure = server
                .failure
                .as_ref()
                .map(|entry| self.format_mcp_failure(entry));
            let mut tool_definitions = std::collections::BTreeMap::new();
            for tool_name in server.tools.iter().chain(server.disabled_tools.iter()) {
                if let Some(tool) = self.mcp_tool_definition_for(&server_name, tool_name) {
                    tool_definitions.insert(tool_name.clone(), tool);
                }
            }
            let resources = resources_by_server
                .get(&server_name)
                .cloned()
                .unwrap_or_default();
            let resource_templates = resource_templates_by_server
                .get(&server_name)
                .cloned()
                .unwrap_or_default();
            let mut resources = resources;
            let mut resource_templates = resource_templates;
            resources.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.uri.cmp(&b.uri)));
            resource_templates.sort_by(|a, b| {
                a.name
                    .cmp(&b.name)
                    .then_with(|| a.uri_template.cmp(&b.uri_template))
            });
            rows.push(McpServerRow {
                name: server.name,
                enabled: server.enabled,
                transport,
                auth_status: server.auth_status,
                startup_timeout: server.config.startup_timeout_sec,
                tool_timeout: server.config.tool_timeout_sec,
                scheduling: server.config.scheduling.clone(),
                tool_scheduling: server.config.tool_scheduling.clone(),
                tools: server.tools,
                disabled_tools: server.disabled_tools,
                resources,
                resource_templates,
                tool_definitions,
                failure,
                status,
            });
        }
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        Some(rows)
    }

    pub(super) fn build_mcp_settings_content(&mut self) -> Option<McpSettingsContent> {
        let rows = self.build_mcp_server_rows()?;
        let view = McpSettingsView::new(rows, self.app_event_tx.clone());
        Some(McpSettingsContent::new(view))
    }

    pub(super) fn is_builtin_agent(name: &str, command: &str) -> bool {
        if let Some(spec) = agent_model_spec(name).or_else(|| agent_model_spec(command)) {
            return matches!(spec.family, "code" | "codex" | "cloud");
        }

        name.eq_ignore_ascii_case("code")
            || name.eq_ignore_ascii_case("codex")
            || name.eq_ignore_ascii_case("cloud")
            || name.eq_ignore_ascii_case("coder")
            || command.eq_ignore_ascii_case("code")
            || command.eq_ignore_ascii_case("codex")
            || command.eq_ignore_ascii_case("cloud")
            || command.eq_ignore_ascii_case("coder")
    }

    pub(super) fn collect_agents_overview_rows(&self) -> (Vec<AgentOverviewRow>, Vec<String>) {
        fn command_exists(cmd: &str) -> bool {
            if cmd.contains(std::path::MAIN_SEPARATOR) || cmd.contains('/') || cmd.contains('\\') {
                return std::fs::metadata(cmd).map(|m| m.is_file()).unwrap_or(false);
            }
            #[cfg(target_os = "windows")]
            {
                which::which(cmd).map(|p| p.is_file()).unwrap_or(false)
            }
            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::PermissionsExt;
                let Some(path_os) = std::env::var_os("PATH") else {
                    return false;
                };
                for dir in std::env::split_paths(&path_os) {
                    if dir.as_os_str().is_empty() {
                        continue;
                    }
                    let candidate = dir.join(cmd);
                    if let Ok(meta) = std::fs::metadata(&candidate)
                        && meta.is_file() && (meta.permissions().mode() & 0o111 != 0) {
                            return true;
                        }
                }
                false
            }
        }

        fn command_for_check(command: &str) -> String {
            let (base, _) = split_command_and_args(command);
            if base.trim().is_empty() {
                command.trim().to_string()
            } else {
                base
            }
        }

        let mut agent_rows: Vec<AgentOverviewRow> = Vec::new();
        let mut ordered: Vec<String> = enabled_agent_model_specs()
            .into_iter()
            .map(|spec| spec.slug.to_string())
            .collect();
        let mut extras: Vec<String> = Vec::new();
        for agent in &self.config.agents {
            if !ordered.iter().any(|name| agent.name.eq_ignore_ascii_case(name)) {
                extras.push(agent.name.to_ascii_lowercase());
            }
        }
        let mut pending_agents: HashMap<String, AgentConfig> = HashMap::new();
        for pending in self.pending_agent_updates.values() {
            let lower = pending.cfg.name.to_ascii_lowercase();
            pending_agents.insert(lower.clone(), pending.cfg.clone());
            if !ordered.iter().any(|name| name.eq_ignore_ascii_case(&lower)) {
                extras.push(lower);
            }
        }
        extras.sort();
        for extra in extras {
            if !ordered.iter().any(|name| name.eq_ignore_ascii_case(&extra)) {
                ordered.push(extra);
            }
        }

        for name in ordered.iter() {
            let name_lower = name.to_ascii_lowercase();
            if let Some(cfg) = self
                .config
                .agents
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case(name))
            {
                let builtin = Self::is_builtin_agent(&cfg.name, &cfg.command);
                let spec_cli = agent_model_spec(&cfg.name)
                    .or_else(|| agent_model_spec(&cfg.command))
                    .map(|spec| spec.cli);
                let command_to_check = command_for_check(&cfg.command);
                let installed = builtin
                    || command_exists(&command_to_check)
                    || spec_cli.is_some_and(command_exists);
                agent_rows.push(AgentOverviewRow {
                    name: cfg.name.clone(),
                    enabled: cfg.enabled && installed,
                    installed,
                    description: Self::agent_description_for(
                        &cfg.name,
                        Some(&cfg.command),
                        cfg.description.as_deref(),
                    ),
                });
            } else if let Some(cfg) = pending_agents.get(&name_lower) {
                let builtin = Self::is_builtin_agent(&cfg.name, &cfg.command);
                let spec_cli = agent_model_spec(&cfg.name)
                    .or_else(|| agent_model_spec(&cfg.command))
                    .map(|spec| spec.cli);
                let command_to_check = command_for_check(&cfg.command);
                let installed = builtin
                    || command_exists(&command_to_check)
                    || spec_cli.is_some_and(command_exists);
                agent_rows.push(AgentOverviewRow {
                    name: cfg.name.clone(),
                    enabled: cfg.enabled && installed,
                    installed,
                    description: Self::agent_description_for(
                        &cfg.name,
                        Some(&cfg.command),
                        cfg.description.as_deref(),
                    ),
                });
            } else {
                let cmd = name.clone();
                let builtin = Self::is_builtin_agent(name, &cmd);
                let spec_cli = agent_model_spec(name).map(|spec| spec.cli);
                let installed = builtin || spec_cli.is_some_and(command_exists) || command_exists(&cmd);
                agent_rows.push(AgentOverviewRow {
                    name: name.clone(),
                    enabled: installed,
                    installed,
                    description: Self::agent_description_for(name, Some(&cmd), None),
                });
            }
        }

        let mut commands: Vec<String> = vec!["plan".into(), "solve".into(), "code".into()];
        let custom: Vec<String> = self
            .config
            .subagent_commands
            .iter()
            .map(|c| c.name.clone())
            .filter(|name| !commands.iter().any(|builtin| builtin.eq_ignore_ascii_case(name)))
            .collect();
        commands.extend(custom);

        (agent_rows, commands)
    }

    pub(super) fn agent_description_for(
        name: &str,
        command: Option<&str>,
        config_description: Option<&str>,
    ) -> Option<String> {
        if let Some(desc) = config_description {
            let trimmed = desc.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        agent_model_spec(name)
            .or_else(|| command.and_then(agent_model_spec))
            .map(|spec| spec.description.trim().to_string())
            .filter(|desc| !desc.is_empty())
    }

    pub(super) fn build_agents_settings_content(&mut self) -> AgentsSettingsContent {
        let (rows, commands) = self.collect_agents_overview_rows();
        let total = rows
            .len()
            .saturating_add(commands.len())
            .saturating_add(AGENTS_OVERVIEW_STATIC_ROWS);
        let selected = if total == 0 {
            0
        } else {
            self.agents_overview_selected_index.min(total.saturating_sub(1))
        };
        self.agents_overview_selected_index = selected;
        AgentsSettingsContent::new_overview(rows, commands, selected, self.app_event_tx.clone())
    }

    pub(super) fn build_limits_settings_content(&mut self) -> LimitsSettingsContent {
        let snapshot = self.rate_limit_snapshot.clone();
        let needs_refresh = self.should_refresh_limits();

        let content = if self.rate_limit_fetch_inflight || needs_refresh {
            LimitsOverlayContent::Loading
        } else {
            let reset_info = self.rate_limit_reset_info();
            let tabs = self.build_limits_tabs(snapshot.clone(), reset_info);
            if tabs.is_empty() {
                LimitsOverlayContent::Placeholder
            } else {
                LimitsOverlayContent::Tabs(tabs)
            }
        };

        if needs_refresh {
            self.request_latest_rate_limits(snapshot.is_none());
        }

        LimitsSettingsContent::new(content, self.config.tui.limits.layout_mode)
    }

    pub(super) fn sync_limits_layout_mode_preference(&mut self) {
        let Some(overlay) = self.settings.overlay.as_ref() else {
            return;
        };
        let Some(limits) = overlay.limits_content() else {
            return;
        };

        let layout_mode = limits.layout_mode_config();
        if self.config.tui.limits.layout_mode == layout_mode {
            return;
        }

        self.config.tui.limits.layout_mode = layout_mode;
        if let Err(err) = code_core::config::set_tui_limits_layout_mode(
            &self.config.code_home,
            layout_mode,
        ) {
            tracing::warn!("Failed to persist limits layout mode: {err}");
        }
    }

}
