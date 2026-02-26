use super::*;
use super::session::{
    QueuedUserInput,
    State,
    TurnScratchpad,
    WaitInterruptReason,
    account_usage_context,
    format_retry_eta,
    is_connectivity_error,
    spawn_usage_task,
};
use super::agent_tool_call::{
    agent_completion_wake_messages,
    enqueue_agent_completion_wake,
    get_last_assistant_message_from_turn,
    send_agent_status_update,
};
use crate::auth;
use crate::auth_accounts;
use crate::account_switching::RateLimitSwitchState;
use crate::collaboration_mode_instructions::{
    render_collaboration_mode_instructions,
};
use crate::openai_tools::OpenAiTool;
use crate::openai_tools::ResponsesApiTool;
use crate::openai_tools::SEARCH_TOOL_BM25_TOOL_NAME;
use crate::protocol::McpListToolsResponseEvent;
use crate::tools::scheduler::PendingToolCall;
use code_app_server_protocol::AuthMode as AppAuthMode;

#[derive(Clone, Debug, Eq, PartialEq)]
enum AgentTaskKind {
    Regular,
    Review,
    Compact,
}

/// A series of Turns in response to user input.
pub(super) struct AgentTask {
    sess: Arc<Session>,
    pub(super) sub_id: String,
    handle: AbortHandle,
    kind: AgentTaskKind,
}

impl AgentTask {
    pub(super) fn spawn(
        sess: Arc<Session>,
        turn_context: Arc<TurnContext>,
        sub_id: String,
        input: Vec<InputItem>,
    ) -> Self {
        let handle = {
            let sess_clone = Arc::clone(&sess);
            let tc_clone = Arc::clone(&turn_context);
            let sub_clone = sub_id.clone();
            tokio::spawn(async move {
                run_agent(sess_clone, tc_clone, sub_clone, input).await;
            })
            .abort_handle()
        };
        Self {
            sess,
            sub_id,
            handle,
            kind: AgentTaskKind::Regular,
        }
    }

    pub(super) fn compact(
        sess: Arc<Session>,
        turn_context: Arc<TurnContext>,
        sub_id: String,
        input: Vec<InputItem>,
    ) -> Self {
        let handle = {
            let sess_clone = Arc::clone(&sess);
            let tc_clone = Arc::clone(&turn_context);
            let sub_clone = sub_id.clone();
            tokio::spawn(async move {
                compact::run_compact_task(
                    sess_clone,
                    tc_clone,
                    sub_clone,
                    input,
                )
                .await;
            })
            .abort_handle()
        };
        Self {
            sess,
            sub_id,
            handle,
            kind: AgentTaskKind::Compact,
        }
    }

    pub(super) fn review(
        sess: Arc<Session>,
        turn_context: Arc<TurnContext>,
        sub_id: String,
        input: Vec<InputItem>,
    ) -> Self {
        let handle = {
            let sess_clone = Arc::clone(&sess);
            let tc_clone = Arc::clone(&turn_context);
            let sub_clone = sub_id.clone();
            tokio::spawn(async move {
                run_agent(sess_clone, tc_clone, sub_clone, input).await;
            })
            .abort_handle()
        };
        Self {
            sess,
            sub_id,
            handle,
            kind: AgentTaskKind::Review,
        }
    }

    pub(super) fn abort(self, reason: TurnAbortReason) {
        if !self.handle.is_finished() {
            self.handle.abort();
            let event = self
                .sess
                .make_event(&self.sub_id, EventMsg::TurnAborted(TurnAbortedEvent { reason }));
            let sess = self.sess.clone();
            let sub_id = self.sub_id.clone();
            let kind = self.kind;
            tokio::spawn(async move {
                if kind == AgentTaskKind::Review {
                    exit_review_mode(sess.clone(), sub_id, None).await;
                }
                sess.send_event(event).await;
            });
        }
    }
}

async fn load_skills_inventory_and_refresh_session(
    sess: &Arc<Session>,
    config_snapshot: Arc<Config>,
) -> crate::skills::model::SkillLoadOutcome {
    let skills_enabled = config_snapshot.skills_enabled;
    let active_shell_style = sess.user_shell.script_style();
    let active_shell_style_label = active_shell_style.map(|style| style.to_string());

    let mut shell_style_skill_filter: Option<HashSet<String>> = None;
    let mut shell_style_disabled_skills: HashSet<String> = HashSet::new();
    let mut shell_style_skill_roots: Vec<PathBuf> = Vec::new();
    if let Some(style) = active_shell_style
        && let Some(profile) = config_snapshot.shell_style_profiles.get(&style)
    {
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
                .iter()
                .filter(|path| !path.as_os_str().is_empty())
                .cloned(),
        );
    }

    let config_for_load = Arc::clone(&config_snapshot);
    let inventory = match tokio::task::spawn_blocking(move || {
        if !skills_enabled {
            return crate::skills::model::SkillLoadOutcome::default();
        }

        if shell_style_skill_roots.is_empty() {
            crate::skills::loader::load_skills(config_for_load.as_ref())
        } else {
            crate::skills::loader::load_skills_with_additional_roots(
                config_for_load.as_ref(),
                shell_style_skill_roots.into_iter(),
            )
        }
    })
    .await
    {
        Ok(outcome) => outcome,
        Err(err) => {
            warn!("failed to load skills: {err}");
            crate::skills::model::SkillLoadOutcome::default()
        }
    };

    for err in &inventory.errors {
        warn!("invalid skill {}: {}", err.path.display(), err.message);
    }

    if skills_enabled {
        let available_skill_names: HashSet<String> = inventory
            .skills
            .iter()
            .map(|skill| skill.name.trim().to_ascii_lowercase())
            .collect();

        let mut matched_skills: HashSet<String> = HashSet::new();
        let mut active_skills: Vec<crate::skills::model::SkillMetadata> = Vec::new();
        for skill in &inventory.skills {
            let normalized = skill.name.trim().to_ascii_lowercase();
            if let Some(skill_filter) = shell_style_skill_filter.as_ref() {
                if !skill_filter.contains(&normalized) {
                    continue;
                }
                matched_skills.insert(normalized.clone());
            }

            if shell_style_disabled_skills.contains(&normalized) {
                continue;
            }

            active_skills.push(crate::skills::model::SkillMetadata {
                name: skill.name.clone(),
                description: skill.description.clone(),
                path: skill.path.clone(),
                scope: skill.scope,
                content: String::new(),
            });
        }

        if let Some(style_label) = active_shell_style_label.as_deref()
            && let Some(skill_filter) = shell_style_skill_filter.as_ref()
        {
            for requested in skill_filter {
                if !matched_skills.contains(requested) {
                    warn!("shell style profile `{style_label}` requested unknown skill `{requested}`");
                }
            }
        }

        if let Some(style_label) = active_shell_style_label.as_deref() {
            for requested in &shell_style_disabled_skills {
                if !available_skill_names.contains(requested) {
                    warn!(
                        "shell style profile `{style_label}` requested unknown disabled skill `{requested}`"
                    );
                }
            }
        }

        *sess.skills.write().await = active_skills;
    } else {
        sess.skills.write().await.clear();
    }

    inventory
}

