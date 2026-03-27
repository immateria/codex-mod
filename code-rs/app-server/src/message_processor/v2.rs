use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use code_app_server_protocol::AskForApproval;
use code_app_server_protocol::CommandExecParams;
use code_app_server_protocol::CommandExecResizeParams;
use code_app_server_protocol::CommandExecTerminateParams;
use code_app_server_protocol::CommandExecWriteParams;
use code_app_server_protocol::ListMcpServerStatusParams;
use code_app_server_protocol::ListMcpServerStatusResponse;
use code_app_server_protocol::McpServerOauthLoginCompletedNotification;
use code_app_server_protocol::McpServerOauthLoginParams;
use code_app_server_protocol::McpServerOauthLoginResponse;
use code_app_server_protocol::McpServerRefreshResponse;
use code_app_server_protocol::FsCopyParams;
use code_app_server_protocol::FsCopyResponse;
use code_app_server_protocol::FsCreateDirectoryParams;
use code_app_server_protocol::FsCreateDirectoryResponse;
use code_app_server_protocol::FsGetMetadataParams;
use code_app_server_protocol::FsGetMetadataResponse;
use code_app_server_protocol::FsUnwatchParams;
use code_app_server_protocol::FsWatchParams;
use code_app_server_protocol::FsReadDirectoryEntry;
use code_app_server_protocol::FsReadDirectoryParams;
use code_app_server_protocol::FsReadDirectoryResponse;
use code_app_server_protocol::FsReadFileParams;
use code_app_server_protocol::FsReadFileResponse;
use code_app_server_protocol::FsRemoveParams;
use code_app_server_protocol::FsRemoveResponse;
use code_app_server_protocol::FsWriteFileParams;
use code_app_server_protocol::FsWriteFileResponse;
use code_app_server_protocol::CommandAction;
use code_app_server_protocol::CommandExecutionOutputDeltaNotification;
use code_app_server_protocol::CommandExecutionStatus;
use code_app_server_protocol::ConfigLayerSource;
use code_app_server_protocol::McpServerStatus;
use code_app_server_protocol::Model;
use code_app_server_protocol::ModelListParams;
use code_app_server_protocol::ModelListResponse;
use code_app_server_protocol::ReasoningEffortOption;
use code_app_server_protocol::SandboxMode;
use code_app_server_protocol::SandboxPolicy;
use code_app_server_protocol::ServerNotification;
use code_app_server_protocol::SkillErrorInfo;
use code_app_server_protocol::SkillMetadata as V2SkillMetadata;
use code_app_server_protocol::SkillScope as V2SkillScope;
use code_app_server_protocol::SkillsChangedNotification;
use code_app_server_protocol::SkillsConfigWriteParams;
use code_app_server_protocol::SkillsConfigWriteResponse;
use code_app_server_protocol::SkillsListEntry;
use code_app_server_protocol::SkillsListParams;
use code_app_server_protocol::SkillsListResponse;
use code_app_server_protocol::AppInfo;
use code_app_server_protocol::AppListUpdatedNotification;
use code_app_server_protocol::AppSummary;
use code_app_server_protocol::AppsListParams;
use code_app_server_protocol::AppsListResponse;
use code_app_server_protocol::MarketplaceInterface as V2MarketplaceInterface;
use code_app_server_protocol::MarketplaceLoadErrorInfo;
use code_app_server_protocol::PluginDetail as V2PluginDetail;
use code_app_server_protocol::PluginInstallParams;
use code_app_server_protocol::PluginInstallResponse;
use code_app_server_protocol::PluginInterface as V2PluginInterface;
use code_app_server_protocol::PluginListParams;
use code_app_server_protocol::PluginListResponse;
use code_app_server_protocol::PluginMarketplaceEntry;
use code_app_server_protocol::PluginReadParams;
use code_app_server_protocol::PluginReadResponse;
use code_app_server_protocol::PluginSource;
use code_app_server_protocol::PluginSummary;
use code_app_server_protocol::PluginUninstallParams;
use code_app_server_protocol::PluginUninstallResponse;
use code_app_server_protocol::SkillSummary as V2SkillSummary;
use code_app_server_protocol::Thread;
use code_app_server_protocol::ThreadItem;
use code_app_server_protocol::ThreadListParams;
use code_app_server_protocol::ThreadListResponse;
use code_app_server_protocol::ThreadLoadedListParams;
use code_app_server_protocol::ThreadLoadedListResponse;
use code_app_server_protocol::ThreadStartedNotification;
use code_app_server_protocol::ThreadArchivedNotification;
use code_app_server_protocol::ThreadStartParams;
use code_app_server_protocol::ThreadStartResponse;
use code_app_server_protocol::ThreadResumeParams;
use code_app_server_protocol::ThreadResumeResponse;
use code_app_server_protocol::ThreadForkParams;
use code_app_server_protocol::ThreadForkResponse;
use code_app_server_protocol::ThreadArchiveParams;
use code_app_server_protocol::ThreadArchiveResponse;
use code_app_server_protocol::ThreadUnsubscribeParams;
use code_app_server_protocol::ThreadUnsubscribeResponse;
use code_app_server_protocol::ThreadUnsubscribeStatus;
use code_app_server_protocol::ThreadSetNameParams;
use code_app_server_protocol::ThreadSetNameResponse;
use code_app_server_protocol::ThreadNameUpdatedNotification;
use code_app_server_protocol::ThreadMetadataUpdateParams;
use code_app_server_protocol::ThreadMetadataUpdateResponse;
use code_app_server_protocol::ThreadUnarchiveParams;
use code_app_server_protocol::ThreadUnarchiveResponse;
use code_app_server_protocol::ThreadUnarchivedNotification;
use code_app_server_protocol::ThreadReadParams;
use code_app_server_protocol::ThreadReadResponse;
use code_app_server_protocol::ThreadSortKey;
use code_app_server_protocol::ThreadSourceKind;
use code_app_server_protocol::Turn;
use code_app_server_protocol::TurnCompletedNotification;
use code_app_server_protocol::TurnStartParams;
use code_app_server_protocol::TurnStartResponse;
use code_app_server_protocol::TurnStartedNotification;
use code_app_server_protocol::TurnInterruptParams;
use code_app_server_protocol::TurnInterruptResponse;
use code_app_server_protocol::TurnStatus;
use code_app_server_protocol::UserInput;
use code_app_server_protocol::WindowsSandboxSetupCompletedNotification;
use code_app_server_protocol::WindowsSandboxSetupMode;
use code_app_server_protocol::WindowsSandboxSetupStartParams;
use code_app_server_protocol::WindowsSandboxSetupStartResponse;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::Utc;
use code_common::model_presets;
use code_chatgpt::connectors as chatgpt_connectors;
use code_core::SessionCatalog;
use code_core::SessionIndexEntry;
use code_core::SessionQuery;
use code_core::config::Config;
use code_core::config::ConfigBuilder;
use code_core::config::ConfigOverrides;
use code_core::exec::ExecExpiration;
use code_core::entry_to_rollout_path;
use code_core::mcp_connection_manager::McpConnectionManager;
use code_core::mcp_snapshot::collect_runtime_snapshot;
use code_core::mcp_snapshot::format_failure_summary;
use code_core::mcp_snapshot::format_transport_summary;
use code_core::mcp_snapshot::group_tool_definitions_by_server;
use code_core::mcp_snapshot::merge_servers;
use code_core::plugins::PluginInstallRequest;
use code_core::plugins::PluginReadRequest;
use code_core::plugins::PluginsManager;
use code_protocol::mcp::Tool as ProtocolMcpTool;
use code_protocol::models::ContentItem;
use code_protocol::models::ReasoningItemContent;
use code_protocol::models::ReasoningItemReasoningSummary;
use code_protocol::models::ResponseItem;
use code_protocol::models::SandboxPermissions;
use code_protocol::protocol::RolloutItem;
use code_protocol::protocol::RolloutLine;
use code_protocol::protocol::SessionSource;
use code_protocol::protocol::SubAgentSource;
use code_rmcp_client::OauthLoginArgs;
use code_rmcp_client::perform_oauth_login_return_url;
use mcp_types::JSONRPCErrorError;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::MessageProcessor;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_PARAMS_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::ConnectionRequestId;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::OutgoingMessageSender;
use crate::outgoing_message::OutgoingNotification;
use crate::thread_state::ThreadState;

mod status_conversion;

use status_conversion::convert_mcp_resource_templates;
use status_conversion::convert_mcp_resources;
use status_conversion::convert_mcp_tool;

