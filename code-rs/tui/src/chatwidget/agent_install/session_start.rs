pub(super) fn start_agent_install_session(args: AgentInstallSessionArgs) {
    let AgentInstallSessionArgs {
        app_event_tx,
        terminal_id,
        agent_name,
        default_command,
        cwd,
        control,
        selected_index,
        debug_enabled,
    } = args;
    start_guided_terminal_session(GuidedTerminalSessionArgs {
        app_event_tx,
        terminal_id,
        mode: GuidedTerminalMode::AgentInstall {
            agent_name,
            default_command,
            selected_index,
        },
        cwd,
        control,
        config: None,
        debug_enabled,
    });
}

pub(super) fn start_prompt_terminal_session(
    app_event_tx: AppEventSender,
    terminal_id: u64,
    user_prompt: String,
    cwd: Option<String>,
    controller: TerminalRunController,
    controller_rx: Receiver<TerminalRunEvent>,
    debug_enabled: bool,
) {
    start_guided_terminal_session(GuidedTerminalSessionArgs {
        app_event_tx,
        terminal_id,
        mode: GuidedTerminalMode::Prompt { user_prompt },
        cwd,
        control: GuidedTerminalControl {
            controller,
            controller_rx,
        },
        config: None,
        debug_enabled,
    });
}

pub(super) fn start_direct_terminal_session(
    app_event_tx: AppEventSender,
    terminal_id: u64,
    command: String,
    cwd: Option<String>,
    controller: TerminalRunController,
    controller_rx: Receiver<TerminalRunEvent>,
    debug_enabled: bool,
) {
    start_guided_terminal_session(GuidedTerminalSessionArgs {
        app_event_tx,
        terminal_id,
        mode: GuidedTerminalMode::DirectCommand { command },
        cwd,
        control: GuidedTerminalControl {
            controller,
            controller_rx,
        },
        config: None,
        debug_enabled,
    });
}

pub(super) fn start_upgrade_terminal_session(args: UpgradeTerminalSessionArgs) {
    let UpgradeTerminalSessionArgs {
        app_event_tx,
        terminal_id,
        initial_command,
        latest_version,
        cwd,
        control,
        config,
        debug_enabled,
    } = args;
    start_guided_terminal_session(GuidedTerminalSessionArgs {
        app_event_tx,
        terminal_id,
        mode: GuidedTerminalMode::Upgrade {
            initial_command,
            latest_version,
        },
        cwd,
        control,
        config: Some(config),
        debug_enabled,
    });
}

fn start_guided_terminal_session(args: GuidedTerminalSessionArgs) {
    let GuidedTerminalSessionArgs {
        app_event_tx,
        terminal_id,
        mode,
        cwd,
        control,
        config,
        debug_enabled,
    } = args;
    let fail_tx = app_event_tx.clone();
    if let Err(err) = std::thread::Builder::new()
        .name("guided-terminal-session".to_string())
        .spawn(move || {
        let mut control = control;
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(err) => {
                let helper = match &mode {
                    GuidedTerminalMode::AgentInstall { .. } => "Install helper",
                    GuidedTerminalMode::Prompt { .. }
                    | GuidedTerminalMode::DirectCommand { .. } => "Terminal helper",
                    GuidedTerminalMode::Upgrade { .. } => "Upgrade helper",
                };
                let msg = format!("Failed to start {helper} runtime: {err}");
                app_event_tx.send(AppEvent::TerminalChunk {
                    id: terminal_id,
                    chunk: format!("{msg}\n").into_bytes(),
                    _is_stderr: true,
                });
                app_event_tx.send(AppEvent::TerminalUpdateMessage {
                    id: terminal_id,
                    message: msg,
                });
                return;
            }
        };

        if let Err(err) = run_guided_loop(
            &runtime,
            GuidedLoopArgs {
                app_event_tx: &app_event_tx,
                terminal_id,
                mode: &mode,
                cwd: cwd.as_deref(),
                controller: control.controller,
                controller_rx: &mut control.controller_rx,
                provided_config: config,
                debug_enabled,
            },
        ) {
            let helper = match &mode {
                GuidedTerminalMode::AgentInstall { .. } => "Install helper",
                GuidedTerminalMode::Prompt { .. }
                | GuidedTerminalMode::DirectCommand { .. } => "Terminal helper",
                GuidedTerminalMode::Upgrade { .. } => "Upgrade helper",
            };
            let msg = if debug_enabled {
                format!("{helper} error: {err:#}")
            } else {
                format!("{helper} error: {err}")
            };
            app_event_tx.send(AppEvent::TerminalChunk {
                id: terminal_id,
                chunk: format!("{msg}\n").into_bytes(),
                _is_stderr: true,
            });
            app_event_tx.send(AppEvent::TerminalUpdateMessage {
                id: terminal_id,
                message: msg,
            });
        }
        })
    {
        let msg = format!("Failed to start guided terminal helper: {err}");
        fail_tx.send(AppEvent::TerminalChunk {
            id: terminal_id,
            chunk: format!("{msg}\n").into_bytes(),
            _is_stderr: true,
        });
        fail_tx.send(AppEvent::TerminalUpdateMessage { id: terminal_id, message: msg });
    }
}