pub(super) async fn submission_loop(
    mut session_id: Uuid,
    config: Arc<Config>,
    auth_manager: Option<Arc<AuthManager>>,
    rx_sub: Receiver<Submission>,
    tx_event: Sender<Event>,
) {
    let mut config = config;
    let mut sess: Option<Arc<Session>> = None;
    let mut agent_manager_initialized = false;

    let file_watcher = crate::file_watcher::FileWatcher::new(config.code_home.clone())
        .unwrap_or_else(|err| {
            warn!("failed to start file watcher: {err}");
            crate::file_watcher::FileWatcher::noop()
        });
    file_watcher.register_config(config.as_ref());
    let mut file_watcher_rx = file_watcher.subscribe();
    let mut file_watcher_enabled = true;
    // shorthand - send an event when there is no active session
    let send_no_session_event = |sub_id: String| async {
        let event = Event {
            id: sub_id,
            event_seq: 0,
            msg: EventMsg::Error(ErrorEvent { message: "No session initialized, expected 'ConfigureSession' as first Op".to_string() }),
            order: None,
        };
        tx_event.send(event).await.ok();
    };

    // To break out of this loop, send Op::Shutdown.
    loop {
        tokio::select! {
            sub = rx_sub.recv() => {
                let sub = match sub {
                    Ok(sub) => sub,
                    Err(_) => break,
                };

                debug!(?sub, "Submission");
                match sub.op {
            Op::Interrupt => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess.clone(),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                tokio::spawn(async move {
                    sess.notify_wait_interrupted(WaitInterruptReason::SessionAborted);
                    sess.abort();
                });
            }
            Op::CancelAgents { batch_ids, agent_ids } => {
                let sess_arc = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                let mut manager = AGENT_MANAGER.write().await;
                let mut seen_batches: HashSet<String> = HashSet::new();
                let mut seen_agents: HashSet<String> = HashSet::new();
                let mut cancelled = 0usize;

                for batch in batch_ids {
                    let trimmed = batch.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if !seen_batches.insert(trimmed.to_string()) {
                        continue;
                    }
                    cancelled += manager.cancel_batch(trimmed).await;
                }

                for agent_id in agent_ids {
                    let trimmed = agent_id.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if !seen_agents.insert(trimmed.to_string()) {
                        continue;
                    }
                    if manager.cancel_agent(trimmed).await {
                        cancelled += 1;
                    }
                }

                drop(manager);

                send_agent_status_update(&sess_arc).await;

                let message = if cancelled == 0 {
                    "No running agents to cancel.".to_string()
                } else {
                    let suffix = if cancelled == 1 { "" } else { "s" };
                    format!("Cancelled {cancelled} running agent{suffix}.")
                };

                let event = sess_arc.make_event(
                    &sub.id,
                    EventMsg::AgentMessage(AgentMessageEvent { message }),
                );
                sess_arc.send_event(event).await;
            }
            Op::AddPendingInputDeveloper { text } => {
                let sess = match sess.as_ref() { Some(s) => s.clone(), None => { send_no_session_event(sub.id).await; continue; } };
                let dev_msg = ResponseInputItem::Message { role: "developer".to_string(), content: vec![ContentItem::InputText { text }] };
                let should_start_turn = sess.enqueue_out_of_turn_item(dev_msg);
                if should_start_turn {
                    sess.cleanup_old_status_items().await;
                    let turn_context = sess.make_turn_context();
                    let sub_id = sess.next_internal_sub_id();
                    let sentinel_input = vec![InputItem::Text {
                        text: PENDING_ONLY_SENTINEL.to_string(),
                    }];
                    let agent = AgentTask::spawn(Arc::clone(&sess), turn_context, sub_id, sentinel_input);
                    sess.set_task(agent);
                }
            }
            Op::ConfigureSession {
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
            } => {
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
                    return;
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
                            return;
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
                    .map(|outcome| strip_skill_contents(outcome.skills.as_slice()))
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
                            .and_then(super::super::auth::CodexAuth::get_account_id),
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
                    continue;
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
                        continue;
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
            }
            Op::UserInput {
                items,
                final_output_json_schema,
            } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                // Clean up old status items when new user input arrives
                // This prevents token buildup from old screenshots/status messages
                sess.cleanup_old_status_items().await;

                // Abort synchronously here to avoid a race that can kill the
                // newly spawned agent if the async abort runs after set_task.
                sess.notify_wait_interrupted(WaitInterruptReason::UserMessage);
                sess.abort();

                // Spawn a new agent for this user input.
                let turn_context = sess.make_turn_context_with_schema(final_output_json_schema);
                let agent = AgentTask::spawn(Arc::clone(sess), turn_context, sub.id.clone(), items);
                sess.set_task(agent);
            }
            Op::QueueUserInput { items } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                if sess.has_running_task() {
                    let mut response_item = response_input_from_core_items(items.clone());
                    sess.enforce_user_message_limits(&sub.id, &mut response_item);
                    sess.notify_wait_interrupted(WaitInterruptReason::UserMessage);
                    let queued = QueuedUserInput {
                        submission_id: sub.id.clone(),
                        response_item,
                        core_items: items,
                    };
                    sess.queue_user_input(queued);
                } else {
                    // No task running: treat this as immediate user input without aborting.
                    sess.cleanup_old_status_items().await;
                    let turn_context = sess.make_turn_context();
                    let agent = AgentTask::spawn(Arc::clone(sess), turn_context, sub.id.clone(), items);
                    sess.set_task(agent);
                }
            }
            Op::ExecApproval { id, decision, .. } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                match decision {
                    ReviewDecision::Abort => {
                        sess.notify_wait_interrupted(WaitInterruptReason::SessionAborted);
                        sess.abort();
                    }
                    other => sess.notify_approval(&id, other),
                }
            }
            Op::UserInputAnswer { id, response } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                sess.notify_user_input_response(&id, response);
            }
            Op::DynamicToolResponse { id, response } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                sess.notify_dynamic_tool_response(&id, response);
            }
            Op::RegisterApprovedCommand {
                command,
                match_kind,
                semantic_prefix,
            } => {
                if command.is_empty() {
                    continue;
                }
                if let Some(sess) = sess.as_ref() {
                    sess.add_approved_command(ApprovedCommandPattern::new(
                        command,
                        match_kind,
                        semantic_prefix,
                    ));
                } else {
                    send_no_session_event(sub.id).await;
                }
            }
            Op::PatchApproval { id, decision } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                match decision {
                    ReviewDecision::Abort => {
                        sess.notify_wait_interrupted(WaitInterruptReason::SessionAborted);
                        sess.abort();
                    }
                    other => sess.notify_approval(&id, other),
                }
            }
            Op::UpdateValidationTool { name, enable } => {
                if let Some(sess) = sess.as_ref() {
                    sess.update_validation_tool(&name, enable);
                } else {
                    send_no_session_event(sub.id).await;
                }
            }
            Op::UpdateValidationGroup { group, enable } => {
                if let Some(sess) = sess.as_ref() {
                    sess.update_validation_group(group, enable);
                } else {
                    send_no_session_event(sub.id).await;
                }
            }
            Op::AddToHistory { text } => {
                // TODO: What should we do if we got AddToHistory before ConfigureSession?
                // currently, if ConfigureSession has resume path, this history will be ignored
                let id = session_id;
                let config = config.clone();
                tokio::spawn(async move {
                    if let Err(e) = crate::message_history::append_entry(&text, &id, &config).await
                    {
                        warn!("failed to append to message history: {e}");
                    }
                });
            }

            Op::PersistHistorySnapshot { snapshot } => {
                let Some(sess) = sess.as_ref() else {
                    send_no_session_event(sub.id).await;
                    continue;
                };
                if let Some(recorder) = sess.clone_rollout_recorder() {
                    tokio::spawn(async move {
                        if let Err(e) = recorder.set_history_snapshot(snapshot).await {
                            warn!("failed to persist history snapshot: {e}");
                        }
                    });
                }
            }

            Op::RunProjectCommand { name } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                let mut tracker = TurnDiffTracker::new();
                let attempt_req = sess.current_request_ordinal();
                sess.run_project_command(&mut tracker, &sub.id, &name, attempt_req)
                    .await;
            }

            Op::GetHistoryEntryRequest { offset, log_id } => {
                let config = config.clone();
                let tx_event = tx_event.clone();
                let sub_id = sub.id.clone();

                tokio::spawn(async move {
                    // Run lookup in blocking thread because it does file IO + locking.
                    let entry_opt = tokio::task::spawn_blocking(move || {
                        crate::message_history::lookup(log_id, offset, &config)
                    })
                    .await
                    .unwrap_or(None);

                    let event = Event {
                        id: sub_id,
                        event_seq: 0,
                        msg: EventMsg::GetHistoryEntryResponse(
                            crate::protocol::GetHistoryEntryResponseEvent {
                                offset,
                                log_id,
                                entry: entry_opt,
                            },
                        ),
                        order: None,
                    };

                    if let Err(e) = tx_event.send(event).await {
                        warn!("failed to send GetHistoryEntryResponse event: {e}");
                    }
                });
            }
            Op::ListMcpTools => {
                let sess = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                let tools = sess
                    .mcp_connection_manager
                    .list_all_tools()
                    .into_iter()
                    .filter_map(|(name, tool)| {
                        let value = match serde_json::to_value(tool) {
                            Ok(value) => value,
                            Err(err) => {
                                warn!("failed to serialize MCP tool {name}: {err}");
                                return None;
                            }
                        };
                        match code_protocol::mcp::Tool::from_mcp_value(value) {
                            Ok(converted) => Some((name, converted)),
                            Err(err) => {
                                warn!("failed to convert MCP tool {name}: {err}");
                                None
                            }
                        }
                    })
                    .collect();
                let server_tools = sess.mcp_connection_manager.list_tools_by_server();
                let server_disabled_tools =
                    sess.mcp_connection_manager.list_disabled_tools_by_server();
                let server_failures = sess.mcp_connection_manager.list_server_failures();
                let resources =
                    convert_mcp_resources_by_server(sess.mcp_connection_manager.list_resources_by_server().await);
                let resource_templates = convert_mcp_resource_templates_by_server(
                    sess.mcp_connection_manager
                        .list_resource_templates_by_server()
                        .await,
                );
                let auth_statuses = sess.mcp_connection_manager.list_auth_statuses().await;

                let event = Event {
                    id: sub.id.clone(),
                    event_seq: 0,
                    msg: EventMsg::McpListToolsResponse(McpListToolsResponseEvent {
                        tools,
                        server_tools: Some(server_tools),
                        server_disabled_tools: Some(server_disabled_tools),
                        server_failures: Some(server_failures),
                        resources,
                        resource_templates,
                        auth_statuses,
                    }),
                    order: None,
                };

                if let Err(e) = tx_event.send(event).await {
                    warn!("failed to send McpListToolsResponse event: {e}");
                }
            }
            Op::RefreshMcpTools => {
                let sess = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                sess.mcp_connection_manager.refresh_tools().await;

                let tools = sess
                    .mcp_connection_manager
                    .list_all_tools()
                    .into_iter()
                    .filter_map(|(name, tool)| {
                        let value = match serde_json::to_value(tool) {
                            Ok(value) => value,
                            Err(err) => {
                                warn!("failed to serialize MCP tool {name}: {err}");
                                return None;
                            }
                        };
                        match code_protocol::mcp::Tool::from_mcp_value(value) {
                            Ok(converted) => Some((name, converted)),
                            Err(err) => {
                                warn!("failed to convert MCP tool {name}: {err}");
                                None
                            }
                        }
                    })
                    .collect();
                let server_tools = sess.mcp_connection_manager.list_tools_by_server();
                let server_disabled_tools =
                    sess.mcp_connection_manager.list_disabled_tools_by_server();
                let server_failures = sess.mcp_connection_manager.list_server_failures();
                let resources =
                    convert_mcp_resources_by_server(sess.mcp_connection_manager.list_resources_by_server().await);
                let resource_templates = convert_mcp_resource_templates_by_server(
                    sess.mcp_connection_manager
                        .list_resource_templates_by_server()
                        .await,
                );
                let auth_statuses = sess.mcp_connection_manager.list_auth_statuses().await;

                let event = Event {
                    id: sub.id.clone(),
                    event_seq: 0,
                    msg: EventMsg::McpListToolsResponse(McpListToolsResponseEvent {
                        tools,
                        server_tools: Some(server_tools),
                        server_disabled_tools: Some(server_disabled_tools),
                        server_failures: Some(server_failures),
                        resources,
                        resource_templates,
                        auth_statuses,
                    }),
                    order: None,
                };

                if let Err(e) = tx_event.send(event).await {
                    warn!("failed to send McpListToolsResponse event: {e}");
                }
            }
            Op::SetMcpToolEnabled {
                server,
                tool,
                enable,
            } => {
                let sess = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                sess.mcp_connection_manager
                    .set_tool_enabled(&server, &tool, enable)
                    .await;

                let tools = sess
                    .mcp_connection_manager
                    .list_all_tools()
                    .into_iter()
                    .filter_map(|(name, tool)| {
                        let value = match serde_json::to_value(tool) {
                            Ok(value) => value,
                            Err(err) => {
                                warn!("failed to serialize MCP tool {name}: {err}");
                                return None;
                            }
                        };
                        match code_protocol::mcp::Tool::from_mcp_value(value) {
                            Ok(converted) => Some((name, converted)),
                            Err(err) => {
                                warn!("failed to convert MCP tool {name}: {err}");
                                None
                            }
                        }
                    })
                    .collect();
                let server_tools = sess.mcp_connection_manager.list_tools_by_server();
                let server_disabled_tools =
                    sess.mcp_connection_manager.list_disabled_tools_by_server();
                let server_failures = sess.mcp_connection_manager.list_server_failures();
                let resources =
                    convert_mcp_resources_by_server(sess.mcp_connection_manager.list_resources_by_server().await);
                let resource_templates = convert_mcp_resource_templates_by_server(
                    sess.mcp_connection_manager
                        .list_resource_templates_by_server()
                        .await,
                );
                let auth_statuses = sess.mcp_connection_manager.list_auth_statuses().await;

                let event = Event {
                    id: sub.id.clone(),
                    event_seq: 0,
                    msg: EventMsg::McpListToolsResponse(McpListToolsResponseEvent {
                        tools,
                        server_tools: Some(server_tools),
                        server_disabled_tools: Some(server_disabled_tools),
                        server_failures: Some(server_failures),
                        resources,
                        resource_templates,
                        auth_statuses,
                    }),
                    order: None,
                };

                if let Err(e) = tx_event.send(event).await {
                    warn!("failed to send McpListToolsResponse event: {e}");
                }
            }
            Op::ListCustomPrompts => {
                let sess = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                let custom_prompts: Vec<code_protocol::custom_prompts::CustomPrompt> =
                    if let Some(dir) = crate::custom_prompts::default_prompts_dir() {
                        crate::custom_prompts::discover_prompts_in(&dir).await
                    } else {
                        Vec::new()
                    };

                let event = Event {
                    id: sub.id.clone(),
                    event_seq: 0,
                    msg: EventMsg::ListCustomPromptsResponse(ListCustomPromptsResponseEvent {
                        custom_prompts,
                    }),
                    order: None,
                };

                sess.send_event(event).await;
            }
            Op::ListSkills => {
                let sess = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                let inventory =
                    load_skills_inventory_and_refresh_session(&sess, Arc::clone(&config)).await;

                let skills: Vec<code_protocol::skills::Skill> = inventory
                    .skills
                    .iter()
                    .map(|skill| code_protocol::skills::Skill {
                        name: skill.name.clone(),
                        description: skill.description.clone(),
                        path: skill.path.clone(),
                        scope: match skill.scope {
                            crate::skills::model::SkillScope::Repo => {
                                code_protocol::skills::SkillScope::Repo
                            }
                            crate::skills::model::SkillScope::User => {
                                code_protocol::skills::SkillScope::User
                            }
                            crate::skills::model::SkillScope::System => {
                                code_protocol::skills::SkillScope::System
                            }
                            crate::skills::model::SkillScope::Admin => {
                                code_protocol::skills::SkillScope::System
                            }
                        },
                        content: skill.content.clone(),
                    })
                    .collect();

                let event = Event {
                    id: sub.id.clone(),
                    event_seq: 0,
                    msg: EventMsg::ListSkillsResponse(ListSkillsResponseEvent { skills }),
                    order: None,
                };

                sess.send_event(event).await;
            }
            Op::Compact => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                let prompt_text = sess.compact_prompt_text();
                // Attempt to inject input into current task
                if let Err(items) = sess.inject_input(vec![InputItem::Text {
                    text: prompt_text,
                }]) {
                    let turn_context = sess.make_turn_context();
                    compact::spawn_compact_task(sess.clone(), turn_context, sub.id.clone(), items);
                } else {
                    let was_empty = sess.enqueue_manual_compact(sub.id.clone());
                    let message = if was_empty {
                        "Manual compact queued; it will run after the current response finishes.".to_string()
                    } else {
                        "Manual compact already queued; waiting for the current response to finish.".to_string()
                    };
                    let event = sess.make_event(
                        &sub.id,
                        EventMsg::AgentMessage(AgentMessageEvent { message }),
                    );
                    sess.send_event(event).await;
                }
            }
            Op::Review { review_request } => {
                let sess = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                let config = Arc::clone(&config);
                let sub_id = sub.id.clone();
                spawn_review_thread(sess, config, sub_id, review_request).await;
            }
            Op::SetNextTextFormat { format } => {
                let sess_arc = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                *sess_arc.next_turn_text_format.lock().unwrap() = Some(format);
            }
            Op::Shutdown => {
                info!("Shutting down Codex instance");

                // Ensure any running agent is aborted so streaming stops promptly.
                if let Some(sess_arc) = sess.as_ref() {
                    let s2 = sess_arc.clone();
                    tokio::spawn(async move {
                        s2.notify_wait_interrupted(WaitInterruptReason::SessionAborted);
                        s2.abort();
                    });
                }

                // Gracefully flush and shutdown rollout recorder on session end so tests
                // that inspect the rollout file do not race with the background writer.
                if let Some(ref sess_arc) = sess {
                    let recorder_opt = sess_arc.rollout.lock().unwrap().take();
                    if let Some(rec) = recorder_opt
                        && let Err(e) = rec.shutdown().await {
                            warn!("failed to shutdown rollout recorder: {e}");
                            let event = sess_arc.make_event(
                                &sub.id,
                                EventMsg::Error(ErrorEvent {
                                    message: "Failed to shutdown rollout recorder".to_string(),
                                }),
                            );
                            if let Err(e) = tx_event.send(event).await {
                                warn!("failed to send error message: {e:?}");
                            }
                        }
                }
                if let Some(ref sess_arc) = sess {
                    sess_arc.run_session_hooks(ProjectHookEvent::SessionEnd).await;
                }
                let event = match sess {
                    Some(ref sess_arc) => sess_arc.make_event(&sub.id, EventMsg::ShutdownComplete),
                    None => Event {
                        id: sub.id.clone(),
                        event_seq: 0,
                        msg: EventMsg::ShutdownComplete,
                        order: None,
                    },
                };
                if let Err(e) = tx_event.send(event).await {
                    warn!("failed to send Shutdown event: {e}");
                }
                break;
            }
                }
            }
            watcher_event = file_watcher_rx.recv(), if file_watcher_enabled => {
                match watcher_event {
                    Ok(crate::file_watcher::FileWatcherEvent::SkillsChanged { .. }) => {
                        let Some(sess_arc) = sess.as_ref() else {
                            continue;
                        };
                        let sess_arc = Arc::clone(sess_arc);
                        let config_snapshot = Arc::clone(&config);
                        tokio::spawn(async move {
                            let _ = load_skills_inventory_and_refresh_session(
                                &sess_arc,
                                config_snapshot,
                            )
                            .await;
                        });
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        warn!("file watcher channel closed; disabling");
                        file_watcher_enabled = false;
                    }
                }
            }
        }
    }
    debug!("Agent loop exited");
}

fn merge_developer_message(existing: Option<String>, extra: &str) -> Option<String> {
    let extra_trimmed = extra.trim();
    if extra_trimmed.is_empty() {
        return existing;
    }

    match existing {
        Some(mut message) => {
            if !message.trim().is_empty() {
                message.push_str("\n\n");
            }
            message.push_str(extra_trimmed);
            Some(message)
        }
        None => Some(extra_trimmed.to_string()),
    }
}

fn build_timeboxed_review_message(base: Option<String>) -> Option<String> {
    let mut message = merge_developer_message(base.clone(), AUTO_EXEC_TIMEBOXED_REVIEW_GUIDANCE);
    if base.as_deref() == Some(AUTO_EXEC_TIMEBOXED_CLI_GUIDANCE) {
        message = Some(AUTO_EXEC_TIMEBOXED_REVIEW_GUIDANCE.to_string());
    }
    message
}