impl MessageProcessor {
    pub(super) async fn command_exec_start(
        &self,
        request_id: ConnectionRequestId,
        params: CommandExecParams,
    ) {
        let CommandExecParams {
            command,
            process_id,
            tty,
            stream_stdin,
            stream_stdout_stderr,
            output_bytes_cap,
            disable_output_cap,
            disable_timeout,
            timeout_ms,
            cwd,
            env,
            size,
            sandbox_policy,
        } = params;

        if command.is_empty() {
            self.outgoing
                .send_error_to_connection(
                    request_id.connection_id,
                    request_id.request_id,
                    JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: "command must not be empty".to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        }

        if size.is_some() && !tty {
            self.outgoing
                .send_error_to_connection(
                    request_id.connection_id,
                    request_id.request_id,
                    JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: "command/exec size requires tty=true".to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        }

        if disable_timeout && timeout_ms.is_some() {
            self.outgoing
                .send_error_to_connection(
                    request_id.connection_id,
                    request_id.request_id,
                    JSONRPCErrorError {
                        code: INVALID_PARAMS_ERROR_CODE,
                        message: "command/exec disableTimeout and timeoutMs are mutually exclusive"
                            .to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        }

        if let Some(timeout_ms) = timeout_ms
            && timeout_ms < 0
        {
            self.outgoing
                .send_error_to_connection(
                    request_id.connection_id,
                    request_id.request_id,
                    JSONRPCErrorError {
                        code: INVALID_PARAMS_ERROR_CODE,
                        message: "command/exec timeoutMs must be non-negative".to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        }

        if disable_output_cap && output_bytes_cap.is_some() {
            self.outgoing
                .send_error_to_connection(
                    request_id.connection_id,
                    request_id.request_id,
                    JSONRPCErrorError {
                        code: INVALID_PARAMS_ERROR_CODE,
                        message:
                            "command/exec disableOutputCap and outputBytesCap are mutually exclusive"
                                .to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        }

        let requested_cwd = cwd.unwrap_or_else(|| self.base_config.cwd.clone());
        let config = match ConfigBuilder::new()
            .with_code_home(self.base_config.code_home.clone())
            .with_cli_overrides(self.cli_overrides.clone())
            .with_overrides(ConfigOverrides {
                cwd: Some(requested_cwd.clone()),
                code_linux_sandbox_exe: self.code_linux_sandbox_exe.clone(),
                ..Default::default()
            })
            .with_loader_overrides(code_core::config_loader::LoaderOverrides::default())
            .load()
        {
            Ok(config) => config,
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        request_id.connection_id,
                        request_id.request_id,
                        JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("failed to load command/exec config: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let effective_policy = sandbox_policy
            .as_ref()
            .map(SandboxPolicy::to_core)
            .unwrap_or_else(|| code_core::sandboxing::protocol_policy_from_local(&config.sandbox_policy));
        let local_policy = code_core::sandboxing::local_policy_from_protocol(&effective_policy);

        let mut env_map = code_core::exec_env::create_env(&config.shell_environment_policy);
        if let Some(env_overrides) = env {
            for (key, value) in env_overrides {
                match value {
                    Some(value) => {
                        env_map.insert(key, value);
                    }
                    None => {
                        env_map.remove(&key);
                    }
                }
            }
        }

        let started_network_proxy = match config.network_proxy.as_ref() {
            Some(proxy_spec) => match proxy_spec
                .start_proxy(&local_policy, None, None, false)
                .await
            {
                Ok(proxy) => Some(proxy),
                Err(err) => {
                    self.outgoing
                        .send_error_to_connection(
                            request_id.connection_id,
                            request_id.request_id,
                            JSONRPCErrorError {
                                code: INTERNAL_ERROR_CODE,
                                message: format!("failed to start network proxy: {err}"),
                                data: None,
                            },
                        )
                        .await;
                    return;
                }
            },
            None => None,
        };

        let size = match size
            .map(crate::command_exec::terminal_size_from_protocol)
            .transpose()
        {
            Ok(size) => size,
            Err(error) => {
                self.outgoing
                    .send_error_to_connection(request_id.connection_id, request_id.request_id, error)
                    .await;
                return;
            }
        };

        let expiration = if disable_timeout {
            ExecExpiration::Cancellation(CancellationToken::new())
        } else if let Some(timeout_ms) = timeout_ms {
            ExecExpiration::Timeout(std::time::Duration::from_millis(timeout_ms as u64))
        } else {
            ExecExpiration::DefaultTimeout
        };

        let output_bytes_cap = if disable_output_cap {
            None
        } else {
            Some(output_bytes_cap.unwrap_or(code_utils_pty::DEFAULT_OUTPUT_BYTES_CAP))
        };

        let network = started_network_proxy
            .as_ref()
            .map(code_core::config::network_proxy_spec::StartedNetworkProxy::proxy);
        let exec_request = match code_core::sandboxing::build_exec_request(
            code_core::sandboxing::BuildExecRequestParams {
                command,
                cwd: requested_cwd,
                env: env_map,
                network,
                expiration,
                sandbox_permissions: SandboxPermissions::UseDefault,
                windows_sandbox_level: config.windows_sandbox_level,
                justification: None,
                sandbox_policy: effective_policy,
                sandbox_policy_cwd: config.cwd.clone(),
                code_linux_sandbox_exe: config.code_linux_sandbox_exe.clone(),
            },
        ) {
            Ok(exec_request) => exec_request,
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        request_id.connection_id,
                        request_id.request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("exec failed: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        if let Err(error) = self
            .command_exec_manager
            .start(crate::command_exec::StartCommandExecParams {
                outgoing: Arc::clone(&self.outgoing),
                request_id: request_id.clone(),
                process_id,
                exec_request,
                started_network_proxy,
                tty,
                stream_stdin,
                stream_stdout_stderr,
                output_bytes_cap,
                size,
            })
            .await
        {
            self.outgoing
                .send_error_to_connection(request_id.connection_id, request_id.request_id, error)
                .await;
        }
    }

    pub(super) async fn command_exec_write(
        &self,
        request_id: ConnectionRequestId,
        params: CommandExecWriteParams,
    ) {
        match self.command_exec_manager.write(request_id.clone(), params).await {
            Ok(response) => self
                .outgoing
                .send_response_to_connection(
                    request_id.connection_id,
                    request_id.request_id,
                    response,
                )
                .await,
            Err(error) => {
                self.outgoing
                    .send_error_to_connection(
                        request_id.connection_id,
                        request_id.request_id,
                        error,
                    )
                    .await
            }
        }
    }

    pub(super) async fn command_exec_resize(
        &self,
        request_id: ConnectionRequestId,
        params: CommandExecResizeParams,
    ) {
        match self.command_exec_manager.resize(request_id.clone(), params).await {
            Ok(response) => self
                .outgoing
                .send_response_to_connection(
                    request_id.connection_id,
                    request_id.request_id,
                    response,
                )
                .await,
            Err(error) => {
                self.outgoing
                    .send_error_to_connection(
                        request_id.connection_id,
                        request_id.request_id,
                        error,
                    )
                    .await
            }
        }
    }

    pub(super) async fn command_exec_terminate(
        &self,
        request_id: ConnectionRequestId,
        params: CommandExecTerminateParams,
    ) {
        match self
            .command_exec_manager
            .terminate(request_id.clone(), params)
            .await
        {
            Ok(response) => self
                .outgoing
                .send_response_to_connection(
                    request_id.connection_id,
                    request_id.request_id,
                    response,
                )
                .await,
            Err(error) => {
                self.outgoing
                    .send_error_to_connection(
                        request_id.connection_id,
                        request_id.request_id,
                        error,
                    )
                    .await
            }
        }
    }

    pub(super) async fn windows_sandbox_setup_start(
        &self,
        request_id: ConnectionRequestId,
        params: WindowsSandboxSetupStartParams,
    ) {
        self.outgoing
            .send_response_to_connection(
                request_id.connection_id,
                request_id.request_id.clone(),
                WindowsSandboxSetupStartResponse { started: true },
            )
            .await;

        let mode = match params.mode {
            WindowsSandboxSetupMode::Elevated => {
                code_core::windows_sandbox::WindowsSandboxSetupMode::Elevated
            }
            WindowsSandboxSetupMode::Unelevated => {
                code_core::windows_sandbox::WindowsSandboxSetupMode::Unelevated
            }
        };
        let command_cwd = params
            .cwd
            .map(PathBuf::from)
            .unwrap_or_else(|| self.base_config.cwd.clone());
        let connection_id = request_id.connection_id;
        let outgoing = Arc::clone(&self.outgoing);
        let code_home = self.base_config.code_home.clone();
        let cli_overrides = self.cli_overrides.clone();
        let code_linux_sandbox_exe = self.code_linux_sandbox_exe.clone();

        tokio::spawn(async move {
            let derived_config = ConfigBuilder::new()
                .with_code_home(code_home)
                .with_cli_overrides(cli_overrides)
                .with_overrides(ConfigOverrides {
                    cwd: Some(command_cwd.clone()),
                    code_linux_sandbox_exe,
                    ..Default::default()
                })
                .with_loader_overrides(code_core::config_loader::LoaderOverrides::default())
                .load();
            let setup_result = match derived_config {
                Ok(config) => {
                    let setup_request = code_core::windows_sandbox::WindowsSandboxSetupRequest {
                        mode,
                        policy: config.sandbox_policy.clone(),
                        policy_cwd: config.cwd.clone(),
                        command_cwd,
                        env_map: std::env::vars().collect(),
                        codex_home: config.code_home.clone(),
                        active_profile: config.active_profile.clone(),
                    };
                    code_core::windows_sandbox::run_windows_sandbox_setup(setup_request).await
                }
                Err(err) => Err(err.into()),
            };
            send_server_notification_to_connection(
                outgoing.as_ref(),
                connection_id,
                ServerNotification::WindowsSandboxSetupCompleted(
                    WindowsSandboxSetupCompletedNotification {
                        mode: match mode {
                            code_core::windows_sandbox::WindowsSandboxSetupMode::Elevated => {
                                WindowsSandboxSetupMode::Elevated
                            }
                            code_core::windows_sandbox::WindowsSandboxSetupMode::Unelevated => {
                                WindowsSandboxSetupMode::Unelevated
                            }
                        },
                        success: setup_result.is_ok(),
                        error: setup_result.err().map(|err| err.to_string()),
                    },
                ),
            )
            .await;
        });
    }

    pub(super) async fn fs_read_file_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: FsReadFileParams,
    ) {
        match tokio::fs::read(params.path.as_path()).await {
            Ok(bytes) => {
                let response = FsReadFileResponse {
                    data_base64: STANDARD.encode(bytes),
                };
                self.outgoing
                    .send_response_to_connection(connection_id, request_id, response)
                    .await;
            }
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, map_fs_error(err))
                    .await;
            }
        }
    }

    pub(super) async fn fs_write_file_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: FsWriteFileParams,
    ) {
        let bytes = match STANDARD.decode(params.data_base64) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!(
                                "fs/writeFile requires valid base64 dataBase64: {err}"
                            ),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        match tokio::fs::write(params.path.as_path(), bytes).await {
            Ok(()) => {
                self.outgoing
                    .send_response_to_connection(connection_id, request_id, FsWriteFileResponse {})
                    .await;
            }
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, map_fs_error(err))
                    .await;
            }
        }
    }

    pub(super) async fn fs_create_directory_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: FsCreateDirectoryParams,
    ) {
        let recursive = params.recursive.unwrap_or(true);
        let result = if recursive {
            tokio::fs::create_dir_all(params.path.as_path()).await
        } else {
            tokio::fs::create_dir(params.path.as_path()).await
        };

        match result {
            Ok(()) => {
                self.outgoing
                    .send_response_to_connection(
                        connection_id,
                        request_id,
                        FsCreateDirectoryResponse {},
                    )
                    .await;
            }
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, map_fs_error(err))
                    .await;
            }
        }
    }

    pub(super) async fn fs_get_metadata_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: FsGetMetadataParams,
    ) {
        match tokio::fs::metadata(params.path.as_path()).await {
            Ok(metadata) => {
                let created_at_ms = metadata
                    .created()
                    .ok()
                    .and_then(system_time_to_unix_ms)
                    .unwrap_or(0);
                let modified_at_ms = metadata
                    .modified()
                    .ok()
                    .and_then(system_time_to_unix_ms)
                    .unwrap_or(0);
                let response = FsGetMetadataResponse {
                    is_directory: metadata.is_dir(),
                    is_file: metadata.is_file(),
                    created_at_ms,
                    modified_at_ms,
                };
                self.outgoing
                    .send_response_to_connection(connection_id, request_id, response)
                    .await;
            }
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, map_fs_error(err))
                    .await;
            }
        }
    }

