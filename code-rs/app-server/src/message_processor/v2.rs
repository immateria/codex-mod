use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use code_app_server_protocol::AskForApproval;
use code_app_server_protocol::ListMcpServerStatusParams;
use code_app_server_protocol::ListMcpServerStatusResponse;
use code_app_server_protocol::CommandAction;
use code_app_server_protocol::CommandExecutionOutputDeltaNotification;
use code_app_server_protocol::CommandExecutionStatus;
use code_app_server_protocol::McpServerStatus;
use code_app_server_protocol::Model;
use code_app_server_protocol::ModelListParams;
use code_app_server_protocol::ModelListResponse;
use code_app_server_protocol::ReasoningEffortOption;
use code_app_server_protocol::SandboxMode;
use code_app_server_protocol::SandboxPolicy;
use code_app_server_protocol::ServerNotification;
use code_app_server_protocol::Thread;
use code_app_server_protocol::ThreadItem;
use code_app_server_protocol::ThreadListParams;
use code_app_server_protocol::ThreadListResponse;
use code_app_server_protocol::ThreadStartedNotification;
use code_app_server_protocol::ThreadStartParams;
use code_app_server_protocol::ThreadStartResponse;
use code_app_server_protocol::ThreadReadParams;
use code_app_server_protocol::ThreadReadResponse;
use code_app_server_protocol::ThreadSortKey;
use code_app_server_protocol::ThreadSourceKind;
use code_app_server_protocol::Turn;
use code_app_server_protocol::TurnCompletedNotification;
use code_app_server_protocol::TurnStartParams;
use code_app_server_protocol::TurnStartResponse;
use code_app_server_protocol::TurnStartedNotification;
use code_app_server_protocol::TurnStatus;
use code_app_server_protocol::UserInput;
use chrono::Utc;
use code_common::model_presets;
use code_core::SessionCatalog;
use code_core::SessionIndexEntry;
use code_core::SessionQuery;
use code_core::config::ConfigBuilder;
use code_core::config::ConfigOverrides;
use code_core::entry_to_rollout_path;
use code_core::mcp_connection_manager::McpConnectionManager;
use code_core::mcp_snapshot::collect_runtime_snapshot;
use code_core::mcp_snapshot::format_failure_summary;
use code_core::mcp_snapshot::format_transport_summary;
use code_core::mcp_snapshot::group_tool_definitions_by_server;
use code_core::mcp_snapshot::merge_servers;
use code_protocol::mcp::Tool as ProtocolMcpTool;
use code_protocol::models::ContentItem;
use code_protocol::models::ReasoningItemContent;
use code_protocol::models::ReasoningItemReasoningSummary;
use code_protocol::models::ResponseItem;
use code_protocol::protocol::RolloutItem;
use code_protocol::protocol::RolloutLine;
use code_protocol::protocol::SessionSource;
use code_protocol::protocol::SubAgentSource;
use mcp_types::JSONRPCErrorError;
use uuid::Uuid;

use super::MessageProcessor;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::OutgoingMessageSender;
use crate::outgoing_message::OutgoingNotification;
use crate::thread_state::ThreadState;

mod status_conversion;

use status_conversion::convert_mcp_resource_templates;
use status_conversion::convert_mcp_resources;
use status_conversion::convert_mcp_tool;

impl MessageProcessor {
    pub(super) async fn list_models_v2(
        &self,
        request_id: mcp_types::RequestId,
        params: ModelListParams,
    ) {
        let include_hidden = params.include_hidden.unwrap_or(false);
        let auth_mode = self.auth_manager.auth().map(|auth| auth.mode);
        let supports_pro_only_models = self.auth_manager.supports_pro_only_models();

        let presets: Vec<code_common::model_presets::ModelPreset> = if include_hidden {
            model_presets::all_model_presets()
                .iter()
                .filter(|preset| {
                    model_presets::model_preset_available_for_auth(
                        preset,
                        auth_mode,
                        supports_pro_only_models,
                    )
                })
                .cloned()
                .collect()
        } else {
            model_presets::builtin_model_presets(auth_mode, supports_pro_only_models)
        };

        let total = presets.len();
        if total == 0 {
            self.outgoing
                .send_response(
                    request_id,
                    ModelListResponse {
                        data: Vec::new(),
                        next_cursor: None,
                    },
                )
                .await;
            return;
        }

        let limit = params.limit.unwrap_or(total as u32).max(1) as usize;
        let start = match parse_cursor_offset(params.cursor.as_deref(), total, "models") {
            Ok(offset) => offset,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };
        let end = start.saturating_add(limit).min(total);

        let data = presets[start..end]
            .iter()
            .map(model_preset_to_v2_model)
            .collect();
        let next_cursor = (end < total).then(|| end.to_string());

        self.outgoing
            .send_response(request_id, ModelListResponse { data, next_cursor })
            .await;
    }

