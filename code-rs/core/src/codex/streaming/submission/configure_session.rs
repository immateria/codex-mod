use super::*;

pub(super) struct ConfigureSessionState {
    pub(super) session_id: Uuid,
    pub(super) config: Arc<Config>,
    pub(super) sess: Option<Arc<Session>>,
    pub(super) agent_manager_initialized: bool,
}

pub(super) enum ConfigureSessionControl {
    Continue,
    Exit,
}

pub(super) async fn handle_configure_session(
    state: ConfigureSessionState,
    auth_manager: Option<Arc<AuthManager>>,
    tx_event: &Sender<Event>,
    file_watcher: &crate::file_watcher::FileWatcher,
    sub_id: String,
    op: Op,
) -> (ConfigureSessionState, ConfigureSessionControl) {
    let ConfigureSessionState {
        mut session_id,
        mut config,
        mut sess,
        mut agent_manager_initialized,
    } = state;

    struct SubmissionStub {
        id: String,
    }

    let sub = SubmissionStub { id: sub_id };

    let Op::ConfigureSession {
        provider,
        model,
        model_explicit,
        model_reasoning_effort,
        preferred_model_reasoning_effort,
        model_reasoning_summary,
        model_text_verbosity,
        user_instructions: provided_user_instructions,
        base_instructions: provided_base_instructions,
        approval_policy,
        sandbox_policy,
        disable_response_storage,
        notify,
        cwd,
        resume_path,
        demo_developer_message,
        dynamic_tools,
        shell: shell_override,
        shell_style_profiles,
        network,
        collaboration_mode,
    } = op else {
        unreachable!("handle_configure_session called with non-ConfigureSession op");
    };

    macro_rules! done {
        ($control:expr) => {
            return (
                ConfigureSessionState {
                    session_id,
                    config,
                    sess,
                    agent_manager_initialized,
                },
                $control,
            );
        };
    }

    // shorthand - send an event when there is no active session
    let send_no_session_event = |sub_id: String| async {
        let event = Event {
            id: sub_id,
            event_seq: 0,
            msg: EventMsg::Error(ErrorEvent {
                message: "No session initialized, expected 'ConfigureSession' as first Op".to_string(),
            }),
            order: None,
        };
        tx_event.send(event).await.ok();
    };
                debug!(
                    "Configuring session: model={model}; provider={provider:?}; resume={resume_path:?}"
                );
                if !cwd.is_absolute() {
                    let message = format!("cwd is not absolute: {cwd:?}");
                    error!(message);
                    let event = Event { id: sub.id, event_seq: 0, msg: EventMsg::Error(ErrorEvent { message }), order: None };
                    if let Err(e) = tx_event.send(event).await {
                        error!("failed to send error message: {e:?}");
                    }
                    done!(ConfigureSessionControl::Exit);
                }
                let current_config = Arc::clone(&config);
                let mut updated_config = (*current_config).clone();

                let model_changed = !updated_config.model.eq_ignore_ascii_case(&model);
                let effort_changed = updated_config.model_reasoning_effort != model_reasoning_effort;
                let preferred_effort_changed = preferred_model_reasoning_effort
                    .as_ref()
                    .map(|preferred| updated_config.preferred_model_reasoning_effort != Some(*preferred))
                    .unwrap_or(false);

                let old_model_family = updated_config.model_family.clone();
                let old_tool_output_max_bytes = updated_config.tool_output_max_bytes;
                let old_default_tool_output_max_bytes = old_model_family.tool_output_max_bytes();

                updated_config.model = model.clone();
                updated_config.model_explicit = model_explicit;
                updated_config.model_provider = provider.clone();
                updated_config.model_reasoning_effort = model_reasoning_effort;
                if let Some(preferred) = preferred_model_reasoning_effort {
                    updated_config.preferred_model_reasoning_effort = Some(preferred);
                }
                updated_config.model_reasoning_summary = model_reasoning_summary;
                updated_config.model_text_verbosity = model_text_verbosity;
                updated_config.user_instructions = provided_user_instructions.clone();
                let base_instructions = provided_base_instructions.or_else(|| {
                    crate::model_family::base_instructions_override_for_personality(
                        &model,
                        updated_config.model_personality,
                    )
                });
                updated_config.base_instructions = base_instructions.clone();
                updated_config.approval_policy = approval_policy;
                updated_config.sandbox_policy = sandbox_policy.clone();
                updated_config.disable_response_storage = disable_response_storage;
                updated_config.notify = notify.clone();
                updated_config.cwd = cwd.clone();
                updated_config.dynamic_tools = dynamic_tools.clone();
                updated_config.network = network.clone();
                updated_config.shell_style_profiles = shell_style_profiles;

                updated_config.network_proxy = match updated_config
                    .network
                    .as_ref()
                    .filter(|net| net.enabled)
                {
                    Some(net) => match crate::config::network_proxy_spec::NetworkProxySpec::from_config(
                        net.to_network_proxy_config(),
                    ) {
                        Ok(spec) => Some(spec),
                        Err(err) => {
                            let message = format!("invalid managed network config: {err}");
                            error!(message);
                            let event = Event {
                                id: sub.id,
                                event_seq: 0,
                                msg: EventMsg::Error(ErrorEvent { message }),
                                order: None,
                            };
                            if let Err(e) = tx_event.send(event).await {
                                error!("failed to send error message: {e:?}");
                            }
                            done!(ConfigureSessionControl::Exit);
                        }
                    },
                    None => None,
                };

                updated_config.model_family = find_family_for_model(&updated_config.model)
                    .unwrap_or_else(|| derive_default_model_family(&updated_config.model));

                let new_default_tool_output_max_bytes =
                    updated_config.model_family.tool_output_max_bytes();

                let old_context_window = old_model_family.context_window;
                let new_context_window = updated_config.model_family.context_window;
                let old_max_tokens = old_model_family.max_output_tokens;
                let new_max_tokens = updated_config.model_family.max_output_tokens;
                let old_auto_compact = old_model_family.auto_compact_token_limit();
                let new_auto_compact = updated_config.model_family.auto_compact_token_limit();

                maybe_update_from_model_info(
                    &mut updated_config.model_context_window,
                    old_context_window,
                    new_context_window,
                );
                maybe_update_from_model_info(
                    &mut updated_config.model_max_output_tokens,
                    old_max_tokens,
                    new_max_tokens,
                );
                maybe_update_from_model_info(
                    &mut updated_config.model_auto_compact_token_limit,
                    old_auto_compact,
                    new_auto_compact,
                );

                if old_tool_output_max_bytes == old_default_tool_output_max_bytes {
                    updated_config.tool_output_max_bytes = new_default_tool_output_max_bytes;
                }

                let resolved_shell = shell::default_user_shell_with_override(
                    shell_override.as_ref().or(updated_config.shell.as_ref()),
                )
                .await;
                let active_shell_style = resolved_shell.script_style();
                let active_shell_style_label = active_shell_style.map(|style| style.to_string());
                let mut shell_style_profile_messages: Vec<String> = Vec::new();
                let mut shell_style_skill_filter: Option<HashSet<String>> = None;
                let mut shell_style_disabled_skills: HashSet<String> = HashSet::new();
                let mut shell_style_skill_roots: Vec<PathBuf> = Vec::new();
                let mut shell_style_mcp_include: HashSet<String> = HashSet::new();
                let mut shell_style_mcp_exclude: HashSet<String> = HashSet::new();
                let mut effective_mcp_servers = updated_config.mcp_servers.clone();

                if let Some(style) = active_shell_style
                    && let Some(profile) = updated_config.shell_style_profiles.get(&style).cloned()
                {
                    shell_style_mcp_include = profile
                        .mcp_servers
                        .include
                        .iter()
                        .map(|name| name.trim().to_ascii_lowercase())
                        .filter(|name| !name.is_empty())
                        .collect();
                    if !shell_style_mcp_include.is_empty() {
                        effective_mcp_servers.retain(|name, _| {
                            shell_style_mcp_include.contains(&name.to_ascii_lowercase())
                        });
                    }

                    shell_style_mcp_exclude = profile
                        .mcp_servers
                        .exclude
                        .iter()
                        .map(|name| name.trim().to_ascii_lowercase())
                        .filter(|name| !name.is_empty())
                        .collect();
                    if !shell_style_mcp_exclude.is_empty() {
                        effective_mcp_servers.retain(|name, _| {
                            !shell_style_mcp_exclude.contains(&name.to_ascii_lowercase())
                        });
                    }

                    for message in profile.prepend_developer_messages {
                        let trimmed = message.trim();
                        if !trimmed.is_empty() {
                            shell_style_profile_messages.push(trimmed.to_string());
                        }
                    }

                    for reference in profile.references {
                        let full_path = if reference.is_relative() {
                            updated_config.cwd.join(&reference)
                        } else {
                            reference.clone()
                        };
                        match std::fs::read_to_string(&full_path) {
                            Ok(contents) => {
                                let trimmed = contents.trim();
                                if !trimmed.is_empty() {
                                    shell_style_profile_messages.push(format!(
                                        "Shell style reference `{style}` from `{}`:\n\n{trimmed}",
                                        full_path.display(),
                                    ));
                                }
                            }
                            Err(err) => {
                                warn!(
                                    "failed to read shell style reference {}: {err}",
                                    full_path.display()
                                );
                            }
                        }
                    }

                    let requested_skills: HashSet<String> = profile
                        .skills
                        .iter()
                        .map(|name| name.trim().to_ascii_lowercase())
                        .filter(|name| !name.is_empty())
                        .collect();
                    if !requested_skills.is_empty() {
                        shell_style_skill_filter = Some(requested_skills);
                    }

                    shell_style_disabled_skills.extend(
                        profile
                            .disabled_skills
                            .iter()
                            .map(|name| name.trim().to_ascii_lowercase())
                            .filter(|name| !name.is_empty()),
                    );

                    shell_style_skill_roots.extend(
                        profile
                            .skill_roots
                            .into_iter()
                            .filter(|path| !path.as_os_str().is_empty()),
                    );
                }

                let command_safety_profile = crate::safety::resolve_command_safety_profile(
                    &resolved_shell,
                    shell_override.as_ref().or(updated_config.shell.as_ref()),
                    &updated_config.shell_style_profiles,
                );

                let mut skills_outcome = if updated_config.skills_enabled {
                    Some(if shell_style_skill_roots.is_empty() {
                        load_skills(&updated_config)
                    } else {
                        crate::skills::loader::load_skills_with_additional_roots(
                            &updated_config,
                            shell_style_skill_roots.iter().cloned(),
                        )
                    })
                } else {
                    None
                };
                if let Some(outcome) = &mut skills_outcome {
                    for err in &outcome.errors {
                        warn!("invalid skill {}: {}", err.path.display(), err.message);
                    }

                    let available_skill_names: HashSet<String> = outcome
                        .skills
                        .iter()
                        .map(|skill| skill.name.trim().to_ascii_lowercase())
                        .collect();

                    if let Some(skill_filter) = shell_style_skill_filter.as_ref() {
                        let mut matched_skills: HashSet<String> = HashSet::new();
                        outcome.skills.retain(|skill| {
                            let normalized = skill.name.trim().to_ascii_lowercase();
                            let keep = skill_filter.contains(&normalized);
                            if keep {
                                matched_skills.insert(normalized);
                            }
                            keep
                        });

                        if let Some(style_label) = active_shell_style_label.as_deref() {
                            for requested in skill_filter {
                                if !matched_skills.contains(requested) {
                                    warn!(
                                        "shell style profile `{style_label}` requested unknown skill `{requested}`"
                                    );
                                }
                            }
                        }
                    }

                    if !shell_style_disabled_skills.is_empty() {
                        outcome.skills.retain(|skill| {
                            let normalized = skill.name.trim().to_ascii_lowercase();
                            !shell_style_disabled_skills.contains(&normalized)
                        });

                        if let Some(style_label) = active_shell_style_label.as_deref() {
                            for requested in &shell_style_disabled_skills {
                                if !available_skill_names.contains(requested) {
                                    warn!(
                                        "shell style profile `{style_label}` requested unknown disabled skill `{requested}`"
                                    );
                                }
                            }
                        }
                    }
                }

                let session_skills = skills_outcome
                    .as_ref()
                    .map(|outcome| super::skills::strip_skill_contents(outcome.skills.as_slice()))
                    .unwrap_or_default();

                let computed_user_instructions = get_user_instructions(
                    &updated_config,
                    skills_outcome.as_ref().map(|outcome| outcome.skills.as_slice()),
                )
                .await;
                updated_config.user_instructions = computed_user_instructions.clone();

                let effective_user_instructions = computed_user_instructions.clone();

                // Optionally resume an existing rollout.
                let mut restored_items: Option<Vec<RolloutItem>> = None;
                let mut restored_history_snapshot: Option<crate::history::HistorySnapshot> = None;
                let mut resume_notice: Option<String> = None;
                let mut rollout_recorder: Option<RolloutRecorder> = None;
                if let Some(path) = resume_path.as_ref() {
                    match RolloutRecorder::resume(&updated_config, path).await {
                        Ok((rec, saved)) => {
                            session_id = saved.session_id;
                            if !saved.items.is_empty() {
                                restored_items = Some(saved.items);
                            }
                            if let Some(snapshot) = saved.history_snapshot {
                                restored_history_snapshot = Some(snapshot);
                            }
                            rollout_recorder = Some(rec);
                        }
                        Err(e) => {
                            warn!("failed to resume rollout from {path:?}: {e}");
                            resume_notice = Some(format!(
                                "WARN: Failed to load previous session from {}: {e}. Starting a new conversation instead.",
                                path.display()
                            ));
                            updated_config.experimental_resume = None;
                        }
                    }
                }

                let new_config = Arc::new(updated_config);

                if new_config.model_explicit && (model_changed || effort_changed || preferred_effort_changed)
                    && let Err(err) = persist_model_selection(
                        &new_config.code_home,
                        new_config.active_profile.as_deref(),
                        &new_config.model,
                        Some(new_config.model_reasoning_effort),
                        new_config.preferred_model_reasoning_effort,
                    )
                    .await
                    {
                        warn!("failed to persist model selection: {err:#}");
                    }

                config = Arc::clone(&new_config);
                file_watcher.register_config(config.as_ref());

                let rollout_recorder = match rollout_recorder {
                    Some(rec) => Some(rec),
                    None => {
                        match RolloutRecorder::new(
                            &config,
                            crate::rollout::recorder::RolloutRecorderParams::new(
                                code_protocol::mcp_protocol::ConversationId::from(session_id),
                                effective_user_instructions.clone(),
                                SessionSource::Cli,
                            ),
                        )
                            .await
                        {
                            Ok(r) => Some(r),
                            Err(e) => {
                                warn!("failed to initialise rollout recorder: {e}");
                                None
                            }
                        }
                    }
                };

                // Create debug logger based on config
                let debug_logger = match crate::debug_logger::DebugLogger::new(config.debug) {
                    Ok(logger) => std::sync::Arc::new(std::sync::Mutex::new(logger)),
                    Err(e) => {
                        warn!("Failed to create debug logger: {}", e);
                        // Create a disabled logger as fallback
                        std::sync::Arc::new(std::sync::Mutex::new(
                            crate::debug_logger::DebugLogger::new(false).unwrap(),
                        ))
                    }
                };

                if config.debug {
                    if let Ok(logger) = debug_logger.lock()
                        && let Err(e) = logger.set_session_usage_file(&session_id) {
                            warn!("failed to initialise session usage log: {e}");
                        }

                    // SAFETY: setting a process-wide env var is intentional here to
                    // coordinate sub-agent debug behaviour launched from this session.
                    unsafe { std::env::set_var("CODE_SUBAGENT_DEBUG", "1"); }
                    match crate::config::find_code_home() {
                        Ok(mut debug_root) => {
                            debug_root.push("debug_logs");
                            let mut manager = AGENT_MANAGER.write().await;
                            manager.set_debug_log_root(Some(debug_root));
                        }
                        Err(err) => {
                            warn!("failed to resolve debug log root: {err}");
                            let mut manager = AGENT_MANAGER.write().await;
                            manager.set_debug_log_root(None);
                        }
                    }
                } else {
                    // SAFETY: removing the coordination flag is safe when debug is off.
                    unsafe { std::env::remove_var("CODE_SUBAGENT_DEBUG"); }
                    let mut manager = AGENT_MANAGER.write().await;
                    manager.set_debug_log_root(None);
                }

                let conversation_id = code_protocol::mcp_protocol::ConversationId::from(session_id);
                let auth_snapshot = auth_manager.as_ref().and_then(|mgr| mgr.auth());
                let otel_event_manager = {
                    let manager = OtelEventManager::new(
                        conversation_id,
                        config.model.as_str(),
                        config.model_family.slug.as_str(),
                        auth_snapshot
                            .as_ref()
                            .and_then(crate::auth::CodexAuth::get_account_id),
                        auth_snapshot.as_ref().map(|auth| auth.mode),
                        config.otel.log_user_prompt,
                        crate::terminal::user_agent(),
                    );
                    manager.conversation_starts(
                        config.model_provider.name.as_str(),
                        Some(to_proto_reasoning_effort(model_reasoning_effort)),
                        to_proto_reasoning_summary(model_reasoning_summary),
                        config.model_context_window,
                        config.model_max_output_tokens,
                        config.model_auto_compact_token_limit,
                        to_proto_approval_policy(approval_policy),
                        to_proto_sandbox_policy(sandbox_policy.clone()),
                        config
                            .mcp_servers
                            .keys()
                            .map(String::as_str)
                            .collect(),
                        config.active_profile.clone(),
                    );
                    manager
                };

                // Wrap provided auth (if any) in a minimal AuthManager for client usage.
                let client = ModelClient::new(crate::client::ModelClientInit {
                    config: config.clone(),
                    auth_manager: auth_manager.clone(),
                    otel_event_manager: Some(otel_event_manager.clone()),
                    provider: provider.clone(),
                    effort: model_reasoning_effort,
                    summary: model_reasoning_summary,
                    verbosity: model_text_verbosity,
                    session_id,
                    debug_logger,
                });

                // abort any current running session and clone its state
                let old_session = sess.take();
                let (mcp_allow_servers, mcp_deny_servers) = old_session
                    .as_ref()
                    .map(|sess_arc| sess_arc.session_mcp_overrides_snapshot())
                    .unwrap_or_default();
                let state = if let Some(sess_arc) = old_session.as_ref() {
                    sess_arc.notify_wait_interrupted(WaitInterruptReason::SessionAborted);
                    sess_arc.abort();
                    sess_arc.state.lock().unwrap().partial_clone()
                } else {
                    State {
                        history: ConversationHistory::new(),
                        ..Default::default()
                    }
                };

                // Error messages to dispatch after SessionConfigured is sent.
                let mut mcp_connection_errors = Vec::<String>::new();
                let mut excluded_tools = HashSet::new();
                if let Some(client_tools) = config.experimental_client_tools.as_ref() {
                    for tool in [
                        client_tools.request_permission.as_ref(),
                        client_tools.read_text_file.as_ref(),
                        client_tools.write_text_file.as_ref(),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        excluded_tools.insert((
                            tool.mcp_server.to_string(),
                            tool.tool_name.to_string(),
                        ));
                    }
                }
                for (server_name, server_cfg) in &config.mcp_servers {
                    for tool_name in &server_cfg.disabled_tools {
                        excluded_tools.insert((server_name.clone(), tool_name.clone()));
                    }
                }

                if let Some(old_session_arc) = old_session {
                    old_session_arc.shutdown_mcp_clients().await;
                    drop(old_session_arc);
                }

                let (mcp_connection_manager, failed_clients) = match McpConnectionManager::new(
                    config.code_home.clone(),
                    config.mcp_oauth_credentials_store_mode,
                    effective_mcp_servers.clone(),
                    excluded_tools,
                )
                .await
                {
                    Ok((mgr, failures)) => (mgr, failures),
                    Err(e) => {
                        let message = format!("Failed to create MCP connection manager: {e:#}");
                        error!("{message}");
                        mcp_connection_errors.push(message);
                        (McpConnectionManager::default(), Default::default())
                    }
                };

                // Surface individual client start-up failures to the user.
                if !failed_clients.is_empty() {
                    for (server_name, failure) in failed_clients {
                        let detail = failure.message;
                        let message = match failure.phase {
                            crate::protocol::McpServerFailurePhase::Start => {
                                format!("MCP server `{server_name}` failed to start: {detail}")
                            }
                            crate::protocol::McpServerFailurePhase::ListTools => format!(
                                "MCP server `{server_name}` failed to list tools: {detail}"
                            ),
                        };
                        error!("{message}");
                        mcp_connection_errors.push(message);
                    }
                }
                let mut tools_config = ToolsConfig::new(crate::openai_tools::ToolsConfigParams {
                    model_family: &config.model_family,
                    approval_policy,
                    sandbox_policy: sandbox_policy.clone(),
                    include_plan_tool: config.include_plan_tool,
                    include_apply_patch_tool: config.include_apply_patch_tool,
                    include_web_search_request: config.tools_web_search_request,
                    use_streamable_shell_tool: config.use_experimental_streamable_shell_tool,
                    include_view_image_tool: config.include_view_image_tool,
                });
                tools_config.web_search_allowed_domains =
                    config.tools_web_search_allowed_domains.clone();
                tools_config.web_search_external = config.tools_web_search_external;
                tools_config.search_tool = config.tools_search_tool;
                tools_config.js_repl = config.tools_js_repl;

                let mut agent_models: Vec<String> = if config.agents.is_empty() {
                    default_agent_configs()
                        .into_iter()
                        .filter(|cfg| cfg.enabled)
                        .map(|cfg| cfg.name)
                        .collect()
                } else {
                    get_enabled_agents(&config.agents)
                };
                let auth_mode = auth_manager
                    .as_ref()
                    .and_then(|mgr| mgr.auth().map(|auth| auth.mode))
                    .or(Some(if config.using_chatgpt_auth {
                        AppAuthMode::Chatgpt
                    } else {
                        AppAuthMode::ApiKey
                    }));
                let supports_pro_only_models = auth_manager
                    .as_ref()
                    .is_some_and(|mgr| mgr.supports_pro_only_models());

                agent_models = filter_agent_model_names_for_auth(
                    agent_models,
                    auth_mode,
                    supports_pro_only_models,
                );
                if agent_models.is_empty() {
                    agent_models = enabled_agent_model_specs_for_auth(
                        auth_mode,
                        supports_pro_only_models,
                    )
                    .into_iter()
                    .map(|spec| spec.slug.to_string())
                    .collect();
                }
                agent_models.sort_by_key(|a| a.to_ascii_lowercase());
                agent_models.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
                tools_config.set_agent_models(agent_models);

                let model_descriptions = model_guide_markdown_with_custom(&config.agents);
                let remote_models_manager = auth_manager.as_ref().map(|mgr| {
                    Arc::new(RemoteModelsManager::new(
                        Arc::clone(mgr),
                        provider.clone(),
                        config.code_home.clone(),
                    ))
                });
                if let Some(remote) = remote_models_manager.as_ref() {
                    let remote = Arc::clone(remote);
                    tokio::spawn(async move {
                        remote.refresh_remote_models().await;
                    });
                }

                let network_approval =
                    Arc::new(crate::network_approval::NetworkApprovalService::default());
                let network_policy_decider_session = config.network_proxy.as_ref().map(|_| {
                    Arc::new(tokio::sync::RwLock::new(std::sync::Weak::<Session>::new()))
                });
                let network_policy_decider = network_policy_decider_session
                    .as_ref()
                    .map(|session| {
                        crate::network_approval::build_network_policy_decider(
                            Arc::clone(&network_approval),
                            Arc::clone(session),
                        )
                    });
                let network_proxy = if let Some(spec) = config.network_proxy.as_ref() {
                    match spec
                        .start_proxy(&sandbox_policy, network_policy_decider, None, true)
                        .await
                    {
                        Ok(proxy) => Some(proxy),
                        Err(err) => {
                            let message =
                                format!("Failed to start managed network proxy: {err}");
                            error!("{message}");
                            mcp_connection_errors.push(message);
                            None
                        }
                    }
                } else {
                    None
                };
                let mut new_session = Arc::new(Session {
                    id: session_id,
                    client,
                    remote_models_manager,
                    tools_config,
                    dynamic_tools,
                    exec_command_manager: Arc::new(crate::exec_command::SessionManager::default()),
                    js_repl: crate::tools::js_repl::JsReplHandle::new(None),
                    network_proxy,
                    network_approval: Arc::clone(&network_approval),
                    tx_event: tx_event.clone(),
                    user_instructions: effective_user_instructions.clone(),
                    base_instructions,
                    skills: tokio::sync::RwLock::new(session_skills),
                    demo_developer_message: demo_developer_message.clone(),
                    compact_prompt_override: config.compact_prompt_override.clone(),
                    approval_policy,
                    sandbox_policy,
                    shell_environment_policy: config.shell_environment_policy.clone(),
                    collaboration_mode,
                    cwd,
                    mcp_connection_manager,
                    client_tools: config.experimental_client_tools.clone(),
                    agents: config.agents.clone(),
                    subagent_max_depth: config.subagent_max_depth,
                    model_reasoning_effort: config.model_reasoning_effort,
                    notify,
                    state: Mutex::new(state),
                    rollout: Mutex::new(rollout_recorder),
                    code_linux_sandbox_exe: config.code_linux_sandbox_exe.clone(),
                    disable_response_storage,
                    user_shell: resolved_shell,
                    dangerous_command_detection_enabled: command_safety_profile
                        .dangerous_command_detection_enabled,
                    safe_command_rules: command_safety_profile.safe_rules,
                    dangerous_command_rules: command_safety_profile.dangerous_rules,
                    shell_style_profile_messages,
                    show_raw_agent_reasoning: config.show_raw_agent_reasoning,
                    last_system_status: Mutex::new(None),
                    last_screenshot_info: Mutex::new(None),
                    time_budget: Mutex::new(config.max_run_seconds.map(|secs| {
                        let total = Duration::from_secs(secs);
                        let deadline = config
                            .max_run_deadline
                            .unwrap_or_else(|| Instant::now() + total);
                        RunTimeBudget::new(deadline, total)
                    })),
                    confirm_guard: ConfirmGuardRuntime::from_config(&config.confirm_guard),
                    project_hooks: config.project_hooks.clone(),
                    project_commands: config.project_commands.clone(),
                    tool_output_max_bytes: config.tool_output_max_bytes,
                    hook_guard: AtomicBool::new(false),
                    github: Arc::new(RwLock::new(config.github.clone())),
                    validation: Arc::new(RwLock::new(config.validation.clone())),
                    self_handle: Weak::new(),
                    active_review: Mutex::new(None),
                    next_turn_text_format: Mutex::new(None),
                    env_ctx_v2: config.env_ctx_v2,
                    retention_config: config.retention.clone(),
                    model_descriptions,
                    mcp_access: std::sync::RwLock::new(crate::codex::session::McpAccessState {
                        style: active_shell_style,
                        style_label: active_shell_style_label.clone(),
                        style_include_servers: shell_style_mcp_include,
                        style_exclude_servers: shell_style_mcp_exclude,
                        session_allow_servers: mcp_allow_servers,
                        session_deny_servers: mcp_deny_servers,
                        turn_id: None,
                        turn_allow_servers: HashSet::new(),
                    }),
                });
                let weak_handle = Arc::downgrade(&new_session);
                if let Some(inner) = Arc::get_mut(&mut new_session) {
                    inner.self_handle = weak_handle;
                }
                sess = Some(new_session);
                if let Some(sess_arc) = sess.as_ref()
                    && let Some(lock) = network_policy_decider_session.as_ref()
                {
                    let mut guard = lock.write().await;
                    *guard = Arc::downgrade(sess_arc);
                }
                if let Some(sess_arc) = &sess {
                    // Reset environment context tracker if shell changed
                    if shell_override.is_some() {
                        let mut st = sess_arc.state.lock().unwrap();
                        st.environment_context_tracker = crate::environment_context::EnvironmentContextTracker::new();
                    }
                    if !config.always_allow_commands.is_empty() {
                        let mut st = sess_arc.state.lock().unwrap();
                        for pattern in &config.always_allow_commands {
                            st.approved_commands.insert(pattern.clone());
                        }
                    }
                }
                let mut replay_history_items: Option<Vec<ResponseItem>> = None;


                // Patch restored state into the newly created session.
                if let Some(sess_arc) = &sess
                    && let Some(items) = &restored_items {
                        let turn_context = sess_arc.make_turn_context();
                        let reconstructed = sess_arc.reconstruct_history_from_rollout(&turn_context, items);
                        {
                            let mut st = sess_arc.state.lock().unwrap();
                            st.history = ConversationHistory::new();
                            st.history.record_items(reconstructed.iter());
                        }
                        replay_history_items = Some(reconstructed);
                    }

                // Gather history metadata for SessionConfiguredEvent.
                let (history_log_id, history_entry_count) =
                    crate::message_history::history_metadata(&config).await;

                // ack
                let Some(sess_arc) = sess.as_ref() else {
                    send_no_session_event(sub.id).await;
                    done!(ConfigureSessionControl::Continue);
                };
                let events = std::iter::once(sess_arc.make_event(
                    INITIAL_SUBMIT_ID,
                    EventMsg::SessionConfigured(SessionConfiguredEvent {
                        session_id,
                        model,
                        history_log_id,
                        history_entry_count,
                    }),
                ))
                .chain(mcp_connection_errors.into_iter().map(|message| {
                    sess_arc.make_event(&sub.id, EventMsg::Error(ErrorEvent { message }))
                }));
                for event in events {
                    if let Err(e) = tx_event.send(event).await {
                        error!("failed to send event: {e:?}");
                    }
                }
                // If we resumed from a rollout, replay the prior transcript into the UI.
                if replay_history_items.is_some()
                    || restored_history_snapshot.is_some()
                    || restored_items.is_some()
                {
                    let items = replay_history_items.clone().unwrap_or_default();
                    let history_snapshot_value = restored_history_snapshot
                        .as_ref()
                        .and_then(|snapshot| serde_json::to_value(snapshot).ok());
                    let event = sess_arc.make_event(
                        &sub.id,
                        EventMsg::ReplayHistory(crate::protocol::ReplayHistoryEvent {
                            items,
                            history_snapshot: history_snapshot_value,
                        }),
                    );
                    if let Err(e) = tx_event.send(event).await {
                        warn!("failed to send ReplayHistory event: {e}");
                    }
                }

                if let Some(notice) = resume_notice {
                    let event = sess_arc.make_event(
                        &sub.id,
                        EventMsg::BackgroundEvent(BackgroundEventEvent { message: notice }),
                    );
                    if let Err(e) = tx_event.send(event).await {
                        warn!("failed to send resume notice event: {e}");
                    }
                }

                if let Some(sess_arc) = &sess {
                    spawn_bridge_listener(sess_arc.clone());
                    sess_arc.run_session_hooks(ProjectHookEvent::SessionStart).await;
                }

                // Initialize agent manager after SessionConfigured is sent
                if !agent_manager_initialized {
                    let mut manager = AGENT_MANAGER.write().await;
                    let (agent_tx, mut agent_rx) =
                        tokio::sync::mpsc::unbounded_channel::<AgentStatusUpdatePayload>();
                    manager.set_event_sender(agent_tx);
                    drop(manager);

                    let Some(sess_for_agents) = sess.as_ref().cloned() else {
                        send_no_session_event(sub.id).await;
                        done!(ConfigureSessionControl::Continue);
                    };
                    // Forward agent events to the main event channel
                    let tx_event_clone = tx_event.clone();
                    tokio::spawn(async move {
                        while let Some(payload) = agent_rx.recv().await {
                            let wake_messages = {
                                let mut state = sess_for_agents.state.lock().unwrap();
                                agent_completion_wake_messages(
                                    &payload,
                                    &mut state.agent_completion_wake_batches,
                                )
                            };
                            if !wake_messages.is_empty() {
                                enqueue_agent_completion_wake(&sess_for_agents, wake_messages)
                                    .await;
                            }
                            let status_event = sess_for_agents.make_event(
                                "agent_status",
                                EventMsg::AgentStatusUpdate(AgentStatusUpdateEvent {
                                    agents: payload.agents.clone(),
                                    context: payload.context.clone(),
                                    task: payload.task.clone(),
                                }),
                            );
                            let _ = tx_event_clone.send(status_event).await;
                        }
                    });
                    agent_manager_initialized = true;
                }

    (
        ConfigureSessionState {
            session_id,
            config,
            sess,
            agent_manager_initialized,
        },
        ConfigureSessionControl::Continue,
    )
}