async fn spawn_review_thread(
    sess: Arc<Session>,
    config: Arc<Config>,
    sub_id: String,
    review_request: ReviewRequest,
) {
    // Ensure any running task is stopped before starting the review flow.
    sess.notify_wait_interrupted(WaitInterruptReason::SessionAborted);
    sess.abort();

    let parent_turn_context = sess.make_turn_context();

    // Determine model + family for review mode.
    let review_model = config.review_model.clone();
    let review_family = find_family_for_model(&review_model)
        .unwrap_or_else(|| derive_default_model_family(&review_model));

    // Prepare a per-review configuration that favors deterministic feedback.
    let mut review_config = (*config).clone();
    review_config.model = review_model.clone();
    review_config.model_family = review_family.clone();
    review_config.model_reasoning_effort = config.review_model_reasoning_effort;
    review_config.model_reasoning_summary = ReasoningSummaryConfig::Detailed;
    review_config.model_text_verbosity = config.model_text_verbosity;
    review_config.user_instructions = None;
    review_config.base_instructions = Some(REVIEW_PROMPT.to_string());
    if let Some(cw) = review_family.context_window {
        review_config.model_context_window = Some(cw);
    }
    if let Some(max) = review_family.max_output_tokens {
        review_config.model_max_output_tokens = Some(max);
    }
    let review_config = Arc::new(review_config);

    let review_debug_logger = match crate::debug_logger::DebugLogger::new(review_config.debug) {
        Ok(logger) => Arc::new(Mutex::new(logger)),
        Err(err) => {
            warn!("failed to create review debug logger: {err}");
            Arc::new(Mutex::new(
                crate::debug_logger::DebugLogger::new(false).unwrap(),
            ))
        }
    };

    let review_otel = parent_turn_context
        .client
        .get_otel_event_manager()
        .map(|mgr| mgr.with_model(review_config.model.as_str(), review_config.model_family.slug.as_str()));

    let review_client = ModelClient::new(crate::client::ModelClientInit {
        config: review_config.clone(),
        auth_manager: parent_turn_context.client.get_auth_manager(),
        otel_event_manager: review_otel,
        provider: parent_turn_context.client.get_provider(),
        effort: review_config.model_reasoning_effort,
        summary: review_config.model_reasoning_summary,
        verbosity: review_config.model_text_verbosity,
        session_id: sess.session_uuid(),
        debug_logger: review_debug_logger,
    });

    let review_demo_message = if config.timeboxed_exec_mode {
        build_timeboxed_review_message(parent_turn_context.demo_developer_message.clone())
    } else {
        parent_turn_context.demo_developer_message.clone()
    };

    let review_turn_context = Arc::new(TurnContext {
        client: review_client,
        cwd: parent_turn_context.cwd.clone(),
        base_instructions: Some(REVIEW_PROMPT.to_string()),
        user_instructions: None,
        demo_developer_message: review_demo_message,
        compact_prompt_override: parent_turn_context.compact_prompt_override.clone(),
        approval_policy: parent_turn_context.approval_policy,
        sandbox_policy: parent_turn_context.sandbox_policy.clone(),
        shell_environment_policy: parent_turn_context.shell_environment_policy.clone(),
        collaboration_mode: parent_turn_context.collaboration_mode,
        is_review_mode: true,
        text_format_override: None,
        final_output_json_schema: None,
    });

    let review_prompt_text = format!(
        "{}\n\n---\n\nNow, here's your task: {}",
        REVIEW_PROMPT.trim(),
        review_request.prompt.trim()
    );
    let review_input = vec![InputItem::Text {
        text: review_prompt_text,
    }];

    let task = AgentTask::review(Arc::clone(&sess), Arc::clone(&review_turn_context), sub_id.clone(), review_input);
    sess.set_active_review(review_request.clone());
    sess.set_task(task);

    let event = sess.make_event(
        &sub_id,
        EventMsg::EnteredReviewMode(review_request.clone()),
    );
    sess.send_event(event).await;
}

async fn exit_review_mode(
    session: Arc<Session>,
    task_sub_id: String,
    review_output: Option<ReviewOutputEvent>,
) {
    let snapshot = capture_review_snapshot(&session).await;
    let event = session.make_event(
        &task_sub_id,
        EventMsg::ExitedReviewMode(ExitedReviewModeEvent {
            review_output: review_output.clone(),
            snapshot,
        }),
    );
    session.send_event(event).await;

    let _active_request = session.take_active_review();

    let developer_text = match review_output.clone() {
        Some(output) => {
            let mut sections: Vec<String> = Vec::new();
            if !output.overall_explanation.trim().is_empty() {
                sections.push(output.overall_explanation.trim().to_string());
            }
            if !output.findings.is_empty() {
                sections.push(format_review_findings_block(&output.findings, None));
            }
            if !output.overall_correctness.trim().is_empty() {
                sections.push(format!(
                    "Overall correctness: {}",
                    output.overall_correctness.trim()
                ));
            }
            if output.overall_confidence_score > 0.0 {
                sections.push(format!(
                    "Confidence score: {:.1}",
                    output.overall_confidence_score
                ));
            }

            let results = if sections.is_empty() {
                "Reviewer did not provide any findings.".to_string()
            } else {
                sections.join("\n\n")
            };

            format!(
                "<user_action>\n  <context>User initiated a review task. Here's the full review output from reviewer model. User may select one or more comments to resolve.</context>\n  <action>review</action>\n  <results>\n  {results}\n  </results>\n</user_action>\n"
            )
        }
        None => {
            "<user_action>\n  <context>User initiated a review task, but it ended without a final response. If the user asks about this, tell them to re-initiate a review with `/review` and wait for it to complete.</context>\n  <action>review</action>\n  <results>\n  None.\n  </results>\n</user_action>\n"
                .to_string()
        }
    };

    let developer_message = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText { text: developer_text.clone() }],
        end_turn: None,
        phase: None,
    };

    session
        .record_conversation_items(&[developer_message])
        .await;
}

async fn capture_review_snapshot(session: &Session) -> Option<ReviewSnapshotInfo> {
    let cwd = session.cwd.clone();
    let repo_root = crate::git_info::get_git_repo_root(&cwd);
    let branch = crate::git_info::current_branch_name(&cwd).await;

    if repo_root.is_none() && branch.is_none() {
        return None;
    }

    Some(ReviewSnapshotInfo {
        snapshot_commit: None,
        branch,
        worktree_path: Some(cwd),
        repo_root,
    })
}

fn parse_review_output_event(text: &str) -> ReviewOutputEvent {
    if let Ok(parsed) = serde_json::from_str::<ReviewOutputEvent>(text) {
        return parsed;
    }

    // Attempt to extract JSON from fenced code blocks if present.
    if let Some(idx) = text.find("```json")
        && let Some(end_idx) = text[idx + 7..].find("```") {
            let json_slice = &text[idx + 7..idx + 7 + end_idx];
            if let Ok(parsed) = serde_json::from_str::<ReviewOutputEvent>(json_slice) {
                return parsed;
            }
        }

    ReviewOutputEvent {
        findings: Vec::new(),
        overall_correctness: String::new(),
        overall_explanation: text.trim().to_string(),
        overall_confidence_score: 0.0,
    }
}

