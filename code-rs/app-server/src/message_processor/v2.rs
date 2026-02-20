use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use code_app_server_protocol::ListMcpServerStatusParams;
use code_app_server_protocol::ListMcpServerStatusResponse;
use code_app_server_protocol::McpServerStatus;
use code_app_server_protocol::Model;
use code_app_server_protocol::ModelListParams;
use code_app_server_protocol::ModelListResponse;
use code_app_server_protocol::ReasoningEffortOption;
use code_app_server_protocol::Thread;
use code_app_server_protocol::ThreadItem;
use code_app_server_protocol::ThreadListParams;
use code_app_server_protocol::ThreadListResponse;
use code_app_server_protocol::ThreadReadParams;
use code_app_server_protocol::ThreadReadResponse;
use code_app_server_protocol::ThreadSortKey;
use code_app_server_protocol::ThreadSourceKind;
use code_app_server_protocol::Turn;
use code_app_server_protocol::TurnStatus;
use code_app_server_protocol::UserInput;
use code_common::model_presets;
use code_core::SessionCatalog;
use code_core::SessionIndexEntry;
use code_core::SessionQuery;
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
