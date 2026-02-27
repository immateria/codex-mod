use super::*;

impl Runner<'_> {
    pub(super) async fn build_session(&mut self, prepared: Prepared) -> Built {
        let Prepared {
            submission_id,
            provider,
            model,
            model_reasoning_effort,
            model_reasoning_summary,
            model_text_verbosity,
            approval_policy,
            sandbox_policy,
            disable_response_storage,
            notify,
            cwd,
            collaboration_mode,
            demo_developer_message,
            dynamic_tools,
            shell_override_present,
            base_instructions,
            effective_user_instructions,
            resolved_shell,
            command_safety_profile,
            active_shell_style,
            active_shell_style_label,
            shell_style_profile_messages,
            shell_style_mcp_include,
            shell_style_mcp_exclude,
            effective_mcp_servers,
            session_skills,
            restored_items,
            restored_history_snapshot,
            resume_notice,
            rollout_recorder,
        } = prepared;

        let config = Arc::clone(&self.config);

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
                && let Err(e) = logger.set_session_usage_file(&self.session_id)
            {
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

        let conversation_id = code_protocol::mcp_protocol::ConversationId::from(self.session_id);
        let auth_snapshot = self.auth_manager.as_ref().and_then(|mgr| mgr.auth());
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
                config.mcp_servers.keys().map(String::as_str).collect(),
                config.active_profile.clone(),
            );
            manager
        };

        // Wrap provided auth (if any) in a minimal AuthManager for client usage.
        let client = ModelClient::new(crate::client::ModelClientInit {
            config: config.clone(),
            auth_manager: self.auth_manager.clone(),
            otel_event_manager: Some(otel_event_manager.clone()),
            provider: provider.clone(),
            effort: model_reasoning_effort,
            summary: model_reasoning_summary,
            verbosity: model_text_verbosity,
            session_id: self.session_id,
            debug_logger,
        });

        // abort any current running session and clone its state
        let old_session = self.sess.take();
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
                excluded_tools.insert((tool.mcp_server.to_string(), tool.tool_name.to_string()));
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
            effective_mcp_servers,
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
                    crate::protocol::McpServerFailurePhase::ListTools => {
                        format!("MCP server `{server_name}` failed to list tools: {detail}")
                    }
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
        tools_config.web_search_allowed_domains = config.tools_web_search_allowed_domains.clone();
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
        let auth_mode = self
            .auth_manager
            .as_ref()
            .and_then(|mgr| mgr.auth().map(|auth| auth.mode))
            .or(Some(if config.using_chatgpt_auth {
                AppAuthMode::Chatgpt
            } else {
                AppAuthMode::ApiKey
            }));
        let supports_pro_only_models = self
            .auth_manager
            .as_ref()
            .is_some_and(|mgr| mgr.supports_pro_only_models());

        agent_models = filter_agent_model_names_for_auth(
            agent_models,
            auth_mode,
            supports_pro_only_models,
        );
        if agent_models.is_empty() {
            agent_models = enabled_agent_model_specs_for_auth(auth_mode, supports_pro_only_models)
                .into_iter()
                .map(|spec| spec.slug.to_string())
                .collect();
        }
        agent_models.sort_by_key(|a| a.to_ascii_lowercase());
        agent_models.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        tools_config.set_agent_models(agent_models);

        let model_descriptions = model_guide_markdown_with_custom(&config.agents);
        let remote_models_manager = self.auth_manager.as_ref().map(|mgr| {
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

        let network_approval = Arc::new(crate::network_approval::NetworkApprovalService::default());
        let network_policy_decider_session = config.network_proxy.as_ref().map(|_| {
            Arc::new(tokio::sync::RwLock::new(std::sync::Weak::<Session>::new()))
        });
        let network_policy_decider = network_policy_decider_session.as_ref().map(|session| {
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
                    let message = format!("Failed to start managed network proxy: {err}");
                    error!("{message}");
                    mcp_connection_errors.push(message);
                    None
                }
            }
        } else {
            None
        };

        let mut new_session = Arc::new(Session {
            id: self.session_id,
            client,
            remote_models_manager,
            tools_config,
            dynamic_tools,
            exec_command_manager: Arc::new(crate::exec_command::SessionManager::default()),
            js_repl: crate::tools::js_repl::JsReplHandle::new(None),
            network_proxy,
            network_approval: Arc::clone(&network_approval),
            tx_event: self.tx_event.clone(),
            user_instructions: effective_user_instructions,
            base_instructions,
            skills: tokio::sync::RwLock::new(session_skills),
            demo_developer_message,
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
                style_label: active_shell_style_label,
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
        self.sess = Some(new_session);
        if let Some(sess_arc) = self.sess.as_ref()
            && let Some(lock) = network_policy_decider_session.as_ref()
        {
            let mut guard = lock.write().await;
            *guard = Arc::downgrade(sess_arc);
        }
        if let Some(sess_arc) = self.sess.as_ref() {
            // Reset environment context tracker if shell changed
            if shell_override_present {
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
        if let Some(sess_arc) = self.sess.as_ref()
            && let Some(items) = restored_items.as_ref()
        {
            let turn_context = sess_arc.make_turn_context();
            let reconstructed = sess_arc.reconstruct_history_from_rollout(&turn_context, items);
            {
                let mut st = sess_arc.state.lock().unwrap();
                st.history = ConversationHistory::new();
                st.history.record_items(reconstructed.iter());
            }
            replay_history_items = Some(reconstructed);
        }

        Built {
            submission_id,
            model,
            mcp_connection_errors,
            restored_items,
            restored_history_snapshot,
            replay_history_items,
            resume_notice,
        }
    }
}