// Intentionally omit upstream review thread spawning; our fork handles review flows differently.
/// Takes a user message as input and runs a loop where, at each turn, the model
/// replies with either:
///
/// - requested function calls
/// - an assistant message
///
/// While it is possible for the model to return multiple of these items in a
/// single turn, in practice, we generally one item per turn:
///
/// - If the model requests a function call, we execute it and send the output
///   back to the model in the next turn.
/// - If the model sends only an assistant message, we record it in the
///   conversation history and consider the agent complete.
async fn run_agent(sess: Arc<Session>, turn_context: Arc<TurnContext>, sub_id: String, input: Vec<InputItem>) {
    if input.is_empty() {
        return;
    }
    let event = sess.make_event(&sub_id, EventMsg::TaskStarted);
    if sess.tx_event.send(event).await.is_err() {
        return;
    }
    // Continue with our fork's history and input handling.

    let is_review_mode = turn_context.is_review_mode;
    let mut review_history: Vec<ResponseItem> = Vec::new();
    let mut review_messages: Vec<String> = Vec::new();
    let mut review_exit_emitted = false;

    let pending_only_turn = input.len() == 1
        && matches!(
            &input[0],
            InputItem::Text { text } if text == PENDING_ONLY_SENTINEL
        );

    // Debug logging for ephemeral images
    let ephemeral_count = input
        .iter()
        .filter(|item| matches!(item, InputItem::EphemeralImage { .. }))
        .count();

    if ephemeral_count > 0 {
        tracing::info!(
            "Processing {} ephemeral images in user input",
            ephemeral_count
        );
    }

    let mut initial_response_item: Option<ResponseItem> = None;

    if !pending_only_turn {
        // Convert input to ResponseInputItem
        let mut response_input = response_input_from_core_items(input.clone());
        sess.enforce_user_message_limits(&sub_id, &mut response_input);
        let response_item: ResponseItem = response_input.into();

        if is_review_mode {
            review_history.push(response_item.clone());
        } else {
            // Record to history but we'll handle ephemeral images separately
            sess.record_conversation_items(std::slice::from_ref(&response_item))
                .await;
        }
        initial_response_item = Some(response_item);
    }

    let mut last_task_message: Option<String> = None;
    // Although from the perspective of codex.rs, TurnDiffTracker has the lifecycle of a Agent which contains
    // many turns, from the perspective of the user, it is a single turn.
    let mut turn_diff_tracker = TurnDiffTracker::new();

    // Track if this is the first iteration - if so, include the initial input
    let mut first_iteration = true;

    // Track if we've done a proactive compaction in this iteration to prevent
    // infinite loops. As long as compaction works well in getting us way below
    // the token limit, we shouldn't need more than one compaction per iteration.
    let mut did_proactive_compact_this_iteration = false;
    let mut auto_compact_pending = false;

    loop {
        // Note that pending_input would be something like a message the user
        // submitted through the UI while the model was running. Though the UI
        // may support this, the model might not.
        // IMPORTANT: Do not inject queued user inputs into the review thread.
        // Doing so routes user messages (e.g., auto-resolve fix prompts) to the
        // review model, causing loops. Only include queued user inputs when not in
        // review mode. They will be picked up after TaskComplete via
        // pop_next_queued_user_input.
        let pending_input = if is_review_mode {
            sess.get_pending_input_filtered(false)
        } else {
            sess.get_pending_input()
        }
        .into_iter()
        .map(ResponseItem::from)
        .collect::<Vec<ResponseItem>>();
        let mut pending_input_tail = pending_input.clone();

        if initial_response_item.is_none() {
            if let Some(first_pending) = pending_input_tail.first().cloned() {
                pending_input_tail.remove(0);
                if is_review_mode {
                    review_history.push(first_pending.clone());
                } else {
                    sess.record_conversation_items(std::slice::from_ref(&first_pending))
                        .await;
                }
                initial_response_item = Some(first_pending);
            } else {
                tracing::warn!(
                    "pending-only turn had no queued input; skipping model invocation"
                );
                break;
            }
        }

        let compact_snapshot = if auto_compact_pending && !is_review_mode {
            Some(sess.turn_input_with_history(pending_input_tail.clone()))
        } else {
            None
        };

        // Do not duplicate the initial input in `pending_input`.
        // It is already recorded to history above; ephemeral items are appended separately.
        if first_iteration {
            first_iteration = false;
        } else {
            // Only record pending input to history on subsequent iterations
            sess.record_conversation_items(&pending_input).await;
        }

        if auto_compact_pending && !is_review_mode {
            let compacted_history = if compact::should_use_remote_compact_task(&sess).await {
                run_inline_remote_auto_compact_task(
                    Arc::clone(&sess),
                    Arc::clone(&turn_context),
                    Vec::new(),
                )
                .await
            } else {
                compact::run_inline_auto_compact_task(
                    Arc::clone(&sess),
                    Arc::clone(&turn_context),
                )
                .await
            };

            if !compacted_history.is_empty() {
                let mut rebuilt = compacted_history;
                if !pending_input_tail.is_empty() {
                    let previous_input_snapshot = compact_snapshot.unwrap_or_default();
                    let (missing_calls, filtered_outputs) = reconcile_pending_tool_outputs(
                        &pending_input_tail,
                        &rebuilt,
                        &previous_input_snapshot,
                    );
                    if !missing_calls.is_empty() {
                        rebuilt.extend(missing_calls);
                    }
                    if !filtered_outputs.is_empty() {
                        rebuilt.extend(filtered_outputs);
                    }
                }
                sess.replace_history(rebuilt);
                pending_input_tail.clear();
                did_proactive_compact_this_iteration = true;
            }
            auto_compact_pending = false;
        }

        // Construct the input that we will send to the model. When using the
        // Chat completions API (or ZDR clients), the model needs the full
        // conversation history on each turn. The rollout file, however, should
        // only record the new items that originated in this turn so that it
        // represents an append-only log without duplicates.
        let turn_input: Vec<ResponseItem> = if is_review_mode {
            if !pending_input_tail.is_empty() {
                review_history.extend(pending_input_tail.clone());
            }
            review_history.clone()
        } else {
            sess.turn_input_with_history(pending_input_tail.clone())
        };

        let turn_input_messages: Vec<String> = turn_input
            .iter()
            .filter_map(|item| match item {
                ResponseItem::Message { role, content, .. } if role == "user" => Some(content),
                _ => None,
            })
            .flat_map(|content| {
                content.iter().filter_map(|item| match item {
                    ContentItem::InputText { text } => Some(text.clone()),
                    _ => None,
                })
            })
            .collect();
        match run_turn(
            &sess,
            &turn_context,
            &mut turn_diff_tracker,
            sub_id.clone(),
            initial_response_item.clone(),
            pending_input_tail,
            turn_input,
        )
        .await
        {
            Ok(turn_output) => {
                let mut items_to_record_in_conversation_history = Vec::<ResponseItem>::new();
                let mut responses = Vec::<ResponseInputItem>::new();
                for processed_response_item in turn_output {
                    let ProcessedResponseItem { item, response } = processed_response_item;
                    match (&item, &response) {
                        (ResponseItem::Message { role, .. }, None) if role == "assistant" => {
                            // If the model returned a message, we need to record it.
                            items_to_record_in_conversation_history.push(item.clone());
                            if is_review_mode
                                && let ResponseItem::Message { content, .. } = &item {
                                    for ci in content {
                                        if let ContentItem::OutputText { text } = ci {
                                            review_messages.push(text.clone());
                                        }
                                    }
                                }
                        }
                        (
                            ResponseItem::LocalShellCall { .. },
                            Some(ResponseInputItem::FunctionCallOutput { call_id, output }),
                        ) => {
                            items_to_record_in_conversation_history.push(item.clone());
                            items_to_record_in_conversation_history.push(
                                ResponseItem::FunctionCallOutput {
                                    call_id: call_id.clone(),
                                    output: output.clone(),
                                },
                            );
                        }
                        (
                            ResponseItem::FunctionCall { .. },
                            Some(ResponseInputItem::FunctionCallOutput { call_id, output }),
                        ) => {
                            debug!(
                                "Recording function call and output for call_id: {}",
                                call_id
                            );
                            items_to_record_in_conversation_history.push(item.clone());
                            items_to_record_in_conversation_history.push(
                                ResponseItem::FunctionCallOutput {
                                    call_id: call_id.clone(),
                                    output: output.clone(),
                                },
                            );
                        }
                        (
                            ResponseItem::CustomToolCall { .. },
                            Some(ResponseInputItem::CustomToolCallOutput { call_id, output }),
                        ) => {
                            items_to_record_in_conversation_history.push(item.clone());
                            items_to_record_in_conversation_history.push(
                                ResponseItem::CustomToolCallOutput {
                                    call_id: call_id.clone(),
                                    output: output.clone(),
                                },
                            );
                        }
                        (
                            ResponseItem::FunctionCall { .. },
                            Some(ResponseInputItem::McpToolCallOutput { call_id, result }),
                        ) => {
                            items_to_record_in_conversation_history.push(item.clone());
                            let output =
                                convert_call_tool_result_to_function_call_output_payload(result);
                            items_to_record_in_conversation_history.push(
                                ResponseItem::FunctionCallOutput {
                                    call_id: call_id.clone(),
                                    output,
                                },
                            );
                        }
                        (
                            ResponseItem::Reasoning {
                                id,
                                summary,
                                content,
                                encrypted_content,
                            },
                            None,
                        ) => {
                            items_to_record_in_conversation_history.push(ResponseItem::Reasoning {
                                id: id.clone(),
                                summary: summary.clone(),
                                content: content.clone(),
                                encrypted_content: encrypted_content.clone(),
                            });
                        }
                        _ => {
                            warn!("Unexpected response item: {item:?} with response: {response:?}");
                        }
                    };
                    if let Some(response) = response {
                        responses.push(response);
                    }
                }

                // Only attempt to take the lock if there is something to record.
                if !items_to_record_in_conversation_history.is_empty() {
                    if is_review_mode {
                        review_history.extend(items_to_record_in_conversation_history.clone());
                    } else {
                        // Record items in their original chronological order to maintain
                        // proper sequence of events. This ensures function calls and their
                        // outputs appear in the correct order in conversation history.
                        sess.record_conversation_items(&items_to_record_in_conversation_history)
                            .await;
                    }
                }

                // Check whether we should proactively compact before queuing follow-up work.
                // Upstream codex-rs compacts as soon as usage hits the configured threshold,
                // which keeps us from hitting hard context-window errors mid-session.
                let limit = turn_context
                    .client
                    .get_auto_compact_token_limit()
                    .unwrap_or(i64::MAX);
                let most_recent_usage_tokens: Option<i64> = {
                    let state = sess.state.lock().unwrap();
                    state.token_usage_info.as_ref().and_then(|info| {
                        info.last_token_usage.total_tokens.try_into().ok()
                    })
                };
                // auto_compact_token_limit is defined relative to a single turn's
                // token usage (input + output). Using the cumulative total caused
                // the limit check to stay tripped permanently once crossed, even
                // after compacting history, which spammed repeated /compact runs.
                let token_limit_reached = most_recent_usage_tokens
                    .is_some_and(|tokens| tokens >= limit);

                // If there are responses, add them to pending input for the next iteration
                if !responses.is_empty() {
                    if !is_review_mode {
                        for response in &responses {
                            sess.add_pending_input(response.clone());
                        }
                    }
                    // Reset the proactive compact guard for the next iteration since we're
                    // about to process new tool calls and may need to compact again
                    did_proactive_compact_this_iteration = false;
                }

                // As long as compaction works well in getting us way below the token limit,
                // we shouldn't worry about being in an infinite loop. However, guard against
                // repeated compaction attempts within a single iteration.
                if token_limit_reached && !did_proactive_compact_this_iteration && !is_review_mode {
                    let attempt_req = sess.current_request_ordinal();
                    let order = sess.next_background_order(&sub_id, attempt_req, None);
                    sess
                        .notify_background_event_with_order(
                            &sub_id,
                            order,
                            "Token limit reached; running /compact and continuing…".to_string(),
                        )
                        .await;

                    if responses.is_empty() {
                        did_proactive_compact_this_iteration = true;
                        // Choose between local and remote compact based on auth mode,
                        // matching upstream codex-rs behavior
                        if compact::should_use_remote_compact_task(&sess).await {
                            let _ = run_inline_remote_auto_compact_task(
                                Arc::clone(&sess),
                                Arc::clone(&turn_context),
                                Vec::new(),
                            )
                            .await;
                        } else {
                            let _ = compact::run_inline_auto_compact_task(
                                Arc::clone(&sess),
                                Arc::clone(&turn_context),
                            )
                            .await;
                        }

                        // Restart this loop with the newly compacted history so the
                        // next turn can see the trimmed conversation state.
                        continue;
                    }

                    if !auto_compact_pending {
                        auto_compact_pending = true;
                    }
                }

                if responses.is_empty() {
                    debug!("Turn completed");
                    last_task_message = get_last_assistant_message_from_turn(
                        &items_to_record_in_conversation_history,
                    );
                    if let Some(m) = last_task_message.as_ref() {
                        tracing::info!("core.turn completed: last_assistant_message.len={}", m.len());
                    }
                    sess.maybe_notify(UserNotification::AgentTurnComplete {
                        turn_id: sub_id.clone(),
                        input_messages: turn_input_messages,
                        last_assistant_message: last_task_message.clone(),
                    });
                    break;
                }
            }
            Err(e) => {
                info!("Turn error: {e:#}");
                let event = sess.make_event(
                    &sub_id,
                    EventMsg::Error(ErrorEvent { message: e.to_string() }),
                );
                sess.tx_event.send(event).await.ok();
                if is_review_mode && !review_exit_emitted {
                    exit_review_mode(sess.clone(), sub_id.clone(), None).await;
                    review_exit_emitted = true;
                }
                // let the user continue the conversation
                break;
            }
        }
    }
    if is_review_mode && !review_exit_emitted {
        let combined = if !review_messages.is_empty() {
            review_messages.join("\n\n")
        } else {
            last_task_message.clone().unwrap_or_default()
        };
        let output = if combined.trim().is_empty() {
            None
        } else {
            Some(parse_review_output_event(&combined))
        };
        exit_review_mode(sess.clone(), sub_id.clone(), output).await;
    }

    sess.remove_task(&sub_id);
    let event = sess.make_event(
        &sub_id,
        EventMsg::TaskComplete(TaskCompleteEvent {
            last_agent_message: last_task_message,
        }),
    );
    if let EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message: Some(m) }) = &event.msg {
        tracing::info!("core.emit TaskComplete last_agent_message.len={}", m.len());
    }
    sess.tx_event.send(event).await.ok();

    if let Some(compact_sub_id) = sess.dequeue_manual_compact() {
        let turn_context = sess.make_turn_context();
        let prompt_text = sess.compact_prompt_text();
        compact::spawn_compact_task(
            Arc::clone(&sess),
            turn_context,
            compact_sub_id,
            vec![InputItem::Text {
                text: prompt_text,
            }],
        );
        return;
    }

    if let Some(queued) = sess.pop_next_queued_user_input() {
        let sess_clone = Arc::clone(&sess);
        tokio::spawn(async move {
            sess_clone.cleanup_old_status_items().await;
            let turn_context = sess_clone.make_turn_context();
            let submission_id = queued.submission_id;
            let items = queued.core_items;
            let agent = AgentTask::spawn(Arc::clone(&sess_clone), turn_context, submission_id, items);
            sess_clone.set_task(agent);
        });
    }
}

fn strip_skill_contents(
    skills: &[crate::skills::model::SkillMetadata],
) -> Vec<crate::skills::model::SkillMetadata> {
    let mut out: Vec<crate::skills::model::SkillMetadata> = Vec::with_capacity(skills.len());
    for skill in skills {
        out.push(crate::skills::model::SkillMetadata {
            name: skill.name.clone(),
            description: skill.description.clone(),
            path: skill.path.clone(),
            scope: skill.scope,
            content: String::new(),
        });
    }
    out
}

