use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use crate::code_message_processor::CodexMessageProcessor;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::OutgoingMessageSender;
use crate::thread_state::ThreadStateManager;
use code_app_server_protocol::AuthMode;
use code_app_server_protocol::CancelLoginAccountParams;
use code_app_server_protocol::ClientRequest as ApiClientRequest;
use code_app_server_protocol::ExperimentalApi;
use code_app_server_protocol::LoginAccountParams;
use code_app_server_protocol::experimental_required_message;
use code_core::AuthManager;
use code_core::ConversationManager;
use code_core::config::Config;
use code_core::config::service::ConfigService;
use code_core::config::service::ConfigServiceError;
use code_core::default_client::get_code_user_agent_with_suffix;
use code_protocol::mcp_protocol::ClientRequest as LegacyClientRequest;
use code_protocol::mcp_protocol::GetUserAgentResponse;
use code_protocol::mcp_protocol::InitializeResponse;
use code_protocol::protocol::SessionSource;
use mcp_types::JSONRPCError;
use mcp_types::JSONRPCErrorError;
use mcp_types::JSONRPCNotification;
use mcp_types::JSONRPCRequest;
use mcp_types::JSONRPCResponse;
use serde_json::json;
use toml::Value as TomlValue;

mod v2;

pub(crate) struct MessageProcessor {
    outgoing: Arc<OutgoingMessageSender>,
    code_message_processor: CodexMessageProcessor,
    base_config: Arc<Config>,
    auth_manager: Arc<AuthManager>,
    conversation_manager: Arc<ConversationManager>,
    config_warnings: Arc<Vec<serde_json::Value>>,
    config_service: ConfigService,
    thread_state_manager: ThreadStateManager,
    cli_overrides: Vec<(String, TomlValue)>,
    code_linux_sandbox_exe: Option<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ConnectionSessionState {
    pub(crate) initialized: bool,
    pub(crate) experimental_api_enabled: bool,
    pub(crate) user_agent_suffix: Option<String>,
    pub(crate) opted_out_notification_methods: HashSet<String>,
}

impl MessageProcessor {
    /// Create a new `MessageProcessor`, retaining a handle to the outgoing
    /// `Sender` so handlers can enqueue messages to be written to the
    /// transport.
    pub(crate) fn new(
        outgoing: Arc<OutgoingMessageSender>,
        code_linux_sandbox_exe: Option<PathBuf>,
        config: Arc<Config>,
        config_warnings: Vec<serde_json::Value>,
        cli_overrides: Vec<(String, TomlValue)>,
    ) -> Self {
        let auth_manager = AuthManager::shared_with_mode_and_originator(
            config.code_home.clone(),
            AuthMode::ApiKey,
            config.responses_originator_header.clone(),
            config.cli_auth_credentials_store_mode,
        );
        let conversation_manager = Arc::new(ConversationManager::new(
            auth_manager.clone(),
            SessionSource::Mcp,
        ));
        let config_for_processor = config;
        let config_home = config_for_processor.code_home.clone();
        let config_cwd = config_for_processor.cwd.clone();
        let sandbox_exe = code_linux_sandbox_exe
            .clone()
            .or_else(|| config_for_processor.code_linux_sandbox_exe.clone());
        let code_message_processor = CodexMessageProcessor::new(
            auth_manager.clone(),
            conversation_manager.clone(),
            outgoing.clone(),
            code_linux_sandbox_exe,
            config_for_processor.clone(),
        );

        Self {
            outgoing,
            code_message_processor,
            base_config: config_for_processor,
            auth_manager,
            conversation_manager,
            config_warnings: Arc::new(config_warnings),
            config_service: ConfigService::new(
                config_home,
                config_cwd,
                sandbox_exe.clone(),
                cli_overrides.clone(),
                code_core::config_loader::LoaderOverrides::default(),
            ),
            thread_state_manager: ThreadStateManager::default(),
            cli_overrides,
            code_linux_sandbox_exe: sandbox_exe,
        }
    }