    pub(super) async fn list_threads_v2(
        &self,
        request_id: mcp_types::RequestId,
        params: ThreadListParams,
    ) {
        let catalog = SessionCatalog::new(self.base_config.code_home.clone());
        let mut entries = match catalog
            .query(&SessionQuery {
                include_archived: true,
                include_deleted: false,
                ..SessionQuery::default()
            })
            .await
        {
            Ok(entries) => entries,
            Err(err) => {
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("failed to query session catalog: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let archived_only = params.archived.unwrap_or(false);
        entries.retain(|entry| entry.archived == archived_only);

        if let Some(model_providers) = params.model_providers.as_ref()
            && !model_providers.is_empty()
        {
            entries.retain(|entry| {
                entry.model_provider.as_ref().is_some_and(|provider| {
                    model_providers
                        .iter()
                        .any(|candidate| candidate.eq_ignore_ascii_case(provider))
                })
            });
        }

        if let Some(cwd) = params.cwd.as_ref() {
            let expected = PathBuf::from(cwd);
            entries.retain(|entry| entry.cwd_real == expected);
        }

        let source_filters = normalize_thread_source_filters(params.source_kinds.as_deref());
        entries.retain(|entry| source_filters.contains(&session_source_to_thread_kind(&entry.session_source)));

        match params.sort_key.unwrap_or(ThreadSortKey::CreatedAt) {
            ThreadSortKey::CreatedAt => entries.sort_by(|a, b| {
                b.created_at
                    .cmp(&a.created_at)
                    .then_with(|| b.session_id.cmp(&a.session_id))
            }),
            ThreadSortKey::UpdatedAt => entries.sort_by(|a, b| {
                b.last_event_at
                    .cmp(&a.last_event_at)
                    .then_with(|| b.session_id.cmp(&a.session_id))
            }),
        }

        let total = entries.len();
        let limit = params.limit.unwrap_or(total as u32).max(1) as usize;
        let start = match parse_cursor_offset(params.cursor.as_deref(), total, "threads") {
            Ok(offset) => offset,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };
        let end = start.saturating_add(limit).min(total);

        let data = entries[start..end]
            .iter()
            .map(|entry| session_entry_to_thread(entry, &self.base_config.code_home, Vec::new()))
            .collect();
        let next_cursor = (end < total).then(|| end.to_string());

        self.outgoing
            .send_response(request_id, ThreadListResponse { data, next_cursor })
            .await;
    }

    pub(super) async fn thread_read_v2(
        &self,
        request_id: mcp_types::RequestId,
        params: ThreadReadParams,
    ) {
        let ThreadReadParams {
            thread_id,
            include_turns,
        } = params;

        let parsed_id = match Uuid::parse_str(&thread_id) {
            Ok(id) => id,
            Err(error) => {
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("invalid thread id: {error}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let catalog = SessionCatalog::new(self.base_config.code_home.clone());
        let entry = match catalog.find_by_id(&thread_id).await {
            Ok(Some(entry)) if entry.session_id == parsed_id => entry,
            Ok(_) => {
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("thread not found: {thread_id}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
            Err(err) => {
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("failed to query session catalog: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let turns = if include_turns {
            let rollout_path = entry_to_rollout_path(&self.base_config.code_home, &entry);
            match read_turns_from_rollout(&rollout_path).await {
                Ok(turns) => turns,
                Err(err) => {
                    self.outgoing
                        .send_error(
                            request_id,
                            JSONRPCErrorError {
                                code: INTERNAL_ERROR_CODE,
                                message: format!(
                                    "failed to read rollout history for {thread_id}: {err}"
                                ),
                                data: None,
                            },
                        )
                        .await;
                    return;
                }
            }
        } else {
            Vec::new()
        };

        let thread = session_entry_to_thread(&entry, &self.base_config.code_home, turns);
        self.outgoing
            .send_response(request_id, ThreadReadResponse { thread })
            .await;
    }

    pub(super) async fn list_mcp_server_status_v2(
        &self,
        request_id: mcp_types::RequestId,
        params: ListMcpServerStatusParams,
    ) {
        let (enabled_servers, _disabled_servers) =
            match code_core::config::list_mcp_servers(&self.base_config.code_home) {
                Ok(servers) => servers,
                Err(err) => {
                    self.outgoing
                        .send_error(
                            request_id,
                            JSONRPCErrorError {
                                code: INTERNAL_ERROR_CODE,
                                message: format!("failed to read MCP server config: {err}"),
                                data: None,
                            },
                        )
                        .await;
                    return;
                }
            };

        let excluded_tools: HashSet<(String, String)> = enabled_servers
            .iter()
            .flat_map(|(server_name, cfg)| {
                let server_name = server_name.clone();
                cfg.disabled_tools
                    .iter()
                    .cloned()
                    .map(move |tool_name| (server_name.clone(), tool_name))
            })
            .collect();

        let enabled_server_map: HashMap<String, code_core::config_types::McpServerConfig> =
            enabled_servers.iter().cloned().collect();

        let (manager, startup_errors) = match McpConnectionManager::new(
            self.base_config.code_home.clone(),
            self.base_config.mcp_oauth_credentials_store_mode,
            enabled_server_map,
            excluded_tools,
        )
        .await
        {
            Ok(result) => result,
            Err(err) => {
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("failed to start MCP manager: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let mut runtime_snapshot = collect_runtime_snapshot(&manager).await;
        for (server_name, failure) in startup_errors {
            runtime_snapshot.failures.entry(server_name).or_insert(failure);
        }

        let merged_servers = match merge_servers(&self.base_config.code_home, &runtime_snapshot) {
            Ok(servers) => servers,
            Err(err) => {
                manager.shutdown_all().await;
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("failed to merge MCP status snapshot: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let tool_definitions_by_server =
            group_tool_definitions_by_server(manager.list_all_tools_with_server_names());
        let resources_by_server = manager.list_resources_by_server().await;
        let resource_templates_by_server = manager.list_resource_templates_by_server().await;

        let total = merged_servers.len();
        let limit = params.limit.unwrap_or(total as u32).max(1) as usize;
        let start = match parse_cursor_offset(params.cursor.as_deref(), total, "MCP servers") {
            Ok(offset) => offset,
            Err(error) => {
                manager.shutdown_all().await;
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };
        let end = start.saturating_add(limit).min(total);

        let data: Vec<McpServerStatus> = merged_servers[start..end]
            .iter()
            .map(|server| {
                let mut tools: HashMap<String, ProtocolMcpTool> = HashMap::new();
                if let Some(definitions) = tool_definitions_by_server.get(&server.name) {
                    for tool_name in &server.tools {
                        if let Some(tool) = definitions.get(tool_name) {
                            match convert_mcp_tool(tool) {
                                Ok(protocol_tool) => {
                                    tools.insert(tool_name.clone(), protocol_tool);
                                }
                                Err(err) => {
                                    tracing::warn!(
                                        "failed to convert MCP tool '{tool_name}' for server '{}': {err}",
                                        server.name
                                    );
                                }
                            }
                        }
                    }
                }

                let resources = resources_by_server
                    .get(&server.name)
                    .map(Vec::as_slice)
                    .map(convert_mcp_resources)
                    .unwrap_or_default();

                let resource_templates = resource_templates_by_server
                    .get(&server.name)
                    .map(Vec::as_slice)
                    .map(convert_mcp_resource_templates)
                    .unwrap_or_default();

                McpServerStatus {
                    name: server.name.clone(),
                    enabled: server.enabled,
                    transport: format_transport_summary(&server.config),
                    startup_timeout_sec: server.config.startup_timeout_sec.map(|d| d.as_secs_f64()),
                    tool_timeout_sec: server.config.tool_timeout_sec.map(|d| d.as_secs_f64()),
                    disabled_tools: server.disabled_tools.clone(),
                    failure: server.failure.as_ref().map(format_failure_summary),
                    tools,
                    resources,
                    resource_templates,
                    auth_status: server.auth_status.into(),
                }
            })
            .collect();
        manager.shutdown_all().await;

        let next_cursor = (end < total).then(|| end.to_string());
        self.outgoing
            .send_response(request_id, ListMcpServerStatusResponse { data, next_cursor })
            .await;
    }

    pub(super) async fn thread_start_v2(
        &mut self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: ThreadStartParams,
    ) {
        let ThreadStartParams {
            model,
            model_provider,
            cwd,
            approval_policy,
            sandbox,
            config,
            base_instructions,
            developer_instructions,
            personality,
            dynamic_tools,
            ..
        } = params;

        let thread_cwd = cwd
            .as_deref()
            .map(PathBuf::from)
            .map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    self.base_config.cwd.join(path)
                }
            })
            .unwrap_or_else(|| self.base_config.cwd.clone());

        let dynamic_tools = dynamic_tools.map(|specs| {
            specs
                .into_iter()
                .map(v2_dynamic_tool_spec_to_protocol)
                .collect()
        });

        let overrides = ConfigOverrides {
            model,
            model_provider,
            approval_policy: approval_policy.map(v2_approval_policy_to_core),
            sandbox_mode: sandbox.map(SandboxMode::to_core),
            cwd: Some(thread_cwd),
            base_instructions,
            code_linux_sandbox_exe: self.code_linux_sandbox_exe.clone(),
            dynamic_tools,
            ..Default::default()
        };

        let mut cli_overrides = self.cli_overrides.clone();
        if let Some(config_overrides) = config {
            for (key, value) in config_overrides {
                cli_overrides.push((key, code_utils_json_to_toml::json_to_toml(value)));
            }
        }

        let mut config = match ConfigBuilder::new()
            .with_code_home(self.base_config.code_home.clone())
            .with_cli_overrides(cli_overrides)
            .with_overrides(overrides)
            .with_loader_overrides(code_core::config_loader::LoaderOverrides::default())
            .load()
        {
            Ok(config) => config,
            Err(err) => {
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("failed to load thread config: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        // V2-specific fields that aren't part of the core config builder yet.
        config.demo_developer_message = developer_instructions;
        config.model_personality = personality.map(protocol_personality_to_core);

        let model = config.model.clone();
        let model_provider = config.model_provider_id.clone();
        let cwd = config.cwd.clone();
        let approval_policy = config.approval_policy;
        let sandbox_policy = config.sandbox_policy.clone();
        let reasoning_effort = Some(map_core_reasoning_effort(config.model_reasoning_effort.into()));

        let new_conversation = match self.conversation_manager.new_conversation(config).await {
            Ok(new_conversation) => new_conversation,
            Err(err) => {
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("failed to start thread: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let thread_id = new_conversation.conversation_id;
        let thread = Thread {
            id: thread_id.to_string(),
            preview: String::new(),
            model_provider: model_provider.clone(),
            created_at: Utc::now().timestamp(),
            updated_at: Utc::now().timestamp(),
            path: None,
            cwd: cwd.clone(),
            cli_version: env!("CARGO_PKG_VERSION").to_string(),
            source: code_app_server_protocol::SessionSource::AppServer,
            git_info: None,
            turns: Vec::new(),
        };

        let thread_state = self
            .thread_state_manager
            .ensure_connection_subscribed(thread_id, connection_id)
            .await;
        self.ensure_thread_listener(
            thread_id.to_string(),
            new_conversation.conversation,
            thread_state,
        )
        .await;

        self.outgoing
            .send_response(
                request_id,
                ThreadStartResponse {
                    thread: thread.clone(),
                    model,
                    model_provider,
                    cwd,
                    approval_policy: core_approval_policy_to_v2(approval_policy),
                    sandbox: core_sandbox_policy_to_v2(sandbox_policy),
                    reasoning_effort,
                },
            )
            .await;

        send_server_notification_to_connection(
            self.outgoing.as_ref(),
            connection_id,
            ServerNotification::ThreadStarted(ThreadStartedNotification { thread }),
        )
        .await;
    }

    pub(super) async fn turn_start_v2(
        &mut self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: TurnStartParams,
    ) {
        let thread_id = match code_protocol::ConversationId::from_string(&params.thread_id) {
            Ok(id) => id,
            Err(err) => {
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("invalid thread id: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let conversation = match self.conversation_manager.get_conversation(thread_id).await {
            Ok(conversation) => conversation,
            Err(err) => {
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("thread not found: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let thread_state = self
            .thread_state_manager
            .ensure_connection_subscribed(thread_id, connection_id)
            .await;
        self.ensure_thread_listener(thread_id.to_string(), Arc::clone(&conversation), thread_state)
            .await;

        let mut items = Vec::with_capacity(params.input.len());
        for input in params.input {
            match v2_user_input_to_core_input_item(input) {
                Ok(item) => items.push(item),
                Err(message) => {
                    self.outgoing
                        .send_error(
                            request_id,
                            JSONRPCErrorError {
                                code: INVALID_REQUEST_ERROR_CODE,
                                message,
                                data: None,
                            },
                        )
                        .await;
                    return;
                }
            }
        }

        let turn_id = match conversation
            .submit(code_core::protocol::Op::UserInput {
                items,
                final_output_json_schema: params.output_schema.clone(),
            })
            .await
        {
            Ok(turn_id) => turn_id,
            Err(err) => {
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("failed to submit turn: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        self.outgoing
            .send_response(
                request_id,
                TurnStartResponse {
                    turn: Turn {
                        id: turn_id,
                        items: Vec::new(),
                        status: TurnStatus::InProgress,
                        error: None,
                    },
                },
            )
            .await;
    }

    async fn ensure_thread_listener(
        &self,
        thread_id: String,
        conversation: Arc<code_core::CodexConversation>,
        thread_state: Arc<tokio::sync::Mutex<ThreadState>>,
    ) {
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        let should_spawn = {
            let mut guard = thread_state.lock().await;
            if guard.listener_matches(&conversation) {
                false
            } else {
                guard.set_listener(cancel_tx, &conversation);
                true
            }
        };

        if !should_spawn {
            return;
        }

        let outgoing = Arc::clone(&self.outgoing);
        tokio::spawn(async move {
            run_thread_event_loop(thread_id, conversation, thread_state, outgoing, cancel_rx).await;
        });
    }
}

#[derive(Clone)]
struct CommandExecutionInfo {
    command: String,
    cwd: PathBuf,
    command_actions: Vec<CommandAction>,
}

async fn run_thread_event_loop(
    thread_id: String,
    conversation: Arc<code_core::CodexConversation>,
    thread_state: Arc<tokio::sync::Mutex<ThreadState>>,
    outgoing: Arc<OutgoingMessageSender>,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let mut agent_message_item_id_by_turn = HashMap::<String, String>::new();
    let mut agent_message_text_by_turn = HashMap::<String, String>::new();
    let mut started_agent_message_turns = HashSet::<String>::new();
    let mut command_exec_by_call_id = HashMap::<String, CommandExecutionInfo>::new();

    loop {
        tokio::select! {
            _ = &mut cancel_rx => break,
            event = conversation.next_event() => {
                let event = match event {
                    Ok(event) => event,
                    Err(err) => {
                        tracing::warn!("thread listener exited: {err}");
                        break;
                    }
                };

                let turn_id = event.id.clone();
                match event.msg {
                    code_core::protocol::EventMsg::TaskStarted => {
                        let turn = Turn { id: turn_id.clone(), items: Vec::new(), status: TurnStatus::InProgress, error: None };
                        broadcast_server_notification(
                            outgoing.as_ref(),
                            &thread_state,
                            ServerNotification::TurnStarted(TurnStartedNotification { thread_id: thread_id.clone(), turn }),
                        ).await;
                    }
                    code_core::protocol::EventMsg::AgentMessageDelta(delta) => {
                        let item_id = agent_message_item_id_by_turn
                            .entry(turn_id.clone())
                            .or_insert_with(|| format!("{turn_id}:agentMessage"))
                            .clone();
                        let text = agent_message_text_by_turn.entry(turn_id.clone()).or_default();
                        text.push_str(&delta.delta);

                        if started_agent_message_turns.insert(turn_id.clone()) {
                            broadcast_server_notification(
                                outgoing.as_ref(),
                                &thread_state,
                                ServerNotification::ItemStarted(code_app_server_protocol::ItemStartedNotification {
                                    item: ThreadItem::AgentMessage { id: item_id.clone(), text: String::new() },
                                    thread_id: thread_id.clone(),
                                    turn_id: turn_id.clone(),
                                }),
                            ).await;
                        }

                        broadcast_server_notification(
                            outgoing.as_ref(),
                            &thread_state,
                            ServerNotification::AgentMessageDelta(code_app_server_protocol::AgentMessageDeltaNotification {
                                thread_id: thread_id.clone(),
                                turn_id: turn_id.clone(),
                                item_id,
                                delta: delta.delta,
                            }),
                        ).await;
                    }
                    code_core::protocol::EventMsg::ExecCommandBegin(begin) => {
                        let command = begin.command.join(" ");
                        let command_actions: Vec<CommandAction> = begin
                            .parsed_cmd
                            .into_iter()
                            .map(|parsed| code_protocol::parse_command::ParsedCommand::from(parsed).into())
                            .collect();
                        let info = CommandExecutionInfo { command: command.clone(), cwd: begin.cwd.clone(), command_actions: command_actions.clone() };
                        command_exec_by_call_id.insert(begin.call_id.clone(), info.clone());

                        broadcast_server_notification(
                            outgoing.as_ref(),
                            &thread_state,
                            ServerNotification::ItemStarted(code_app_server_protocol::ItemStartedNotification {
                                item: ThreadItem::CommandExecution {
                                    id: begin.call_id.clone(),
                                    command,
                                    cwd: begin.cwd,
                                    process_id: None,
                                    status: CommandExecutionStatus::InProgress,
                                    command_actions,
                                    aggregated_output: None,
                                    exit_code: None,
                                    duration_ms: None,
                                },
                                thread_id: thread_id.clone(),
                                turn_id: turn_id.clone(),
                            }),
                        ).await;
                    }
                    code_core::protocol::EventMsg::ExecCommandOutputDelta(delta) => {
                        let delta_text = String::from_utf8_lossy(&delta.chunk).to_string();
                        broadcast_server_notification(
                            outgoing.as_ref(),
                            &thread_state,
                            ServerNotification::CommandExecutionOutputDelta(CommandExecutionOutputDeltaNotification {
                                thread_id: thread_id.clone(),
                                turn_id: turn_id.clone(),
                                item_id: delta.call_id,
                                delta: delta_text,
                            }),
                        ).await;
                    }
                    code_core::protocol::EventMsg::ExecCommandEnd(end) => {
                        let info = command_exec_by_call_id.remove(&end.call_id);
                        let (command, cwd, command_actions) = match info {
                            Some(info) => (info.command, info.cwd, info.command_actions),
                            None => (String::new(), PathBuf::new(), Vec::new()),
                        };

                        let (status, exit_code) = if end.exit_code == 0 {
                            (CommandExecutionStatus::Completed, Some(0))
                        } else {
                            (CommandExecutionStatus::Failed, Some(end.exit_code))
                        };

                        let aggregated_output = if end.stdout.is_empty() && end.stderr.is_empty() {
                            None
                        } else {
                            Some(format!("{}{}", end.stdout, end.stderr))
                        };

                        broadcast_server_notification(
                            outgoing.as_ref(),
                            &thread_state,
                            ServerNotification::ItemCompleted(code_app_server_protocol::ItemCompletedNotification {
                                item: ThreadItem::CommandExecution {
                                    id: end.call_id,
                                    command,
                                    cwd,
                                    process_id: None,
                                    status,
                                    command_actions,
                                    aggregated_output,
                                    exit_code,
                                    duration_ms: Some(end.duration.as_millis() as i64),
                                },
                                thread_id: thread_id.clone(),
                                turn_id: turn_id.clone(),
                            }),
                        ).await;
                    }
                    code_core::protocol::EventMsg::TaskComplete(complete) => {
                        if let Some(final_message) = complete.last_agent_message {
                            let item_id = agent_message_item_id_by_turn
                                .entry(turn_id.clone())
                                .or_insert_with(|| format!("{turn_id}:agentMessage"))
                                .clone();
                            let text = agent_message_text_by_turn.entry(turn_id.clone()).or_default();
                            if text.is_empty() {
                                text.push_str(&final_message);
                            }
                            if started_agent_message_turns.insert(turn_id.clone()) {
                                broadcast_server_notification(
                                    outgoing.as_ref(),
                                    &thread_state,
                                    ServerNotification::ItemStarted(code_app_server_protocol::ItemStartedNotification {
                                        item: ThreadItem::AgentMessage { id: item_id.clone(), text: String::new() },
                                        thread_id: thread_id.clone(),
                                        turn_id: turn_id.clone(),
                                    }),
                                ).await;
                            }
                            let final_text = agent_message_text_by_turn.get(&turn_id).cloned().unwrap_or_default();
                            broadcast_server_notification(
                                outgoing.as_ref(),
                                &thread_state,
                                ServerNotification::ItemCompleted(code_app_server_protocol::ItemCompletedNotification {
                                    item: ThreadItem::AgentMessage { id: item_id, text: final_text },
                                    thread_id: thread_id.clone(),
                                    turn_id: turn_id.clone(),
                                }),
                            ).await;
                        } else if started_agent_message_turns.contains(&turn_id) {
                            let item_id = agent_message_item_id_by_turn
                            .entry(turn_id.clone())
                            .or_insert_with(|| format!("{turn_id}:agentMessage"))
                            .clone();
                            let final_text = agent_message_text_by_turn.get(&turn_id).cloned().unwrap_or_default();
                            broadcast_server_notification(
                                outgoing.as_ref(),
                                &thread_state,
                                ServerNotification::ItemCompleted(code_app_server_protocol::ItemCompletedNotification {
                                    item: ThreadItem::AgentMessage { id: item_id, text: final_text },
                                    thread_id: thread_id.clone(),
                                    turn_id: turn_id.clone(),
                                }),
                            ).await;
                        }

                        let turn = Turn { id: turn_id.clone(), items: Vec::new(), status: TurnStatus::Completed, error: None };
                        broadcast_server_notification(
                            outgoing.as_ref(),
                            &thread_state,
                            ServerNotification::TurnCompleted(TurnCompletedNotification { thread_id: thread_id.clone(), turn }),
                        ).await;
                    }
                    code_core::protocol::EventMsg::TurnAborted(_) => {
                        let turn = Turn { id: turn_id.clone(), items: Vec::new(), status: TurnStatus::Interrupted, error: None };
                        broadcast_server_notification(
                            outgoing.as_ref(),
                            &thread_state,
                            ServerNotification::TurnCompleted(TurnCompletedNotification { thread_id: thread_id.clone(), turn }),
                        ).await;
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn broadcast_server_notification(
    outgoing: &OutgoingMessageSender,
    thread_state: &Arc<tokio::sync::Mutex<ThreadState>>,
    notification: ServerNotification,
) {
    let connection_ids = thread_state.lock().await.subscribed_connection_ids();
    for connection_id in connection_ids {
        send_server_notification_to_connection(outgoing, connection_id, notification.clone()).await;
    }
}

async fn send_server_notification_to_connection(
    outgoing: &OutgoingMessageSender,
    connection_id: ConnectionId,
    notification: ServerNotification,
) {
    let method = notification.to_string();
    let params = match notification.to_params() {
        Ok(params) => Some(params),
        Err(err) => {
            tracing::warn!("failed to serialize notification params: {err}");
            None
        }
    };
    outgoing
        .send_notification_to_connection(
            connection_id,
            OutgoingNotification {
                method,
                params,
            },
        )
        .await;
}

fn v2_approval_policy_to_core(policy: AskForApproval) -> code_core::protocol::AskForApproval {
    match policy {
        AskForApproval::UnlessTrusted => code_core::protocol::AskForApproval::UnlessTrusted,
        AskForApproval::OnFailure => code_core::protocol::AskForApproval::OnFailure,
        AskForApproval::OnRequest => code_core::protocol::AskForApproval::OnRequest,
        AskForApproval::Never => code_core::protocol::AskForApproval::Never,
    }
}

fn core_approval_policy_to_v2(policy: code_core::protocol::AskForApproval) -> AskForApproval {
    match policy {
        code_core::protocol::AskForApproval::UnlessTrusted => AskForApproval::UnlessTrusted,
        code_core::protocol::AskForApproval::OnFailure => AskForApproval::OnFailure,
        code_core::protocol::AskForApproval::OnRequest => AskForApproval::OnRequest,
        code_core::protocol::AskForApproval::Never => AskForApproval::Never,
    }
}

fn core_sandbox_policy_to_v2(policy: code_core::protocol::SandboxPolicy) -> SandboxPolicy {
    match policy {
        code_core::protocol::SandboxPolicy::DangerFullAccess => SandboxPolicy::DangerFullAccess,
        code_core::protocol::SandboxPolicy::ReadOnly => SandboxPolicy::ReadOnly,
        code_core::protocol::SandboxPolicy::WorkspaceWrite {
            writable_roots,
            network_access,
            exclude_tmpdir_env_var,
            exclude_slash_tmp,
            ..
        } => SandboxPolicy::WorkspaceWrite {
            writable_roots: writable_roots
                .into_iter()
                .filter_map(|path| code_utils_absolute_path::AbsolutePathBuf::try_from(path).ok())
                .collect(),
            network_access,
            exclude_tmpdir_env_var,
            exclude_slash_tmp,
        },
    }
}

fn protocol_personality_to_core(
    personality: code_protocol::config_types::Personality,
) -> code_core::config_types::Personality {
    match personality {
        code_protocol::config_types::Personality::None => code_core::config_types::Personality::None,
        code_protocol::config_types::Personality::Friendly => {
            code_core::config_types::Personality::Friendly
        }
        code_protocol::config_types::Personality::Pragmatic => {
            code_core::config_types::Personality::Pragmatic
        }
    }
}

fn v2_dynamic_tool_spec_to_protocol(
    spec: code_app_server_protocol::DynamicToolSpec,
) -> code_protocol::dynamic_tools::DynamicToolSpec {
    code_protocol::dynamic_tools::DynamicToolSpec {
        name: spec.name,
        description: spec.description,
        input_schema: spec.input_schema,
    }
}

fn v2_user_input_to_core_input_item(
    input: UserInput,
) -> Result<code_core::protocol::InputItem, String> {
    match input {
        UserInput::Text { text, .. } => Ok(code_core::protocol::InputItem::Text { text }),
        UserInput::Image { url } => Ok(code_core::protocol::InputItem::Image { image_url: url }),
        UserInput::LocalImage { path } => Ok(code_core::protocol::InputItem::LocalImage { path }),
        UserInput::Skill { .. } => Err("skill inputs are not supported by turn/start yet".to_string()),
        UserInput::Mention { .. } => Err("mention inputs are not supported by turn/start yet".to_string()),
    }
}

fn parse_cursor_offset(
    cursor: Option<&str>,
    total: usize,
    label: &str,
) -> Result<usize, JSONRPCErrorError> {
    let start = match cursor {
        Some(cursor) => cursor.parse::<usize>().map_err(|_| JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message: format!("invalid cursor: {cursor}"),
            data: None,
        })?,
        None => 0,
    };

    if start > total {
        return Err(JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message: format!("cursor {start} exceeds total {label} {total}"),
            data: None,
        });
    }

    Ok(start)
}

fn model_preset_to_v2_model(preset: &model_presets::ModelPreset) -> Model {
    Model {
        id: preset.id.clone(),
        model: preset.model.clone(),
        upgrade: preset.upgrade.as_ref().map(|upgrade| upgrade.id.clone()),
        display_name: preset.display_name.clone(),
        description: preset.description.clone(),
        hidden: !preset.show_in_picker,
        supported_reasoning_efforts: preset
            .supported_reasoning_efforts
            .iter()
            .map(|effort| ReasoningEffortOption {
                reasoning_effort: map_core_reasoning_effort(effort.effort),
                description: effort.description.clone(),
            })
            .collect(),
        default_reasoning_effort: map_core_reasoning_effort(preset.default_reasoning_effort),
        input_modalities: code_protocol::openai_models::default_input_modalities(),
        supports_personality: true,
        is_default: preset.is_default,
    }
}

fn map_core_reasoning_effort(
    effort: code_core::protocol_config_types::ReasoningEffort,
) -> code_protocol::openai_models::ReasoningEffort {
    match effort {
        code_core::protocol_config_types::ReasoningEffort::None => {
            code_protocol::openai_models::ReasoningEffort::None
        }
        code_core::protocol_config_types::ReasoningEffort::Minimal => {
            code_protocol::openai_models::ReasoningEffort::Minimal
        }
        code_core::protocol_config_types::ReasoningEffort::Low => {
            code_protocol::openai_models::ReasoningEffort::Low
        }
        code_core::protocol_config_types::ReasoningEffort::Medium => {
            code_protocol::openai_models::ReasoningEffort::Medium
        }
        code_core::protocol_config_types::ReasoningEffort::High => {
            code_protocol::openai_models::ReasoningEffort::High
        }
        code_core::protocol_config_types::ReasoningEffort::XHigh => {
            code_protocol::openai_models::ReasoningEffort::XHigh
        }
    }
}

fn normalize_thread_source_filters(
    source_kinds: Option<&[ThreadSourceKind]>,
) -> Vec<ThreadSourceKind> {
    match source_kinds {
        Some(kinds) if !kinds.is_empty() => kinds.to_vec(),
        _ => vec![ThreadSourceKind::Cli, ThreadSourceKind::VsCode],
    }
}

fn session_source_to_thread_kind(source: &SessionSource) -> ThreadSourceKind {
    match source {
        SessionSource::Cli => ThreadSourceKind::Cli,
        SessionSource::VSCode => ThreadSourceKind::VsCode,
        SessionSource::Exec => ThreadSourceKind::Exec,
        SessionSource::Mcp => ThreadSourceKind::AppServer,
        SessionSource::SubAgent(kind) => match kind {
            SubAgentSource::Review => ThreadSourceKind::SubAgentReview,
            SubAgentSource::Compact => ThreadSourceKind::SubAgentCompact,
            SubAgentSource::ThreadSpawn { .. } => ThreadSourceKind::SubAgentThreadSpawn,
            SubAgentSource::Other(_) | SubAgentSource::MemoryConsolidation => {
                ThreadSourceKind::SubAgentOther
            }
        },
        SessionSource::Unknown => ThreadSourceKind::Unknown,
    }
}

fn session_entry_to_thread(
    entry: &SessionIndexEntry,
    code_home: &std::path::Path,
    turns: Vec<Turn>,
) -> Thread {
    let preview = entry
        .nickname
        .clone()
        .or_else(|| entry.last_user_snippet.clone())
        .unwrap_or_default();
    let path = entry_to_rollout_path(code_home, entry);

    Thread {
        id: entry.session_id.to_string(),
        preview,
        model_provider: entry
            .model_provider
            .clone()
            .unwrap_or_else(|| "openai".to_string()),
        created_at: parse_rfc3339_to_unix_seconds(&entry.created_at),
        updated_at: parse_rfc3339_to_unix_seconds(&entry.last_event_at),
        path: Some(path),
        cwd: entry.cwd_real.clone(),
        cli_version: env!("CARGO_PKG_VERSION").to_string(),
        source: entry.session_source.clone().into(),
        git_info: Some(code_app_server_protocol::GitInfo {
            sha: None,
            branch: entry.git_branch.clone(),
            origin_url: None,
        }),
        turns,
    }
}

fn parse_rfc3339_to_unix_seconds(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.timestamp())
        .unwrap_or(0)
}

async fn read_turns_from_rollout(path: &std::path::Path) -> std::io::Result<Vec<Turn>> {
    let contents = tokio::fs::read_to_string(path).await?;
    let mut turns = Vec::new();

    for (idx, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let rollout_line: RolloutLine = match serde_json::from_str(trimmed) {
            Ok(line) => line,
            Err(_) => continue,
        };
        let RolloutItem::ResponseItem(item) = rollout_line.item else {
            continue;
        };
        let Some(thread_item) = response_item_to_thread_item(item) else {
            continue;
        };

        turns.push(Turn {
            id: format!("turn-{}", idx + 1),
            items: vec![thread_item],
            status: TurnStatus::Completed,
            error: None,
        });
    }

    Ok(turns)
}

fn response_item_to_thread_item(item: ResponseItem) -> Option<ThreadItem> {
    match item {
        ResponseItem::Message {
            id, role, content, ..
        } => {
            let item_id = id.unwrap_or_else(|| Uuid::new_v4().to_string());
            if role.eq_ignore_ascii_case("user") {
                let mapped_content: Vec<UserInput> =
                    content.iter().filter_map(content_item_to_user_input).collect();
                Some(ThreadItem::UserMessage {
                    id: item_id,
                    content: mapped_content,
                })
            } else if role.eq_ignore_ascii_case("assistant") {
                let text = content
                    .into_iter()
                    .filter_map(|content_item| match content_item {
                        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                            Some(text)
                        }
                        ContentItem::InputImage { .. } => None,
                    })
                    .collect::<String>();
                Some(ThreadItem::AgentMessage { id: item_id, text })
            } else {
                None
            }
        }
        ResponseItem::Reasoning {
            id,
            summary,
            content,
            ..
        } => Some(ThreadItem::Reasoning {
            id,
            summary: summary
                .into_iter()
                .map(|summary_item| match summary_item {
                    ReasoningItemReasoningSummary::SummaryText { text } => text,
                })
                .collect(),
            content: content
                .unwrap_or_default()
                .into_iter()
                .map(|content_item| match content_item {
                    ReasoningItemContent::ReasoningText { text }
                    | ReasoningItemContent::Text { text } => text,
                })
                .collect(),
        }),
        _ => None,
    }
}

fn content_item_to_user_input(item: &ContentItem) -> Option<UserInput> {
    match item {
        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
            Some(UserInput::Text {
                text: text.clone(),
                text_elements: Vec::new(),
            })
        }
        ContentItem::InputImage { image_url } => Some(UserInput::Image {
            url: image_url.clone(),
        }),
    }
}
