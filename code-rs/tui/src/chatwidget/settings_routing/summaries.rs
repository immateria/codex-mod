impl ChatWidget<'_> {
    pub(super) fn build_settings_overview_rows(&mut self) -> Vec<SettingsOverviewRow> {
        SettingsSection::ALL
            .iter()
            .copied()
            .map(|section| {
                let summary = match section {
                    SettingsSection::Model         => self.settings_summary_model(),
                    SettingsSection::Theme         => self.settings_summary_theme(),
                    SettingsSection::Interface     => self.settings_summary_interface(),
                    SettingsSection::Shell         => self.settings_summary_shell(),
                    SettingsSection::ShellProfiles => self.settings_summary_shell_profiles(),
                    SettingsSection::ExecLimits    => self.settings_summary_exec_limits(),
                    SettingsSection::Planning      => self.settings_summary_planning(),
                    SettingsSection::Updates       => self.settings_summary_updates(),
                    SettingsSection::Accounts      => self.settings_summary_accounts(),
                    SettingsSection::Apps          => self.settings_summary_apps(),
                    SettingsSection::Agents        => self.settings_summary_agents(),
                    SettingsSection::Memories      => self.settings_summary_memories(),
                    SettingsSection::Prompts       => self.settings_summary_prompts(),
                    SettingsSection::Skills        => self.settings_summary_skills(),
                    SettingsSection::Plugins       => self.settings_summary_plugins(),
                    SettingsSection::AutoDrive     => self.settings_summary_auto_drive(),
                    SettingsSection::Review        => self.settings_summary_review(),
                    SettingsSection::Validation    => self.settings_summary_validation(),
                    SettingsSection::Chrome        => self.settings_summary_chrome(),
                    SettingsSection::Mcp           => self.settings_summary_mcp(),
                    SettingsSection::JsRepl        => self.settings_summary_js_repl(),
                    SettingsSection::Network       => self.settings_summary_network(),
                    SettingsSection::Notifications => self.settings_summary_notifications(),
                    SettingsSection::Limits        => self.settings_summary_limits(),
                };
                SettingsOverviewRow::new(section, summary)
            })
            .collect()
    }

    pub(super) fn settings_summary_apps(&self) -> Option<String> {
        let state = self
            .apps_shared_state
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let pins = state.sources_snapshot.pinned_account_ids.len();
        let mode = match state.sources_snapshot.mode {
            code_core::config_types::AppsSourcesModeToml::ActiveOnly => "active_only",
            code_core::config_types::AppsSourcesModeToml::ActivePlusPinned => "active_plus_pinned",
            code_core::config_types::AppsSourcesModeToml::PinnedOnly => "pinned_only",
        };
        Some(format!("Mode: {mode} · Pinned: {pins}"))
    }

    pub(super) fn settings_summary_plugins(&self) -> Option<String> {
        let state = self
            .plugins_shared_state
            .lock()
            .unwrap_or_else(|err| err.into_inner());

        match &state.list {
            crate::chatwidget::PluginsListState::Uninitialized => {
                Some("Browse and manage plugins".to_string())
            }
            crate::chatwidget::PluginsListState::Loading {
                force_remote_sync,
                ..
            } => Some(if *force_remote_sync {
                "Syncing plugins...".to_string()
            } else {
                "Loading plugins...".to_string()
            }),
            crate::chatwidget::PluginsListState::Failed { error, .. } => {
                Some(format!("Error: {error}"))
            }
            crate::chatwidget::PluginsListState::Ready {
                marketplaces,
                marketplace_load_errors,
                remote_sync_error,
                ..
            } => {
                let marketplace_count = marketplaces.len();
                let mut plugin_count = 0usize;
                let mut installed_count = 0usize;
                let mut enabled_installed_count = 0usize;

                for marketplace in marketplaces {
                    plugin_count = plugin_count.saturating_add(marketplace.plugins.len());
                    for plugin in &marketplace.plugins {
                        if plugin.installed {
                            installed_count = installed_count.saturating_add(1);
                            if plugin.enabled {
                                enabled_installed_count = enabled_installed_count.saturating_add(1);
                            }
                        }
                    }
                }

                let enabled_summary = if installed_count == 0 {
                    "Enabled: 0".to_string()
                } else {
                    format!("Enabled: {enabled_installed_count}/{installed_count}")
                };

                let mut summary_parts = vec![
                    format!("Installed: {installed_count}/{plugin_count}"),
                    enabled_summary,
                    format!("Marketplaces: {marketplace_count}"),
                ];

                if remote_sync_error.is_some() {
                    summary_parts.push("Sync error".to_string());
                }
                if !marketplace_load_errors.is_empty() {
                    summary_parts.push(format!("Load errors: {}", marketplace_load_errors.len()));
                }

                Some(summary_parts.join(" · "))
            }
        }
    }

    pub(super) fn settings_summary_js_repl(&self) -> Option<String> {
        let enabled = if self.config.tools_js_repl { "Enabled" } else { "Disabled" };
        let runtime = match self.config.js_repl_runtime {
            code_core::config::JsReplRuntimeKindToml::Node => "node",
            code_core::config::JsReplRuntimeKindToml::Deno => "deno",
        };
        let runtime_path = self
            .config
            .js_repl_runtime_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| "auto".to_string());

        Some(format!(
            "Status: {enabled} · Runtime: {runtime} · Path: {runtime_path}"
        ))
    }

    pub(super) fn settings_summary_exec_limits(&self) -> Option<String> {
        fn fmt_limit(limit: code_core::config::ExecLimitToml, unit: Option<&'static str>) -> String {
            match limit {
                code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Auto) => {
                    "Auto".to_string()
                }
                code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Disabled) => {
                    "Disabled".to_string()
                }
                code_core::config::ExecLimitToml::Value(v) => match unit {
                    Some(unit) => format!("{v} {unit}"),
                    None => v.to_string(),
                },
            }
        }

        Some(format!(
            "PIDs: {} · Mem: {}",
            fmt_limit(self.config.exec_limits.pids_max, None),
            fmt_limit(self.config.exec_limits.memory_max_mb, Some("MiB")),
        ))
    }

    pub(super) fn settings_summary_shell(&self) -> Option<String> {
        match self.config.shell.as_ref() {
            Some(shell) => {
                let style = shell
                    .script_style
                    .or_else(|| ShellScriptStyle::infer_from_shell_program(&shell.path))
                    .map(|style| style.to_string())
                    .unwrap_or_else(|| "auto".to_string());
                Some(format!("Shell: {} · Style: {style}", shell.path))
            }
            None => Some("Shell: auto".to_string()),
        }
    }

    pub(super) fn settings_summary_shell_profiles(&self) -> Option<String> {
        let active_style = self.config.shell.as_ref().and_then(|shell| {
            shell.script_style
                .or_else(|| ShellScriptStyle::infer_from_shell_program(&shell.path))
        });
        let Some(style) = active_style else {
            return Some("Active: auto".to_string());
        };
        let Some(profile) = self.config.shell_style_profiles.get(&style) else {
            return Some(format!("Active: {style} · (no overrides)"));
        };

        let refs = profile.references.len();
        let roots = profile.skill_roots.len();
        let include = profile.mcp_servers.include.len();
        let exclude = profile.mcp_servers.exclude.len();
        let skills = profile.skills.len();
        let disabled = profile.disabled_skills.len();

        Some(format!(
            "Active: {style} · refs:{refs} · roots:{roots} · skills:{skills}/{disabled} · mcp:+{include}/-{exclude}"
        ))
    }

    pub(super) fn settings_summary_interface(&self) -> Option<String> {
        let settings = &self.config.tui.settings_menu;
        let width = settings.overlay_min_width;
        match settings.open_mode {
            code_core::config_types::SettingsMenuOpenMode::Auto => {
                Some(format!("Mode: auto · Overlay >= {width}"))
            }
            code_core::config_types::SettingsMenuOpenMode::Overlay => Some("Mode: overlay".to_string()),
            code_core::config_types::SettingsMenuOpenMode::Bottom => Some("Mode: bottom".to_string()),
        }
    }

    pub(super) fn settings_summary_network(&self) -> Option<String> {
        let Some(network) = self.config.network.as_ref() else {
            return Some("Status: Disabled".to_string());
        };
        let status = if network.enabled { "Enabled" } else { "Disabled" };
        let mode = match network.mode {
            code_core::config::NetworkModeToml::Limited => "limited",
            code_core::config::NetworkModeToml::Full => "full",
        };
        Some(format!(
            "Status: {status} · Mode: {mode} · Allow: {} · Deny: {}",
            network.allowed_domains.len(),
            network.denied_domains.len()
        ))
    }

    pub(super) fn settings_summary_model(&self) -> Option<String> {
        let model = self.config.model.trim();
        let model_display_storage;
        let model_display = if model.is_empty() {
            "—"
        } else {
            model_display_storage = Self::format_model_label(model);
            &model_display_storage
        };
        let effort = Self::format_reasoning_effort(self.config.model_reasoning_effort);
        let mut parts: Vec<String> = vec![format!("Model: {} ({})", model_display, effort)];
        if code_core::model_family::supports_service_tier(&self.config.model)
            && matches!(
                self.config.service_tier,
                Some(code_core::config_types::ServiceTier::Fast)
            )
        {
            parts.push("Fast mode".to_string());
        }
        if let Some(profile) = self
            .config
            .active_profile
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            parts.push(format!("Profile: {profile}"));
        }
        Some(parts.join(" · "))
    }

    pub(super) fn settings_summary_planning(&self) -> Option<String> {
        if self.config.planning_use_chat_model {
            return Some("Model: Follow Chat Mode".to_string());
        }
        let model = self.config.planning_model.trim();
        let model_display_storage;
        let model_display = if model.is_empty() {
            "(default)"
        } else {
            model_display_storage = Self::format_model_label(model);
            &model_display_storage
        };
        let effort = Self::format_reasoning_effort(self.config.planning_model_reasoning_effort);
        Some(format!("Model: {model_display} ({effort})"))
    }

    pub(super) fn settings_summary_theme(&self) -> Option<String> {
        let theme_label = Self::theme_display_name(self.config.tui.theme.name);
        let spinner_name = &self.config.tui.spinner.name;
        let spinner_label = spinner::spinner_label_for(spinner_name);
        Some(format!("Theme: {theme_label} · Spinner: {spinner_label}"))
    }

    pub(super) fn settings_summary_updates(&self) -> Option<String> {
        if !crate::updates::upgrade_ui_enabled() {
            return Some("Auto update: Disabled".to_string());
        }
        let status = if self.config.auto_upgrade_enabled {
            "Enabled"
        } else {
            "Disabled"
        };
        let mut parts = vec![format!("Auto update: {}", status)];
        if let Some(latest) = self
            .latest_upgrade_version
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            parts.push(format!("Latest available: {latest}"));
        }
        Some(parts.join(" · "))
    }

    pub(super) fn settings_summary_accounts(&self) -> Option<String> {
        let auto_switch = if self.config.auto_switch_accounts_on_rate_limit {
            "Auto-switch: On"
        } else {
            "Auto-switch: Off"
        };

        let api_key_fallback = if self.config.api_key_fallback_on_all_accounts_limited {
            "API key fallback: On"
        } else {
            "API key fallback: Off"
        };

        Some(format!("{auto_switch} · {api_key_fallback}"))
    }

    pub(super) fn settings_summary_agents(&self) -> Option<String> {
        let (enabled, total) = agent_summary_counts(&self.config.agents);
        let commands = self.config.subagent_commands.len();
        let mut parts = vec![format!("Enabled: {}/{}", enabled, total)];
        if commands > 0 {
            parts.push(format!("Custom commands: {commands}"));
        }
        Some(parts.join(" · "))
    }

    pub(super) fn settings_summary_auto_drive(&self) -> Option<String> {
        let diagnostics_enabled = self.auto_state.qa_automation_enabled
            && (self.auto_state.review_enabled || self.auto_state.cross_check_enabled);
        let (model_text, effort_text) = if self.config.auto_drive_use_chat_model {
            ("Follow Chat Mode".to_string(), None)
        } else {
            let model_label = if self.config.auto_drive.model.trim().is_empty() {
                "(default)".to_string()
            } else {
                Self::format_model_label(&self.config.auto_drive.model)
            };
            let effort = Self::format_reasoning_effort(self.config.auto_drive.model_reasoning_effort);
            (model_label, Some(effort))
        };
        let model_segment = if let Some(effort) = effort_text {
            format!("Model: {model_text} ({effort})")
        } else {
            format!("Model: {model_text}")
        };
        Some(format!(
            "{} · Agents: {} · Diagnostics: {} · Continue: {}",
            model_segment,
            Self::on_off_label(self.auto_state.subagents_enabled),
            Self::on_off_label(diagnostics_enabled),
            self.auto_state.continue_mode.label()
        ))
    }

    pub(super) fn settings_summary_validation(&self) -> Option<String> {
        let groups = &self.config.validation.groups;
        Some(format!(
            "Functional: {} · Stylistic: {}",
            Self::on_off_label(groups.functional),
            Self::on_off_label(groups.stylistic)
        ))
    }

    pub(super) fn settings_summary_review(&self) -> Option<String> {
        let attempts = self.configured_auto_resolve_re_reviews();
        let auto_followups = self.config.auto_drive.auto_review_followup_attempts.get();

        let review_model_label = if self.config.review_use_chat_model {
            "Chat".to_string()
        } else {
            format!(
                "{} ({})",
                Self::format_model_label(&self.config.review_model),
                Self::format_reasoning_effort(self.config.review_model_reasoning_effort)
            )
        };

        let review_resolve_label = if self.config.review_resolve_use_chat_model {
            "Chat".to_string()
        } else {
            format!(
                "{} ({})",
                Self::format_model_label(&self.config.review_resolve_model),
                Self::format_reasoning_effort(self.config.review_resolve_model_reasoning_effort)
            )
        };

        let auto_review_model_label = if self.config.auto_review_use_chat_model {
            "Chat".to_string()
        } else {
            format!(
                "{} ({})",
                Self::format_model_label(&self.config.auto_review_model),
                Self::format_reasoning_effort(self.config.auto_review_model_reasoning_effort)
            )
        };

        let auto_review_resolve_label = if self.config.auto_review_resolve_use_chat_model {
            "Chat".to_string()
        } else {
            format!(
                "{} ({})",
                Self::format_model_label(&self.config.auto_review_resolve_model),
                Self::format_reasoning_effort(self.config.auto_review_resolve_model_reasoning_effort)
            )
        };

        Some(format!(
            "/review: {} · Resolve: {} · Follow-ups: {} · Auto Review: {} ({} · resolve {} · follow-ups {})",
            review_model_label,
            review_resolve_label,
            attempts,
            Self::on_off_label(self.config.tui.auto_review_enabled),
            auto_review_model_label,
            auto_review_resolve_label,
            auto_followups
        ))
    }

    pub(super) fn settings_summary_limits(&self) -> Option<String> {
        if let Some(snapshot) = &self.rate_limit_snapshot {
            let primary = snapshot.primary_used_percent.clamp(0.0, 100.0).round() as i64;
            let secondary = snapshot.secondary_used_percent.clamp(0.0, 100.0).round() as i64;
            Some(format!("Primary: {primary}% · Secondary: {secondary}%"))
        } else if self.rate_limit_fetch_inflight {
            Some("Refreshing usage...".to_string())
        } else {
            Some("Usage data not loaded".to_string())
        }
    }

    pub(super) fn settings_summary_chrome(&self) -> Option<String> {
        if self.browser_is_external {
            Some("Browser: external".to_string())
        } else {
            Some("Browser: available".to_string())
        }
    }

    pub(super) fn settings_summary_mcp(&self) -> Option<String> {
        Some(format!(
            "Servers configured: {}",
            self.config.mcp_servers.len()
        ))
    }

    pub(super) fn settings_summary_notifications(&self) -> Option<String> {
        match &self.config.tui.notifications {
            Notifications::Enabled(enabled) => {
                Some(format!("Desktop alerts: {}", Self::on_off_label(*enabled)))
            }
            Notifications::Custom(entries) => Some(format!("Custom rules: {}", entries.len())),
        }
    }

    pub(super) fn settings_summary_prompts(&self) -> Option<String> {
        let count = self.bottom_pane.custom_prompts().len();
        Some(format!("Prompts enabled: {count}"))
    }

    pub(super) fn settings_summary_skills(&self) -> Option<String> {
        let count = self.bottom_pane.skills().len();
        Some(format!("Skills loaded: {count}"))
    }

    pub(super) fn settings_summary_memories(&self) -> Option<String> {
        Some(format!(
            "Generate: {} · Use: {} · Skip polluted: {}",
            Self::on_off_label(self.config.memories.generate_memories),
            Self::on_off_label(self.config.memories.use_memories),
            Self::on_off_label(self.config.memories.no_memories_if_mcp_or_web_search),
        ))
    }

    pub(super) fn refresh_mcp_settings_overlay(&mut self) {
        let prior_state = self
            .settings
            .overlay
            .as_ref()
            .and_then(|overlay| overlay.mcp_content().map(crate::chatwidget::McpSettingsContent::snapshot_state));

        let mut content = self.build_mcp_settings_content();
        let Some(mut content) = content.take() else {
            return;
        };

        if let Some(state) = prior_state.as_ref() {
            content.restore_state(state);
        }

        let Some(overlay) = self.settings.overlay.as_mut() else {
            return;
        };
        overlay.set_mcp_content(content);
        self.request_redraw();
    }

    pub(super) fn refresh_settings_overview_rows(&mut self) {
        if self.settings.overlay.is_none() {
            return;
        }
        let rows = self.build_settings_overview_rows();
        if let Some(overlay) = self.settings.overlay.as_mut() {
            overlay.set_overview_rows(rows);
        }
        self.request_redraw();
    }

    pub(crate) fn apply_tui_settings_menu(
        &mut self,
        settings: code_core::config_types::SettingsMenuConfig,
    ) {
        self.config.tui.settings_menu = settings;
        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    pub(crate) fn apply_tui_hotkeys(
        &mut self,
        hotkeys: code_core::config_types::TuiHotkeysConfig,
    ) {
        self.config.tui.hotkeys = hotkeys;
        if self.help.overlay.is_some() {
            // Rebuild the overlay so the shortcut labels reflect the new mapping.
            self.show_help_popup();
        }
        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    pub(super) fn format_reasoning_effort(effort: ReasoningEffort) -> &'static str {
        match effort {
            ReasoningEffort::Minimal | ReasoningEffort::None => "Minimal",
            ReasoningEffort::Low => "Low",
            ReasoningEffort::Medium => "Medium",
            ReasoningEffort::High => "High",
            ReasoningEffort::XHigh => "XHigh",
        }
    }

    pub(super) fn format_model_label(model: &str) -> String {
        // Strip the internal "code-" prefix from agent models so user-facing labels
        // display the canonical model name (e.g., code-gpt-5.1-codex-mini -> GPT-5.1-Codex-Mini).
        let model = if model.to_ascii_lowercase().starts_with("code-") {
            &model[5..]
        } else {
            model
        };

        let mut parts = Vec::new();
        for (idx, part) in model.split('-').enumerate() {
            if idx == 0 {
                parts.push(part.to_ascii_uppercase());
                continue;
            }
            let mut chars = part.chars();
            let formatted = match chars.next() {
                Some(first) if first.is_ascii_alphabetic() => {
                    let mut s = String::new();
                    s.push(first.to_ascii_uppercase());
                    s.push_str(chars.as_str());
                    s
                }
                Some(first) => {
                    let mut s = String::new();
                    s.push(first);
                    s.push_str(chars.as_str());
                    s
                }
                None => String::new(),
            };
            parts.push(formatted);
        }
        parts.join("-")
    }

    pub(super) fn on_off_label(value: bool) -> &'static str {
        if value { "On" } else { "Off" }
    }

    pub(super) fn theme_display_name(theme: code_core::config_types::ThemeName) -> String {
        match theme {
            code_core::config_types::ThemeName::LightPhoton => "Light - Photon".to_string(),
            code_core::config_types::ThemeName::LightPhotonAnsi16 => {
                "Light - Photon (16-color)".to_string()
            }
            code_core::config_types::ThemeName::LightPrismRainbow => {
                "Light - Prism Rainbow".to_string()
            }
            code_core::config_types::ThemeName::LightVividTriad => {
                "Light - Vivid Triad".to_string()
            }
            code_core::config_types::ThemeName::LightPorcelain => "Light - Porcelain".to_string(),
            code_core::config_types::ThemeName::LightSandbar => "Light - Sandbar".to_string(),
            code_core::config_types::ThemeName::LightGlacier => "Light - Glacier".to_string(),
            code_core::config_types::ThemeName::DarkCarbonNight => {
                "Dark - Carbon Night".to_string()
            }
            code_core::config_types::ThemeName::DarkCarbonAnsi16 => {
                "Dark - Carbon (16-color)".to_string()
            }
            code_core::config_types::ThemeName::DarkShinobiDusk => {
                "Dark - Shinobi Dusk".to_string()
            }
            code_core::config_types::ThemeName::DarkOledBlackPro => {
                "Dark - OLED Black Pro".to_string()
            }
            code_core::config_types::ThemeName::DarkAmberTerminal => {
                "Dark - Amber Terminal".to_string()
            }
            code_core::config_types::ThemeName::DarkAuroraFlux => "Dark - Aurora Flux".to_string(),
            code_core::config_types::ThemeName::DarkCharcoalRainbow => {
                "Dark - Charcoal Rainbow".to_string()
            }
            code_core::config_types::ThemeName::DarkZenGarden => "Dark - Zen Garden".to_string(),
            code_core::config_types::ThemeName::DarkPaperLightPro => {
                "Dark - Paper Light Pro".to_string()
            }
            code_core::config_types::ThemeName::Custom => {
                let mut label =
                    crate::theme::custom_theme_label().unwrap_or_else(|| "Custom".to_string());
                for pref in ["Light - ", "Dark - ", "Light ", "Dark "] {
                    if label.starts_with(pref) {
                        label = label[pref.len()..].trim().to_string();
                        break;
                    }
                }
                if crate::theme::custom_theme_is_dark().unwrap_or(false) {
                    format!("Dark - {label}")
                } else {
                    format!("Light - {label}")
                }
            }
        }
    }

    pub(crate) fn close_settings_overlay(&mut self) {
        if let Some(overlay) = self.settings.overlay.as_mut() {
            overlay.notify_close();
        }
        self.settings.overlay = None;
        self.request_redraw();
    }

}
