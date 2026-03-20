            match event {
                AppEvent::UpdateReviewResolveModelSelection { model, effort } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_review_resolve_model_selection(model, effort);
                    }
                }
                AppEvent::UpdateReviewUseChatModel(use_chat) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_review_use_chat_model(use_chat);
                    }
                }
                AppEvent::UpdateReviewResolveUseChatModel(use_chat) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_review_resolve_use_chat_model(use_chat);
                    }
                }
                AppEvent::UpdatePlanningModelSelection { model, effort } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_planning_model_selection(model, effort);
                    }
                }
                AppEvent::UpdateAutoDriveModelSelection { model, effort } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_auto_drive_model_selection(model, effort);
                    }
                }
                AppEvent::UpdateAutoDriveUseChatModel(use_chat) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_auto_drive_use_chat_model(use_chat);
                    }
                }
                AppEvent::UpdateAutoReviewModelSelection { model, effort } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_auto_review_model_selection(model, effort);
                    }
                }
                AppEvent::UpdateAutoReviewUseChatModel(use_chat) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_auto_review_use_chat_model(use_chat);
                    }
                }
                AppEvent::UpdateAutoReviewResolveModelSelection { model, effort } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_auto_review_resolve_model_selection(model, effort);
                    }
                }
                AppEvent::UpdateAutoReviewResolveUseChatModel(use_chat) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_auto_review_resolve_use_chat_model(use_chat);
                    }
                }
                AppEvent::ModelSelectionClosed { target, accepted } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_model_selection_closed(target, accepted);
                    }
                }
                AppEvent::UpdateTextVerbosity(new_verbosity) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_text_verbosity(new_verbosity);
                    }
                }
                AppEvent::UpdateTuiNotifications(enabled) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_tui_notifications(enabled);
                    }
                    self.config.tui.notifications = Notifications::Enabled(enabled);
                    self.config.tui_notifications = Notifications::Enabled(enabled);
                }
                AppEvent::UpdateValidationTool { name, enable } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.toggle_validation_tool(&name, enable);
                    }
                }
                AppEvent::UpdateValidationGroup { group, enable } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.toggle_validation_group(group, enable);
                    }
                }
                AppEvent::SetTerminalTitle { title } => {
                    self.terminal_title_override = title;
                    self.apply_terminal_title();
                }
                AppEvent::EmitTuiNotification { title, body } => {
                    if let Some(message) = Self::format_notification_message(&title, body.as_deref()) {
                        Self::emit_osc9_notification(&message);
                    }
                }
                AppEvent::UpdateMcpServer { name, enable } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.toggle_mcp_server(&name, enable);
                    }
                }
                AppEvent::UpdateMcpServerTool {
                    server_name,
                    tool_name,
                    enable,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.toggle_mcp_server_tool(&server_name, &tool_name, enable);
                    }
                }
                AppEvent::SetMcpServerScheduling { server, scheduling } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_mcp_server_scheduling(&server, scheduling);
                    }
                }
                AppEvent::SetMcpToolSchedulingOverride {
                    server,
                    tool,
                    override_cfg,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_mcp_tool_scheduling_override(&server, &tool, override_cfg);
                    }
                }
                AppEvent::UpdateSubagentCommand(cmd) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_subagent_update(cmd);
                    }
                }
                AppEvent::DeleteSubagentCommand(name) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.delete_subagent_by_name(&name);
                    }
                }
                // ShowAgentsSettings removed
                AppEvent::ShowAgentsOverview => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.ensure_settings_overlay_section(SettingsSection::Agents);
                        widget.show_agents_overview_ui();
                    }
                }
                // ShowSubagentEditor removed; use ShowSubagentEditorForName/ShowSubagentEditorNew
                AppEvent::ShowSubagentEditorForName { name } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.ensure_settings_overlay_section(SettingsSection::Agents);
                        widget.show_subagent_editor_for_name(name);
                    }
                }
                AppEvent::ShowSubagentEditorNew => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.ensure_settings_overlay_section(SettingsSection::Agents);
                        widget.show_new_subagent_editor();
                    }
                }
                AppEvent::UpdateAgentConfig { name, enabled, args_read_only, args_write, instructions, description, command } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_agent_update(crate::chatwidget::AgentUpdateRequest {
                            name,
                            enabled,
                            args_ro: args_read_only,
                            args_wr: args_write,
                            instructions,
                            description,
                            command,
                        });
                    }
                }
                AppEvent::AgentValidationFinished { name, result, attempt_id } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_agent_validation_finished(&name, attempt_id, result);
                    }
                }
                AppEvent::PrefillComposer(text) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.insert_str(&text);
                    }
                }
                AppEvent::ConfirmGitInit { resume } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.confirm_git_init(resume);
                    }
                }
                AppEvent::DeclineGitInit => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.decline_git_init();
                    }
                }
                AppEvent::GitInitFinished { ok, message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_git_init_finished(ok, message);
                    }
                }
                AppEvent::SubmitTextWithPreface { visible, preface } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.submit_text_message_with_preface(visible, preface);
                    }
                }
                AppEvent::SubmitHiddenTextWithPreface {
                    agent_text,
                    preface,
                    surface_notice,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.submit_hidden_text_message_with_preface_and_notice(
                            agent_text,
                            preface,
                            surface_notice,
                        );
                    }
                }
                AppEvent::RunReviewCommand(args) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_review_command(args);
                    }
                }
                AppEvent::UpdateReviewAutoResolveEnabled(enabled) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_review_auto_resolve_enabled(enabled);
                    }
                }
                AppEvent::UpdateAutoReviewEnabled(enabled) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_auto_review_enabled(enabled);
                    }
                }
                AppEvent::UpdateReviewAutoResolveAttempts(attempts) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_review_auto_resolve_attempts(attempts);
                    }
                }
                AppEvent::UpdateAutoReviewFollowupAttempts(attempts) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_auto_review_followup_attempts(attempts);
                    }
                }
                AppEvent::ShowReviewModelSelector => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_review_model_selector();
                    }
                }
                AppEvent::ShowReviewResolveModelSelector => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_review_resolve_model_selector();
                    }
                }
                AppEvent::ShowPlanningModelSelector => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_planning_model_selector();
                    }
                }
                AppEvent::ShowAutoDriveModelSelector => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_auto_drive_model_selector();
                    }
                }
                AppEvent::ShowAutoReviewModelSelector => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_auto_review_model_selector();
                    }
                }
                AppEvent::ShowAutoReviewResolveModelSelector => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_auto_review_resolve_model_selector();
                    }
                }
                AppEvent::RunReviewWithScope {
                    target,
                    prompt,
                    hint,
                    preparation_label,
                    auto_resolve,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.start_review_with_scope(
                            target,
                            prompt,
                            hint,
                            preparation_label,
                            auto_resolve,
                        );
                    }
                }
                AppEvent::BackgroundReviewStarted { worktree_path, branch, agent_id, snapshot } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.on_background_review_started(worktree_path, branch, agent_id, snapshot);
                    }
                }
                AppEvent::BackgroundReviewFinished { worktree_path, branch, has_findings, findings, summary, error, agent_id, snapshot } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.on_background_review_finished(crate::chatwidget::BackgroundReviewFinishedEvent {
                            worktree_path,
                            branch,
                            has_findings,
                            findings,
                            summary,
                            error,
                            agent_id,
                            snapshot,
                        });
                    }
                }
                AppEvent::OpenReviewCustomPrompt => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_review_custom_prompt();
                    }
                }
                event => {
                    include!("cloud_tasks.rs");
                }
            }
