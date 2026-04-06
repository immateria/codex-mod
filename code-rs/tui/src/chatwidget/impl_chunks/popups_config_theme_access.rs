impl ChatWidget<'_> {
    pub(crate) fn show_diffs_popup(&mut self) {
        use crate::diff_render::create_diff_details_only;
        // Build a latest-first unique file list
        let mut order: Vec<PathBuf> = Vec::new();
        let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        for changes in self.diffs.session_patch_sets.iter().rev() {
            for (path, change) in changes.iter() {
                // If this change represents a move/rename, show the destination path in the tabs
                let display_path: PathBuf = match change {
                    code_core::protocol::FileChange::Update {
                        move_path: Some(dest),
                        ..
                    } => dest.clone(),
                    _ => path.clone(),
                };
                if seen.insert(display_path.clone()) {
                    order.push(display_path);
                }
            }
        }
        // Build tabs: for each file, create a single unified diff against the original baseline
        let mut tabs: Vec<(String, Vec<DiffBlock>)> = Vec::new();
        for path in order {
            // Resolve baseline (first-seen content) and current (on-disk) content
            let baseline = self
                .diffs
                .baseline_file_contents
                .get(&path)
                .cloned()
                .unwrap_or_default();
            let current = std::fs::read_to_string(&path).unwrap_or_default();
            // Build a unified diff from baseline -> current
            let unified = diffy::create_patch(&baseline, &current).to_string();
            // Render detailed lines (no header) using our diff renderer helpers
            let mut single = HashMap::new();
            single.insert(
                path.clone(),
                code_core::protocol::FileChange::Update {
                    unified_diff: unified.clone(),
                    move_path: None,
                    original_content: baseline.clone(),
                    new_content: current.clone(),
                },
            );
            let detail = create_diff_details_only(&single);
            let mut blocks: Vec<DiffBlock> = vec![DiffBlock { lines: detail }];

            // Count adds/removes for the header label from the unified diff
            let mut total_added: usize = 0;
            let mut total_removed: usize = 0;
            if let Ok(patch) = diffy::Patch::from_str(&unified) {
                for h in patch.hunks() {
                    for l in h.lines() {
                        match l {
                            diffy::Line::Insert(_) => total_added += 1,
                            diffy::Line::Delete(_) => total_removed += 1,
                            _ => {}
                        }
                    }
                }
            } else {
                for l in unified.lines() {
                    if l.starts_with("+++") || l.starts_with("---") || l.starts_with("@@") {
                        continue;
                    }
                    if let Some(b) = l.as_bytes().first() {
                        if *b == b'+' {
                            total_added += 1;
                        } else if *b == b'-' {
                            total_removed += 1;
                        }
                    }
                }
            }
            // Prepend a header block with the full path and counts
            let header_line = {
                use ratatui::style::Modifier;
                use ratatui::style::Style;
                use ratatui::text::Line as RtLine;
                use ratatui::text::Span as RtSpan;
                let mut spans: Vec<RtSpan<'static>> = Vec::new();
                spans.push(RtSpan::styled(
                    path.display().to_string(),
                    Style::default()
                        .fg(crate::colors::text())
                        .add_modifier(Modifier::BOLD),
                ));
                spans.push(RtSpan::raw(" "));
                spans.push(RtSpan::styled(
                    format!("+{total_added}"),
                    Style::default().fg(crate::colors::success()),
                ));
                spans.push(RtSpan::raw(" "));
                spans.push(RtSpan::styled(
                    format!("-{total_removed}"),
                    Style::default().fg(crate::colors::error()),
                ));
                RtLine::from(spans)
            };
            blocks.insert(
                0,
                DiffBlock {
                    lines: vec![header_line],
                },
            );

            // Tab title: file name only
            let title = path
                .file_name()
                .and_then(|s| s.to_str())
                .map(ToString::to_string)
                .unwrap_or_else(|| path.display().to_string());
            tabs.push((title, blocks));
        }
        if tabs.is_empty() {
            // Nothing to show — surface a small notice so Ctrl+D feels responsive
            self.bottom_pane
                .flash_footer_notice("No diffs recorded this session".to_string());
            return;
        }
        self.diffs.overlay = Some(DiffOverlay::new(tabs));
        self.diffs.confirm = None;
        self.request_redraw();
    }

    pub(crate) fn toggle_diffs_popup(&mut self) {
        if self.diffs.overlay.is_some() {
            self.diffs.overlay = None;
            self.request_redraw();
        } else {
            self.show_diffs_popup();
        }
    }

    pub(crate) fn show_help_popup(&mut self) {
        let t_dim = Style::default().fg(crate::colors::text_dim());
        let t_fg = Style::default().fg(crate::colors::text());

        let mut lines: Vec<RtLine<'static>> = Vec::new();
        lines.push(RtLine::from(vec![RtSpan::styled(
            "Keyboard shortcuts",
            t_fg.add_modifier(Modifier::BOLD),
        )]));

        let kv = |k: &str, v: &str| -> RtLine<'static> {
            RtLine::from(vec![
                // Left-align the key column for improved readability
                RtSpan::styled(format!("{k:<12}"), t_fg),
                RtSpan::raw("  —  "),
                RtSpan::styled(v.to_string(), t_dim),
            ])
        };
        // Top quick action
        lines.push(kv(
            "Shift+Tab",
            "Rotate agent between Read Only / Write with Approval / Full Access",
        ));

        // Global
        let hotkeys = self.config.tui.hotkeys.effective_for_runtime();
        lines.push(kv("F1", "Help overlay"));
        let model_hotkey = hotkeys.model_selector.display_name();
        lines.push(kv(
            model_hotkey.as_ref(),
            "Model + reasoning selector",
        ));
        let reasoning_hotkey = hotkeys.reasoning_effort.display_name();
        lines.push(kv(
            reasoning_hotkey.as_ref(),
            "Cycle reasoning effort",
        ));
        let shell_hotkey = hotkeys.shell_selector.display_name();
        lines.push(kv(
            shell_hotkey.as_ref(),
            "Shell selector",
        ));
        let network_hotkey = hotkeys.network_settings.display_name();
        lines.push(kv(
            network_hotkey.as_ref(),
            "Network settings",
        ));
        let history_label = |hk: code_core::config_types::TuiHotkey, legacy_key: &str| -> String {
            if hk.is_legacy() {
                legacy_key.to_string()
            } else {
                hk.display_name().into_owned()
            }
        };
        let fold_exec_hotkey = history_label(hotkeys.exec_output_fold, "[");
        lines.push(kv(
            &fold_exec_hotkey,
            "Fold latest exec output/tool details (composer empty)",
        ));
        let fold_js_hotkey = history_label(hotkeys.js_repl_code_fold, "\\");
        lines.push(kv(
            &fold_js_hotkey,
            "Fold latest JS REPL code (composer empty)",
        ));
        let jump_parent_hotkey = history_label(hotkeys.jump_to_parent_call, "]");
        lines.push(kv(
            &jump_parent_hotkey,
            "Jump to parent tool call (composer empty)",
        ));
        let jump_child_hotkey = history_label(hotkeys.jump_to_latest_child_call, "}");
        lines.push(kv(
            &jump_child_hotkey,
            "Jump to latest spawned tool call (composer empty)",
        ));
        lines.push(kv("Ctrl+G", "Open external editor"));
        lines.push(kv("Ctrl+R", "Toggle reasoning"));
        lines.push(kv("Ctrl+T", "Toggle screen"));
        lines.push(kv("Ctrl+D", "Diff viewer"));
        lines.push(kv("Esc", &format!("{} / close popups", Self::double_esc_hint_label())));
        // Task control shortcuts
        lines.push(kv("Esc", "End current task"));
        lines.push(kv("Ctrl+C", "End current task"));
        lines.push(kv("Ctrl+C twice", "Quit"));
        lines.push(RtLine::from(""));

        // Composer
        lines.push(RtLine::from(vec![RtSpan::styled(
            "Compose field",
            t_fg.add_modifier(Modifier::BOLD),
        )]));
        lines.push(kv("Enter", "Send message"));
        lines.push(kv("Ctrl+J", "Insert newline"));
        lines.push(kv("Shift+Enter", "Insert newline"));
        // Split combined shortcuts into separate rows for readability
        lines.push(kv("Shift+Up", "Browse input history"));
        lines.push(kv("Shift+Down", "Browse input history"));
        lines.push(kv("Ctrl+B", "Move left"));
        lines.push(kv("Ctrl+F", "Move right"));
        lines.push(kv("Alt+Left", "Move by word"));
        lines.push(kv("Alt+Right", "Move by word"));
        // Simplify delete shortcuts; remove Alt+Backspace/Backspace/Delete variants
        lines.push(kv("Ctrl+W", "Delete previous word"));
        lines.push(kv("Ctrl+H", "Delete previous char"));
        lines.push(kv("Ctrl+D", "Delete next char"));
        lines.push(kv("Ctrl+Backspace", "Delete current line"));
        lines.push(kv("Ctrl+U", "Delete to line start"));
        lines.push(kv("Ctrl+K", "Delete to line end"));
        lines.push(kv(
            "Home/End",
            "Jump to line start/end (jump to history start/end when input is empty)",
        ));
        lines.push(RtLine::from(""));

        lines.push(RtLine::from(vec![RtSpan::styled(
            "Terminal",
            t_fg.add_modifier(Modifier::BOLD),
        )]));
        lines.push(kv("$", "Open shell terminal without a preset command"));
        lines.push(kv("$ <command>", "Run shell command immediately"));
        lines.push(kv("$$ <prompt>", "Request guided shell command help"));
        lines.push(RtLine::from(""));

        // Panels
        lines.push(RtLine::from(vec![RtSpan::styled(
            "Panels",
            t_fg.add_modifier(Modifier::BOLD),
        )]));
        lines.push(kv("Ctrl+B", "Toggle Browser overlay"));
        lines.push(kv("Ctrl+A", "Open Agents terminal"));

        // Slash command reference
        lines.push(RtLine::from(""));
        lines.push(RtLine::from(vec![RtSpan::styled(
            "Slash commands",
            t_fg.add_modifier(Modifier::BOLD),
        )]));
        for (cmd_str, cmd) in crate::slash_command::built_in_slash_commands() {
            // Hide internal test command from the Help panel
            if cmd_str == "test-approval" {
                continue;
            }
            // Prefer "Code" branding in the Help panel
            let desc = cmd.description().replace("Codex", "Code");
            // Render as "/command  —  description"
            lines.push(RtLine::from(vec![
                RtSpan::styled(format!("/{cmd_str:<12}"), t_fg),
                RtSpan::raw("  —  "),
                RtSpan::styled(desc.clone(), t_dim),
            ]));
        }

        self.help.overlay = Some(HelpOverlay::new(lines));
        self.request_redraw();
    }

    pub(crate) fn toggle_help_popup(&mut self) {
        if self.help.overlay.is_some() {
            self.help.overlay = None;
        } else {
            self.show_help_popup();
        }
        self.request_redraw();
    }

    pub(crate) fn set_auto_upgrade_enabled(&mut self, enabled: bool) {
        if self.config.auto_upgrade_enabled == enabled {
            return;
        }
        self.config.auto_upgrade_enabled = enabled;

        let code_home = self.config.code_home.clone();
        let profile = self.config.active_profile.clone();
        tokio::spawn(async move {
            if let Err(err) = code_core::config_edit::persist_overrides(
                &code_home,
                profile.as_deref(),
                &[(&["auto_upgrade_enabled"], if enabled { "true" } else { "false" })],
            )
            .await
            {
                tracing::warn!("failed to persist auto-upgrade setting: {err}");
            }
        });

        let notice = if enabled {
            "Automatic upgrades enabled"
        } else {
            "Automatic upgrades disabled"
        };
        self.bottom_pane.flash_footer_notice(notice.to_string());

        let should_refresh_updates = matches!(
            self.settings
                .overlay
                .as_ref()
                .map(settings_overlay::SettingsOverlayView::active_section),
            Some(SettingsSection::Updates)
        );

        if should_refresh_updates
            && let Some(content) = self.build_updates_settings_content()
                && let Some(overlay) = self.settings.overlay.as_mut() {
                    overlay.set_updates_content(content);
                }
        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    pub(crate) fn set_auto_switch_accounts_on_rate_limit(&mut self, enabled: bool) {
        if self.config.auto_switch_accounts_on_rate_limit == enabled {
            return;
        }
        self.config.auto_switch_accounts_on_rate_limit = enabled;

        let code_home = self.config.code_home.clone();
        let profile = self.config.active_profile.clone();
        tokio::spawn(async move {
            if let Err(err) = code_core::config_edit::persist_overrides(
                &code_home,
                profile.as_deref(),
                &[(&["auto_switch_accounts_on_rate_limit"], if enabled { "true" } else { "false" })],
            )
            .await
            {
                tracing::warn!("failed to persist account auto-switch setting: {err}");
            }
        });

        let notice = if enabled {
            "Auto-switch accounts enabled"
        } else {
            "Auto-switch accounts disabled"
        };
        self.bottom_pane.flash_footer_notice(notice.to_string());

        let should_refresh_accounts = matches!(
            self.settings
                .overlay
                .as_ref()
                .map(settings_overlay::SettingsOverlayView::active_section),
            Some(SettingsSection::Accounts)
        );
        if should_refresh_accounts {
            let content = self.build_accounts_settings_content();
            if let Some(overlay) = self.settings.overlay.as_mut() {
                overlay.set_accounts_content(content);
            }
        }

        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    pub(crate) fn set_api_key_fallback_on_all_accounts_limited(&mut self, enabled: bool) {
        if self.config.api_key_fallback_on_all_accounts_limited == enabled {
            return;
        }
        self.config.api_key_fallback_on_all_accounts_limited = enabled;

        let code_home = self.config.code_home.clone();
        let profile = self.config.active_profile.clone();
        tokio::spawn(async move {
            if let Err(err) = code_core::config_edit::persist_overrides(
                &code_home,
                profile.as_deref(),
                &[(&["api_key_fallback_on_all_accounts_limited"], if enabled { "true" } else { "false" })],
            )
            .await
            {
                tracing::warn!("failed to persist API key fallback setting: {err}");
            }
        });

        let notice = if enabled {
            "API key fallback enabled"
        } else {
            "API key fallback disabled"
        };
        self.bottom_pane.flash_footer_notice(notice.to_string());

        let should_refresh_accounts = matches!(
            self.settings
                .overlay
                .as_ref()
                .map(settings_overlay::SettingsOverlayView::active_section),
            Some(SettingsSection::Accounts)
        );
        if should_refresh_accounts {
            let content = self.build_accounts_settings_content();
            if let Some(overlay) = self.settings.overlay.as_mut() {
                overlay.set_accounts_content(content);
            }
        }

        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    pub(crate) fn flash_footer_notice(&mut self, text: String) {
        self.bottom_pane.flash_footer_notice(text);
        self.request_redraw();
    }

    pub(crate) fn refresh_accounts_settings_content(&mut self) {
        let should_refresh_accounts = matches!(
            self.settings
                .overlay
                .as_ref()
                .map(settings_overlay::SettingsOverlayView::active_section),
            Some(SettingsSection::Accounts)
        );
        if should_refresh_accounts {
            let content = self.build_accounts_settings_content();
            if let Some(overlay) = self.settings.overlay.as_mut() {
                overlay.set_accounts_content(content);
            }
        }

        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    pub(crate) fn set_cli_auth_credentials_store_mode(
        &mut self,
        mode: code_core::config_types::AuthCredentialsStoreMode,
    ) {
        if self.config.cli_auth_credentials_store_mode == mode {
            self.refresh_accounts_settings_content();
            return;
        }
        self.config.cli_auth_credentials_store_mode = mode;

        let label = match mode {
            code_core::config_types::AuthCredentialsStoreMode::File => "file",
            code_core::config_types::AuthCredentialsStoreMode::Keyring => "keyring",
            code_core::config_types::AuthCredentialsStoreMode::Auto => "auto",
            code_core::config_types::AuthCredentialsStoreMode::Ephemeral => "ephemeral",
        };
        self.bottom_pane
            .flash_footer_notice(format!("Credential store: {label}"));

        self.refresh_accounts_settings_content();
    }

    /// Forward file-search results to the bottom pane.
    pub(crate) fn apply_file_search_result(&mut self, query: String, matches: Vec<FileMatch>) {
        self.bottom_pane.on_file_search_result(query, matches);
    }


    // Ctrl+Y syntax cycling disabled intentionally.

    /// Show a brief debug notice in the footer.
    pub(crate) fn debug_notice(&mut self, text: String) {
        self.bottom_pane.flash_footer_notice(text);
        self.request_redraw();
    }

    fn maybe_start_auto_upgrade_task(&mut self) {
        if !crate::updates::auto_upgrade_runtime_enabled() {
            return;
        }
        if !self.config.auto_upgrade_enabled {
            return;
        }

        let cfg = self.config.clone();
        let tx = self.app_event_tx.clone();
        let upgrade_ticket = self.make_background_tail_ticket();
        tokio::spawn(async move {
            match crate::updates::auto_upgrade_if_enabled(&cfg).await {
                Ok(outcome) => {
                    if let Some(version) = outcome.installed_version {
                        tx.send(AppEvent::AutoUpgradeCompleted { version });
                    }
                    if let Some(message) = outcome.user_notice {
                        tx.send_background_event_with_ticket(&upgrade_ticket, message);
                    }
                }
                Err(err) => {
                    tracing::warn!("auto-upgrade: background task failed: {err:?}");
                }
            }
        });
    }

    pub(crate) fn set_theme(&mut self, new_theme: code_core::config_types::ThemeName) {
        let custom_hint = if matches!(new_theme, code_core::config_types::ThemeName::Custom) {
            self.config
                .tui
                .theme
                .is_dark
                .or_else(crate::theme::custom_theme_is_dark)
        } else {
            None
        };
        let mapped_theme = crate::theme::map_theme_for_palette(new_theme, custom_hint);

        // Update the config
        self.config.tui.theme.name = mapped_theme;
        if matches!(new_theme, code_core::config_types::ThemeName::Custom) {
            self.config.tui.theme.is_dark = custom_hint;
        } else {
            self.config.tui.theme.is_dark = None;
        }

        // Save the theme to config file
        self.save_theme_to_config(mapped_theme);

        // Retint pre-rendered history cell lines to the new palette
        self.restyle_history_after_theme_change();

        // Add confirmation message to history (replaceable system notice)
        let theme_name = Self::theme_display_name(mapped_theme);
        let message = format!("Theme changed to {theme_name}");
        let placement = self.ui_placement_for_now();
        let cell = history_cell::new_background_event(message);
        let record = HistoryDomainRecord::BackgroundEvent(cell.state().clone());
        self.push_system_cell(
            Box::new(cell),
            placement,
            Some("ui:theme".to_string()),
            None,
            "background",
            Some(record),
        );
        self.refresh_settings_overview_rows();
    }

    pub(crate) fn set_spinner(&mut self, spinner_name: String) {
        // Update the config
        self.config.tui.spinner.name = spinner_name.clone();
        // Persist selection to config file
        if let Ok(home) = code_core::config::find_code_home() {
            if let Err(e) = code_core::config::set_tui_spinner_name(&home, &spinner_name) {
                tracing::warn!("Failed to persist spinner to config.toml: {}", e);
            } else {
                tracing::info!("Persisted TUI spinner selection to config.toml");
            }
        } else {
            tracing::warn!("Could not locate Codex home to persist spinner selection");
        }

        // Confirmation message (replaceable system notice)
        let message = format!("Spinner changed to {spinner_name}");
        let placement = self.ui_placement_for_now();
        let cell = history_cell::new_background_event(message);
        let record = HistoryDomainRecord::BackgroundEvent(cell.state().clone());
        self.push_system_cell(
            Box::new(cell),
            placement,
            Some("ui:spinner".to_string()),
            None,
            "background",
            Some(record),
        );

        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    fn apply_access_mode_indicator_from_config(&mut self) {
        use code_core::protocol::AskForApproval;
        use code_core::protocol::SandboxPolicy;
        let label = match (&self.config.sandbox_policy, self.config.approval_policy) {
            (SandboxPolicy::ReadOnly, _) => Some("Read Only".to_string()),
            (
                SandboxPolicy::WorkspaceWrite {
                    network_access: false,
                    ..
                },
                AskForApproval::UnlessTrusted,
            ) => Some("Write with Approval".to_string()),
            _ => None,
        };
        self.bottom_pane.set_access_mode_label(label);
    }

    pub(crate) fn current_collaboration_mode(&self) -> CollaborationModeKind {
        self.collaboration_mode
    }

    pub(crate) fn current_configure_session_op(&self) -> Op {
        Op::configure_session(code_core::protocol::ConfigureSessionOp {
            provider: self.config.model_provider.clone(),
            model: self.config.model.clone(),
            model_explicit: self.config.model_explicit,
            model_reasoning_effort: self.config.model_reasoning_effort,
            preferred_model_reasoning_effort: self.config.preferred_model_reasoning_effort,
            model_reasoning_summary: self.config.model_reasoning_summary,
            model_text_verbosity: self.config.model_text_verbosity,
            service_tier: self.config.service_tier,
            context_mode: self.config.context_mode,
            model_context_window: self.config.model_context_window,
            model_auto_compact_token_limit: self.config.model_auto_compact_token_limit,
            user_instructions: self.config.user_instructions.clone(),
            base_instructions: self.config.base_instructions.clone(),
            approval_policy: self.config.approval_policy,
            sandbox_policy: self.config.sandbox_policy.clone(),
            disable_response_storage: self.config.disable_response_storage,
            notify: self.config.notify.clone(),
            cwd: self.config.cwd.clone(),
            resume_path: None,
            demo_developer_message: self.config.demo_developer_message.clone(),
            dynamic_tools: Vec::new(),
            shell: self.config.shell.clone(),
            shell_style_profiles: self.config.shell_style_profiles.clone(),
            network: self.config.network.clone(),
            tools_js_repl: self.config.tools_js_repl,
            js_repl_runtime: self.config.js_repl_runtime,
            js_repl_runtime_path: self.config.js_repl_runtime_path.clone(),
            js_repl_runtime_args: self.config.js_repl_runtime_args.clone(),
            js_repl_node_module_dirs: self.config.js_repl_node_module_dirs.clone(),
            memories: self.config.memories.clone(),
            collaboration_mode: self.current_collaboration_mode(),
        })
    }

    /// Rotate the access preset: Read Only (Plan Mode) → Write with Approval → Full Access
    pub(crate) fn cycle_access_mode(&mut self) {
        use code_core::config::set_project_access_mode;
        use code_core::protocol::AskForApproval;
        use code_core::protocol::SandboxPolicy;

        // Determine current index
        let idx = match (&self.config.sandbox_policy, self.config.approval_policy) {
            (SandboxPolicy::ReadOnly, _) => 0,
            (
                SandboxPolicy::WorkspaceWrite {
                    network_access: false,
                    ..
                },
                AskForApproval::UnlessTrusted,
            ) => 1,
            (SandboxPolicy::DangerFullAccess, AskForApproval::Never) => 2,
            _ => 0,
        };
        let next = (idx + 1) % 3;
        self.collaboration_mode = if next == 0 {
            CollaborationModeKind::Plan
        } else {
            CollaborationModeKind::Default
        };

        // Apply mapping
        let (label, approval, sandbox) = match next {
            0 => (
                "Read Only (Plan Mode)",
                AskForApproval::OnRequest,
                SandboxPolicy::ReadOnly,
            ),
            1 => (
                "Write with Approval",
                AskForApproval::UnlessTrusted,
                SandboxPolicy::new_workspace_write_policy(),
            ),
            _ => (
                "Full Access",
                AskForApproval::Never,
                SandboxPolicy::DangerFullAccess,
            ),
        };

        // Apply planning model when entering plan mode; restore when leaving it.
        if next == 0 {
            self.apply_planning_session_model();
        } else if idx == 0 {
            self.restore_planning_session_model();
        }

        // Update local config
        self.config.approval_policy = approval;
        self.config.sandbox_policy = sandbox;

        // Send ConfigureSession op to backend
        let op = self.current_configure_session_op();
        self.submit_op(op);

        // Persist selection into CODEX_HOME/config.toml for this project directory so it sticks.
        let _ = set_project_access_mode(
            &self.config.code_home,
            &self.config.cwd,
            self.config.approval_policy,
            match &self.config.sandbox_policy {
                SandboxPolicy::ReadOnly => code_protocol::config_types::SandboxMode::ReadOnly,
                SandboxPolicy::WorkspaceWrite { .. } => {
                    code_protocol::config_types::SandboxMode::WorkspaceWrite
                }
                SandboxPolicy::DangerFullAccess => {
                    code_protocol::config_types::SandboxMode::DangerFullAccess
                }
            },
        );

        // Footer indicator: persistent for RO/Approval; ephemeral for Full Access
        if next == 2 {
            self.bottom_pane.set_access_mode_label_ephemeral(
                "Full Access".to_string(),
                std::time::Duration::from_secs(4),
            );
        } else {
            let persistent = if next == 0 {
                "Read Only"
            } else {
                "Write with Approval"
            };
            self.bottom_pane
                .set_access_mode_label(Some(persistent.to_string()));
        }

        // Announce in history: replace the last access-mode status, inserting early
        // in the current request so it appears above upcoming commands.
        let msg = format!("Mode changed: {label}");
        self.set_access_status_message(msg);
        // No footer notice: the indicator covers this; avoid duplicate texts.

        // Prepare a single consolidated note for the agent to see before the
        // next turn begins. Subsequent cycles will overwrite this note.
        let agent_note = match next {
            0 => {
                "System: access mode changed to Read Only. Do not attempt write operations or apply_patch."
            }
            1 => {
                "System: access mode changed to Write with Approval. Request approval before writes."
            }
            _ => "System: access mode changed to Full Access. Writes and network are allowed.",
        };
        self.queue_agent_note(agent_note);
    }

    pub(crate) fn cycle_auto_drive_variant(&mut self) {
        self.auto_drive_variant = self.auto_drive_variant.next();
        self
            .bottom_pane
            .set_auto_drive_variant(self.auto_drive_variant);
        let notice = format!(
            "Auto Drive style: {}",
            self.auto_drive_variant.name()
        );
        self.bottom_pane.flash_footer_notice(notice);
    }

    /// Insert or replace the access-mode status background event. Uses a near-time
    /// key so it appears above any imminent Exec/Tool cells in this request.
    fn set_access_status_message(&mut self, message: String) {
        let cell = crate::history_cell::new_background_event(message);
        if let Some(idx) = self.access_status_idx
            && idx < self.history_cells.len()
                && matches!(
                    self.history_cells[idx].kind(),
                    crate::history_cell::HistoryCellType::BackgroundEvent
                )
            {
                self.history_replace_at(idx, Box::new(cell));
                self.request_redraw();
                return;
            }
        // Insert new status near the top of this request window
        let key = self.near_time_key(None);
        let pos = self.history_insert_with_key_global_tagged(Box::new(cell), key, "background", None);
        self.access_status_idx = Some(pos);
    }

    fn restyle_history_after_theme_change(&mut self) {
        let old = self.last_theme.clone();
        let new = crate::theme::current_theme();
        if old == new {
            return;
        }

        for cell in &mut self.history_cells {
            if let Some(plain) = cell
                .as_any_mut()
                .downcast_mut::<history_cell::PlainHistoryCell>()
            {
                plain.invalidate_layout_cache();
            } else if let Some(tool) = cell
                .as_any_mut()
                .downcast_mut::<history_cell::ToolCallCell>()
            {
                tool.retint(&old, &new);
            } else if let Some(reason) = cell
                .as_any_mut()
                .downcast_mut::<history_cell::CollapsibleReasoningCell>()
            {
                reason.retint(&old, &new);
            } else if let Some(stream) = cell
                .as_any_mut()
                .downcast_mut::<history_cell::StreamingContentCell>()
            {
                stream.update_context(self.config.file_opener, &self.config.cwd);
            } else if let Some(wait) = cell
                .as_any_mut()
                .downcast_mut::<history_cell::WaitStatusCell>()
            {
                wait.retint(&old, &new);
            } else if let Some(assist) = cell
                .as_any_mut()
                .downcast_mut::<history_cell::AssistantMarkdownCell>()
            {
                // Fully rebuild from raw to apply new theme + syntax highlight
                let current = assist.state().clone();
                assist.update_state(current, &self.config);
            } else if let Some(merged) = cell
                .as_any_mut()
                .downcast_mut::<history_cell::MergedExecCell>()
            {
                merged.rebuild_with_theme();
            } else if let Some(diff) = cell
                .as_any_mut()
                .downcast_mut::<history_cell::DiffCell>()
            {
                diff.rebuild_with_theme();
            }
        }

        // Update snapshot and redraw; height caching can remain (colors don't affect wrap)
        self.last_theme = new;
        self.render_theme_epoch = self.render_theme_epoch.saturating_add(1);
        self.history_render.invalidate_all();
        self.mark_render_requests_dirty();
        self.history_prefix_append_only.set(false);
        self.app_event_tx.send(AppEvent::RequestRedraw);
    }

    /// Public-facing hook for preview mode to retint existing history lines
    /// without persisting the theme or adding history events.
    pub(crate) fn retint_history_for_preview(&mut self) {
        self.restyle_history_after_theme_change();
    }

    fn save_theme_to_config(&self, new_theme: code_core::config_types::ThemeName) {
        // Persist the theme selection to CODE_HOME/CODEX_HOME config.toml
        match code_core::config::find_code_home() {
            Ok(home) => {
                if let Err(e) = code_core::config::set_tui_theme_name(&home, new_theme) {
                    tracing::warn!("Failed to persist theme to config.toml: {}", e);
                } else {
                    tracing::info!("Persisted TUI theme selection to config.toml");
                }
            }
            Err(e) => {
                tracing::warn!("Could not locate Codex home to persist theme: {}", e);
            }
        }
    }

}
