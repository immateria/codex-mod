            match event {
                AppEvent::SetAutoSwitchAccountsOnRateLimit(enabled) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_auto_switch_accounts_on_rate_limit(enabled);
                    }
                    self.config.auto_switch_accounts_on_rate_limit = enabled;
                }
                AppEvent::SetApiKeyFallbackOnAllAccountsLimited(enabled) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_api_key_fallback_on_all_accounts_limited(enabled);
                    }
                    self.config.api_key_fallback_on_all_accounts_limited = enabled;
                }
                AppEvent::RequestSetAuthCredentialsStoreMode { mode, migrate_existing } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        let label = auth_credentials_store_mode_label(mode);
                        widget.flash_footer_notice(format!(
                            "Applying credential store: {label}…"
                        ));
                    }

                    let code_home = self.config.code_home.clone();
                    let old_mode = self.config.cli_auth_credentials_store_mode;
                    let auth_manager = self._server.auth_manager();
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let result: Result<bool, String> = async {
                            if mode == AuthCredentialsStoreMode::Keyring {
                                tokio::task::spawn_blocking({
                                    let code_home = code_home.clone();
                                    move || {
                                        code_core::auth::load_auth_dot_json(
                                            &code_home,
                                            AuthCredentialsStoreMode::Keyring,
                                        )
                                        .map(|_| ())
                                    }
                                })
                                .await
                                .map_err(|err| format!("keyring validation task failed: {err}"))?
                                .map_err(|err| err.to_string())?;
                            }

                            if migrate_existing {
                                tokio::task::spawn_blocking({
                                    let code_home = code_home.clone();
                                    move || -> std::io::Result<()> {
                                        if let Some(auth) = code_core::auth::load_auth_dot_json(
                                            &code_home,
                                            old_mode,
                                        )? {
                                            code_core::auth::save_auth(&code_home, &auth, mode)?;
                                        }
                                        code_core::auth_accounts::migrate_accounts_store_mode(
                                            &code_home,
                                            old_mode,
                                            mode,
                                        )?;
                                        Ok(())
                                    }
                                })
                                .await
                                .map_err(|err| format!("migration task failed: {err}"))?
                                .map_err(|err| err.to_string())?;
                            }

                            code_core::config_edit::persist_root_overrides(
                                &code_home,
                                &[(
                                    &["cli_auth_credentials_store"],
                                    auth_credentials_store_mode_label(mode),
                                )],
                            )
                            .await
                            .map_err(|err| err.to_string())?;

                            let using_chatgpt_auth = tokio::task::spawn_blocking({
                                let auth_manager = auth_manager.clone();
                                move || {
                                    auth_manager.set_auth_credentials_store_mode(mode);
                                    auth_manager.reload();
                                    auth_manager
                                        .auth()
                                        .is_some_and(|auth| auth.mode.is_chatgpt())
                                }
                            })
                            .await
                            .map_err(|err| format!("auth reload task failed: {err}"))?;

                            Ok(using_chatgpt_auth)
                        }
                        .await;

                        match result {
                            Ok(using_chatgpt_auth) => {
                                tx.send(AppEvent::AuthCredentialsStoreModeApplied {
                                    mode,
                                    using_chatgpt_auth,
                                });
                            }
                            Err(error) => {
                                tx.send(AppEvent::AuthCredentialsStoreModeApplyFailed {
                                    mode,
                                    error,
                                });
                            }
                        }
                    });
                }
                AppEvent::AuthCredentialsStoreModeApplied { mode, using_chatgpt_auth } => {
                    self.config.cli_auth_credentials_store_mode = mode;

                    let changed_using_chatgpt =
                        self.config.using_chatgpt_auth != using_chatgpt_auth;
                    if changed_using_chatgpt {
                        self.config.using_chatgpt_auth = using_chatgpt_auth;
                    }

                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_cli_auth_credentials_store_mode(mode);
                        if changed_using_chatgpt {
                            widget.set_using_chatgpt_auth(using_chatgpt_auth);
                        }
                    }

                    if changed_using_chatgpt {
                        self.spawn_remote_model_discovery();
                    }

                    self.schedule_redraw();
                }
                AppEvent::AuthCredentialsStoreModeApplyFailed { mode, error } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        let mode_label = auth_credentials_store_mode_label(mode);
                        widget.flash_footer_notice(format!(
                            "Failed to set credential store to {mode_label}: {error}"
                        ));
                        widget.refresh_accounts_settings_content();
                    }
                    self.schedule_redraw();
                }
                AppEvent::ShowAutoDriveSettings => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_auto_drive_settings();
                    }
                }
                AppEvent::CloseAutoDriveSettings => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.close_auto_drive_settings();
                    }
                }
                AppEvent::AutoDriveSettingsChanged(update) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_auto_drive_settings(update);
                    }
                }
                AppEvent::RequestAgentInstall { name, selected_index } => {
                    if let AppState::Chat { widget } = &mut self.app_state
                        && let Some(launch) = widget.launch_agent_install(name, selected_index) {
                            self.app_event_tx.send(AppEvent::OpenTerminal(launch));
                        }
                }
                AppEvent::AgentsOverviewSelectionChanged { index } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_agents_overview_selection(index);
                    }
                }
                // fallthrough handled by break
                AppEvent::CodexOp(op) => match &mut self.app_state {
                    AppState::Chat { widget } => widget.submit_op(*op),
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::RequestUserInputAnswer { turn_id, response } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.on_request_user_input_answer(turn_id, response);
                    }
                }
                AppEvent::AutoCoordinatorDecision {
                    seq,
                    status,
                    status_title,
                    status_sent_to_user,
                    goal,
                    cli,
                    agents_timing,
                    agents,
                    transcript,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_decision(crate::chatwidget::AutoDecisionEvent {
                            seq,
                            status,
                            status_title,
                            status_sent_to_user,
                            goal,
                            cli,
                            agents_timing,
                            agents,
                            transcript,
                        });
                    }
                }
                AppEvent::AutoCoordinatorUserReply {
                    user_response,
                    cli_command,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_user_reply(user_response, cli_command);
                    }
                }
                AppEvent::AutoCoordinatorThinking { delta, summary_index } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_thinking(delta, summary_index);
                    }
                }
                AppEvent::AutoCoordinatorAction { message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_action(message);
                    }
                }
                AppEvent::AutoCoordinatorTokenMetrics {
                    total_usage,
                    last_turn_usage,
                    turn_count,
                    duplicate_items,
                    replay_updates,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_token_metrics(
                            total_usage,
                            last_turn_usage,
                            turn_count,
                            duplicate_items,
                            replay_updates,
                        );
                    }
                }
                AppEvent::AutoCoordinatorStopAck => {
                    // Coordinator acknowledged stop; no additional action required currently.
                }
                AppEvent::AutoCoordinatorCompactedHistory { conversation, show_notice } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_compacted_history(conversation, show_notice);
                    }
                }
                AppEvent::AutoCoordinatorCountdown { countdown_id, seconds_left } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_countdown(countdown_id, seconds_left);
                    }
                }
                AppEvent::AutoCoordinatorRestart { token, attempt } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_restart(token, attempt);
                    }
                }
                AppEvent::PerformUndoRestore {
                    commit,
                    restore_files,
                    restore_conversation,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.perform_undo_restore(commit.as_deref(), restore_files, restore_conversation);
                    }
                }
                AppEvent::OpenSettings { section } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_settings_overlay(section);
                    }
                }
                AppEvent::DispatchCommand(command, command_text) => {
                    // Persist UI-only slash commands to cross-session history.
                    // For prompt-expanding commands (/plan, /solve, /code) we let the
                    // expanded prompt be recorded by the normal submission path.
                    if !command.is_prompt_expanding() {
                        self
                            .app_event_tx
                            .send(AppEvent::codex_op(Op::AddToHistory { text: command_text.clone() }));
                    }
                    // Extract command arguments by removing the slash command from the beginning
                    // e.g., "/browser status" -> "status", "/chrome 9222" -> "9222"
                    let command_args = {
                        let cmd_with_slash = format!("/{}", command.command());
                        if command_text.starts_with(&cmd_with_slash) {
                            command_text[cmd_with_slash.len()..].trim().to_string()
                        } else {
                            // Fallback: if format doesn't match, use the full text
                            command_text.clone()
                        }
                    };

                    match command {
                        SlashCommand::Undo => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_undo_command();
                            }
                        }
                        SlashCommand::Review => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                if command_args.is_empty() {
                                    widget.open_review_dialog();
                                } else {
                                    widget.handle_review_command(command_args);
                                }
                            }
                        }
                        SlashCommand::Cloud => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_cloud_command(command_args);
                            }
                        }
                        SlashCommand::Branch => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_branch_command(command_args);
                            }
                        }
                        SlashCommand::Merge => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_merge_command();
                            }
                        }
                        SlashCommand::Push => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_push_command();
                            }
                        }
                        SlashCommand::Resume => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.show_resume_picker();
                            }
                        }
                        SlashCommand::Rename => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                let trimmed = command_args.trim();
                                if trimmed.is_empty() {
                                    widget.debug_notice(
                                        "Usage: /rename <name> (or /rename - to clear)".to_string(),
                                    );
                                } else if let Some(session_id) = widget.session_id() {
                                    let nickname =
                                        if trimmed == "-" || trimmed.eq_ignore_ascii_case("clear") {
                                            None
                                        } else {
                                            Some(trimmed.to_string())
                                        };
                                    let code_home = self.config.code_home.clone();
                                    let tx = self.app_event_tx.clone();
                                    let nickname_label = nickname.clone();
                                    if let Err(err) = std::thread::Builder::new()
                                        .name("session-rename".to_string())
                                        .spawn(move || {
                                            let message = match tokio::runtime::Builder::new_current_thread()
                                                .enable_all()
                                                .build()
                                            {
                                                Ok(rt) => {
                                                    let catalog = SessionCatalog::new(code_home);
                                                    match rt.block_on(
                                                        catalog.set_nickname(session_id, nickname),
                                                    ) {
                                                        Ok(true) => match nickname_label {
                                                            Some(name) => {
                                                                format!("Session renamed to \"{name}\".")
                                                            }
                                                            None => "Session nickname cleared.".to_string(),
                                                        },
                                                        Ok(false) => {
                                                            "Session not found in catalog.".to_string()
                                                        }
                                                        Err(err) => {
                                                            format!("Failed to rename session: {err}")
                                                        }
                                                    }
                                                }
                                                Err(err) => {
                                                    format!("Failed to start rename task: {err}")
                                                }
                                            };
                                            tx.send(AppEvent::SessionRenameCompleted { message });
                                        })
                                    {
                                        widget.debug_notice(format!(
                                            "Failed to spawn rename task: {err}",
                                        ));
                                    }
                                } else {
                                    widget.debug_notice("Session not ready yet.".to_string());
                                }
                            }
                        }
                        SlashCommand::New => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.abort_active_turn_for_new_chat();
                            }
                            // Start a brand new conversation (core session) with no carried history.
                            // Replace the chat widget entirely, mirroring SwitchCwd flow but without import.
                            let mut new_widget = ChatWidget::new(crate::chatwidget::ChatWidgetInit {
                                config: self.config.clone(),
                                app_event_tx: self.app_event_tx.clone(),
                                initial_prompt: None,
                                initial_images: Vec::new(),
                                terminal_info: self.terminal_info.clone(),
                                show_order_overlay: self.show_order_overlay,
                                latest_upgrade_version: self.latest_upgrade_version.clone(),
                                startup_model_migration_notice: self
                                    .startup_model_migration_notice
                                    .clone(),
                            });
                            new_widget.enable_perf(self.timing_enabled);
                            self.app_state = AppState::Chat { widget: Box::new(new_widget) };
                            self.terminal_runs.clear();
                            self.app_event_tx.send(AppEvent::RequestRedraw);
                        }
                        SlashCommand::Init => {
                            // Guard: do not run if a task is active.
                            if let AppState::Chat { widget } = &mut self.app_state {
                                const INIT_PROMPT: &str =
                                    include_str!("../../../../prompt_for_init_command.md");
                                widget.submit_text_message(INIT_PROMPT.to_string());
                            }
                        }
                        SlashCommand::Compact => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.clear_token_usage();
                                self.app_event_tx.send(AppEvent::codex_op(Op::Compact));
                            }
                        }
                        SlashCommand::Quit => { break 'main; }
                        SlashCommand::Login => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_login_command();
                            }
                        }
                        SlashCommand::Accounts => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.show_settings_overlay(Some(SettingsSection::Accounts));
                            }
                        }
                        SlashCommand::Secrets => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_secrets_command();
                            }
                        }
                        SlashCommand::Logout => {
                            if let Err(e) = code_login::logout(&self.config.code_home) { tracing::error!("failed to logout: {e}"); }
                            break 'main;
                        }
                        SlashCommand::Diff => {
                            let tx = self.app_event_tx.clone();
                            tokio::spawn(async move {
                                match get_git_diff().await {
                                    Ok((is_git_repo, diff_text)) => {
                                        let text = if is_git_repo {
                                            diff_text
                                        } else {
                                            "`/diff` — _not inside a git repository_".to_string()
                                        };
                                        tx.send(AppEvent::DiffResult(text));
                                    }
                                    Err(e) => {
                                        tx.send(AppEvent::DiffResult(format!("Failed to compute diff: {e}")));
                                    }
                                }
                            });
                        }
                        SlashCommand::Mention => {
                            // The mention feature is handled differently in our fork
                            // For now, just add @ to the composer
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.insert_str("@");
                            }
                        }
                        SlashCommand::Cmd => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_project_command(command_args);
                            }
                        }
                        SlashCommand::Auto => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                let goal = if command_args.is_empty() {
                                    None
                                } else {
                                    Some(command_args.clone())
                                };
                                widget.handle_auto_command(goal);
                            }
                        }
                        SlashCommand::Status => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.add_status_output();
                            }
                        }
                        SlashCommand::Statusline => {
                            if let AppState::Chat { widget } = &mut self.app_state
                                && let Err(msg) =
                                    widget.open_status_line_setup_from_args(&command_args)
                            {
                                widget.debug_notice(msg);
                            }
                        }
                        SlashCommand::Limits => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_limits_command(command_args);
                            }
                        }
                        SlashCommand::Update => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_update_command(command_args.trim());
                            }
                        }
                        SlashCommand::Settings => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                let section = command
                                    .settings_section_from_args(&command_args)
                                    .and_then(ChatWidget::settings_section_from_hint);
                                widget.show_settings_overlay(section);
                            }
                        }
                        SlashCommand::Experimental => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                let _ = command_args;
                                widget.show_settings_overlay(Some(SettingsSection::Experimental));
                            }
                        }
                        SlashCommand::Memories => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_memories_command(command_args);
                            }
                        }
                        SlashCommand::Shell => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_shell_command(command_args.to_string());
                            }
                        }
                        SlashCommand::Notifications => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_notifications_command(command_args);
                            }
                        }
                        SlashCommand::Agents => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_agents_command(command_args);
                            }
                        }
                        SlashCommand::Validation => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_validation_command(command_args);
                            }
                        }
                        SlashCommand::Mcp => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_mcp_command(command_args);
                            }
                        }
                        SlashCommand::Model => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                if command_args.trim().is_empty() {
                                    widget.show_settings_overlay(Some(SettingsSection::Model));
                                } else {
                                    widget.handle_model_command(command_args);
                                }
                            }
                        }
                        SlashCommand::Fast => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.show_settings_overlay(Some(SettingsSection::Model));
                            }
                        }
                        SlashCommand::ContextWindow => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_context_window_command(command_args);
                            }
                        }
                        SlashCommand::AutoCompact => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_auto_compact_command(command_args);
                            }
                        }
                        SlashCommand::Mode => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_mode_command(command_args);
                            }
                        }
                        SlashCommand::Reasoning => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_reasoning_command(command_args);
                            }
                        }
                        SlashCommand::Verbosity => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_verbosity_command(command_args);
                            }
                        }
                        SlashCommand::Theme => {
                            // Theme selection is handled in submit_user_message
                            // This case is here for completeness
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.show_settings_overlay(Some(SettingsSection::Theme));
                            }
                        }
                        SlashCommand::Prompts => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_prompts_command(command_args.as_str());
                            }
                        }
                        SlashCommand::Skills => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_skills_command(command_args.as_str());
                            }
                        }
                        SlashCommand::Apps => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                let _ = command_args;
                                widget.show_apps_picker();
                            }
                        }
                        SlashCommand::Plugins => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.show_settings_overlay(Some(SettingsSection::Plugins));
                            }
                        }
                        SlashCommand::Perf => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_perf_command(command_args);
                            }
                        }
                        SlashCommand::Demo => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_demo_command(command_args);
                            }
                        }
                        // Prompt-expanding commands should have been handled in submit_user_message
                        // but add a fallback just in case. Use a helper that shows the original
                        // slash command in history while sending the expanded prompt to the model.
                        SlashCommand::Plan | SlashCommand::Solve | SlashCommand::Code => {
                            // These should have been expanded already, but handle them anyway
                            if let AppState::Chat { widget } = &mut self.app_state {
                                let expanded = command.expand_prompt(command_args.trim());
                                if let Some(prompt) = expanded {
                                    widget.submit_prompt_with_display(command_text.clone(), prompt);
                                }
                            }
                        }
                        SlashCommand::Browser => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_browser_command(command_args);
                            }
                        }
                        SlashCommand::Chrome => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                tracing::info!("[cdp] /chrome invoked, args='{}'", command_args);
                                if command_args.trim().is_empty() {
                                    widget.show_settings_overlay(Some(SettingsSection::Chrome));
                                } else {
                                    widget.handle_chrome_command(command_args);
                                }
                            }
                        }
                        #[cfg(debug_assertions)]
                        SlashCommand::TestApproval => {
                            use code_core::protocol::EventMsg;
                            use std::collections::HashMap;

                            use code_core::protocol::ApplyPatchApprovalRequestEvent;
                            use code_core::protocol::FileChange;

                            self.app_event_tx.send(AppEvent::codex_event(Event {
                                id: "1".to_string(),
                                event_seq: 0,
                                // msg: EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
                                //     call_id: "1".to_string(),
                                //     command: vec!["git".into(), "apply".into()],
                                //     cwd: self.config.cwd.clone(),
                                //     reason: Some("test".to_string()),
                                // }),
                                msg: EventMsg::ApplyPatchApprovalRequest(
                                    ApplyPatchApprovalRequestEvent {
                                        call_id: "1".to_string(),
                                        changes: HashMap::from([
                                            (
                                                PathBuf::from("/tmp/test.txt"),
                                                FileChange::Add {
                                                    content: "test".to_string(),
                                                },
                                            ),
                                            (
                                                PathBuf::from("/tmp/test2.txt"),
                                                FileChange::Update {
                                                    unified_diff: "+test\n-test2".to_string(),
                                                    move_path: None,
                                                    original_content: "test2".to_string(),
                                                    new_content: "test".to_string(),
                                                },
                                            ),
                                        ]),
                                        reason: None,
                                        grant_root: Some(PathBuf::from("/tmp")),
                                    },
                                ),
                                order: None,
                            }));
                        }
                    }
                }
                AppEvent::SwitchCwd(new_cwd, initial_prompt) => {
                    let target = new_cwd.clone();
                    self.config.cwd = target.clone();
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.switch_cwd(target, initial_prompt);
                    }
                }
                AppEvent::SessionPickerLoaded {
                    action,
                    cwd,
                    candidates,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.present_session_picker(action, cwd, candidates);
                    }
                }
                AppEvent::SessionPickerLoadFailed { action, message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        let action_label = match action {
                            SessionPickerAction::Resume => "resume",
                            SessionPickerAction::Fork => "fork",
                        };
                        widget.handle_session_picker_load_failed(format!(
                            "Failed to load {action_label} sessions: {message}"
                        ));
                    }
                }
                AppEvent::SessionRenameCompleted { message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.debug_notice(message);
                    }
                }
                AppEvent::ResumeFrom(path) => {
                    // Replace the current chat widget with a new one configured to resume
                    let mut cfg = self.config.clone();
                    cfg.experimental_resume = Some(path);
                    if let AppState::Chat { .. } = &self.app_state {
                        let mut new_widget = ChatWidget::new(crate::chatwidget::ChatWidgetInit {
                            config: cfg,
                            app_event_tx: self.app_event_tx.clone(),
                            initial_prompt: None,
                            initial_images: Vec::new(),
                            terminal_info: self.terminal_info.clone(),
                            show_order_overlay: self.show_order_overlay,
                            latest_upgrade_version: self.latest_upgrade_version.clone(),
                            startup_model_migration_notice: self
                                .startup_model_migration_notice
                                .clone(),
                        });
                        new_widget.enable_perf(self.timing_enabled);
                        self.app_state = AppState::Chat { widget: Box::new(new_widget) };
                        self.terminal_runs.clear();
                        self.app_event_tx.send(AppEvent::RequestRedraw);
                    }
                }
                AppEvent::ForkFrom(path) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.flash_footer_notice("Forking session…".to_string());
                    }

                    let cfg = self.config.clone();
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        match code_core::fork_rollout(&cfg, &path).await {
                            Ok(new_path) => {
                                tx.send(AppEvent::ResumeFrom(new_path));
                            }
                            Err(err) => {
                                tx.send(AppEvent::SessionRenameCompleted {
                                    message: format!("Failed to fork session: {err}"),
                                });
                            }
                        }
                    });
                }
                AppEvent::PrepareAgents => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.prepare_agents();
                    }
                }
                AppEvent::ShowAgentEditor { name } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.ensure_settings_overlay_section(SettingsSection::Agents);
                        widget.show_agent_editor_ui(name);
                    }
                }
                AppEvent::ShowAgentEditorNew => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.ensure_settings_overlay_section(SettingsSection::Agents);
                        widget.show_agent_editor_new_ui();
                    }
                }
                AppEvent::UpdateModelSelection { model, effort } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_model_selection(model, effort);
                    }
                }
                AppEvent::AcceptStartupModelMigration(notice) => {
                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    let profile = self.config.active_profile.clone();
                    tokio::spawn(async move {
                        let result = crate::model_migration::persist_startup_model_migration_acceptance(
                            &code_home,
                            profile.as_deref(),
                            &notice,
                        )
                        .await
                        .map_err(|err| err.to_string());
                        tx.send(AppEvent::StartupModelMigrationAcceptanceFinished {
                            notice,
                            result,
                        });
                    });
                }
                AppEvent::DismissStartupModelMigration(notice) => {
                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    let profile = self.config.active_profile.clone();
                    tokio::spawn(async move {
                        let result = crate::model_migration::persist_startup_model_migration_dismissal(
                            &code_home,
                            profile.as_deref(),
                            &notice.hide_key,
                        )
                        .await
                        .map_err(|err| err.to_string());
                        tx.send(AppEvent::StartupModelMigrationDismissalFinished {
                            notice,
                            result,
                        });
                    });
                }
                AppEvent::StartupModelMigrationAcceptanceFinished { notice, result } => {
                    let target_model = notice.target_model.clone();
                    let target_model_label = notice.target_model_label.clone();
                    match result {
                        Ok(()) => {
                            self.config.model = target_model.clone();
                            if let Some(effort) = notice.new_effort {
                                self.config.model_reasoning_effort = effort;
                            }
                            crate::model_migration::set_notice_flag(
                                &mut self.config.notices,
                                &notice.hide_key,
                            );
                            self.startup_model_migration_notice = None;
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.apply_model_selection(target_model, notice.new_effort);
                                widget.set_startup_model_migration_notice(None);
                                widget.flash_footer_notice(format!(
                                    "Switched to {target_model_label}."
                                ));
                            }
                            self.schedule_redraw();
                        }
                        Err(err) => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.flash_footer_notice(format!(
                                    "Failed to switch models: {err}"
                                ));
                            }
                        }
                    }
                }
                AppEvent::StartupModelMigrationDismissalFinished { notice, result } => {
                    let current_model_label = notice.current_model_label.clone();
                    match result {
                        Ok(()) => {
                            crate::model_migration::set_notice_flag(
                                &mut self.config.notices,
                                &notice.hide_key,
                            );
                            self.startup_model_migration_notice = None;
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.set_startup_model_migration_notice(None);
                                widget.flash_footer_notice(format!(
                                    "Keeping {current_model_label}."
                                ));
                            }
                            self.schedule_redraw();
                        }
                        Err(err) => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.flash_footer_notice(format!(
                                    "Failed to hide model notice: {err}"
                                ));
                            }
                        }
                    }
                }
                AppEvent::UpdateServiceTierSelection { service_tier } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_service_tier_selection(service_tier);
                    }
                    self.config.service_tier = service_tier;
                }
                AppEvent::UpdateShellSelection {
                    path,
                    args,
                    script_style,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_shell_selection(path, args, script_style);
                    }
                }
                AppEvent::ShellPersisted { shell } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.on_shell_persisted(shell);
                    }
                }
                AppEvent::ShellPersistFailed {
                    attempted_shell,
                    previous_shell,
                    error,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.on_shell_persist_failed(attempted_shell, previous_shell, error);
                    }
                }
                AppEvent::ShellSelectionClosed { confirmed } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.on_shell_selection_closed(confirmed);
                    }
                }
                AppEvent::UpdateShellStyleProfiles { shell_style_profiles } => {
                    self.config.shell_style_profiles = shell_style_profiles.clone();
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_shell_style_profiles(shell_style_profiles);
                    }
                }
                AppEvent::RequestGenerateShellStyleProfileSummary { style, profile } => {
                    let tx = self.app_event_tx.clone();
                    let config = std::sync::Arc::new(self.config.clone());
                    let auth_manager = self._server.auth_manager();
                    tokio::spawn(async move {
                        match generate_shell_style_profile_summary(config, auth_manager, style, profile).await {
                            Ok(summary) => tx.send(AppEvent::ShellStyleProfileSummaryGenerated {
                                style,
                                summary,
                            }),
                            Err(err) => tx.send(AppEvent::ShellStyleProfileSummaryGenerationFailed {
                                style,
                                error: err.to_string(),
                            }),
                        }
                    });
                }
                AppEvent::ShellStyleProfileSummaryGenerated { style, summary } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_shell_style_profile_summary_generated(style, summary);
                    }
                }
                AppEvent::ShellStyleProfileSummaryGenerationFailed { style, error } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_shell_style_profile_summary_generation_failed(style, error);
                    }
                }
                AppEvent::UpdateSessionContextModeSelection { context_mode } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_session_context_mode_selection(context_mode);
                    }
                }
                AppEvent::UpdateSessionContextSettingsSelection {
                    context_mode,
                    context_window,
                    auto_compact_token_limit,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_session_context_settings(
                            context_mode,
                            context_window,
                            auto_compact_token_limit,
                        );
                    }
                }
                AppEvent::PersistSessionContextSettings {
                    context_window,
                    auto_compact_token_limit,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.flash_footer_notice("Saving context settings…".to_string());
                    }
                    self.schedule_redraw();

                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    let profile = self.config.active_profile.clone();
                    tokio::spawn(async move {
                        let result = code_core::config_edit::set_session_context_settings(
                            code_home.as_path(),
                            profile.as_deref(),
                            context_window,
                            auto_compact_token_limit,
                        )
                        .await
                        .map_err(|err| err.to_string());

                        tx.send(AppEvent::SessionContextSettingsPersisted {
                            context_window,
                            auto_compact_token_limit,
                            result,
                        });
                    });
                }
                AppEvent::SessionContextSettingsPersisted {
                    context_window,
                    auto_compact_token_limit,
                    result,
                } => {
                    match result {
                        Ok(()) => {
                            let reload = self.reload_config_with_startup_overrides();
                            match reload {
                                Ok(config) => {
                                    self.config = config.clone();
                                    if let AppState::Chat { widget } = &mut self.app_state {
                                        widget.apply_reloaded_config_keep_settings_state(config);
                                        widget.flash_footer_notice(
                                            "Saved context settings to config.".to_string(),
                                        );
                                    }
                                }
                                Err(err) => {
                                    let mut config = self.config.clone();
                                    config.model_context_window = context_window;
                                    config.model_auto_compact_token_limit = auto_compact_token_limit;
                                    self.config = config.clone();
                                    if let AppState::Chat { widget } = &mut self.app_state {
                                        widget.apply_reloaded_config_keep_settings_state(config);
                                        widget.flash_footer_notice(format!(
                                            "Saved context settings, but failed to reload config: {err}",
                                        ));
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.flash_footer_notice(format!(
                                    "Failed to save context settings: {err}"
                                ));
                            }
                        }
                    }
                    self.schedule_redraw();
                }
                AppEvent::UpdateReviewModelSelection { model, effort } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_review_model_selection(model, effort);
                    }
                }
                event => {
                    include!("review_model_selection.rs");
                }
            }