async fn run_turn(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: String,
    initial_user_item: Option<ResponseItem>,
    pending_input_tail: Vec<ResponseItem>,
    mut input: Vec<ResponseItem>,
) -> CodexResult<Vec<ProcessedResponseItem>> {
    // Check if browser is enabled
    let browser_enabled = code_browser::global::get_browser_manager().await.is_some();

    let tc = &**turn_context;
    let agents_active = {
        let manager = AGENT_MANAGER.read().await;
        manager.has_active_agents()
    };

    let mut retries = 0;
    let mut rate_limit_switch_state = RateLimitSwitchState::default();
    let collaboration_mode_instructions =
        render_collaboration_mode_instructions(tc.collaboration_mode);
    // Ensure we only auto-compact once per turn to avoid loops
    let mut did_auto_compact = false;
    // Attempt input starts as the provided input, and may be augmented with
    // items from a previous dropped stream attempt so we don't lose progress.
    let _mcp_turn_allow_guard =
        super::mcp_access::McpTurnAllowGuard::new(Arc::clone(sess), sub_id.clone());
    super::mcp_access::preflight_turn_skill_input(
        sess,
        turn_context,
        sub_id.as_str(),
        initial_user_item.as_ref(),
        pending_input_tail.as_slice(),
        &mut input,
    )
    .await;

    let mut attempt_input: Vec<ResponseItem> = input.clone();
    loop {
        // Each loop iteration corresponds to a single provider HTTP request.
        // Increment the attempt ordinal first and capture its value so all
        // OrderMeta emitted during this attempt share the same `req`, even if
        // later attempts start before all events have been delivered.
        sess.begin_http_attempt();
        let attempt_req = sess.current_request_ordinal();
        // Build status items (screenshots, system status) fresh for each attempt
        let status_items = build_turn_status_items(sess).await;

        let mut prepend_developer_messages: Vec<String> = tc
            .demo_developer_message
            .clone()
            .into_iter()
            .collect();
        let trimmed_mode_instructions = collaboration_mode_instructions.trim();
        if !trimmed_mode_instructions.is_empty() {
            prepend_developer_messages.push(trimmed_mode_instructions.to_string());
        }
        if let Some(shell_style) = sess.user_shell.script_style() {
            prepend_developer_messages.push(shell_style.developer_instruction().to_string());
        }
        prepend_developer_messages.extend(
            sess
                .shell_style_profile_messages
                .iter()
                .filter_map(|message| {
                    let trimmed = message.trim();
                    (!trimmed.is_empty()).then(|| trimmed.to_string())
                }),
        );
        if should_inject_html_sanitizer_guardrails(&attempt_input) {
            prepend_developer_messages.push(HTML_SANITIZER_GUARDRAILS_MESSAGE.to_string());
        }

        let mut prompt = Prompt {
            input: attempt_input.clone(),
            store: !sess.disable_response_storage,
            user_instructions: tc.user_instructions.clone(),
            environment_context: Some(EnvironmentContext::new(
                Some(tc.cwd.clone()),
                Some(tc.approval_policy),
                Some(tc.sandbox_policy.clone()),
                Some(sess.user_shell.clone()),
            )),
            tools: Vec::new(),
            status_items, // Include status items with this request
            base_instructions_override: tc.base_instructions.clone(),
            include_additional_instructions: true,
            prepend_developer_messages,
            text_format: tc.text_format_override.clone(),
            model_override: None,
            model_family_override: None,
            output_schema: tc.final_output_json_schema.clone(),
            log_tag: Some("codex/turn".to_string()),
            session_id_override: None,
            model_descriptions: sess.model_descriptions.clone(),
        };

        sess.apply_remote_model_overrides(&mut prompt).await;

        let effective_family = prompt
            .model_family_override
            .as_ref()
            .unwrap_or_else(|| tc.client.default_model_family());
        let tools_config = tc.client.build_tools_config_with_sandbox_for_family(
            tc.sandbox_policy.clone(),
            effective_family,
        );
        let mcp_access = sess.mcp_access_snapshot();
        let mcp_tools = if tools_config.search_tool {
            let selection = sess.mcp_tool_selection_snapshot().unwrap_or_default();
            if selection.is_empty() {
                None
            } else {
                let selection_lower: std::collections::HashSet<String> = selection
                    .iter()
                    .map(|tool| tool.to_ascii_lowercase())
                    .collect();
                let session_deny: std::collections::HashSet<String> = mcp_access
                    .session_deny_servers
                    .iter()
                    .map(|name| name.to_ascii_lowercase())
                    .collect();
                let mut selected = std::collections::HashMap::new();
                for (qualified_name, server_name, tool) in sess
                    .mcp_connection_manager
                    .list_all_tools_with_server_names()
                {
                    if session_deny.contains(&server_name.to_ascii_lowercase()) {
                        continue;
                    }
                    if !selection_lower.contains(&qualified_name.to_ascii_lowercase()) {
                        continue;
                    }
                    selected.insert(qualified_name, tool);
                }
                (!selected.is_empty()).then_some(selected)
            }
        } else {
            Some(crate::mcp::policy::filter_tools_for_turn(
                &sess.mcp_connection_manager,
                &mcp_access,
                sub_id.as_str(),
            ))
        };
        prompt.tools = get_openai_tools(
            &tools_config,
            mcp_tools,
            browser_enabled,
            agents_active,
            sess.dynamic_tools.as_slice(),
        );
        if should_inject_search_tool_developer_instructions(&prompt.tools) {
            let search_tool_instructions = SEARCH_TOOL_DEVELOPER_INSTRUCTIONS.trim();
            if !search_tool_instructions.is_empty()
                && !prompt
                    .prepend_developer_messages
                    .iter()
                    .any(|message| message.trim() == search_tool_instructions)
            {
                prompt
                    .prepend_developer_messages
                    .push(search_tool_instructions.to_string());
            }
        }

        // Start a new scratchpad for this HTTP attempt
        sess.begin_attempt_scratchpad();

        match try_run_turn(sess, turn_diff_tracker, &sub_id, &prompt, attempt_req).await {
            Ok(output) => {
                // Record status items to conversation history after successful turn
                // This ensures they persist for future requests in the right chronological order
                if !prompt.status_items.is_empty() {
                    sess.record_conversation_items(&prompt.status_items).await;
                }
                // Commit successful attempt – scratchpad is no longer needed.
                sess.clear_scratchpad();
                return Ok(output);
            }
            Err(CodexErr::Interrupted) => return Err(CodexErr::Interrupted),
            Err(CodexErr::EnvVar(var)) => return Err(CodexErr::EnvVar(var)),
            Err(CodexErr::UsageLimitReached(limit_err)) => {
                if let Some(ctx) = account_usage_context(sess) {
                    let usage_home = ctx.code_home.clone();
                    let usage_account = ctx.account_id.clone();
                    let usage_plan = ctx.plan.clone();
                    let resets = limit_err.resets_in_seconds;
                    spawn_usage_task(move || {
                        if let Err(err) = account_usage::record_usage_limit_hint(
                            &usage_home,
                            &usage_account,
                            usage_plan.as_deref(),
                            resets,
                            Utc::now(),
                        ) {
                            warn!("Failed to persist usage limit hint: {err}");
                        }
                    });
                }

                let mut switched = false;
                if sess.client.auto_switch_accounts_on_rate_limit()
                    && auth::read_code_api_key_from_env().is_none()
                    && let Some(auth_manager) = sess.client.get_auth_manager() {
                        let auth = auth_manager.auth();
                        let current_account_id = auth
                            .as_ref()
                            .and_then(super::super::auth::CodexAuth::get_account_id)
                            .or_else(|| {
                                auth_accounts::get_active_account_id(sess.client.code_home())
                                    .ok()
                                    .flatten()
                            });
                        if let Some(current_account_id) = current_account_id {
                            let now = Utc::now();
                            let blocked_until = limit_err.resets_in_seconds.map(|seconds| {
                                now + chrono::Duration::seconds(seconds as i64)
                            });
                            let current_auth_mode = auth
                                .as_ref()
                                .map(|current| current.mode)
                                .unwrap_or(AppAuthMode::ApiKey);
                            match crate::account_switching::switch_active_account_on_rate_limit(
                                crate::account_switching::SwitchActiveAccountOnRateLimitParams {
                                    code_home: sess.client.code_home(),
                                    auth_credentials_store_mode: sess
                                        .client
                                        .auth_credentials_store_mode(),
                                    state: &mut rate_limit_switch_state,
                                    allow_api_key_fallback: sess
                                        .client
                                        .api_key_fallback_on_all_accounts_limited(),
                                    now,
                                    current_account_id: current_account_id.as_str(),
                                    current_mode: current_auth_mode,
                                    blocked_until,
                                },
                            ) {
                                Ok(Some(next_account_id)) => {
                                    let next_label = auth_accounts::find_account(
                                        sess.client.code_home(),
                                        &next_account_id,
                                    )
                                    .ok()
                                    .flatten()
                                    .and_then(|account| account.label)
                                    .unwrap_or_else(|| next_account_id.clone());
                                    tracing::info!(
                                        from_account_id = %current_account_id,
                                        to_account_id = %next_account_id,
                                        reason = "usage_limit_reached",
                                        "rate limit hit; auto-switching active account"
                                    );
                                    auth_manager.reload();
                                    let order = sess.next_background_order(&sub_id, attempt_req, None);
                                    let notice = format!(
                                        "Auto-switch: now using {next_label} due to usage limit."
                                    );
                                    sess
                                        .notify_background_event_with_order(
                                            &sub_id,
                                            order,
                                            notice,
                                        )
                                        .await;
                                    switched = true;
                                }
                                Ok(None) => {}
                                Err(err) => {
                                    tracing::warn!(
                                        from_account_id = %current_account_id,
                                        error = %err,
                                        "failed to activate account after usage limit"
                                    );
                                }
                            }
                        }
                    }

                if switched {
                    retries = 0;
                    continue;
                }

                let now = Utc::now();
                let retry_after = limit_err
                    .retry_after(now)
                    .unwrap_or_else(|| RetryAfter::from_duration(std::time::Duration::from_secs(5 * 60), now));
                let eta = format_retry_eta(&retry_after);
                let mut retry_message = format!("{limit_err} Auto-retrying");
                if let Some(eta) = eta {
                    retry_message.push_str(&format!(" at {eta}"));
                }
                retry_message.push('…');
                sess.notify_stream_error(&sub_id, retry_message).await;
                tokio::time::sleep(retry_after.delay).await;
                retries = 0;
                continue;
            }
            Err(CodexErr::UsageNotIncluded) => return Err(CodexErr::UsageNotIncluded),
            Err(CodexErr::QuotaExceeded) => return Err(CodexErr::QuotaExceeded),
            Err(e) => {
                // Detect context-window overflow and auto-run a compact summarization once
                if !did_auto_compact
                    && let CodexErr::Stream(msg, _maybe_delay, _req_id) = &e {
                        let lower = msg.to_ascii_lowercase();
                        let looks_like_context_overflow =
                            lower.contains("exceeds the context window")
                                || lower.contains("exceed the context window")
                                || lower.contains("context length exceeded")
                                || lower.contains("maximum context length")
                                || (lower.contains("context window")
                                    && (lower.contains("exceed")
                                        || lower.contains("exceeded")
                                        || lower.contains("full")
                                        || lower.contains("too long")));

                        if looks_like_context_overflow {
                            did_auto_compact = true;
                            sess
                                .notify_stream_error(
                                    &sub_id,
                                    "Model hit context-window limit; running /compact and retrying…"
                                        .to_string(),
                                )
                                .await;

                            let previous_input_snapshot = input.clone();
                            let compacted_history = if compact::should_use_remote_compact_task(sess).await {
                                run_inline_remote_auto_compact_task(
                                    Arc::clone(sess),
                                    Arc::clone(turn_context),
                                    Vec::new(),
                                )
                                .await
                            } else {
                                compact::run_inline_auto_compact_task(
                                    Arc::clone(sess),
                                    Arc::clone(turn_context),
                                )
                                .await
                            };

                            // Reset any partial attempt state and rebuild the request payload using the
                            // newly compacted history plus the current user turn items.
                            sess.clear_scratchpad();

                            if compacted_history.is_empty() {
                                attempt_input = input.clone();
                            } else {
                                let mut rebuilt = compacted_history;
                                if let Some(initial_item) = initial_user_item.clone() {
                                    rebuilt.push(initial_item);
                                }
                                if !pending_input_tail.is_empty() {
                                    let (missing_calls, filtered_outputs) =
                                        reconcile_pending_tool_outputs(&pending_input_tail, &rebuilt, &previous_input_snapshot);
                                    if !missing_calls.is_empty() {
                                        rebuilt.extend(missing_calls);
                                    }
                                    if !filtered_outputs.is_empty() {
                                        rebuilt.extend(filtered_outputs);
                                    }
                                }
                                input = rebuilt.clone();
                                attempt_input = rebuilt;
                            }
                            continue;
                        }
                    }

                // Use the configured provider-specific stream retry budget.
                let max_retries = tc.client.get_provider().stream_max_retries();
                let req_id = match &e {
                    CodexErr::Stream(_, _, req) => req.clone(),
                    _ => None,
                };
                let is_connectivity = is_connectivity_error(&e);
                let drain_scratchpad_into_attempt = |attempt_input: &mut Vec<ResponseItem>| {
                    if let Some(sp) = sess.take_scratchpad() {
                        inject_scratchpad_into_attempt_input(attempt_input, sp);
                    }
                };

                if is_connectivity && retries >= max_retries {
                    let probe = tc.client.get_provider().base_url_for_probe();
                    let wait_message = format!(
                        "Network unavailable; waiting to reconnect to {probe} ({e})"
                    );
                    sess.notify_stream_error(&sub_id, wait_message).await;
                    drain_scratchpad_into_attempt(&mut attempt_input);
                    wait_for_connectivity(&probe).await;
                    retries = 0;
                    continue;
                }

                if retries < max_retries {
                    retries += 1;
                    let (delay, retry_eta) = match e {
                        CodexErr::Stream(_, Some(ref retry_after), _) => {
                            let eta = format_retry_eta(retry_after);
                            (retry_after.delay, eta)
                        }
                        _ => (backoff(retries), None),
                    };
                    warn!(
                        error = %e,
                        request_id = req_id.as_deref(),
                        "stream disconnected - retrying turn in {delay:?} (attempt {retries}/{max_retries})",
                    );

                    // Surface retry information to any UI/front‑end so the
                    // user understands what is happening instead of staring
                    // at a seemingly frozen screen.
                    let mut retry_message =
                        format!("stream error: {e}; retrying in {delay:?}");
                    if let Some(eta) = retry_eta {
                        retry_message.push_str(&format!(" (next attempt at {eta})"));
                    }
                    retry_message.push('…');
                    sess.notify_stream_error(&sub_id, retry_message.clone()).await;
                    // Pull any partial progress from this attempt and append to
                    // the next request's input so we do not lose tool progress
                    // or already-finalized items.
                    drain_scratchpad_into_attempt(&mut attempt_input);

                    tokio::time::sleep(delay).await;
                } else {
                    error!(
                        retries,
                        max_retries,
                        auto_compact_attempted = did_auto_compact,
                        request_id = req_id.as_deref(),
                        error = %e,
                        "stream disconnected - retries exhausted"
                    );
                    return Err(e);
                }
            }
        }
    }
}

const HTML_SANITIZER_GUARDRAILS_MESSAGE: &str =
    "TB2 HTML/XSS guardrails:\n- Do NOT use DOTALL/full-document regex (e.g. `<script.*?>.*?</script>`); catastrophic backtracking risk.\n- Prefer linear-time scanning with quote/state tracking; if using regex, only on bounded substrings (single tags).\n- Perf smoke test: write malformed `/tmp/stress.html` and run `timeout 5s python3 /app/filter.py /tmp/stress.html` (or equivalent). If it times out, rewrite for linear-time behavior.";
const SEARCH_TOOL_DEVELOPER_INSTRUCTIONS: &str =
    include_str!("../../templates/search_tool/developer_instructions.md");

fn should_inject_html_sanitizer_guardrails(input: &[ResponseItem]) -> bool {
    let mut user_messages_seen = 0u32;
    let mut text = String::new();
    for item in input.iter().rev() {
        if user_messages_seen >= 6 || text.len() >= 1_200 {
            break;
        }
        let ResponseItem::Message { role, content, .. } = item else {
            continue;
        };
        if role != "user" {
            continue;
        }
        user_messages_seen = user_messages_seen.saturating_add(1);
        for entry in content {
            let ContentItem::InputText { text: piece } = entry else {
                continue;
            };
            if piece.trim().is_empty() {
                continue;
            }
            text.push_str(piece);
            text.push('\n');
            if text.len() >= 1_200 {
                break;
            }
        }
    }

    if text.is_empty() {
        return false;
    }

    let lower = text.to_ascii_lowercase();
    let has_xss = lower.contains("xss");
    let has_sanitize = lower.contains("sanitize") || lower.contains("sanitiz");
    let has_filter_js_from_html =
        lower.contains("filter-js-from-html") || lower.contains("break-filter-js-from-html");
    let has_html = lower.contains("html");
    let has_script_tag =
        lower.contains("<script") || lower.contains("script tag") || lower.contains("script-tag");
    let has_filtering =
        lower.contains("filter") || lower.contains("strip") || lower.contains("remove");

    has_xss || has_sanitize || has_filter_js_from_html || (has_html && has_script_tag && has_filtering)
}

fn should_inject_search_tool_developer_instructions(tools: &[OpenAiTool]) -> bool {
    tools.iter().any(|tool| {
        matches!(tool, OpenAiTool::Function(ResponsesApiTool { name, .. }) if name == SEARCH_TOOL_BM25_TOOL_NAME)
    })
}

fn inject_scratchpad_into_attempt_input(
    attempt_input: &mut Vec<ResponseItem>,
    sp: TurnScratchpad,
) {
    // Build a set of call ids we have already included to avoid duplicate call items.
    let mut seen_calls: std::collections::HashSet<String> = attempt_input
        .iter()
        .filter_map(|ri| match ri {
            ResponseItem::FunctionCall { call_id, .. } => Some(call_id.clone()),
            ResponseItem::CustomToolCall { call_id, .. } => Some(call_id.clone()),
            ResponseItem::LocalShellCall { call_id, id, .. } => {
                call_id.clone().or_else(|| id.clone())
            }
            _ => None,
        })
        .collect();

    // Append finalized tool calls from the dropped attempt so retry payloads include
    // the same call ids as their tool outputs.
    for item in sp.items {
        let call_id = match &item {
            ResponseItem::FunctionCall { call_id, .. } => Some(call_id.as_str()),
            ResponseItem::CustomToolCall { call_id, .. } => Some(call_id.as_str()),
            ResponseItem::LocalShellCall { call_id, id, .. } => {
                call_id.as_deref().or(id.as_deref())
            }
            _ => None,
        };

        let Some(call_id) = call_id else {
            continue;
        };

        if seen_calls.insert(call_id.to_string()) {
            attempt_input.push(item);
        }
    }

    // Append tool outputs produced during the dropped attempt.
    for resp in sp.responses {
        attempt_input.push(ResponseItem::from(resp));
    }

    // If we have partial deltas, include a short ephemeral hint so the model can resume.
    if !sp.partial_assistant_text.is_empty() || !sp.partial_reasoning_summary.is_empty() {
        use code_protocol::models::ContentItem;
        let mut hint = String::from(
            "[EPHEMERAL:RETRY_HINT]\nPrevious attempt aborted mid-stream. Continue without repeating.\n",
        );
        if !sp.partial_reasoning_summary.is_empty() {
            let s = &sp.partial_reasoning_summary;
            // Take the last 800 characters, respecting UTF-8 boundaries
            let start_idx = if s.chars().count() > 800 {
                s.char_indices()
                    .rev()
                    .nth(800 - 1)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            } else {
                0
            };
            let tail = &s[start_idx..];
            hint.push_str(&format!("Last reasoning summary fragment:\n{tail}\n\n"));
        }
        if !sp.partial_assistant_text.is_empty() {
            let s = &sp.partial_assistant_text;
            // Take the last 800 characters, respecting UTF-8 boundaries
            let start_idx = if s.chars().count() > 800 {
                s.char_indices()
                    .rev()
                    .nth(800 - 1)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            } else {
                0
            };
            let tail = &s[start_idx..];
            hint.push_str(&format!("Last assistant text fragment:\n{tail}\n"));
        }
        attempt_input.push(ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text: hint }],
            end_turn: None,
            phase: None,
        });
    }
}