    pub(super) async fn fs_read_directory_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: FsReadDirectoryParams,
    ) {
        let mut entries_out = Vec::new();
        match tokio::fs::read_dir(params.path.as_path()).await {
            Ok(mut entries) => loop {
                match entries.next_entry().await {
                    Ok(Some(entry)) => {
                        let file_name = entry.file_name().to_string_lossy().to_string();
                        let file_type = match entry.file_type().await {
                            Ok(file_type) => file_type,
                            Err(err) => {
                                self.outgoing
                                    .send_error_to_connection(
                                        connection_id,
                                        request_id,
                                        map_fs_error(err),
                                    )
                                    .await;
                                return;
                            }
                        };
                        entries_out.push(FsReadDirectoryEntry {
                            file_name,
                            is_directory: file_type.is_dir(),
                            is_file: file_type.is_file(),
                        });
                    }
                    Ok(None) => break,
                    Err(err) => {
                        self.outgoing
                            .send_error_to_connection(connection_id, request_id, map_fs_error(err))
                            .await;
                        return;
                    }
                }
            },
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, map_fs_error(err))
                    .await;
                return;
            }
        }

        self.outgoing
            .send_response_to_connection(
                connection_id,
                request_id,
                FsReadDirectoryResponse { entries: entries_out },
            )
            .await;
    }

    pub(super) async fn fs_remove_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: FsRemoveParams,
    ) {
        let recursive = params.recursive.unwrap_or(true);
        let force = params.force.unwrap_or(true);

        let path = params.path.as_path();
        let metadata = tokio::fs::metadata(path).await;
        let result = match metadata {
            Ok(metadata) => {
                if metadata.is_dir() {
                    if recursive {
                        tokio::fs::remove_dir_all(path).await
                    } else {
                        tokio::fs::remove_dir(path).await
                    }
                } else {
                    tokio::fs::remove_file(path).await
                }
            }
            Err(err) if force && err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        };

        match result {
            Ok(()) => {
                self.outgoing
                    .send_response_to_connection(connection_id, request_id, FsRemoveResponse {})
                    .await;
            }
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, map_fs_error(err))
                    .await;
            }
        }
    }

    pub(super) async fn fs_copy_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: FsCopyParams,
    ) {
        let result = if params.recursive {
            copy_dir_recursive(
                params.source_path.as_path(),
                params.destination_path.as_path(),
            )
            .await
        } else {
            tokio::fs::copy(params.source_path.as_path(), params.destination_path.as_path())
                .await
                .map(|_| ())
        };

        match result {
            Ok(()) => {
                self.outgoing
                    .send_response_to_connection(connection_id, request_id, FsCopyResponse {})
                    .await;
            }
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, map_fs_error(err))
                    .await;
            }
        }
    }

    pub(super) async fn fs_watch_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: FsWatchParams,
    ) {
        match self.fs_watch_manager.watch(connection_id, params).await {
            Ok(response) => {
                self.outgoing
                    .send_response_to_connection(connection_id, request_id, response)
                    .await;
            }
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, err)
                    .await;
            }
        }
    }

    pub(super) async fn fs_unwatch_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: FsUnwatchParams,
    ) {
        match self.fs_watch_manager.unwatch(connection_id, params).await {
            Ok(response) => {
                self.outgoing
                    .send_response_to_connection(connection_id, request_id, response)
                    .await;
            }
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, err)
                    .await;
            }
        }
    }

    pub(super) async fn list_models_v2(
        &self,
        connection_id: ConnectionId,
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
                .send_response_to_connection(
                    connection_id,
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
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, error)
                    .await;
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
            .send_response_to_connection(
                connection_id,
                request_id,
                ModelListResponse { data, next_cursor },
            )
            .await;
    }

    pub(super) async fn skills_list_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: SkillsListParams,
    ) {
        let SkillsListParams {
            cwds,
            force_reload: _,
            per_cwd_extra_user_roots,
        } = params;

        let requested_cwds = if cwds.is_empty() {
            vec![self.base_config.cwd.clone()]
        } else {
            cwds
        };

        let per_cwd_extra_user_roots = per_cwd_extra_user_roots.unwrap_or_default();

        let mut data = Vec::new();
        for cwd in requested_cwds {
            let mut entry_errors = Vec::new();

            let extra_user_roots: Vec<PathBuf> = per_cwd_extra_user_roots
                .iter()
                .find(|entry| entry.cwd == cwd)
                .map(|entry| entry.extra_user_roots.clone())
                .unwrap_or_default();

            let mut validated_extra_roots = Vec::new();
            for root in extra_user_roots {
                if !root.is_absolute() {
                    entry_errors.push(SkillErrorInfo {
                        path: root.clone(),
                        message: "extra_user_roots entries must be absolute paths".to_string(),
                    });
                    continue;
                }
                validated_extra_roots.push(root);
            }

            let config = match ConfigBuilder::new()
                .with_code_home(self.base_config.code_home.clone())
                .with_cli_overrides(self.cli_overrides.clone())
                .with_overrides(ConfigOverrides {
                    cwd: Some(cwd.clone()),
                    code_linux_sandbox_exe: self.code_linux_sandbox_exe.clone(),
                    ..Default::default()
                })
                .with_loader_overrides(code_core::config_loader::LoaderOverrides::default())
                .load()
            {
                Ok(config) => config,
                Err(err) => {
                    entry_errors.push(SkillErrorInfo {
                        path: cwd.clone(),
                        message: format!("failed to load effective config: {err}"),
                    });
                    data.push(SkillsListEntry {
                        cwd,
                        skills: Vec::new(),
                        errors: entry_errors,
                    });
                    continue;
                }
            };

            let disabled_paths = match code_core::config_loader::load_config_layers_state_with_cwd(
                &self.base_config.code_home,
                Some(&cwd),
                &self.cli_overrides,
                code_core::config_loader::LoaderOverrides::default(),
            )
            .await
            {
                Ok(stack) => disabled_skill_paths_from_stack(&stack),
                Err(err) => {
                    entry_errors.push(SkillErrorInfo {
                        path: cwd.clone(),
                        message: format!("failed to load config layers: {err}"),
                    });
                    HashSet::new()
                }
            };

            let outcome = if validated_extra_roots.is_empty() {
                code_core::skills::loader::load_skills(&config)
            } else {
                code_core::skills::loader::load_skills_with_extra_user_roots(
                    &config,
                    validated_extra_roots,
                )
            };

            let mut skills = Vec::new();
            for skill in outcome.skills {
                let enabled = !disabled_paths.contains(&skill.path);
                skills.push(V2SkillMetadata {
                    name: skill.name,
                    description: skill.description,
                    short_description: None,
                    interface: None,
                    dependencies: None,
                    path: skill.path,
                    scope: core_skill_scope_to_v2(skill.scope),
                    enabled,
                });
            }

            let mut errors = entry_errors;
            for err in outcome.errors {
                errors.push(SkillErrorInfo {
                    path: err.path,
                    message: err.message,
                });
            }

            data.push(SkillsListEntry { cwd, skills, errors });
        }

        self.outgoing
            .send_response_to_connection(connection_id, request_id, SkillsListResponse { data })
            .await;
    }

    pub(super) async fn skills_config_write_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: SkillsConfigWriteParams,
    ) {
        let SkillsConfigWriteParams { path, enabled } = params;

        if path.as_os_str().is_empty() {
            self.outgoing
                .send_error_to_connection(
                    connection_id,
                    request_id,
                    JSONRPCErrorError {
                        code: INVALID_PARAMS_ERROR_CODE,
                        message: "skills/config/write path must not be empty".to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        }

        if !path.is_absolute() {
            self.outgoing
                .send_error_to_connection(
                    connection_id,
                    request_id,
                    JSONRPCErrorError {
                        code: INVALID_PARAMS_ERROR_CODE,
                        message: "skills/config/write path must be an absolute path".to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        }

        let mutated =
            match code_core::config_edit::set_skill_config(&self.base_config.code_home, &path, enabled)
                .await
            {
                Ok(mutated) => mutated,
                Err(err) => {
                    self.outgoing
                        .send_error_to_connection(
                            connection_id,
                            request_id,
                            JSONRPCErrorError {
                                code: INTERNAL_ERROR_CODE,
                                message: format!("failed to update skills config: {err}"),
                                data: None,
                            },
                        )
                        .await;
                    return;
                }
            };

        self.outgoing
            .send_response_to_connection(
                connection_id,
                request_id,
                SkillsConfigWriteResponse {
                    effective_enabled: enabled,
                },
            )
            .await;

        if mutated {
            broadcast_server_notification_simple(
                &self.outgoing,
                ServerNotification::SkillsChanged(SkillsChangedNotification {}),
            )
            .await;
        }
    }

    pub(super) async fn plugin_list_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: PluginListParams,
    ) {
        let PluginListParams {
            cwds,
            force_remote_sync,
        } = params;

        let manager = PluginsManager::new(self.base_config.code_home.clone());
        let auth = self.auth_manager.auth();
        let roots = cwds.unwrap_or_default();

        let mut remote_sync_error = None;
        if force_remote_sync {
            if let Err(err) = manager.sync_marketplace_sources(&self.base_config).await {
                remote_sync_error = Some(err);
            }
            match manager
                .sync_plugins_from_remote(&self.base_config, auth.as_ref(), /*additive_only*/ false)
                .await
            {
                Ok(sync_result) => {
                    tracing::info!(
                        installed_plugin_ids = ?sync_result.installed_plugin_ids,
                        enabled_plugin_ids = ?sync_result.enabled_plugin_ids,
                        disabled_plugin_ids = ?sync_result.disabled_plugin_ids,
                        uninstalled_plugin_ids = ?sync_result.uninstalled_plugin_ids,
                        "completed plugin/list remote sync"
                    );
                }
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        "plugin/list remote sync failed; returning local marketplace state"
                    );
                    match remote_sync_error.as_mut() {
                        Some(existing) => {
                            existing.push_str("; ");
                            existing.push_str(&err.to_string());
                        }
                        None => remote_sync_error = Some(err.to_string()),
                    }
                }
            }
        }

        let marketplaces_outcome = match manager.list_marketplaces_for_roots(&self.base_config, &roots) {
            Ok(outcome) => outcome,
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("failed to list marketplaces: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let marketplaces: Vec<PluginMarketplaceEntry> = marketplaces_outcome
            .marketplaces
            .into_iter()
            .map(configured_marketplace_to_v2)
            .collect();

        let marketplace_load_errors = marketplaces_outcome
            .errors
            .into_iter()
            .map(|err| MarketplaceLoadErrorInfo {
                marketplace_path: err.path,
                message: err.message,
            })
            .collect::<Vec<_>>();

        let featured_plugin_ids =
            if marketplaces.iter().any(|marketplace| {
                marketplace.name == code_core::plugins::OPENAI_CURATED_MARKETPLACE_NAME
            }) {
                match manager
                    .featured_plugin_ids_for_config(&self.base_config, auth.as_ref())
                    .await
                {
                    Ok(featured_plugin_ids) => featured_plugin_ids,
                    Err(err) => {
                        tracing::warn!(
                            error = %err,
                            "plugin/list featured plugin fetch failed; returning empty featured ids"
                        );
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            };

        self.outgoing
            .send_response_to_connection(
                connection_id,
                request_id,
                PluginListResponse {
                    marketplaces,
                    marketplace_load_errors,
                    remote_sync_error,
                    featured_plugin_ids,
                },
            )
            .await;
    }

    pub(super) async fn plugin_read_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: PluginReadParams,
    ) {
        let PluginReadParams {
            marketplace_path,
            plugin_name,
        } = params;

        if plugin_name.trim().is_empty() {
            self.outgoing
                .send_error_to_connection(
                    connection_id,
                    request_id,
                    JSONRPCErrorError {
                        code: INVALID_PARAMS_ERROR_CODE,
                        message: "plugin/read plugin_name must not be empty".to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        }

        let manager = PluginsManager::new(self.base_config.code_home.clone());
        let outcome = match manager.read_plugin_for_config(&PluginReadRequest {
            plugin_name,
            marketplace_path,
        }) {
            Ok(outcome) => outcome,
            Err(err) => {
                let (code, message) = match &err {
                    code_core::plugins::MarketplaceError::PluginNotFound { .. } => {
                        (INVALID_PARAMS_ERROR_CODE, err.to_string())
                    }
                    _ => (INTERNAL_ERROR_CODE, format!("failed to read plugin: {err}")),
                };
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code,
                            message,
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let plugin = plugin_read_outcome_to_v2(outcome);
        self.outgoing
            .send_response_to_connection(
                connection_id,
                request_id,
                PluginReadResponse { plugin },
            )
            .await;
    }

    pub(super) async fn plugin_install_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: PluginInstallParams,
    ) {
        let PluginInstallParams {
            marketplace_path,
            plugin_name,
            force_remote_sync,
        } = params;

        if plugin_name.trim().is_empty() {
            self.outgoing
                .send_error_to_connection(
                    connection_id,
                    request_id,
                    JSONRPCErrorError {
                        code: INVALID_PARAMS_ERROR_CODE,
                        message: "plugin/install plugin_name must not be empty".to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        }

        let manager = PluginsManager::new(self.base_config.code_home.clone());
        let auth = self.auth_manager.auth();
        let request = PluginInstallRequest {
            plugin_name,
            marketplace_path,
        };
        let outcome = match if force_remote_sync {
            manager
                .install_plugin_with_remote_sync(&self.base_config, auth.as_ref(), request)
                .await
        } else {
            manager.install_plugin(request).await
        } {
            Ok(outcome) => outcome,
            Err(err) => {
                let code = if err.is_invalid_request() {
                    INVALID_PARAMS_ERROR_CODE
                } else {
                    INTERNAL_ERROR_CODE
                };
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code,
                            message: err.to_string(),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        self.outgoing
            .send_response_to_connection(
                connection_id,
                request_id,
                PluginInstallResponse {
                    auth_policy: outcome.auth_policy.into(),
                    apps_needing_auth: Vec::new(),
                },
            )
            .await;

        let apps: Vec<AppInfo> = manager
            .effective_apps()
            .into_iter()
            .map(|code_core::plugins::AppConnectorId(id)| AppInfo {
                id: id.clone(),
                name: id,
                description: None,
                logo_url: None,
                logo_url_dark: None,
                distribution_channel: None,
                branding: None,
                app_metadata: None,
                labels: None,
                install_url: None,
                is_accessible: true,
                is_enabled: true,
                plugin_display_names: Vec::new(),
            })
            .collect();
        broadcast_server_notification_simple(
            &self.outgoing,
            ServerNotification::AppListUpdated(AppListUpdatedNotification { data: apps }),
        )
        .await;
        broadcast_server_notification_simple(
            &self.outgoing,
            ServerNotification::SkillsChanged(SkillsChangedNotification {}),
        )
        .await;
    }

    pub(super) async fn plugin_uninstall_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: PluginUninstallParams,
    ) {
        let PluginUninstallParams {
            plugin_id,
            force_remote_sync,
        } = params;

        if plugin_id.trim().is_empty() {
            self.outgoing
                .send_error_to_connection(
                    connection_id,
                    request_id,
                    JSONRPCErrorError {
                        code: INVALID_PARAMS_ERROR_CODE,
                        message: "plugin/uninstall plugin_id must not be empty".to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        }

        let manager = PluginsManager::new(self.base_config.code_home.clone());
        let auth = self.auth_manager.auth();
        let uninstall_result = if force_remote_sync {
            manager
                .uninstall_plugin_with_remote_sync(&self.base_config, auth.as_ref(), plugin_id)
                .await
        } else {
            manager.uninstall_plugin(plugin_id).await
        };
        if let Err(err) = uninstall_result {
            let code = if err.is_invalid_request() {
                INVALID_PARAMS_ERROR_CODE
            } else {
                INTERNAL_ERROR_CODE
            };
            self.outgoing
                .send_error_to_connection(
                    connection_id,
                    request_id,
                    JSONRPCErrorError {
                        code,
                        message: err.to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        }

        self.outgoing
            .send_response_to_connection(
                connection_id,
                request_id,
                PluginUninstallResponse {},
            )
            .await;

        let apps: Vec<AppInfo> = manager
            .effective_apps()
            .into_iter()
            .map(|code_core::plugins::AppConnectorId(id)| AppInfo {
                id: id.clone(),
                name: id,
                description: None,
                logo_url: None,
                logo_url_dark: None,
                distribution_channel: None,
                branding: None,
                app_metadata: None,
                labels: None,
                install_url: None,
                is_accessible: true,
                is_enabled: true,
                plugin_display_names: Vec::new(),
            })
            .collect();
        broadcast_server_notification_simple(
            &self.outgoing,
            ServerNotification::AppListUpdated(AppListUpdatedNotification { data: apps }),
        )
        .await;
        broadcast_server_notification_simple(
            &self.outgoing,
            ServerNotification::SkillsChanged(SkillsChangedNotification {}),
        )
        .await;
    }

    pub(super) async fn apps_list_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: AppsListParams,
    ) {
        let AppsListParams {
            cursor,
            limit,
            thread_id,
            force_refetch,
        } = params;

        let config = if let Some(thread_id) = thread_id.as_deref() {
            let catalog = SessionCatalog::new(self.base_config.code_home.clone());
            let entry = match catalog.find_by_id(thread_id).await {
                Ok(Some(entry)) => entry,
                Ok(None) => {
                    self.outgoing
                        .send_error_to_connection(
                            connection_id,
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
                        .send_error_to_connection(
                            connection_id,
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
            let cwd_override = entry.cwd_real.to_string_lossy().to_string();
            match self.load_effective_config(Some(&cwd_override)) {
                Ok(config) => config,
                Err(error) => {
                    self.outgoing
                        .send_error_to_connection(connection_id, request_id, error)
                        .await;
                    return;
                }
            }
        } else {
            match self.load_effective_config(/*cwd*/ None) {
                Ok(config) => config,
                Err(error) => {
                    self.outgoing
                        .send_error_to_connection(connection_id, request_id, error)
                        .await;
                    return;
                }
            }
        };

        if !config.features_effective.enabled("apps") {
            self.outgoing
                .send_response_to_connection(
                    connection_id,
                    request_id,
                    AppsListResponse {
                        data: Vec::new(),
                        next_cursor: None,
                    },
                )
                .await;
            return;
        }

        let directory_connectors =
            match chatgpt_connectors::list_all_connectors_with_options(&config, force_refetch).await
            {
                Ok(connectors) => connectors,
                Err(err) => {
                    tracing::warn!("apps/list: failed to list directory connectors: {err:#}");
                    Vec::new()
                }
            };

        let accessible_connectors = list_accessible_connector_metadata_from_codex_apps_mcp(&config).await;
        let data = merge_accessible_apps(directory_connectors, accessible_connectors);

        let total = data.len();
        if total == 0 {
            self.outgoing
                .send_response_to_connection(
                    connection_id,
                    request_id,
                    AppsListResponse {
                        data,
                        next_cursor: None,
                    },
                )
                .await;
            return;
        }

        let limit = limit.unwrap_or(total as u32).max(1) as usize;
        let start = match parse_cursor_offset(cursor.as_deref(), total, "apps") {
            Ok(offset) => offset,
            Err(error) => {
                self.outgoing
                .send_error_to_connection(connection_id, request_id, error)
                .await;
                return;
            }
        };
        let end = start.saturating_add(limit).min(total);
        let data = data[start..end].to_vec();
        let next_cursor = (end < total).then(|| end.to_string());
        self.outgoing
            .send_response_to_connection(
                connection_id,
                request_id,
                AppsListResponse {
                    data,
                    next_cursor,
                },
            )
            .await;
    }

    pub(super) async fn list_threads_v2(
        &self,
        connection_id: ConnectionId,
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
                    .send_error_to_connection(
                        connection_id,
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
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, error)
                    .await;
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
            .send_response_to_connection(
                connection_id,
                request_id,
                ThreadListResponse { data, next_cursor },
            )
            .await;
    }

    pub(super) async fn list_loaded_threads_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: ThreadLoadedListParams,
    ) {
        let ids = self.conversation_manager.loaded_conversation_ids().await;

        let total = ids.len();
        let limit = params.limit.unwrap_or(total as u32).max(1) as usize;
        let start = match parse_cursor_offset(params.cursor.as_deref(), total, "threads") {
            Ok(offset) => offset,
            Err(error) => {
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, error)
                    .await;
                return;
            }
        };
        let end = start.saturating_add(limit).min(total);

        let data = ids[start..end].iter().map(ToString::to_string).collect();
        let next_cursor = (end < total).then(|| end.to_string());

        self.outgoing
            .send_response_to_connection(
                connection_id,
                request_id,
                ThreadLoadedListResponse { data, next_cursor },
            )
            .await;
    }

    pub(super) async fn thread_read_v2(
        &self,
        connection_id: ConnectionId,
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
                    .send_error_to_connection(
                        connection_id,
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
                    .send_error_to_connection(
                        connection_id,
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
                    .send_error_to_connection(
                        connection_id,
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
                        .send_error_to_connection(
                            connection_id,
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
            .send_response_to_connection(connection_id, request_id, ThreadReadResponse { thread })
            .await;
    }

    pub(super) async fn list_mcp_server_status_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: ListMcpServerStatusParams,
    ) {
        let (enabled_servers, _disabled_servers) =
            match code_core::config::list_mcp_servers(&self.base_config.code_home) {
                Ok(servers) => servers,
                Err(err) => {
                    self.outgoing
                        .send_error_to_connection(
                            connection_id,
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

        let (tx_event, _rx_event) = code_core::protocol::unbounded_event_channel();
        let (manager, startup_errors) = match McpConnectionManager::new(
            self.base_config.code_home.clone(),
            self.base_config.mcp_oauth_credentials_store_mode,
            enabled_server_map,
            excluded_tools,
            tx_event,
            code_core::protocol::AskForApproval::Never,
        )
        .await
        {
            Ok(result) => result,
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
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
                    .send_error_to_connection(
                        connection_id,
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
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, error)
                    .await;
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
            .send_response_to_connection(
                connection_id,
                request_id,
                ListMcpServerStatusResponse { data, next_cursor },
            )
            .await;
    }

    pub(super) async fn mcp_server_refresh_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        _params: Option<()>,
    ) {
        let _config = match self.load_effective_config(/*cwd*/ None) {
            Ok(config) => config,
            Err(error) => {
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, error)
                    .await;
                return;
            }
        };

        self.outgoing
            .send_response_to_connection(connection_id, request_id, McpServerRefreshResponse {})
            .await;
    }

    pub(super) async fn mcp_server_oauth_login_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: McpServerOauthLoginParams,
    ) {
        let config = match self.load_effective_config(/*cwd*/ None) {
            Ok(config) => config,
            Err(error) => {
                self.outgoing
                    .send_error_to_connection(connection_id, request_id, error)
                    .await;
                return;
            }
        };

        let McpServerOauthLoginParams {
            name,
            scopes,
            timeout_secs,
        } = params;

        let Some(server) = config.mcp_servers.get(&name) else {
            self.outgoing
                .send_error_to_connection(
                    connection_id,
                    request_id,
                    JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: format!("No MCP server named '{name}' found."),
                        data: None,
                    },
                )
                .await;
            return;
        };

        let (url, http_headers, env_http_headers) = match &server.transport {
            code_core::config_types::McpServerTransportConfig::StreamableHttp {
                url,
                http_headers,
                env_http_headers,
                ..
            } => (url.clone(), http_headers.clone(), env_http_headers.clone()),
            _ => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: "OAuth login is only supported for streamable HTTP servers."
                                .to_string(),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let resolved_scopes = scopes.unwrap_or_default();
        match perform_oauth_login_return_url(OauthLoginArgs {
            code_home: &config.code_home,
            server_name: &name,
            server_url: &url,
            store_mode: config.mcp_oauth_credentials_store_mode,
            http_headers,
            env_http_headers,
            scopes: &resolved_scopes,
            timeout_secs,
            callback_port: config.mcp_oauth_callback_port,
        })
        .await
        {
            Ok(handle) => {
                let authorization_url = handle.authorization_url().to_string();
                let notification_name = name.clone();
                let outgoing = Arc::clone(&self.outgoing);

                tokio::spawn(async move {
                    let (success, error) = match handle.wait().await {
                        Ok(()) => (true, None),
                        Err(err) => (false, Some(err.to_string())),
                    };

                    let notification =
                        ServerNotification::McpServerOauthLoginCompleted(
                            McpServerOauthLoginCompletedNotification {
                                name: notification_name,
                                success,
                                error,
                            },
                        );
                    broadcast_server_notification_simple(outgoing.as_ref(), notification).await;
                });

                self.outgoing
                    .send_response_to_connection(
                        connection_id,
                        request_id,
                        McpServerOauthLoginResponse { authorization_url },
                    )
                    .await;
            }
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("failed to login to MCP server '{name}': {err}"),
                            data: None,
                        },
                    )
                    .await;
            }
        }
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
                    .send_error_to_connection(
                        connection_id,
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
                    .send_error_to_connection(
                        connection_id,
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
            .send_response_to_connection(
                connection_id,
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

    pub(super) async fn thread_resume_v2(
        &mut self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: ThreadResumeParams,
    ) {
        let ThreadResumeParams {
            thread_id,
            history,
            path,
            model,
            model_provider,
            cwd,
            approval_policy,
            sandbox,
            config,
            base_instructions,
            developer_instructions,
            personality,
        } = params;

        if history.is_some() {
            self.outgoing
                .send_error_to_connection(
                    connection_id,
                    request_id,
                    JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: "thread/resume history is not supported".to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        }

        let catalog = SessionCatalog::new(self.base_config.code_home.clone());
        let entry = if path.is_none() {
            match catalog.find_by_id(&thread_id).await {
                Ok(Some(entry)) => Some(entry),
                Ok(None) => {
                    self.outgoing
                        .send_error_to_connection(
                            connection_id,
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
                        .send_error_to_connection(
                            connection_id,
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
            }
        } else {
            None
        };

        let rollout_path = if let Some(path) = path {
            if path.is_absolute() {
                path
            } else {
                self.base_config.code_home.join(path)
            }
        } else if let Some(entry) = entry.as_ref() {
            entry_to_rollout_path(&self.base_config.code_home, entry)
        } else {
            self.outgoing
                .send_error_to_connection(
                    connection_id,
                    request_id,
                    JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: "thread/resume requires thread_id or path".to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        };

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
            .or_else(|| entry.as_ref().map(|entry| entry.cwd_real.clone()))
            .unwrap_or_else(|| self.base_config.cwd.clone());

        let model_provider = model_provider.or_else(|| {
            entry.as_ref()
                .and_then(|entry| entry.model_provider.clone())
        });

        let overrides = ConfigOverrides {
            model,
            model_provider,
            approval_policy: approval_policy.map(v2_approval_policy_to_core),
            sandbox_mode: sandbox.map(SandboxMode::to_core),
            cwd: Some(thread_cwd),
            base_instructions,
            code_linux_sandbox_exe: self.code_linux_sandbox_exe.clone(),
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
                    .send_error_to_connection(
                        connection_id,
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

        let resumed = match self
            .conversation_manager
            .resume_conversation_from_rollout(config, rollout_path, self.auth_manager.clone())
            .await
        {
            Ok(conversation) => conversation,
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("failed to resume thread: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let thread_id = resumed.conversation_id;
        let thread = if let Some(entry) = entry.as_ref()
            && entry.session_id == Uuid::from(thread_id)
        {
            session_entry_to_thread(entry, &self.base_config.code_home, Vec::new())
        } else {
            Thread {
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
            }
        };

        let thread_state = self
            .thread_state_manager
            .ensure_connection_subscribed(thread_id, connection_id)
            .await;
        self.ensure_thread_listener(
            thread_id.to_string(),
            resumed.conversation,
            thread_state,
        )
        .await;

        self.outgoing
            .send_response_to_connection(
                connection_id,
                request_id,
                ThreadResumeResponse {
                    thread,
                    model,
                    model_provider,
                    cwd,
                    approval_policy: core_approval_policy_to_v2(approval_policy),
                    sandbox: core_sandbox_policy_to_v2(sandbox_policy),
                    reasoning_effort,
                },
            )
            .await;
    }

    pub(super) async fn thread_fork_v2(
        &mut self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: ThreadForkParams,
    ) {
        let ThreadForkParams {
            thread_id,
            path,
            model,
            model_provider,
            cwd,
            approval_policy,
            sandbox,
            config,
            base_instructions,
            developer_instructions,
        } = params;

        let catalog = SessionCatalog::new(self.base_config.code_home.clone());
        let entry = if path.is_none() {
            match catalog.find_by_id(&thread_id).await {
                Ok(Some(entry)) => Some(entry),
                Ok(None) => {
                    self.outgoing
                        .send_error_to_connection(
                            connection_id,
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
                        .send_error_to_connection(
                            connection_id,
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
            }
        } else {
            None
        };

        let rollout_path = if let Some(path) = path {
            if path.is_absolute() {
                path
            } else {
                self.base_config.code_home.join(path)
            }
        } else if let Some(entry) = entry.as_ref() {
            entry_to_rollout_path(&self.base_config.code_home, entry)
        } else {
            self.outgoing
                .send_error_to_connection(
                    connection_id,
                    request_id,
                    JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: "thread/fork requires thread_id or path".to_string(),
                        data: None,
                    },
                )
                .await;
            return;
        };

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
            .or_else(|| entry.as_ref().map(|entry| entry.cwd_real.clone()))
            .unwrap_or_else(|| self.base_config.cwd.clone());

        let model_provider = model_provider.or_else(|| {
            entry.as_ref()
                .and_then(|entry| entry.model_provider.clone())
        });

        let overrides = ConfigOverrides {
            model,
            model_provider,
            approval_policy: approval_policy.map(v2_approval_policy_to_core),
            sandbox_mode: sandbox.map(SandboxMode::to_core),
            cwd: Some(thread_cwd),
            base_instructions,
            code_linux_sandbox_exe: self.code_linux_sandbox_exe.clone(),
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
                    .send_error_to_connection(
                        connection_id,
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

        config.demo_developer_message = developer_instructions;

        let model = config.model.clone();
        let model_provider = config.model_provider_id.clone();
        let cwd = config.cwd.clone();
        let approval_policy = config.approval_policy;
        let sandbox_policy = config.sandbox_policy.clone();
        let reasoning_effort = Some(map_core_reasoning_effort(config.model_reasoning_effort.into()));

        let forked = match self
            .conversation_manager
            .fork_conversation(0, config, rollout_path)
            .await
        {
            Ok(conversation) => conversation,
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("failed to fork thread: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let thread_id = forked.conversation_id;
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
            forked.conversation,
            thread_state,
        )
        .await;

        self.outgoing
            .send_response_to_connection(
                connection_id,
                request_id,
                ThreadForkResponse {
                    thread,
                    model,
                    model_provider,
                    cwd,
                    approval_policy: core_approval_policy_to_v2(approval_policy),
                    sandbox: core_sandbox_policy_to_v2(sandbox_policy),
                    reasoning_effort,
                },
            )
            .await;
    }

    pub(super) async fn thread_archive_v2(
        &mut self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: ThreadArchiveParams,
    ) {
        let thread_id = match code_protocol::ConversationId::from_string(&params.thread_id) {
            Ok(id) => id,
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
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

        let catalog = SessionCatalog::new(self.base_config.code_home.clone());
        let entry = match catalog.find_by_id(&params.thread_id).await {
            Ok(Some(entry)) => entry,
            Ok(None) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("thread not found: {}", params.thread_id),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
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

        let rollout_path = entry_to_rollout_path(&self.base_config.code_home, &entry);
        let session_id = Uuid::from(thread_id);

        let _conversation = self.conversation_manager.remove_conversation(&thread_id).await;
        self.thread_state_manager.unload_thread(thread_id).await;

        match catalog.archive_conversation(session_id, &rollout_path).await {
            Ok(true) => {
                self.outgoing
                    .send_response_to_connection(connection_id, request_id, ThreadArchiveResponse {})
                    .await;

                send_server_notification_to_connection(
                    self.outgoing.as_ref(),
                    connection_id,
                    ServerNotification::ThreadArchived(ThreadArchivedNotification {
                        thread_id: params.thread_id,
                    }),
                )
                .await;
            }
            Ok(false) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("thread not found: {}", params.thread_id),
                            data: None,
                        },
                    )
                    .await;
            }
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("failed to archive thread: {err}"),
                            data: None,
                        },
                    )
                    .await;
            }
        }
    }

    pub(super) async fn thread_unsubscribe_v2(
        &mut self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: ThreadUnsubscribeParams,
    ) {
        let thread_id = match code_protocol::ConversationId::from_string(&params.thread_id) {
            Ok(id) => id,
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
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

        let loaded = self.conversation_manager.get_conversation(thread_id).await.is_ok();
        let status = if !loaded {
            ThreadUnsubscribeStatus::NotLoaded
        } else if self
            .thread_state_manager
            .unsubscribe_connection_from_thread(thread_id, connection_id)
            .await
        {
            ThreadUnsubscribeStatus::Unsubscribed
        } else {
            ThreadUnsubscribeStatus::NotSubscribed
        };

        self.outgoing
            .send_response_to_connection(
                connection_id,
                request_id,
                ThreadUnsubscribeResponse { status },
            )
            .await;
    }

    pub(super) async fn thread_set_name_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: ThreadSetNameParams,
    ) {
        let session_id = match Uuid::parse_str(&params.thread_id) {
            Ok(id) => id,
            Err(error) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
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
        let updated = match catalog
            .set_nickname(session_id, Some(params.name.clone()))
            .await
        {
            Ok(updated) => updated,
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("failed to update thread name: {err}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };
        if !updated {
            self.outgoing
                .send_error_to_connection(
                    connection_id,
                    request_id,
                    JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: format!("thread not found: {}", params.thread_id),
                        data: None,
                    },
                )
                .await;
            return;
        }

        self.outgoing
            .send_response_to_connection(connection_id, request_id, ThreadSetNameResponse {})
            .await;

        send_server_notification_to_connection(
            self.outgoing.as_ref(),
            connection_id,
            ServerNotification::ThreadNameUpdated(ThreadNameUpdatedNotification {
                thread_id: params.thread_id,
                thread_name: Some(params.name),
            }),
        )
        .await;
    }

    pub(super) async fn thread_metadata_update_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: ThreadMetadataUpdateParams,
    ) {
        let session_id = match Uuid::parse_str(&params.thread_id) {
            Ok(id) => id,
            Err(error) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
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
        let entry = if let Some(update) = params.git_info {
            match catalog
                .update_git_info(session_id, update.sha, update.branch, update.origin_url)
                .await
            {
                Ok(Some(entry)) => entry,
                Ok(None) => {
                    self.outgoing
                        .send_error_to_connection(
                            connection_id,
                            request_id,
                            JSONRPCErrorError {
                                code: INVALID_REQUEST_ERROR_CODE,
                                message: format!("thread not found: {}", params.thread_id),
                                data: None,
                            },
                        )
                        .await;
                    return;
                }
                Err(err) => {
                    self.outgoing
                        .send_error_to_connection(
                            connection_id,
                            request_id,
                            JSONRPCErrorError {
                                code: INTERNAL_ERROR_CODE,
                                message: format!("failed to update thread metadata: {err}"),
                                data: None,
                            },
                        )
                        .await;
                    return;
                }
            }
        } else {
            match catalog.find_by_id(&params.thread_id).await {
                Ok(Some(entry)) => entry,
                Ok(None) => {
                    self.outgoing
                        .send_error_to_connection(
                            connection_id,
                            request_id,
                            JSONRPCErrorError {
                                code: INVALID_REQUEST_ERROR_CODE,
                                message: format!("thread not found: {}", params.thread_id),
                                data: None,
                            },
                        )
                        .await;
                    return;
                }
                Err(err) => {
                    self.outgoing
                        .send_error_to_connection(
                            connection_id,
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
            }
        };

        let thread = session_entry_to_thread(&entry, &self.base_config.code_home, Vec::new());
        self.outgoing
            .send_response_to_connection(
                connection_id,
                request_id,
                ThreadMetadataUpdateResponse { thread },
            )
            .await;
    }

    pub(super) async fn thread_unarchive_v2(
        &self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: ThreadUnarchiveParams,
    ) {
        let session_id = match Uuid::parse_str(&params.thread_id) {
            Ok(id) => id,
            Err(error) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
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
        let entry = match catalog.find_by_id(&params.thread_id).await {
            Ok(Some(entry)) => entry,
            Ok(None) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("thread not found: {}", params.thread_id),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
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

        let rollout_path = entry_to_rollout_path(&self.base_config.code_home, &entry);
        match catalog.unarchive_conversation(session_id, &rollout_path).await {
            Ok(true) => {
                let refreshed = match catalog.find_by_id(&params.thread_id).await {
                    Ok(Some(entry)) => entry,
                    _ => entry,
                };
                let thread = session_entry_to_thread(&refreshed, &self.base_config.code_home, Vec::new());
                self.outgoing
                    .send_response_to_connection(
                        connection_id,
                        request_id,
                        ThreadUnarchiveResponse { thread: thread.clone() },
                    )
                    .await;

                send_server_notification_to_connection(
                    self.outgoing.as_ref(),
                    connection_id,
                    ServerNotification::ThreadUnarchived(ThreadUnarchivedNotification {
                        thread_id: params.thread_id,
                    }),
                )
                .await;
            }
            Ok(false) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("thread not found: {}", params.thread_id),
                            data: None,
                        },
                    )
                    .await;
            }
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("failed to unarchive thread: {err}"),
                            data: None,
                        },
                    )
                    .await;
            }
        }
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
                    .send_error_to_connection(
                        connection_id,
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
                    .send_error_to_connection(
                        connection_id,
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
                        .send_error_to_connection(
                            connection_id,
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
                    .send_error_to_connection(
                        connection_id,
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
            .send_response_to_connection(
                connection_id,
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

    pub(super) async fn turn_interrupt_v2(
        &mut self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        params: TurnInterruptParams,
    ) {
        let thread_id = match code_protocol::ConversationId::from_string(&params.thread_id) {
            Ok(id) => id,
            Err(err) => {
                self.outgoing
                    .send_error_to_connection(
                        connection_id,
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
                    .send_error_to_connection(
                        connection_id,
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

        if let Err(err) = conversation.submit(code_core::protocol::Op::Interrupt).await {
            self.outgoing
                .send_error_to_connection(
                    connection_id,
                    request_id,
                    JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: format!("failed to interrupt turn {}: {err}", params.turn_id),
                        data: None,
                    },
                )
                .await;
            return;
        }

        self.outgoing
            .send_response_to_connection(connection_id, request_id, TurnInterruptResponse {})
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

async fn broadcast_server_notification_simple(
    outgoing: &OutgoingMessageSender,
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
    outgoing.send_notification(OutgoingNotification { method, params }).await;
}

#[derive(Debug, Deserialize)]
struct SkillConfigEntryToml {
    path: PathBuf,
    enabled: bool,
}

#[derive(Debug, Default, Deserialize)]
struct SkillsConfigToml {
    #[serde(default)]
    config: Vec<SkillConfigEntryToml>,
}

fn normalize_override_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn disabled_skill_paths_from_stack(
    config_layer_stack: &code_core::config_loader::ConfigLayerStack,
) -> HashSet<PathBuf> {
    let mut configs: HashMap<PathBuf, bool> = HashMap::new();
    for layer in config_layer_stack.layers_high_to_low() {
        if !matches!(layer.name, ConfigLayerSource::User { .. } | ConfigLayerSource::SessionFlags) {
            continue;
        }

        let Some(skills_value) = layer.config.get("skills") else {
            continue;
        };

        let skills: SkillsConfigToml = match skills_value.clone().try_into() {
            Ok(skills) => skills,
            Err(err) => {
                tracing::warn!("invalid skills config: {err}");
                continue;
            }
        };

        for entry in skills.config {
            let normalized = normalize_override_path(&entry.path);
            if configs.contains_key(&normalized) {
                continue;
            }
            configs.insert(normalized, entry.enabled);
        }
    }

    configs
        .into_iter()
        .filter_map(|(path, enabled)| (!enabled).then_some(path))
        .collect()
}

fn core_skill_scope_to_v2(scope: code_core::skills::model::SkillScope) -> V2SkillScope {
    match scope {
        code_core::skills::model::SkillScope::Repo => V2SkillScope::Repo,
        code_core::skills::model::SkillScope::User => V2SkillScope::User,
        code_core::skills::model::SkillScope::System => V2SkillScope::System,
        code_core::skills::model::SkillScope::Admin => V2SkillScope::Admin,
    }
}

fn configured_marketplace_to_v2(
    marketplace: code_core::plugins::ConfiguredMarketplace,
) -> PluginMarketplaceEntry {
    PluginMarketplaceEntry {
        name: marketplace.name,
        path: marketplace.path,
        interface: marketplace.interface.map(marketplace_interface_to_v2),
        plugins: marketplace
            .plugins
            .into_iter()
            .map(configured_marketplace_plugin_to_v2)
            .collect(),
    }
}

fn marketplace_interface_to_v2(interface: code_core::plugins::MarketplaceInterface) -> V2MarketplaceInterface {
    V2MarketplaceInterface {
        display_name: interface.display_name,
    }
}

fn configured_marketplace_plugin_to_v2(
    plugin: code_core::plugins::ConfiguredMarketplacePlugin,
) -> PluginSummary {
    PluginSummary {
        id: plugin.id,
        name: plugin.name,
        source: plugin_source_to_v2(plugin.source),
        installed: plugin.installed,
        enabled: plugin.enabled,
        install_policy: plugin.policy.installation.into(),
        auth_policy: plugin.policy.authentication.into(),
        interface: plugin.interface.map(plugin_interface_to_v2),
    }
}

fn plugin_source_to_v2(source: code_core::plugins::MarketplacePluginSource) -> PluginSource {
    match source {
        code_core::plugins::MarketplacePluginSource::Local { path } => PluginSource::Local { path },
    }
}

fn plugin_interface_to_v2(interface: code_core::plugins::PluginManifestInterface) -> V2PluginInterface {
    V2PluginInterface {
        display_name: interface.display_name,
        short_description: interface.short_description,
        long_description: interface.long_description,
        developer_name: interface.developer_name,
        category: interface.category,
        capabilities: interface.capabilities,
        website_url: interface.website_url,
        privacy_policy_url: interface.privacy_policy_url,
        terms_of_service_url: interface.terms_of_service_url,
        default_prompt: interface.default_prompt,
        brand_color: interface.brand_color,
        composer_icon: interface.composer_icon,
        logo: interface.logo,
        screenshots: interface.screenshots,
    }
}

fn plugin_read_outcome_to_v2(outcome: code_core::plugins::PluginReadOutcome) -> V2PluginDetail {
    let plugin = outcome.plugin;
    V2PluginDetail {
        marketplace_name: outcome.marketplace_name,
        marketplace_path: outcome.marketplace_path,
        summary: PluginSummary {
            id: plugin.id,
            name: plugin.name,
            source: plugin_source_to_v2(plugin.source),
            installed: plugin.installed,
            enabled: plugin.enabled,
            install_policy: plugin.policy.installation.into(),
            auth_policy: plugin.policy.authentication.into(),
            interface: plugin.interface.map(plugin_interface_to_v2),
        },
        description: plugin.description,
        skills: plugin.skills.into_iter().map(skill_metadata_to_v2).collect(),
        apps: plugin.apps.into_iter().map(app_connector_to_v2).collect(),
        mcp_servers: plugin.mcp_server_names,
    }
}

fn skill_metadata_to_v2(skill: code_core::skills::model::SkillMetadata) -> V2SkillSummary {
    V2SkillSummary {
        name: skill.name,
        description: skill.description,
        short_description: None,
        interface: None,
        path: skill.path,
    }
}

fn app_connector_to_v2(app: code_core::plugins::AppConnectorId) -> AppSummary {
    let code_core::plugins::AppConnectorId(id) = app;
    AppSummary {
        id: id.clone(),
        name: id,
        description: None,
        install_url: None,
        needs_auth: false,
    }
}

fn v2_approval_policy_to_core(policy: AskForApproval) -> code_core::protocol::AskForApproval {
    match policy {
        AskForApproval::UnlessTrusted => code_core::protocol::AskForApproval::UnlessTrusted,
        AskForApproval::OnFailure => code_core::protocol::AskForApproval::OnFailure,
        AskForApproval::OnRequest => code_core::protocol::AskForApproval::OnRequest,
        AskForApproval::Granular {
            sandbox_approval,
            rules,
            skill_approval,
            request_permissions,
            mcp_elicitations,
        } => code_core::protocol::AskForApproval::Reject(code_core::protocol::RejectConfig {
            sandbox_approval: !sandbox_approval,
            rules: !rules,
            skill_approval: !skill_approval,
            request_permissions: !request_permissions,
            mcp_elicitations: !mcp_elicitations,
        }),
        AskForApproval::Never => code_core::protocol::AskForApproval::Never,
    }
}

fn core_approval_policy_to_v2(policy: code_core::protocol::AskForApproval) -> AskForApproval {
    match policy {
        code_core::protocol::AskForApproval::UnlessTrusted => AskForApproval::UnlessTrusted,
        code_core::protocol::AskForApproval::OnFailure => AskForApproval::OnFailure,
        code_core::protocol::AskForApproval::OnRequest => AskForApproval::OnRequest,
        code_core::protocol::AskForApproval::Reject(reject_config) => AskForApproval::Granular {
            sandbox_approval: !reject_config.sandbox_approval,
            rules: !reject_config.rules,
            skill_approval: !reject_config.skill_approval,
            request_permissions: !reject_config.request_permissions,
            mcp_elicitations: !reject_config.mcp_elicitations,
        },
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
        // Our core submission pipeline only accepts text/images today. To keep protocol
        // parity with upstream v2, encode skill/mention selections as a standalone linked
        // `$name` mention. This preserves behavior for skill injection (via mention parsing)
        // and keeps rollouts readable. When replaying rollouts, we decode exact matches
        // back into structured `UserInput::{Skill,Mention}` variants.
        UserInput::Skill { name, path } => Ok(code_core::protocol::InputItem::Text {
            text: format!("[${name}](skill://{})", path.to_string_lossy()),
        }),
        UserInput::Mention { name, path } => Ok(code_core::protocol::InputItem::Text {
            text: format!("[${name}]({path})"),
        }),
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

async fn list_accessible_connector_metadata_from_codex_apps_mcp(
    config: &Config,
) -> HashMap<String, (String, Option<String>)> {
    let active_account_id = match code_core::apps_sources::active_chatgpt_account_id(&config.code_home) {
        Ok(id) => id,
        Err(err) => {
            tracing::warn!("apps/list: failed to read active ChatGPT account id: {err}");
            None
        }
    };

    let (enabled_server_map, warnings) = code_core::apps_sources::build_codex_apps_source_servers(
        config,
        active_account_id.as_deref(),
    )
    .await;
    for warning in warnings {
        tracing::warn!("apps/list: {warning}");
    }
    if enabled_server_map.is_empty() {
        return HashMap::new();
    }

    let (tx_event, _rx_event) = code_core::protocol::unbounded_event_channel();
    let (manager, startup_errors) = match McpConnectionManager::new(
        config.code_home.clone(),
        config.mcp_oauth_credentials_store_mode,
        enabled_server_map,
        HashSet::new(),
        tx_event,
        code_core::protocol::AskForApproval::Never,
    )
    .await
    {
        Ok(result) => result,
        Err(err) => {
            tracing::warn!("apps/list: failed to start apps MCP manager: {err}");
            return HashMap::new();
        }
    };

    for (server_name, failure) in startup_errors {
        tracing::warn!(
            "apps/list: MCP server '{server_name}' {summary}",
            summary = format_failure_summary(&failure)
        );
    }

    let mut accessible: HashMap<String, (String, Option<String>)> = HashMap::new();
    for (_qualified_name, _server_name, tool) in manager.list_all_tools_with_server_names() {
        let protocol_tool = match convert_mcp_tool(&tool) {
            Ok(tool) => tool,
            Err(err) => {
                tracing::warn!("apps/list: failed to convert MCP tool '{}': {err}", tool.name);
                continue;
            }
        };

        let Some(meta) = protocol_tool
            .annotations
            .as_ref()
            .or(protocol_tool.meta.as_ref())
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };
        let Some(connector_id) = meta
            .get("connector_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        let connector_name = meta
            .get("connector_name")
            .or_else(|| meta.get("connector_display_name"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(connector_id);
        let connector_description = meta
            .get("connector_description")
            .or_else(|| meta.get("connectorDescription"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|description| !description.is_empty())
            .map(str::to_string);

        accessible
            .entry(connector_id.to_string())
            .or_insert_with(|| (connector_name.to_string(), connector_description.clone()));
    }
    manager.shutdown_all().await;

    accessible
}

fn merge_accessible_apps(
    mut directory: Vec<AppInfo>,
    accessible: HashMap<String, (String, Option<String>)>,
) -> Vec<AppInfo> {
    let mut present_ids: HashSet<String> = HashSet::new();
    for connector in &mut directory {
        if accessible.contains_key(&connector.id) {
            connector.is_accessible = true;
        }
        present_ids.insert(connector.id.clone());
    }

    for (id, (name, description)) in accessible {
        if present_ids.contains(&id) {
            continue;
        }
        let install_url = {
            let synthetic = AppInfo {
                id: id.clone(),
                name: name.clone(),
                description: description.clone(),
                logo_url: None,
                logo_url_dark: None,
                distribution_channel: None,
                branding: None,
                app_metadata: None,
                labels: None,
                install_url: None,
                is_accessible: true,
                is_enabled: true,
                plugin_display_names: Vec::new(),
            };
            let slug = code_connectors::connector_mention_slug(&synthetic);
            format!("https://chatgpt.com/apps/{slug}/{id}")
        };
        directory.push(AppInfo {
            id,
            name,
            description,
            logo_url: None,
            logo_url_dark: None,
            distribution_channel: None,
            branding: None,
            app_metadata: None,
            labels: None,
            install_url: Some(install_url),
            is_accessible: true,
            is_enabled: true,
            plugin_display_names: Vec::new(),
        });
    }

    directory.sort_by(|left, right| {
        right
            .is_accessible
            .cmp(&left.is_accessible)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
    directory
}

fn model_preset_to_v2_model(preset: &model_presets::ModelPreset) -> Model {
    Model {
        id: preset.id.clone(),
        model: preset.model.clone(),
        upgrade: preset.upgrade.as_ref().map(|upgrade| upgrade.id.clone()),
        upgrade_info: None,
        availability_nux: None,
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
            sha: entry.git_sha.clone(),
            branch: entry.git_branch.clone(),
            origin_url: entry.git_origin_url.clone(),
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

fn parse_exact_linked_tool_mention(text: &str) -> Option<(&str, &str)> {
    let trimmed = text.trim();
    let bytes = trimmed.as_bytes();
    if bytes.first() != Some(&b'[') {
        return None;
    }

    let (name, path, end_index) = parse_linked_tool_mention(trimmed, bytes, 0)?;
    if end_index != bytes.len() {
        return None;
    }

    Some((name, path))
}

fn parse_linked_tool_mention<'a>(
    text: &'a str,
    text_bytes: &[u8],
    start: usize,
) -> Option<(&'a str, &'a str, usize)> {
    let dollar_index = start + 1;
    if text_bytes.get(dollar_index) != Some(&b'$') {
        return None;
    }

    let name_start = dollar_index + 1;
    let first_name_byte = text_bytes.get(name_start)?;
    if !is_mention_name_char(*first_name_byte) {
        return None;
    }

    let mut name_end = name_start + 1;
    while let Some(next_byte) = text_bytes.get(name_end)
        && is_mention_name_char(*next_byte)
    {
        name_end += 1;
    }

    if text_bytes.get(name_end) != Some(&b']') {
        return None;
    }

    let mut path_start = name_end + 1;
    while let Some(next_byte) = text_bytes.get(path_start)
        && next_byte.is_ascii_whitespace()
    {
        path_start += 1;
    }
    if text_bytes.get(path_start) != Some(&b'(') {
        return None;
    }

    let mut path_end = path_start + 1;
    while let Some(next_byte) = text_bytes.get(path_end)
        && *next_byte != b')'
    {
        path_end += 1;
    }
    if text_bytes.get(path_end) != Some(&b')') {
        return None;
    }

    let path = text[path_start + 1..path_end].trim();
    if path.is_empty() {
        return None;
    }

    let name = &text[name_start..name_end];
    Some((name, path, path_end + 1))
}

fn is_mention_name_char(byte: u8) -> bool {
    matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-')
}

fn content_item_to_user_input(item: &ContentItem) -> Option<UserInput> {
    match item {
        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
            if let Some((name, path)) = parse_exact_linked_tool_mention(text.as_str()) {
                if path.starts_with("skill://") || path.ends_with("SKILL.md") || path.ends_with("skill.md")
                {
                    let normalized = path.strip_prefix("skill://").unwrap_or(path);
                    return Some(UserInput::Skill {
                        name: name.to_string(),
                        path: PathBuf::from(normalized),
                    });
                }

                return Some(UserInput::Mention {
                    name: name.to_string(),
                    path: path.to_string(),
                });
            }

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

fn map_fs_error(err: std::io::Error) -> JSONRPCErrorError {
    if err.kind() == std::io::ErrorKind::InvalidInput {
        JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message: err.to_string(),
            data: None,
        }
    } else {
        JSONRPCErrorError {
            code: INTERNAL_ERROR_CODE,
            message: err.to_string(),
            data: None,
        }
    }
}

fn system_time_to_unix_ms(time: std::time::SystemTime) -> Option<i64> {
    let duration = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_millis()).ok()
}

async fn copy_dir_recursive(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    // Avoid async recursion (`E0733`) by doing an explicit DFS.
    let mut stack = vec![(
        source.to_path_buf(),
        destination.to_path_buf(),
    )];
    while let Some((src_dir, dst_dir)) = stack.pop() {
        tokio::fs::create_dir_all(&dst_dir).await?;
        let mut entries = tokio::fs::read_dir(&src_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            let src_path = entry.path();
            let dst_path = dst_dir.join(entry.file_name());
            if file_type.is_dir() {
                stack.push((src_path, dst_path));
            } else if file_type.is_file() {
                tokio::fs::copy(&src_path, &dst_path).await?;
            } else if file_type.is_symlink() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "fs/copy does not support symlinks",
                ));
            }
        }
    }
    Ok(())
}