    pub(crate) async fn process_request(
        &mut self,
        connection_id: ConnectionId,
        request: JSONRPCRequest,
        session: &mut ConnectionSessionState,
        outbound_initialized: &AtomicBool,
        outbound_opted_out_notification_methods: &RwLock<HashSet<String>>,
    ) {
        let request_id = request.id.clone();
        let request_json = match serde_json::to_value(request) {
            Ok(request_json) => request_json,
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("Invalid request: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let api_request = match serde_json::from_value::<ApiClientRequest>(request_json.clone()) {
            Ok(api_request) => api_request,
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("Invalid request: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        match &api_request {
            ApiClientRequest::Initialize { params, .. } => {
                if session.initialized {
                    let error = JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: "Already initialized".to_string(),
                        data: None,
                    };
                    self.outgoing.send_error(request_id.clone(), error).await;
                    return;
                }

                let client_info = &params.client_info;
                let experimental_api_enabled = params
                    .capabilities
                    .as_ref()
                    .is_some_and(|capabilities| capabilities.experimental_api);
                let opt_out_notification_methods =
                    match serde_json::from_value::<LegacyClientRequest>(request_json.clone()) {
                        Ok(LegacyClientRequest::Initialize { params, .. }) => params
                            .capabilities
                            .and_then(|capabilities| capabilities.opt_out_notification_methods)
                            .unwrap_or_default(),
                        _ => Vec::new(),
                    };
                session.experimental_api_enabled = experimental_api_enabled;
                session.opted_out_notification_methods =
                    opt_out_notification_methods.into_iter().collect();
                session.user_agent_suffix = Some(format!("{}; {}", client_info.name, client_info.version));

                if let Ok(mut methods) = outbound_opted_out_notification_methods.write() {
                    *methods = session.opted_out_notification_methods.clone();
                }

                let user_agent = get_code_user_agent_with_suffix(
                    Some(&self.base_config.responses_originator_header),
                    session.user_agent_suffix.as_deref(),
                );
                self.outgoing
                    .send_response(request_id.clone(), InitializeResponse { user_agent })
                    .await;

                session.initialized = true;
                outbound_initialized.store(true, Ordering::Release);
                return;
            }
            _ if !session.initialized => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "Not initialized".to_string(),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
            _ => {}
        }

        if let Some(reason) = api_request.experimental_reason()
            && !session.experimental_api_enabled
        {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: experimental_required_message(reason),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        if self
            .try_process_v2_request(connection_id, request_id.clone(), &api_request)
            .await
        {
            return;
        }

        let code_request = match serde_json::from_value::<LegacyClientRequest>(request_json) {
            Ok(code_request) => code_request,
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("Invalid request: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        if let LegacyClientRequest::GetUserAgent { request_id, .. } = code_request {
            let response = GetUserAgentResponse {
                user_agent: get_code_user_agent_with_suffix(
                    Some(&self.base_config.responses_originator_header),
                    session.user_agent_suffix.as_deref(),
                ),
            };
            self.outgoing.send_response(request_id, response).await;
            return;
        }

        self.code_message_processor
            .process_request_for_connection(connection_id, code_request)
            .await;
    }

    pub(crate) async fn process_notification(&self, notification: JSONRPCNotification) {
        // Currently, we do not expect to receive any notifications from the
        // client, so we just log them.
        tracing::info!("<- notification: {:?}", notification);
    }

    pub(crate) async fn send_initialize_notifications(&self, connection_id: ConnectionId) {
        for params in self.config_warnings.iter().cloned() {
            self.outgoing
                .send_notification_to_connection(
                    connection_id,
                    crate::outgoing_message::OutgoingNotification {
                        method: "configWarning".to_string(),
                        params: Some(params),
                    },
                )
                .await;
        }
    }

    pub(crate) async fn on_connection_closed(&mut self, connection_id: ConnectionId) {
        self.thread_state_manager.remove_connection(connection_id).await;
        self.code_message_processor
            .on_connection_closed(connection_id)
            .await;
    }

    /// Handle a standalone JSON-RPC response originating from the peer.
    pub(crate) async fn process_response(
        &mut self,
        connection_id: ConnectionId,
        response: JSONRPCResponse,
    ) {
        tracing::info!("<- response: {:?}", response);
        let JSONRPCResponse { id, result, .. } = response;
        self.outgoing
            .notify_client_response_for_connection(Some(connection_id), id, result)
            .await
    }

    /// Handle an error object received from the peer.
    pub(crate) async fn process_error(&mut self, connection_id: ConnectionId, err: JSONRPCError) {
        tracing::error!("<- error: {:?}", err);
        self.outgoing
            .notify_client_error_for_connection(Some(connection_id), err.id, err.error)
            .await;
    }

    async fn try_process_v2_request(
        &mut self,
        connection_id: ConnectionId,
        request_id: mcp_types::RequestId,
        request: &ApiClientRequest,
    ) -> bool {
        match request {
            ApiClientRequest::ConfigRead { params, .. } => {
                match self.config_service.read(params.clone()) {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(err) => {
                        self.outgoing
                            .send_error(request_id, map_config_service_error(err))
                            .await
                    }
                }
                true
            }
            ApiClientRequest::ConfigRequirementsRead { .. } => {
                match self.config_service.read_requirements() {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(err) => {
                        self.outgoing
                            .send_error(request_id, map_config_service_error(err))
                            .await
                    }
                }
                true
            }
            ApiClientRequest::ConfigValueWrite { params, .. } => {
                match self.config_service.write_value(params.clone()) {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(err) => {
                        self.outgoing
                            .send_error(request_id, map_config_service_error(err))
                            .await
                    }
                }
                true
            }
            ApiClientRequest::ConfigBatchWrite { params, .. } => {
                match self.config_service.batch_write(params.clone()) {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(err) => {
                        self.outgoing
                            .send_error(request_id, map_config_service_error(err))
                            .await
                    }
                }
                true
            }
            ApiClientRequest::GetAccount { params, .. } => {
                match self
                    .code_message_processor
                    .get_account_response_v2(params.refresh_token)
                    .await
                {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(error) => self.outgoing.send_error(request_id, error).await,
                }
                true
            }
            ApiClientRequest::LoginAccount { params, .. } => {
                let params: LoginAccountParams = params.clone();
                match self.code_message_processor.login_account_v2(params).await {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(error) => self.outgoing.send_error(request_id, error).await,
                }
                true
            }
            ApiClientRequest::CancelLoginAccount { params, .. } => {
                let params: CancelLoginAccountParams = params.clone();
                match self.code_message_processor.cancel_login_account_v2(params).await {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(error) => self.outgoing.send_error(request_id, error).await,
                }
                true
            }
            ApiClientRequest::LogoutAccount { .. } => {
                match self.code_message_processor.logout_account_v2().await {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(error) => self.outgoing.send_error(request_id, error).await,
                }
                true
            }
            ApiClientRequest::GetAccountRateLimits { .. } => {
                match self.code_message_processor.get_account_rate_limits_v2() {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(error) => self.outgoing.send_error(request_id, error).await,
                }
                true
            }
            ApiClientRequest::ModelList { params, .. } => {
                self.list_models_v2(request_id, params.clone()).await;
                true
            }
            ApiClientRequest::ThreadList { params, .. } => {
                self.list_threads_v2(request_id, params.clone()).await;
                true
            }
            ApiClientRequest::ThreadRead { params, .. } => {
                self.thread_read_v2(request_id, params.clone()).await;
                true
            }
            ApiClientRequest::ThreadStart { params, .. } => {
                self.thread_start_v2(connection_id, request_id, params.clone())
                    .await;
                true
            }
            ApiClientRequest::TurnStart { params, .. } => {
                self.turn_start_v2(connection_id, request_id, params.clone())
                    .await;
                true
            }
            ApiClientRequest::McpServerStatusList { params, .. } => {
                self.list_mcp_server_status_v2(request_id, params.clone()).await;
                true
            }
            _ => false,
        }
    }

}
fn map_config_service_error(err: ConfigServiceError) -> JSONRPCErrorError {
    if let Some(code) = err.write_error_code() {
        return JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message: err.to_string(),
            data: Some(json!({
                "config_write_error_code": code,
            })),
        };
    }

    JSONRPCErrorError {
        code: INTERNAL_ERROR_CODE,
        message: err.to_string(),
        data: None,
    }
}

#[cfg(test)]
mod tests;