#[cfg(test)]
mod search_tool_instructions_tests {
    use super::*;

    #[test]
    fn detects_search_tool_presence() {
        let tools = vec![OpenAiTool::Function(ResponsesApiTool {
            name: SEARCH_TOOL_BM25_TOOL_NAME.to_string(),
            description: "search".to_string(),
            strict: false,
            parameters: crate::openai_tools::JsonSchema::Object {
                properties: Default::default(),
                required: None,
                additional_properties: None,
            },
        })];
        assert!(should_inject_search_tool_developer_instructions(&tools));
    }

    #[test]
    fn ignores_non_search_tools() {
        let tools = vec![OpenAiTool::Function(ResponsesApiTool {
            name: "not_search_tool".to_string(),
            description: "other".to_string(),
            strict: false,
            parameters: crate::openai_tools::JsonSchema::Object {
                properties: Default::default(),
                required: None,
                additional_properties: None,
            },
        })];
        assert!(!should_inject_search_tool_developer_instructions(&tools));
    }
}

fn reconcile_pending_tool_outputs(
    pending_outputs: &[ResponseItem],
    rebuilt_history: &[ResponseItem],
    previous_input_snapshot: &[ResponseItem],
) -> (Vec<ResponseItem>, Vec<ResponseItem>) {
    let mut call_ids = collect_tool_call_ids(rebuilt_history);
    let mut missing_calls = Vec::new();
    let mut filtered_outputs = Vec::new();

    for item in pending_outputs {
        match item {
            ResponseItem::FunctionCallOutput { call_id, .. }
            | ResponseItem::CustomToolCallOutput { call_id, .. } => {
                if call_ids.contains(call_id) {
                    filtered_outputs.push(item.clone());
                    continue;
                }

                if let Some(call_item) = find_call_item_by_id(previous_input_snapshot, call_id) {
                    call_ids.insert(call_id.clone());
                    missing_calls.push(call_item);
                    filtered_outputs.push(item.clone());
                } else {
                    warn!("Skipping tool output for missing call_id={call_id} after auto-compact");
                }
            }
            _ => {
                filtered_outputs.push(item.clone());
            }
        }
    }

    (missing_calls, filtered_outputs)
}

fn collect_tool_call_ids(items: &[ResponseItem]) -> HashSet<String> {
    let mut ids = HashSet::new();
    for item in items {
        match item {
            ResponseItem::FunctionCall { call_id, .. } => {
                ids.insert(call_id.clone());
            }
            ResponseItem::LocalShellCall { call_id, id, .. } => {
                if let Some(call_id) = call_id.as_ref().or(id.as_ref()) {
                    ids.insert(call_id.clone());
                }
            }
            ResponseItem::CustomToolCall { call_id, .. } => {
                ids.insert(call_id.clone());
            }
            _ => {}
        }
    }
    ids
}

fn find_call_item_by_id(items: &[ResponseItem], call_id: &str) -> Option<ResponseItem> {
    items.iter().rev().find_map(|item| match item {
        ResponseItem::FunctionCall { call_id: existing, .. } if existing == call_id => Some(item.clone()),
        ResponseItem::LocalShellCall { call_id: call_id_field, id, .. } => {
            let effective = call_id_field.as_deref().or(id.as_deref());
            if effective == Some(call_id) {
                Some(item.clone())
            } else {
                None
            }
        }
        ResponseItem::CustomToolCall { call_id: existing, .. } if existing == call_id => Some(item.clone()),
        _ => None,
    })
}

#[cfg(test)]
mod tool_call_id_tests {
    use super::*;

    fn legacy_local_shell_call(id: &str) -> ResponseItem {
        ResponseItem::LocalShellCall {
            id: Some(id.to_string()),
            call_id: None,
            status: code_protocol::models::LocalShellStatus::Completed,
            action: code_protocol::models::LocalShellAction::Exec(
                code_protocol::models::LocalShellExecAction {
                    command: vec!["echo".to_string(), "hi".to_string()],
                    timeout_ms: None,
                    working_directory: None,
                    env: None,
                    user: None,
                },
            ),
        }
    }

    #[test]
    fn collect_tool_call_ids_includes_local_shell_id() {
        let items = vec![legacy_local_shell_call("sh_1")];
        let ids = collect_tool_call_ids(&items);
        assert!(ids.contains("sh_1"));
    }

    #[test]
    fn find_call_item_by_id_matches_local_shell_id() {
        let items = vec![legacy_local_shell_call("sh_1")];
        let found = find_call_item_by_id(&items, "sh_1");
        assert!(matches!(
            found,
            Some(ResponseItem::LocalShellCall { id: Some(id), .. }) if id == "sh_1"
        ));
    }

    #[test]
    fn retry_scratchpad_injects_custom_tool_call_before_output() {
        let sp = TurnScratchpad {
            items: vec![ResponseItem::CustomToolCall {
                id: None,
                status: None,
                call_id: "c1".to_string(),
                name: "apply_patch".to_string(),
                input: "*** Begin Patch\n*** End Patch".to_string(),
            }],
            responses: vec![ResponseInputItem::CustomToolCallOutput {
                call_id: "c1".to_string(),
                output: "ok".to_string(),
            }],
            partial_assistant_text: String::new(),
            partial_reasoning_summary: String::new(),
        };

        let mut attempt_input: Vec<ResponseItem> = Vec::new();
        inject_scratchpad_into_attempt_input(&mut attempt_input, sp);

        let call_pos = attempt_input
            .iter()
            .position(|item| matches!(item, ResponseItem::CustomToolCall { call_id, .. } if call_id == "c1"))
            .expect("expected CustomToolCall to be injected");
        let output_pos = attempt_input
            .iter()
            .position(|item| matches!(item, ResponseItem::CustomToolCallOutput { call_id, .. } if call_id == "c1"))
            .expect("expected CustomToolCallOutput to be injected");
        assert!(call_pos < output_pos, "tool call should precede output");
    }

    #[test]
    fn missing_tool_outputs_inserts_function_call_output_for_function_call() {
        let items = vec![ResponseItem::FunctionCall {
            id: None,
            name: "shell".to_string(),
            arguments: "{}".to_string(),
            call_id: "f1".to_string(),
        }];

        let missing = missing_tool_outputs_to_insert(&items);
        assert_eq!(missing.len(), 1);

        let mut input = items;
        for (idx, output_item) in missing.into_iter().rev() {
            input.insert(idx + 1, output_item);
        }

        assert!(matches!(
            input.get(1),
            Some(ResponseItem::FunctionCallOutput { call_id, output })
                if call_id == "f1" && matches!(&output.body, code_protocol::models::FunctionCallOutputBody::Text(text) if text == "aborted")
        ));
    }

    #[test]
    fn missing_tool_outputs_inserts_function_call_output_for_local_shell_legacy_id() {
        let items = vec![legacy_local_shell_call("sh_1")];
        let missing = missing_tool_outputs_to_insert(&items);
        assert_eq!(missing.len(), 1);

        let mut input = items;
        for (idx, output_item) in missing.into_iter().rev() {
            input.insert(idx + 1, output_item);
        }

        assert!(matches!(
            input.get(1),
            Some(ResponseItem::FunctionCallOutput { call_id, output })
                if call_id == "sh_1" && matches!(&output.body, code_protocol::models::FunctionCallOutputBody::Text(text) if text == "aborted")
        ));
    }

    #[test]
    fn missing_tool_outputs_inserts_custom_tool_call_output_for_custom_tool_call() {
        let items = vec![ResponseItem::CustomToolCall {
            id: None,
            status: None,
            call_id: "c1".to_string(),
            name: "apply_patch".to_string(),
            input: "noop".to_string(),
        }];

        let missing = missing_tool_outputs_to_insert(&items);
        assert_eq!(missing.len(), 1);

        let mut input = items;
        for (idx, output_item) in missing.into_iter().rev() {
            input.insert(idx + 1, output_item);
        }

        assert!(matches!(
            input.get(1),
            Some(ResponseItem::CustomToolCallOutput { call_id, output })
                if call_id == "c1" && output == "aborted"
        ));
    }

    #[test]
    fn missing_tool_outputs_noops_when_outputs_exist() {
        let items = vec![
            ResponseItem::FunctionCall {
                id: None,
                name: "shell".to_string(),
                arguments: "{}".to_string(),
                call_id: "f1".to_string(),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "f1".to_string(),
                output: FunctionCallOutputPayload::from_text("ok".to_string()),
            },
        ];

        let missing = missing_tool_outputs_to_insert(&items);
        assert!(missing.is_empty());
    }
}

/// When the model is prompted, it returns a stream of events. Some of these
/// events map to a `ResponseItem`. A `ResponseItem` may need to be
/// "handled" such that it produces a `ResponseInputItem` that needs to be
/// sent back to the model on the next turn.
#[derive(Debug)]
struct ProcessedResponseItem {
    item: ResponseItem,
    response: Option<ResponseInputItem>,
}

struct TurnLatencyGuard<'a> {
    sess: &'a Session,
    attempt_req: u64,
    active: bool,
}

impl<'a> TurnLatencyGuard<'a> {
    fn new(sess: &'a Session, attempt_req: u64, prompt: &Prompt) -> Self {
        sess.turn_latency_request_scheduled(attempt_req, prompt);
        Self {
            sess,
            attempt_req,
            active: true,
        }
    }

    fn mark_completed(&mut self, output_item_count: usize, token_usage: Option<&TokenUsage>) {
        if !self.active {
            return;
        }
        self
            .sess
            .turn_latency_request_completed(self.attempt_req, output_item_count, token_usage);
        self.active = false;
    }

    fn mark_failed(&mut self, note: Option<String>) {
        if !self.active {
            return;
        }
        self.sess.turn_latency_request_failed(self.attempt_req, note);
        self.active = false;
    }
}

impl Drop for TurnLatencyGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            self
                .sess
                .turn_latency_request_failed(self.attempt_req, Some("dropped_without_outcome".to_string()));
        }
    }
}

fn missing_tool_outputs_to_insert(items: &[ResponseItem]) -> Vec<(usize, ResponseItem)> {
    let mut function_outputs: HashSet<String> = HashSet::new();
    let mut custom_outputs: HashSet<String> = HashSet::new();

    for item in items {
        match item {
            ResponseItem::FunctionCallOutput { call_id, .. } => {
                function_outputs.insert(call_id.clone());
            }
            ResponseItem::CustomToolCallOutput { call_id, .. } => {
                custom_outputs.insert(call_id.clone());
            }
            _ => {}
        }
    }

    let mut missing_outputs_to_insert: Vec<(usize, ResponseItem)> = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        match item {
            ResponseItem::FunctionCall { call_id, .. } => {
                if function_outputs.insert(call_id.clone()) {
                    missing_outputs_to_insert.push((
                        idx,
                        ResponseItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload::from_text("aborted".to_string()),
                        },
                    ));
                }
            }
            ResponseItem::CustomToolCall { call_id, .. } => {
                if custom_outputs.insert(call_id.clone()) {
                    missing_outputs_to_insert.push((
                        idx,
                        ResponseItem::CustomToolCallOutput {
                            call_id: call_id.clone(),
                            output: "aborted".to_string(),
                        },
                    ));
                }
            }
            ResponseItem::LocalShellCall { call_id, id, .. } => {
                let Some(effective_call_id) = call_id.as_ref().or(id.as_ref()) else {
                    continue;
                };

                if function_outputs.insert(effective_call_id.clone()) {
                    missing_outputs_to_insert.push((
                        idx,
                        ResponseItem::FunctionCallOutput {
                            call_id: effective_call_id.clone(),
                            output: FunctionCallOutputPayload::from_text("aborted".to_string()),
                        },
                    ));
                }
            }
            _ => {}
        }
    }

    missing_outputs_to_insert
}

