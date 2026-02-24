use super::*;
use code_core::config_types::ShellStyleProfileConfig;

impl ChatWidget<'_> {

    pub(crate) fn handle_shell_command(&mut self, args: String) {
        let args = args.trim();
        if args.is_empty() {
            self.show_shell_selector();
            return;
        }

        if args == "?" {
            let current_shell = self
                .config
                .shell
                .as_ref()
                .map(Self::format_shell_config)
                .unwrap_or_else(|| "auto-detected".to_string());
            self.history_push_plain_paragraphs(
                crate::history::state::PlainMessageKind::Notice,
                vec![format!("Current shell: {current_shell}")],
            );
            return;
        }

        let shell_config = match Self::parse_shell_command_override(args, self.config.shell.as_ref()) {
            Ok(shell_config) => shell_config,
            Err(error) => {
                self.history_push_plain_state(history_cell::new_error_event(format!(
                    "Invalid /shell value: {error}",
                )));
                return;
            }
        };

        self.update_shell_config(shell_config);
        self.history_push_plain_paragraphs(
            crate::history::state::PlainMessageKind::Notice,
            vec!["Updating shell setting...".to_string()],
        );
    }

    pub(super) fn available_shell_presets(&self) -> Vec<ShellPreset> {
        let user_presets: Vec<ShellPreset> = self
            .config
            .tui
            .shell_presets
            .iter()
            .filter_map(Self::shell_preset_from_config)
            .collect();
        merge_shell_presets(user_presets)
    }

    fn shell_preset_from_config(preset: &ShellPresetConfig) -> Option<ShellPreset> {
        let id = preset.id.trim();
        let command = preset.command.trim();
        if id.is_empty() || command.is_empty() {
            return None;
        }

        let display_name = if preset.display_name.trim().is_empty() {
            command.to_string()
        } else {
            preset.display_name.trim().to_string()
        };

        Some(ShellPreset {
            id: id.to_string(),
            command: command.to_string(),
            display_name,
            description: preset.description.trim().to_string(),
            default_args: preset.default_args.clone(),
            script_style: preset.script_style.map(|style| style.to_string()),
            show_in_picker: preset.show_in_picker,
        })
    }

    fn parse_shell_command_override(
        input: &str,
        current_shell: Option<&ShellConfig>,
    ) -> Result<Option<ShellConfig>, String> {
        if input == "-" || input.eq_ignore_ascii_case("clear") {
            return Ok(None);
        }

        let Some(mut parts) = shlex::split(input) else {
            return Err("could not parse command (check quoting)".to_string());
        };

        let mut explicit_style: Option<ShellScriptStyle> = None;
        if parts.first().map(String::as_str) == Some("--style") {
            if parts.len() < 2 {
                return Err("missing value after --style".to_string());
            }
            let style_value = parts.remove(1);
            parts.remove(0);
            explicit_style = Some(
                ShellScriptStyle::parse(style_value.as_str())
                    .ok_or_else(|| {
                        format!(
                            "unknown style `{style_value}` (expected one of: posix-sh, bash-zsh-compatible, zsh)",
                        )
                    })?,
            );
        }

        if parts.is_empty() {
            let Some(existing_shell) = current_shell else {
                return Err("missing shell executable".to_string());
            };
            let mut shell = existing_shell.clone();
            if let Some(style) = explicit_style {
                shell.script_style = Some(style);
            }
            return Ok(Some(shell));
        }

        let path = parts.remove(0);
        let script_style = explicit_style.or_else(|| ShellScriptStyle::infer_from_shell_program(&path));
        Ok(Some(Self::build_shell_config(
            path,
            parts,
            script_style,
            current_shell,
        )))
    }

    pub(super) fn build_shell_config(
        path: String,
        args: Vec<String>,
        script_style: Option<ShellScriptStyle>,
        current_shell: Option<&ShellConfig>,
    ) -> ShellConfig {
        let (command_safety, dangerous_command_detection) = current_shell
            .map(|shell| (shell.command_safety.clone(), shell.dangerous_command_detection))
            .unwrap_or((code_core::config_types::CommandSafetyProfileConfig::default(), None));

        ShellConfig {
            path,
            args,
            script_style,
            command_safety,
            dangerous_command_detection,
        }
    }

    fn format_shell_config(shell: &ShellConfig) -> String {
        if shell.args.is_empty() {
            shell.path.clone()
        } else {
            let path = &shell.path;
            let args = shell.args.join(" ");
            format!("{path} {args}")
        }
    }

    fn submit_configure_session_for_current_settings(&self) {
        let op = Op::ConfigureSession {
            provider: self.config.model_provider.clone(),
            model: self.config.model.clone(),
            model_explicit: self.config.model_explicit,
            model_reasoning_effort: self.config.model_reasoning_effort,
            preferred_model_reasoning_effort: self.config.preferred_model_reasoning_effort,
            model_reasoning_summary: self.config.model_reasoning_summary,
            model_text_verbosity: self.config.model_text_verbosity,
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
            collaboration_mode: self.current_collaboration_mode(),
        };
        self.submit_op(op);
    }

    fn persist_shell_config(
        &self,
        attempted_shell: Option<ShellConfig>,
        previous_shell: Option<ShellConfig>,
    ) {
        let code_home = self.config.code_home.clone();
        let tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            match persist_shell(&code_home, attempted_shell.as_ref()).await {
                Ok(()) => {
                    tx.send(AppEvent::ShellPersisted {
                        shell: attempted_shell,
                    });
                }
                Err(error) => {
                    tx.send(AppEvent::ShellPersistFailed {
                        attempted_shell,
                        previous_shell,
                        error: error.to_string(),
                    });
                }
            }
        });
    }

    pub(super) fn update_shell_config(&mut self, shell: Option<ShellConfig>) {
        let previous_shell = self.config.shell.clone();
        self.config.shell = shell.clone();
        self.request_redraw();
        self.submit_configure_session_for_current_settings();
        self.persist_shell_config(shell, previous_shell);
    }

    pub(crate) fn apply_shell_style_profiles(
        &mut self,
        shell_style_profiles: std::collections::HashMap<ShellScriptStyle, ShellStyleProfileConfig>,
    ) {
        self.config.shell_style_profiles = shell_style_profiles;
        self.submit_configure_session_for_current_settings();
        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    pub(crate) fn apply_network_proxy_settings(
        &mut self,
        settings: Option<code_core::config::NetworkProxySettingsToml>,
    ) {
        self.config.network = settings;
        self.submit_configure_session_for_current_settings();
        self.refresh_settings_overview_rows();
        self.request_redraw();
    }

    pub(crate) fn on_shell_persisted(&mut self, shell: Option<ShellConfig>) {
        if self.config.shell != shell {
            return;
        }

        let message = match shell.as_ref() {
            Some(shell) => {
                let label = Self::format_shell_config(shell);
                format!("Shell set to: {label}")
            }
            None => "Shell setting cleared.".to_string(),
        };
        self.push_background_tail(message);
    }

    pub(crate) fn on_shell_persist_failed(
        &mut self,
        attempted_shell: Option<ShellConfig>,
        previous_shell: Option<ShellConfig>,
        error: String,
    ) {
        if self.config.shell != attempted_shell {
            return;
        }

        self.push_background_tail(format!("Failed to persist shell setting: {error}"));
        self.config.shell = previous_shell;
        self.request_redraw();
        self.submit_configure_session_for_current_settings();

        let restored = match self.config.shell.as_ref() {
            Some(shell) => {
                let label = Self::format_shell_config(shell);
                format!("Restored previous shell: {label}")
            }
            None => "Restored previous shell: auto-detected".to_string(),
        };
        self.push_background_tail(restored);
    }
}
