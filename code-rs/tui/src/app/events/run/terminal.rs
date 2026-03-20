            match event {
                AppEvent::CancelRunningTask => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.cancel_running_task_from_approval();
                    }
                }
                AppEvent::RegisterApprovedCommand { command, match_kind, persist, semantic_prefix } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.register_approved_command(
                            command.clone(),
                            match_kind,
                            semantic_prefix.clone(),
                        );
                        if persist {
                            if let Err(err) = add_project_allowed_command(
                                &self.config.code_home,
                                &self.config.cwd,
                                &command,
                                match_kind,
                            ) {
                                widget.history_push_plain_state(history_cell::new_error_event(format!(
                                    "Failed to persist always-allow command: {err:#}",
                                )));
                            } else {
                                let display = strip_bash_lc_and_escape(&command);
                                widget.push_background_tail(format!(
                                    "Always allowing `{display}` for this project.",
                                ));
                            }
                        }
                    }
                }
                AppEvent::MarkTaskIdle => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.mark_task_idle_after_denied();
                    }
                }
                AppEvent::OpenTerminal(launch) => {
                    let mut spawn = None;
                    let requires_immediate_command = !launch.command.is_empty();
                    let restricted = !matches!(self.config.sandbox_policy, SandboxPolicy::DangerFullAccess);
                    if let AppState::Chat { widget } = &mut self.app_state {
                        if restricted && requires_immediate_command {
                            widget.history_push_plain_state(history_cell::new_error_event(
                                "Terminal requires Full Access to auto-run install commands.".to_string(),
                            ));
                            widget.show_agents_overview_ui();
                        } else {
                            widget.terminal_open(&launch);
                            if requires_immediate_command {
                                spawn = Some((
                                    launch.id,
                                    launch.command.clone(),
                                    Some(launch.command_display.clone()),
                                    launch.controller.clone(),
                                ));
                            }
                        }
                    }
                    if let Some((id, command, display, controller)) = spawn {
                        self.start_terminal_run(id, command, display, controller);
                    }
                }
                AppEvent::TerminalChunk {
                    id,
                    chunk,
                    _is_stderr: is_stderr,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.terminal_append_chunk(id, &chunk, is_stderr);
                    }
                }
                AppEvent::TerminalExit {
                    id,
                    exit_code,
                    _duration: duration,
                } => {
                    let after = if let AppState::Chat { widget } = &mut self.app_state {
                        widget.terminal_finalize(id, exit_code, duration)
                    } else {
                        None
                    };
                    let controller_present = if let Some(run) = self.terminal_runs.get_mut(&id) {
                        run.running = false;
                        run.cancel_tx = None;
                        if let Some(writer_shared) = run.writer_tx.take() {
                            let mut guard = writer_shared
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            guard.take();
                        }
                        run.pty = None;
                        run.controller.is_some()
                    } else {
                        false
                    };
                    if exit_code == Some(0) && !controller_present {
                        self.terminal_runs.remove(&id);
                    }
                    if let Some(after) = after {
                        self.app_event_tx.send(AppEvent::TerminalAfter(after));
                    }
                }
                AppEvent::TerminalCancel { id } => {
                    let mut remove_entry = false;
                    if let Some(run) = self.terminal_runs.get_mut(&id) {
                        let had_controller = run.controller.is_some();
                        if let Some(tx) = run.cancel_tx.take()
                            && !tx.is_closed() {
                                let _ = tx.send(());
                            }
                        run.running = false;
                        run.controller = None;
                        if let Some(writer_shared) = run.writer_tx.take() {
                            let mut guard = writer_shared
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            guard.take();
                        }
                        run.pty = None;
                        remove_entry = had_controller;
                    }
                    if remove_entry {
                        self.terminal_runs.remove(&id);
                    }
                }
                AppEvent::TerminalRerun { id } => {
                    let command_and_controller = self
                        .terminal_runs
                        .get(&id)
                        .and_then(|run| {
                            (!run.running).then(|| {
                                (
                                    run.command.clone(),
                                    run.display.clone(),
                                    run.controller.clone(),
                                )
                            })
                        });
                    if let Some((command, display, controller)) = command_and_controller {
                        if let AppState::Chat { widget } = &mut self.app_state {
                            widget.terminal_mark_running(id);
                        }
                        self.start_terminal_run(id, command, Some(display), controller);
                    }
                }
                AppEvent::TerminalRunCommand {
                    id,
                    command,
                    command_display,
                    controller,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.terminal_set_command_display(id, command_display.clone());
                        widget.terminal_mark_running(id);
                    }
                    self.start_terminal_run(id, command, Some(command_display), controller);
                }
                AppEvent::TerminalSendInput { id, data } => {
                    if let Some(run) = self.terminal_runs.get_mut(&id)
                        && let Some(writer_shared) = run.writer_tx.as_ref() {
                            let mut guard = writer_shared
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            if let Some(tx) = guard.as_ref()
                                && tx.send(data).is_err() {
                                    guard.take();
                                }
                        }
                }
                AppEvent::TerminalResize { id, rows, cols } => {
                    if rows == 0 || cols == 0 {
                        continue;
                    }
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.terminal_apply_resize(id, rows, cols);
                    }
                    if let Some(run) = self.terminal_runs.get(&id)
                        && let Some(pty) = run.pty.as_ref()
                            && let Ok(guard) = pty.lock() {
                                let _ = guard.resize(PtySize {
                                    rows,
                                    cols,
                                    pixel_width: 0,
                                    pixel_height: 0,
                                });
                            }
                }
                AppEvent::TerminalUpdateMessage { id, message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.terminal_update_message(id, message);
                    }
                }
                AppEvent::TerminalSetAssistantMessage { id, message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.terminal_set_assistant_message(id, message);
                    }
                }
                AppEvent::TerminalAwaitCommand { id, suggestion, ack } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.terminal_prepare_command(id, suggestion, ack.0);
                    }
                }
                AppEvent::TerminalForceClose { id } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.close_terminal_overlay();
                    }
                    self.terminal_runs.remove(&id);
                }
                AppEvent::TerminalApprovalDecision { id, approved } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_terminal_approval_decision(id, approved);
                    }
                }
                AppEvent::StartAutoDriveCelebration { message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.start_auto_drive_card_celebration(message);
                    }
                }
                AppEvent::StopAutoDriveCelebration => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.stop_auto_drive_card_celebration();
                    }
                }
                AppEvent::TerminalAfter(after) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_terminal_after(after);
                    }
                }
                AppEvent::RequestValidationToolInstall { name, command } => {
                    if let AppState::Chat { widget } = &mut self.app_state
                        && let Some(launch) = widget.launch_validation_tool_install(&name, &command) {
                            self.app_event_tx.send(AppEvent::OpenTerminal(launch));
                        }
                }
                AppEvent::RunUpdateCommand { command, display, latest_version } => {
                    if crate::updates::upgrade_ui_enabled()
                        && let AppState::Chat { widget } = &mut self.app_state
                            && let Some(launch) = widget.launch_update_command(command, display, latest_version) {
                                self.app_event_tx.send(AppEvent::OpenTerminal(launch));
                            }
                }
                event => {
                    include!("memories.rs");
                }
            }