async fn try_run_turn(
    sess: &Session,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: &str,
    prompt: &Prompt,
    attempt_req: u64,
) -> CodexResult<Vec<ProcessedResponseItem>> {
    // Ensure any pending tool calls from a previous interrupted attempt are paired with
    // an "aborted" output before we send a new request to the model.
    let missing_outputs = missing_tool_outputs_to_insert(&prompt.input);
    let prompt: Cow<Prompt> = if missing_outputs.is_empty() {
        Cow::Borrowed(prompt)
    } else {
        let mut input = prompt.input.clone();
        for (idx, output_item) in missing_outputs.into_iter().rev() {
            input.insert(idx + 1, output_item);
        }

        Cow::Owned(Prompt { input, ..prompt.clone() })
    };

    let enable_parallel_tool_calls = prompt
        .as_ref()
        .model_family_override
        .as_ref()
        .unwrap_or_else(|| sess.client.default_model_family())
        .supports_parallel_tool_calls;

    let mut turn_latency_guard = TurnLatencyGuard::new(sess, attempt_req, prompt.as_ref());
    let mut stream = match sess.client.clone().stream(&prompt).await {
        Ok(stream) => stream,
        Err(e) => {
            turn_latency_guard.mark_failed(Some(format!("stream_init_failed: {e}")));
            sess
                .notify_stream_error(
                    sub_id,
                    format!("[transport] failed to start stream: {e}"),
                )
                .await;
            return Err(e);
        }
    };

    let mut output = Vec::new();
    let mut pending_tool_calls: Vec<PendingToolCall> = Vec::new();
    loop {
        // Poll the next item from the model stream. We must inspect *both* Ok and Err
        // cases so that transient stream failures (e.g., dropped SSE connection before
        // `response.completed`) bubble up and trigger the caller's retry logic.
        let event = stream.next().await;
        let Some(event) = event else {
            // Channel closed without yielding a final Completed event or explicit error.
            // Treat as a disconnected stream so the caller can retry.
            turn_latency_guard
                .mark_failed(Some("stream_closed_before_completed".to_string()));
            return Err(CodexErr::Stream(
                "stream closed before response.completed".into(),
                None,
                None,
            ));
        };

        let event = match event {
            Ok(ev) => ev,
            Err(e) => {
                // Propagate the underlying stream error to the caller (run_turn), which
                // will apply the configured `stream_max_retries` policy.
                turn_latency_guard.mark_failed(Some(format!("stream_event_error: {e}")));
                return Err(e);
            }
        };

        match event {
            ResponseEvent::Created { .. } => {}
            ResponseEvent::ServerReasoningIncluded(_included) => {}
            ResponseEvent::OutputItemDone { item, sequence_number, output_index } => {
                let is_tool_call = matches!(
                    item,
                    ResponseItem::FunctionCall { .. }
                        | ResponseItem::LocalShellCall { .. }
                        | ResponseItem::CustomToolCall { .. }
                );

                if enable_parallel_tool_calls && is_tool_call {
                    let output_pos = output.len();
                    // Persist finalized tool call items so retries can re-seed them if the
                    // stream disconnects before `response.completed`.
                    sess.scratchpad_push(&item, &None, sub_id);
                    output.push(ProcessedResponseItem {
                        item,
                        response: None,
                    });
                    pending_tool_calls.push(PendingToolCall {
                        output_pos,
                        seq_hint: sequence_number,
                        output_index,
                    });
                } else {
                    let response = handle_response_item(
                        sess,
                        turn_diff_tracker,
                        sub_id,
                        item.clone(),
                        sequence_number,
                        output_index,
                        attempt_req,
                    )
                    .await?;

                    // Save into scratchpad so we can seed a retry if the stream drops later.
                    sess.scratchpad_push(&item, &response, sub_id);

                    // If this was a finalized assistant message, clear partial text buffer
                    if let ResponseItem::Message { .. } = &item {
                        sess.scratchpad_clear_partial_message();
                    }

                    output.push(ProcessedResponseItem { item, response });
                }
            }
            ResponseEvent::WebSearchCallBegin { call_id } => {
                // Stamp OrderMeta so the TUI can place the search block within
                // the correct request window instead of using an internal epilogue.
                let ctx = ToolCallCtx::new(sub_id.to_string(), call_id.clone(), None, None);
                let order = ctx.order_meta(attempt_req);
                let ev = sess.make_event_with_order(
                    sub_id,
                    EventMsg::WebSearchBegin(WebSearchBeginEvent { call_id, query: None }),
                    order,
                    None,
                );
                sess.send_event(ev).await;
            }
            ResponseEvent::WebSearchCallCompleted { call_id, query } => {
                let ctx = ToolCallCtx::new(sub_id.to_string(), call_id.clone(), None, None);
                let order = ctx.order_meta(attempt_req);
                let ev = sess.make_event_with_order(
                    sub_id,
                    EventMsg::WebSearchComplete(WebSearchCompleteEvent { call_id, query }),
                    order,
                    None,
                );
                sess.send_event(ev).await;
            }
            ResponseEvent::Completed {
                response_id: _,
                token_usage,
            } => {
                let (new_info, rate_limits, should_emit);
                {
                    let mut state = sess.state.lock().unwrap();
                    let info = TokenUsageInfo::new_or_append(
                        &state.token_usage_info,
                        &token_usage,
                        sess.client.get_model_context_window(),
                    );
                    let limits = state.latest_rate_limits.clone();
                    let emit = info.is_some() || limits.is_some();
                    state.token_usage_info = info.clone();
                    new_info = info;
                    rate_limits = limits;
                    should_emit = emit;
                }

                if should_emit {
                    let payload = TokenCountEvent {
                        info: new_info,
                        rate_limits,
                    };
                    sess.tx_event
                        .send(sess.make_event(sub_id, EventMsg::TokenCount(payload)))
                        .await
                        .ok();
                }

                if let Some(usage) = token_usage.as_ref()
                    && let Some(ctx) = account_usage_context(sess) {
                        let usage_home = ctx.code_home.clone();
                        let usage_account = ctx.account_id.clone();
                        let usage_plan = ctx.plan;
                        let usage_clone = usage.clone();
                        spawn_usage_task(move || {
                            if let Err(err) = account_usage::record_token_usage(
                                &usage_home,
                                &usage_account,
                                usage_plan.as_deref(),
                                &usage_clone,
                                Utc::now(),
                            ) {
                                warn!("Failed to persist token usage: {err}");
                            }
                        });
                    }

                if enable_parallel_tool_calls && !pending_tool_calls.is_empty() {
                    let results = crate::tools::scheduler::dispatch_pending_tool_calls(
                        sess,
                        turn_diff_tracker,
                        sub_id,
                        attempt_req,
                        &pending_tool_calls,
                        |pos| output.get(pos).map(|cell| &cell.item),
                    )
                    .await;

                    for (pos, resp) in results {
                        if let Some(cell) = output.get_mut(pos) {
                            cell.response = resp;
                            sess.scratchpad_push(&cell.item, &cell.response, sub_id);
                        }
                    }
                }

                let unified_diff = turn_diff_tracker.get_unified_diff();
                if let Ok(Some(unified_diff)) = unified_diff {
                    let msg = EventMsg::TurnDiff(TurnDiffEvent { unified_diff });
                    let _ = sess.tx_event.send(sess.make_event(sub_id, msg)).await;
                }

                turn_latency_guard.mark_completed(output.len(), token_usage.as_ref());
                return Ok(output);
            }
            ResponseEvent::OutputTextDelta { delta, item_id, sequence_number, output_index } => {
                // Don't append to history during streaming - only send UI events.
                // The complete message will be added to history when OutputItemDone arrives.
                // This ensures items are recorded in the correct chronological order.

                // Use the item_id if present and non-empty, otherwise fall back to sub_id.
                let event_id = item_id
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(|| sub_id.to_string());
                let order = crate::protocol::OrderMeta {
                    request_ordinal: attempt_req,
                    output_index,
                    sequence_number,
                };
                let stamped = sess.make_event_with_order(&event_id, EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta: delta.clone() }), order, sequence_number);
                sess.tx_event.send(stamped).await.ok();

                // Track partial assistant text in the scratchpad to help resume on retry.
                // Only accumulate when we have an item context or a single active stream.
                // We deliberately do not scope by item_id to keep implementation simple.
                sess.scratchpad_add_text_delta(&delta);
            }
            ResponseEvent::ReasoningSummaryDelta { delta, item_id, sequence_number, output_index, summary_index } => {
                // Use the item_id if present and non-empty, otherwise fall back to sub_id.
                let mut event_id = item_id
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(|| sub_id.to_string());
                if let Some(si) = summary_index { event_id = format!("{event_id}#s{si}"); }
                let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number };
                let stamped = sess.make_event_with_order(&event_id, EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta: delta.clone() }), order, sequence_number);
                sess.tx_event.send(stamped).await.ok();

                // Buffer reasoning summary so we can include a hint on retry.
                sess.scratchpad_add_reasoning_delta(&delta);
            }
            ResponseEvent::ReasoningSummaryPartAdded => {
                let stamped = sess.make_event(sub_id, EventMsg::AgentReasoningSectionBreak(AgentReasoningSectionBreakEvent {}));
                sess.tx_event.send(stamped).await.ok();
            }
            ResponseEvent::ReasoningContentDelta { delta, item_id, sequence_number, output_index, content_index } => {
                if sess.show_raw_agent_reasoning {
                    // Use the item_id if present and non-empty, otherwise fall back to sub_id.
                    let mut event_id = item_id
                        .filter(|id| !id.is_empty())
                        .unwrap_or_else(|| sub_id.to_string());
                    if let Some(ci) = content_index { event_id = format!("{event_id}#c{ci}"); }
                    let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number };
                    let stamped = sess.make_event_with_order(&event_id, EventMsg::AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent { delta }), order, sequence_number);
                    sess.tx_event.send(stamped).await.ok();
                }
            }
            ResponseEvent::ModelsEtag(etag) => {
                if let Some(remote) = sess.remote_models_manager.as_ref() {
                    remote.refresh_if_new_etag(etag).await;
                }
            }
            ResponseEvent::RateLimits(snapshot) => {
                let mut state = sess.state.lock().unwrap();
                state.latest_rate_limits = Some(snapshot.clone());
                if let Some(ctx) = account_usage_context(sess) {
                    let usage_home = ctx.code_home.clone();
                    let usage_account = ctx.account_id.clone();
                    let usage_plan = ctx.plan.clone();
                    let snapshot_clone = snapshot.clone();
                    spawn_usage_task(move || {
                        if let Err(err) = account_usage::record_rate_limit_snapshot(
                            &usage_home,
                            &usage_account,
                            usage_plan.as_deref(),
                            &snapshot_clone,
                            Utc::now(),
                        ) {
                            warn!("Failed to persist rate limit snapshot: {err}");
                        }
                    });
                }
            }
            // Note: ReasoningSummaryPartAdded handled above without scratchpad mutation.
        }
    }
}

async fn handle_response_item(
    sess: &Session,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: &str,
    item: ResponseItem,
    seq_hint: Option<u64>,
    output_index: Option<u32>,
    attempt_req: u64,
) -> CodexResult<Option<ResponseInputItem>> {
    debug!(?item, "Output item");
    let output = match item {
        ResponseItem::Message { content, id, .. } => {
            // Use the item_id if present and non-empty, otherwise fall back to sub_id.
            let event_id = id
                .filter(|id| !id.is_empty())
                .unwrap_or_else(|| sub_id.to_string());
            for item in content {
                if let ContentItem::OutputText { text } = item {
                    let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number: seq_hint };
                    let stamped = sess.make_event_with_order(&event_id, EventMsg::AgentMessage(AgentMessageEvent { message: text }), order, seq_hint);
                    sess.tx_event.send(stamped).await.ok();
                }
            }
            None
        }
        ResponseItem::CompactionSummary { .. } => {
            // Keep compaction summaries in history; no user-visible event to emit.
            None
        }
        ResponseItem::Reasoning {
            id,
            summary,
            content,
            encrypted_content: _,
        } => {
            // Use the item_id if present and not empty, otherwise fall back to sub_id
            let event_id = if !id.is_empty() {
                id.clone()
            } else {
                sub_id.to_string()
            };
            for (i, item) in summary.into_iter().enumerate() {
                let text = match item {
                    ReasoningItemReasoningSummary::SummaryText { text } => text,
                };
                let eid = format!("{event_id}#s{i}");
                let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number: seq_hint };
                let stamped = sess.make_event_with_order(&eid, EventMsg::AgentReasoning(AgentReasoningEvent { text }), order, seq_hint);
                sess.tx_event.send(stamped).await.ok();
            }
            if sess.show_raw_agent_reasoning && let Some(content) = content {
                for item in content.into_iter() {
                    let text = match item {
                        ReasoningItemContent::ReasoningText { text } => text,
                        ReasoningItemContent::Text { text } => text,
                    };
                    let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number: seq_hint };
                    let stamped = sess.make_event_with_order(&event_id, EventMsg::AgentReasoningRawContent(AgentReasoningRawContentEvent { text }), order, seq_hint);
                    sess.tx_event.send(stamped).await.ok();
                }
            }
            None
        }
        tool_item @ (ResponseItem::FunctionCall { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::CustomToolCall { .. }) => {
            crate::tools::router::ToolRouter::global()
                .dispatch_response_item(
                    sess,
                    turn_diff_tracker,
                    crate::tools::router::ToolDispatchMeta::new(
                        sub_id,
                        seq_hint,
                        output_index,
                        attempt_req,
                    ),
                    tool_item,
                )
                .await
        }
        ResponseItem::FunctionCallOutput { .. } => {
            debug!("unexpected FunctionCallOutput from stream");
            None
        }
        ResponseItem::CustomToolCallOutput { .. } => {
            debug!("unexpected CustomToolCallOutput from stream");
            None
        }
        ResponseItem::WebSearchCall { id, action, .. } => {
            if let Some(WebSearchAction::Search { query, queries }) = action {
                let call_id = id.unwrap_or_else(|| "".to_string());
                let query = web_search_query(&query, &queries);
                let event = sess.make_event_with_hint(
                    sub_id,
                    EventMsg::WebSearchComplete(WebSearchCompleteEvent { call_id, query }),
                    seq_hint,
                );
                sess.tx_event.send(event).await.ok();
            }
            None
        }
        ResponseItem::GhostSnapshot { .. } => None,
        ResponseItem::Other => None,
    };
    Ok(output)
}

fn web_search_query(query: &Option<String>, queries: &Option<Vec<String>>) -> Option<String> {
    if let Some(value) = query.clone().filter(|q| !q.is_empty()) {
        return Some(value);
    }

    let items = queries.as_ref();
    let first = items
        .and_then(|queries| queries.first())
        .cloned()
        .unwrap_or_default();
    if first.is_empty() {
        return None;
    }
    if items.is_some_and(|queries| queries.len() > 1) {
        Some(format!("{first} ..."))
    } else {
        Some(first)
    }
}


fn convert_mcp_resources_by_server(
    resources_by_server: std::collections::HashMap<String, Vec<mcp_types::Resource>>,
) -> std::collections::HashMap<String, Vec<code_protocol::mcp::Resource>> {
    resources_by_server
        .into_iter()
        .map(|(server, resources)| {
            let converted = resources
                .into_iter()
                .filter_map(|resource| match serde_json::to_value(resource) {
                    Ok(value) => match code_protocol::mcp::Resource::from_mcp_value(value) {
                        Ok(resource) => Some(resource),
                        Err(err) => {
                            warn!("failed to convert MCP resource for server {server}: {err}");
                            None
                        }
                    },
                    Err(err) => {
                        warn!("failed to serialize MCP resource for server {server}: {err}");
                        None
                    }
                })
                .collect();
            (server, converted)
        })
        .collect()
}

fn convert_mcp_resource_templates_by_server(
    templates_by_server: std::collections::HashMap<String, Vec<mcp_types::ResourceTemplate>>,
) -> std::collections::HashMap<String, Vec<code_protocol::mcp::ResourceTemplate>> {
    templates_by_server
        .into_iter()
        .map(|(server, templates)| {
            let converted = templates
                .into_iter()
                .filter_map(|template| match serde_json::to_value(template) {
                    Ok(value) => {
                        match code_protocol::mcp::ResourceTemplate::from_mcp_value(value) {
                            Ok(template) => Some(template),
                            Err(err) => {
                                warn!(
                                    "failed to convert MCP resource template for server {server}: {err}"
                                );
                                None
                            }
                        }
                    }
                    Err(err) => {
                        warn!(
                            "failed to serialize MCP resource template for server {server}: {err}"
                        );
                        None
                    }
                })
                .collect();
            (server, converted)
        })
        .collect()
}






/// Add a screenshot to pending screenshots for the next model request
pub(super) fn add_pending_screenshot(
    sess: &Session,
    screenshot_path: PathBuf,
    url: String,
) {
    // Do not queue screenshots for next turn anymore; we inject fresh per-turn.
    tracing::info!("Captured screenshot; updating UI and using per-turn injection");

    // Also send an immediate event to update the TUI display
    let event = sess.make_event(
        "browser_screenshot",
        EventMsg::BrowserScreenshotUpdate(BrowserScreenshotUpdateEvent {
            screenshot_path,
            url,
        }),
    );

    // Send event asynchronously to avoid blocking
    let tx_event = sess.tx_event.clone();
    tokio::spawn(async move {
        if let Err(e) = tx_event.send(event).await {
            tracing::error!("Failed to send browser screenshot update event: {}", e);
        }
    });
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    use super::super::session::prune_history_items;
    use code_protocol::protocol::{
        BROWSER_SNAPSHOT_CLOSE_TAG,
        BROWSER_SNAPSHOT_OPEN_TAG,
        ENVIRONMENT_CONTEXT_CLOSE_TAG,
        ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG,
        ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG,
        ENVIRONMENT_CONTEXT_OPEN_TAG,
    };

    fn make_text_message(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
            end_turn: None,
            phase: None,
        }
    }

    fn make_screenshot_message(tag: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputImage {
                image_url: tag.to_string(),
            }],
            end_turn: None,
            phase: None,
        }
    }

    #[test]
    fn prune_history_retains_recent_env_items() {
        let baseline1 = make_text_message(&format!(
            "{ENVIRONMENT_CONTEXT_OPEN_TAG}\n{{}}\n{ENVIRONMENT_CONTEXT_CLOSE_TAG}"
        ));
        let delta1 = make_text_message(&format!(
            "{ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG}\n{{\"cwd\":\"/repo\"}}\n{ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG}"
        ));
        let snapshot1 = make_text_message(&format!(
            "{BROWSER_SNAPSHOT_OPEN_TAG}\n{{\"url\":\"https://first\"}}\n{BROWSER_SNAPSHOT_CLOSE_TAG}"
        ));
        let screenshot1 = make_screenshot_message("data:image/png;base64,AAA");
        let user_msg = make_text_message("Regular user message");
        let baseline2 = make_text_message(&format!(
            "{ENVIRONMENT_CONTEXT_OPEN_TAG}\n{{\"cwd\":\"/repo2\"}}\n{ENVIRONMENT_CONTEXT_CLOSE_TAG}"
        ));
        let delta2 = make_text_message(&format!(
            "{ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG}\n{{\"cwd\":\"/repo2\"}}\n{ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG}"
        ));
        let snapshot2 = make_text_message(&format!(
            "{BROWSER_SNAPSHOT_OPEN_TAG}\n{{\"url\":\"https://second\"}}\n{BROWSER_SNAPSHOT_CLOSE_TAG}"
        ));
        let screenshot2 = make_screenshot_message("data:image/png;base64,BBB");
        let delta3 = make_text_message(&format!(
            "{ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG}\n{{\"cwd\":\"/repo3\"}}\n{ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG}"
        ));
        let snapshot3 = make_text_message(&format!(
            "{BROWSER_SNAPSHOT_OPEN_TAG}\n{{\"url\":\"https://third\"}}\n{BROWSER_SNAPSHOT_CLOSE_TAG}"
        ));
        let delta4 = make_text_message(&format!(
            "{ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG}\n{{\"cwd\":\"/repo4\"}}\n{ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG}"
        ));
        let screenshot3 = make_screenshot_message("data:image/png;base64,CCC");

        let history = vec![
            user_msg,
            baseline1,
            delta1.clone(),
            snapshot1.clone(),
            screenshot1,
            baseline2.clone(),
            delta2.clone(),
            snapshot2.clone(),
            screenshot2,
            delta3.clone(),
            snapshot3.clone(),
            delta4.clone(),
            screenshot3,
        ];

        let (pruned, stats) = prune_history_items(&history);

        // Baseline 1 should be removed; only the latest baseline retained
        assert!(pruned.contains(&baseline2));
        assert!(!pruned.contains(&history[1]));

        // Only the last three deltas should remain
        assert!(pruned.contains(&delta2));
        assert!(pruned.contains(&delta3));
        assert!(pruned.contains(&delta4));
        assert!(!pruned.contains(&delta1));

        // Only the last two browser snapshots should remain
        assert!(pruned.contains(&snapshot2));
        assert!(pruned.contains(&snapshot3));
        assert!(!pruned.contains(&snapshot1));

        // Stats reflect removals and kept counts
        assert_eq!(stats.removed_env_baselines, 1);
        assert_eq!(stats.removed_env_deltas, 1);
        assert_eq!(stats.removed_browser_snapshots, 1);
        assert_eq!(stats.kept_env_deltas, 3);
        assert_eq!(stats.kept_browser_snapshots, 2);
        assert_eq!(stats.kept_recent_screenshots, 1);
    }

    #[test]
    fn prune_history_no_env_items_is_identity() {
        let user = make_text_message("hi");
        let assistant = ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: "response".to_string(),
            }],
            end_turn: None,
            phase: None,
        };
        let history = vec![user, assistant];

        let (pruned, stats) = prune_history_items(&history);
        assert_eq!(pruned, history);
        assert!(!stats.any_removed());
    }
}

pub(super) fn debug_history(label: &str, items: &[ResponseItem]) {
    let preview: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| match item {
            ResponseItem::Message { role, content, .. } => {
                let text = content
                    .iter()
                    .filter_map(|c| match c {
                        ContentItem::InputText { text }
                        | ContentItem::OutputText { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let snippet: String = text.chars().take(80).collect();
                format!("{idx}:{role}:{snippet}")
            }
            _ => format!("{idx}:{item:?}"),
        })
        .collect();
    let rendered = preview.join(" | ");
    if std::env::var_os("CODEX_COMPACT_TRACE").is_some() {
        tracing::debug!("[compact_history] {label} => [{rendered}]");
    }
    info!(target = "code_core::compact_history", "{} => [{}]", label, rendered);
}

#[derive(Debug)]
pub(super) struct TimelineReplayContext {
    pub(super) timeline: ContextTimeline,
    pub(super) next_sequence: u64,
    pub(super) last_snapshot: Option<EnvironmentContextSnapshot>,
    pub(super) legacy_baseline: Option<EnvironmentContextSnapshot>,
}

impl Default for TimelineReplayContext {
    fn default() -> Self {
        Self {
            timeline: ContextTimeline::new(),
            next_sequence: 1,
            last_snapshot: None,
            legacy_baseline: None,
        }
    }
}

pub(super) fn process_rollout_env_item(ctx: &mut TimelineReplayContext, item: &ResponseItem) {
    if let Some(snapshot) = parse_env_snapshot_from_response(item) {
        if ctx.timeline.baseline().is_none()
            && let Err(err) = ctx.timeline.add_baseline_once(snapshot.clone()) {
                tracing::warn!("env_ctx_v2: failed to seed baseline during replay: {err}");
            }

        match ctx.timeline.record_snapshot(snapshot.clone()) {
            Ok(true) => crate::telemetry::global_telemetry().record_snapshot_commit(),
            Ok(false) => crate::telemetry::global_telemetry().record_dedup_drop(),
            Err(err) => tracing::warn!("env_ctx_v2: failed to record snapshot during replay: {err}"),
        }

        ctx.last_snapshot = Some(snapshot);
        return;
    }

    if let Some(delta) = parse_env_delta_from_response(item) {
        if let Some(base_snapshot) = ctx.last_snapshot.clone() {
            if delta.base_fingerprint != base_snapshot.fingerprint() {
                tracing::warn!(
                    "env_ctx_v2: delta base fingerprint mismatch during replay; requesting baseline resend"
                );
                crate::telemetry::global_telemetry().record_baseline_resend();
                crate::telemetry::global_telemetry().record_delta_gap();
                ctx.timeline = ContextTimeline::new();
                ctx.last_snapshot = None;
                ctx.legacy_baseline = None;
                ctx.next_sequence = 1;
                return;
            }

            let sequence = ctx.next_sequence;
            match ctx.timeline.apply_delta(sequence, delta.clone()) {
                Ok(_) => {
                    ctx.next_sequence = ctx.next_sequence.saturating_add(1);
                }
                Err(err) => {
                    tracing::warn!("env_ctx_v2: failed to apply delta during replay: {err}");
                    crate::telemetry::global_telemetry().record_delta_gap();
                    return;
                }
            }

            let next_snapshot = base_snapshot.apply_delta(&delta);
            match ctx.timeline.record_snapshot(next_snapshot.clone()) {
                Ok(true) => crate::telemetry::global_telemetry().record_snapshot_commit(),
                Ok(false) => crate::telemetry::global_telemetry().record_dedup_drop(),
                Err(err) => tracing::warn!("env_ctx_v2: failed to record snapshot during replay: {err}"),
            }

            ctx.last_snapshot = Some(next_snapshot);
        } else {
            tracing::warn!(
                "env_ctx_v2: encountered delta before baseline while replaying rollout"
            );
            crate::telemetry::global_telemetry().record_delta_gap();
        }
        return;
    }

    if ctx.legacy_baseline.is_none() && is_legacy_system_status(item)
        && let Some(snapshot) = parse_legacy_status_snapshot(item) {
            ctx.legacy_baseline = Some(snapshot);
        }
}

fn extract_tagged_json<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let end = text.rfind(close)?;
    if end <= start {
        return None;
    }
    Some(text[start..end].trim())
}

pub(super) fn parse_env_snapshot_from_response(
    item: &ResponseItem,
) -> Option<EnvironmentContextSnapshot> {
    if let ResponseItem::Message { role, content, .. } = item {
        if role != "user" {
            return None;
        }
        for piece in content {
            if let ContentItem::InputText { text } = piece
                && let Some(json) = extract_tagged_json(
                    text,
                    ENVIRONMENT_CONTEXT_OPEN_TAG,
                    ENVIRONMENT_CONTEXT_CLOSE_TAG,
                )
                    && let Ok(snapshot) = serde_json::from_str::<EnvironmentContextSnapshot>(json) {
                        return Some(snapshot);
                    }
        }
    }
    None
}

pub(super) fn parse_env_delta_from_response(
    item: &ResponseItem,
) -> Option<EnvironmentContextDelta> {
    if let ResponseItem::Message { role, content, .. } = item {
        if role != "user" {
            return None;
        }
        for piece in content {
            if let ContentItem::InputText { text } = piece
                && let Some(json) = extract_tagged_json(
                    text,
                    ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG,
                    ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG,
                )
                    && let Ok(delta) = serde_json::from_str::<EnvironmentContextDelta>(json) {
                        return Some(delta);
                    }
        }
    }
    None
}

fn is_legacy_system_status(item: &ResponseItem) -> bool {
    if let ResponseItem::Message { role, content, .. } = item {
        if role != "user" {
            return false;
        }
        return content.iter().any(|c| {
            if let ContentItem::InputText { text } = c {
                text.contains("== System Status ==")
            } else {
                false
            }
        });
    }
    false
}

fn parse_legacy_status_snapshot(item: &ResponseItem) -> Option<EnvironmentContextSnapshot> {
    if let ResponseItem::Message { role, content, .. } = item {
        if role != "user" {
            return None;
        }
        for piece in content {
            if let ContentItem::InputText { text } = piece {
                if !text.contains("== System Status ==") {
                    continue;
                }

                let mut cwd: Option<String> = None;
                let mut branch: Option<String> = None;
                for line in text.lines() {
                    let trimmed = line.trim();
                    if let Some(rest) = trimmed.strip_prefix("cwd:") {
                        let value = rest.trim();
                        if !value.is_empty() {
                            cwd = Some(value.to_string());
                        }
                    } else if let Some(rest) = trimmed.strip_prefix("branch:") {
                        let value = rest.trim();
                        if !value.is_empty() && value != "unknown" {
                            branch = Some(value.to_string());
                        }
                    }
                }

                return Some(EnvironmentContextSnapshot {
                    version: EnvironmentContextSnapshot::VERSION,
                    cwd,
                    approval_policy: None,
                    sandbox_mode: None,
                    network_access: None,
                    writable_roots: Vec::new(),
                    operating_system: None,
                    common_tools: Vec::new(),
                    shell: None,
                    git_branch: branch,
                    reasoning_effort: None,
                });
            }
        }
    }
    None
}
