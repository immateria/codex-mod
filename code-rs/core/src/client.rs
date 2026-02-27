use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::AuthManager;
use crate::RefreshTokenError;
use crate::account_usage;
use crate::auth;
use crate::auth_accounts;
use bytes::Bytes;
use code_app_server_protocol::AuthMode;
use code_protocol::models::ResponseItem;
use futures::prelude::*;
use regex_lite::Regex;
use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::debug;
use tracing::trace;
use tracing::warn;
use uuid::Uuid;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

const AUTH_REQUIRED_MESSAGE: &str = "Authentication required. Run `code login` to continue.";

use crate::agent_defaults::{
    default_agent_configs,
    enabled_agent_model_specs_for_auth,
    filter_agent_model_names_for_auth,
};
use crate::chat_completions::AggregateStreamExt;
use crate::chat_completions::ChatCompletionsRequest;
use crate::chat_completions::stream_chat_completions;
use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::client_common::ResponseStream;
use crate::client_common::ResponsesApiRequest;
use crate::client_common::create_reasoning_param_for_request;
use crate::client_common::replace_image_payloads_for_model;
use crate::config::Config;
use crate::config_types::ReasoningEffort as ReasoningEffortConfig;
use crate::config_types::ReasoningSummary as ReasoningSummaryConfig;
use crate::config_types::TextVerbosity as TextVerbosityConfig;
use crate::debug_logger::DebugLogger;
use crate::default_client::create_client;
use crate::error::{CodexErr, RetryAfter};
use crate::error::Result;
use crate::error::ModelCapError;
use crate::error::RetryLimitReachedError;
use crate::error::UnexpectedResponseError;
use crate::error::UsageLimitReachedError;
use crate::flags::CODEX_RS_SSE_FIXTURE;
use crate::model_family::{find_family_for_model, ModelFamily};
use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::WireApi;
use crate::openai_tools::create_tools_json_for_responses_api;
use crate::openai_tools::ConfigShellToolType;
use crate::openai_tools::ToolsConfig;
use crate::protocol::SandboxPolicy;
use crate::reasoning::clamp_reasoning_effort_for_model;
use crate::slash_commands::get_enabled_agents;
use crate::util::backoff;
use code_otel::otel_event_manager::{OtelEventManager, TurnLatencyPayload};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;

const RESPONSES_BETA_HEADER_V1: &str = "responses=v1";
const RESPONSES_BETA_HEADER_EXPERIMENTAL: &str = "responses=experimental";
const RESPONSES_WEBSOCKETS_BETA_HEADER_V1: &str = "responses_websockets=2026-02-04";
const RESPONSES_WEBSOCKETS_BETA_HEADER_V2: &str = "responses_websockets=2026-02-06";
const RESPONSES_WEBSOCKET_INGRESS_BUFFER: usize = 256;

mod sse;
mod transport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponsesWebsocketVersion {
    V1,
    V2,
}

fn preferred_ws_version_from_env() -> ResponsesWebsocketVersion {
    match std::env::var("CODE_RESPONSES_WEBSOCKET_VERSION") {
        Ok(value) if value.eq_ignore_ascii_case("v1") => ResponsesWebsocketVersion::V1,
        _ => ResponsesWebsocketVersion::V2,
    }
}

// Sticky-routing token captured at the start of a turn. When present, it must
// be replayed on every subsequent request within the same turn (retries,
// continuations, websocket reconnects).
const X_CODEX_TURN_STATE_HEADER: &str = "x-codex-turn-state";

const MODEL_CAP_MODEL_HEADER: &str = "x-codex-model-cap-model";
const MODEL_CAP_RESET_AFTER_HEADER: &str = "x-codex-model-cap-reset-after-seconds";

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: Error,
}

#[derive(Debug, Deserialize)]
struct Error {
    r#type: Option<String>,
    #[allow(dead_code)]
    code: Option<String>,
    /// Optional parameter that triggered the error (e.g. "reasoning.summary").
    #[allow(dead_code)]
    param: Option<String>,
    message: Option<String>,

    // Optional fields available on "usage_limit_reached" and "usage_not_included" errors
    plan_type: Option<String>,
    resets_in_seconds: Option<u64>,
}

#[derive(Serialize)]
struct CompactHistoryRequest<'a> {
    model: &'a str,
    #[serde(borrow)]
    input: &'a [ResponseItem],
    instructions: String,
}

#[derive(Debug, Deserialize)]
struct CompactHistoryResponse {
    output: Vec<ResponseItem>,
}

fn rate_limit_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:please\s+try\s+again|try\s+again|please\s+retry|retry|try)\s+(?:in|after)\s*(\d+(?:\.\d+)?)\s*(ms|milliseconds?|s|sec|secs|seconds?)"
        )
            .unwrap_or_else(|err| panic!("valid rate limit regex: {err}"))
    })
}

fn try_parse_retry_after(err: &Error, now: DateTime<Utc>) -> Option<RetryAfter> {
    if let Some(seconds) = err.resets_in_seconds {
        return Some(RetryAfter::from_duration(Duration::from_secs(seconds), now));
    }

    let message = err.message.as_deref()?;
    let re = rate_limit_regex();
    let captures = re.captures(message)?;
    let value = captures.get(1)?.as_str().trim().parse::<f64>().ok()?;
    if value.is_sign_negative() {
        return None;
    }
    let unit = captures.get(2)?.as_str().trim().to_ascii_lowercase();

    if unit.starts_with("ms") {
        Some(RetryAfter::from_duration(Duration::from_millis(value.round() as u64), now))
    } else if unit.starts_with("sec") || unit == "s" || unit.starts_with("second") {
        Some(RetryAfter::from_duration(Duration::from_secs_f64(value), now))
    } else {
        None
    }
}

fn is_quota_exceeded_error(error: &Error) -> bool {
    matches!(
        error.code.as_deref().or(error.r#type.as_deref()),
        Some("insufficient_quota")
    )
}

fn is_quota_exceeded_http_error(status: StatusCode, error: &Error) -> bool {
    status.is_client_error() && is_quota_exceeded_error(error)
}

fn is_server_overloaded_error(error: &Error) -> bool {
    matches!(
        error.code.as_deref(),
        Some("server_is_overloaded") | Some("slow_down")
    )
}

fn is_reasoning_summary_rejected(error: &Error) -> bool {
    let param_matches = matches!(error.param.as_deref(), Some("reasoning.summary"));
    let code_matches = matches!(error.code.as_deref(), Some("unsupported_value"));

    let message_matches = error
        .message
        .as_deref()
        .map(|msg| {
            let msg = msg.to_ascii_lowercase();
            msg.contains("organization must be verified") && msg.contains("reasoning summar")
        })
        .unwrap_or(false);

    // Only treat as rejection if it's specifically an "unsupported_value" error
    // for the reasoning.summary parameter, or if the message explicitly says
    // the organization must be verified for reasoning summaries.
    code_matches && (param_matches || message_matches)
}

fn map_unauthorized_outcome(
    had_auth: bool,
    refresh_error: Option<&RefreshTokenError>,
) -> Option<CodexErr> {
    if let Some(err) = refresh_error {
        if err.is_permanent() {
            return Some(CodexErr::AuthRefreshPermanent(err.message.clone()));
        }
        return None;
    }

    if !had_auth {
        return Some(CodexErr::AuthRefreshPermanent(
            AUTH_REQUIRED_MESSAGE.to_string(),
        ));
    }

    None
}

#[derive(Debug)]
pub struct ModelClient {
    config: Arc<Config>,
    auth_manager: Option<Arc<AuthManager>>,
    otel_event_manager: Option<OtelEventManager>,
    client: reqwest::Client,
    provider: ModelProviderInfo,
    session_id: Uuid,
    effort: ReasoningEffortConfig,
    summary: ReasoningSummaryConfig,
    reasoning_summary_disabled: AtomicBool,
    websockets_disabled: AtomicBool,
    verbosity: TextVerbosityConfig,
    debug_logger: Arc<Mutex<DebugLogger>>,
}

pub struct ModelClientInit {
    pub config: Arc<Config>,
    pub auth_manager: Option<Arc<AuthManager>>,
    pub otel_event_manager: Option<OtelEventManager>,
    pub provider: ModelProviderInfo,
    pub effort: ReasoningEffortConfig,
    pub summary: ReasoningSummaryConfig,
    pub verbosity: TextVerbosityConfig,
    pub session_id: Uuid,
    pub debug_logger: Arc<Mutex<DebugLogger>>,
}

impl Clone for ModelClient {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            auth_manager: self.auth_manager.clone(),
            otel_event_manager: self.otel_event_manager.clone(),
            client: self.client.clone(),
            provider: self.provider.clone(),
            session_id: self.session_id,
            effort: self.effort,
            summary: self.summary,
            reasoning_summary_disabled: AtomicBool::new(
                self.reasoning_summary_disabled.load(Ordering::Relaxed),
            ),
            websockets_disabled: AtomicBool::new(
                self.websockets_disabled.load(Ordering::Relaxed),
            ),
            verbosity: self.verbosity,
            debug_logger: Arc::clone(&self.debug_logger),
        }
    }
}

impl ModelClient {
    pub fn new(init: ModelClientInit) -> Self {
        let ModelClientInit {
            config,
            auth_manager,
            otel_event_manager,
            provider,
            effort,
            summary,
            verbosity,
            session_id,
            debug_logger,
        } = init;
        let effective_verbosity =
            transport::clamp_text_verbosity_for_model(config.model.as_str(), verbosity);
        let clamped_effort = clamp_reasoning_effort_for_model(config.model.as_str(), effort);
        let client = create_client(&config.responses_originator_header);

        Self {
            config,
            auth_manager,
            otel_event_manager,
            client,
            provider,
            session_id,
            effort: clamped_effort,
            summary,
            reasoning_summary_disabled: AtomicBool::new(false),
            websockets_disabled: AtomicBool::new(false),
            verbosity: effective_verbosity,
            debug_logger,
        }
    }

    pub fn config(&self) -> Arc<Config> {
        Arc::clone(&self.config)
    }

    fn active_ws_version_for_prompt(&self, prompt: &Prompt) -> Option<ResponsesWebsocketVersion> {
        if self.websockets_disabled.load(Ordering::Relaxed) {
            return None;
        }

        match self.provider.wire_api {
            WireApi::ResponsesWebsocket => Some(preferred_ws_version_from_env()),
            WireApi::Responses => {
                let prefer_websockets = prompt
                    .model_family_override
                    .as_ref()
                    .map(|family| family.prefer_websockets)
                    .or_else(|| {
                        prompt
                            .model_override
                            .as_deref()
                            .and_then(find_family_for_model)
                            .map(|family| family.prefer_websockets)
                    })
                    .unwrap_or(self.config.model_family.prefer_websockets);

                prefer_websockets.then_some(preferred_ws_version_from_env())
            }
            WireApi::Chat => None,
        }
    }

    /// Get the reasoning effort configuration
    pub fn get_reasoning_effort(&self) -> ReasoningEffortConfig {
        self.effort
    }

    /// Get the reasoning summary configuration
    pub fn get_reasoning_summary(&self) -> ReasoningSummaryConfig {
        if self.reasoning_summary_disabled.load(Ordering::Relaxed) {
            ReasoningSummaryConfig::None
        } else {
            self.summary
        }
    }

    fn current_reasoning_param(
        &self,
        family: &ModelFamily,
        effort: ReasoningEffortConfig,
    ) -> Option<crate::client_common::Reasoning> {
        if self.reasoning_summary_disabled.load(Ordering::Relaxed) {
            return None;
        }

        create_reasoning_param_for_request(
            family,
            Some(effort),
            self.summary,
        )
    }

    fn disable_reasoning_summary(&self) {
        if !self.reasoning_summary_disabled.swap(true, Ordering::Relaxed) {
            tracing::warn!("disabling reasoning summaries after API rejection");
        }
    }

    /// Get the text verbosity configuration
    #[allow(dead_code)]
    pub fn get_text_verbosity(&self) -> TextVerbosityConfig {
        self.verbosity
    }

    pub fn get_otel_event_manager(&self) -> Option<OtelEventManager> {
        self.otel_event_manager.clone()
    }

    pub fn log_turn_latency_debug(&self, payload: &TurnLatencyPayload) {
        if let Ok(logger) = self.debug_logger.lock() {
            let _ = logger.log_turn_latency(payload);
        }
    }

    pub fn code_home(&self) -> &Path {
        &self.config.code_home
    }

    pub fn auth_credentials_store_mode(&self) -> crate::config_types::AuthCredentialsStoreMode {
        self.config.cli_auth_credentials_store_mode
    }

    pub fn debug_enabled(&self) -> bool {
        self.config.debug
    }

    pub fn auto_switch_accounts_on_rate_limit(&self) -> bool {
        self.config.auto_switch_accounts_on_rate_limit
    }

    pub fn api_key_fallback_on_all_accounts_limited(&self) -> bool {
        self.config.api_key_fallback_on_all_accounts_limited
    }

    pub fn build_tools_config_with_sandbox(
        &self,
        sandbox_policy: SandboxPolicy,
    ) -> ToolsConfig {
        self.build_tools_config_with_sandbox_for_family(sandbox_policy, &self.config.model_family)
    }

    pub fn build_tools_config_with_sandbox_for_family(
        &self,
        sandbox_policy: SandboxPolicy,
        model_family: &ModelFamily,
    ) -> ToolsConfig {
        let mut tools_config = ToolsConfig::new(crate::openai_tools::ToolsConfigParams {
            model_family,
            approval_policy: self.config.approval_policy,
            sandbox_policy: sandbox_policy.clone(),
            include_plan_tool: self.config.include_plan_tool,
            include_apply_patch_tool: self.config.include_apply_patch_tool,
            include_web_search_request: self.config.tools_web_search_request,
            use_streamable_shell_tool: self.config.use_experimental_streamable_shell_tool,
            include_view_image_tool: self.config.include_view_image_tool,
        });
        tools_config.web_search_allowed_domains = self.config.tools_web_search_allowed_domains.clone();
        tools_config.web_search_external = self.config.tools_web_search_external;
        tools_config.search_tool = self.config.tools_search_tool;
        tools_config.js_repl = self.config.tools_js_repl;

        let auth_mode = self
            .auth_manager
            .as_ref()
            .and_then(|manager| manager.auth().map(|auth| auth.mode))
            .or(Some(if self.config.using_chatgpt_auth {
                AuthMode::Chatgpt
            } else {
                AuthMode::ApiKey
            }));
        let supports_pro_only_models = self
            .auth_manager
            .as_ref()
            .is_some_and(|manager| manager.supports_pro_only_models());

        let mut agent_models: Vec<String> = if self.config.agents.is_empty() {
            default_agent_configs()
                .into_iter()
                .filter(|cfg| cfg.enabled)
                .map(|cfg| cfg.name)
                .collect()
        } else {
            get_enabled_agents(&self.config.agents)
        };
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

        let base_shell_type = tools_config.shell_type.clone();
        let base_uses_native_shell = matches!(
            &base_shell_type,
            ConfigShellToolType::LocalShell | ConfigShellToolType::StreamableShell
        );

        tools_config.shell_type = match sandbox_policy {
            SandboxPolicy::ReadOnly => {
                if base_uses_native_shell {
                    base_shell_type
                } else {
                    ConfigShellToolType::ShellWithRequest {
                        sandbox_policy: SandboxPolicy::ReadOnly,
                    }
                }
            }
            sp @ SandboxPolicy::WorkspaceWrite { .. } => {
                if base_uses_native_shell {
                    base_shell_type
                } else {
                    ConfigShellToolType::ShellWithRequest { sandbox_policy: sp }
                }
            }
            SandboxPolicy::DangerFullAccess => base_shell_type,
        };

        tools_config
    }

    pub fn build_tools_config(&self) -> ToolsConfig {
        self.build_tools_config_with_sandbox(self.config.sandbox_policy.clone())
    }

    pub fn get_auto_compact_token_limit(&self) -> Option<i64> {
        self.config
            .model_auto_compact_token_limit
            .or_else(|| self.config.model_family.auto_compact_token_limit())
    }

    pub fn default_model_slug(&self) -> &str {
        self.config.model.as_str()
    }

    pub fn default_model_family(&self) -> &ModelFamily {
        &self.config.model_family
    }

    /// Dispatches to either the Responses or Chat implementation depending on
    /// the provider config.  Public callers always invoke `stream()` – the
    /// specialised helpers are private to avoid accidental misuse.
    pub async fn stream(&self, prompt: &Prompt) -> Result<ResponseStream> {
        let env_log_tag = std::env::var("CODE_DEBUG_LOG_TAG").ok();
        let log_tag = env_log_tag
            .as_deref()
            .or(prompt.log_tag.as_deref());
        match self.provider.wire_api {
            WireApi::Responses => {
                if let Some(ws_version) = self.active_ws_version_for_prompt(prompt) {
                    match self
                        .stream_responses_websocket(prompt, log_tag, ws_version)
                        .await
                    {
                        Ok(stream) => Ok(stream),
                        Err(err) => {
                            self.websockets_disabled.store(true, Ordering::Relaxed);
                            warn!(
                                "preferred websocket transport failed; falling back to responses HTTP stream: {err}"
                            );
                            self.stream_responses(prompt, log_tag).await
                        }
                    }
                } else {
                    self.stream_responses(prompt, log_tag).await
                }
            }
            WireApi::ResponsesWebsocket => {
                if self.websockets_disabled.load(Ordering::Relaxed) {
                    warn!(
                        "responses_websocket transport disabled for this session; using responses HTTP stream"
                    );
                    return self.stream_responses(prompt, log_tag).await;
                }
                let ws_version = self
                    .active_ws_version_for_prompt(prompt)
                    .unwrap_or(preferred_ws_version_from_env());
                match self
                    .stream_responses_websocket(prompt, log_tag, ws_version)
                    .await
                {
                    Ok(stream) => Ok(stream),
                    Err(err) => {
                        self.websockets_disabled.store(true, Ordering::Relaxed);
                        warn!(
                            "responses_websocket transport failed; falling back to responses HTTP stream: {err}"
                        );
                        self.stream_responses(prompt, log_tag).await
                    }
                }
            }
            WireApi::Chat => {
                let effective_family = prompt
                    .model_family_override
                    .as_ref()
                    .unwrap_or(&self.config.model_family);
                let model_slug = prompt
                    .model_override
                    .as_deref()
                    .unwrap_or(self.config.model.as_str());
                // Create the raw streaming connection first.
                let response_stream = stream_chat_completions(ChatCompletionsRequest {
                    prompt,
                    model_family: effective_family,
                    model_slug,
                    client: &self.client,
                    provider: &self.provider,
                    debug_logger: &self.debug_logger,
                    auth_manager: self.auth_manager.clone(),
                    otel_event_manager: self.otel_event_manager.clone(),
                    log_tag,
                })
                .await?;

                // Wrap it with the aggregation adapter so callers see *only*
                // the final assistant message per turn (matching the
                // behaviour of the Responses API).
                let mut aggregated = if self.config.show_raw_agent_reasoning {
                    crate::chat_completions::AggregatedChatStream::streaming_mode(response_stream)
                } else {
                    response_stream.aggregate()
                };

                // Bridge the aggregated stream back into a standard
                // `ResponseStream` by forwarding events through a channel.
                let (tx, rx) = mpsc::channel::<Result<ResponseEvent>>(16);

                tokio::spawn(async move {
                    use futures::StreamExt;
                    while let Some(ev) = aggregated.next().await {
                        // Exit early if receiver hung up.
                        if tx.send(ev).await.is_err() {
                            break;
                        }
                    }
                });

                Ok(ResponseStream { rx_event: rx })
            }
        }
    }

    async fn stream_responses_websocket(
        &self,
        prompt: &Prompt,
        log_tag: Option<&str>,
        ws_version: ResponsesWebsocketVersion,
    ) -> Result<ResponseStream> {
        let auth_manager = self.auth_manager.clone();
        let auth_mode = auth_manager
            .as_ref()
            .and_then(|m| m.auth())
            .as_ref()
            .map(|a| a.mode);

        // Use non-stored turns on all paths for stability.
        let store = false;

        let request_model = prompt
            .model_override
            .as_deref()
            .unwrap_or(self.config.model.as_str());
        let effective_effort = clamp_reasoning_effort_for_model(request_model, self.effort);
        let request_family = prompt
            .model_family_override
            .clone()
            .or_else(|| find_family_for_model(request_model))
            .unwrap_or_else(|| self.config.model_family.clone());

        let full_instructions = prompt.get_full_instructions(&request_family);
        let mut tools_json = create_tools_json_for_responses_api(&prompt.tools)?;
        if matches!(effective_effort, ReasoningEffortConfig::Minimal) {
            tools_json.retain(|tool| {
                tool.get("type")
                    .and_then(|value| value.as_str())
                    .map(|tool_type| tool_type != "web_search")
                    .unwrap_or(true)
            });
        }

        let mut input_with_instructions = prompt.get_formatted_input();
        replace_image_payloads_for_model(&mut input_with_instructions, request_model);

        let want_format = prompt.text_format.clone().or_else(|| {
            prompt.output_schema.as_ref().map(|schema| crate::client_common::TextFormat {
                r#type: "json_schema".to_string(),
                name: Some("code_output_schema".to_string()),
                strict: Some(true),
                schema: Some(schema.clone()),
            })
        });

        let effective_verbosity =
            transport::clamp_text_verbosity_for_model(request_model, self.verbosity);
        let verbosity = match &request_family.family {
            family if family == "gpt-5" || family == "gpt-5.1" => Some(effective_verbosity),
            _ => None,
        };

        let text_template = match (auth_mode, want_format, verbosity) {
            (Some(mode), None, _) if mode.is_chatgpt() => None,
            (_, Some(fmt), _) => Some(crate::client_common::Text {
                verbosity: effective_verbosity.into(),
                format: Some(fmt),
            }),
            (_, None, Some(_)) => Some(crate::client_common::Text {
                verbosity: effective_verbosity.into(),
                format: None,
            }),
            (_, None, None) => None,
        };

        let model_slug = request_model;
        let session_id = prompt.session_id_override.unwrap_or(self.session_id);
        let session_id_str = session_id.to_string();
        let turn_state: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
        let mut attempt = 0;
        let max_retries = self.provider.request_max_retries();
        let mut request_id = String::new();

        loop {
            attempt += 1;

            let reasoning = self.current_reasoning_param(&request_family, effective_effort);
            let include: Vec<String> = if !store && reasoning.is_some() {
                vec!["reasoning.encrypted_content".to_string()]
            } else {
                Vec::new()
            };

            let payload = ResponsesApiRequest {
                model: model_slug,
                instructions: &full_instructions,
                input: &input_with_instructions,
                tools: &tools_json,
                tool_choice: "auto",
                parallel_tool_calls: request_family.supports_parallel_tool_calls,
                reasoning,
                text: text_template.clone(),
                store: self.provider.is_azure_responses_endpoint(),
                stream: true,
                include,
                prompt_cache_key: Some(session_id_str.clone()),
            };

            let mut payload_json = serde_json::to_value(&payload)?;
            if let Some(model_value) = payload_json.get_mut("model") {
                *model_value = serde_json::Value::String(model_slug.to_string());
            }
            if self.provider.is_azure_responses_endpoint() {
                sse::attach_item_ids(&mut payload_json, &input_with_instructions);
            }
            if let Some(openrouter_cfg) = self.provider.openrouter_config()
                && let Some(obj) = payload_json.as_object_mut() {
                    if let Some(provider) = &openrouter_cfg.provider {
                        obj.insert("provider".to_string(), serde_json::to_value(provider)?);
                    }
                    if let Some(route) = &openrouter_cfg.route {
                        obj.insert("route".to_string(), route.clone());
                    }
                    for (key, value) in &openrouter_cfg.extra {
                        obj.entry(key.clone()).or_insert(value.clone());
                    }
                }

            let auth = auth_manager.as_ref().and_then(|m| m.auth());
            let endpoint = self.provider.get_full_url(&auth);

            let url = reqwest::Url::parse(&endpoint).map_err(|err| {
                CodexErr::Stream(
                    format!("[ws] invalid URL: {err}"),
                    None,
                    Some(request_id.clone()),
                )
            })?;

            let ws_endpoint = match url.scheme() {
                "http" => endpoint.replacen("http://", "ws://", 1),
                "https" => endpoint.replacen("https://", "wss://", 1),
                _ => endpoint.clone(),
            };
            let mut req_builder = self
                .provider
                .create_request_builder_for_url(&self.client, &auth, reqwest::Method::GET, url)
                .await?;

            let has_beta_header = req_builder
                .try_clone()
                .and_then(|builder| builder.build().ok())
                .is_some_and(|req| req.headers().contains_key("OpenAI-Beta"));

            if !has_beta_header {
                let beta_value = if self.provider.is_public_openai_responses_endpoint() {
                    RESPONSES_BETA_HEADER_V1
                } else {
                    RESPONSES_BETA_HEADER_EXPERIMENTAL
                };
                req_builder = req_builder.header("OpenAI-Beta", beta_value);
            }

            req_builder = transport::attach_openai_subagent_header(req_builder);
            req_builder = transport::attach_codex_beta_features_header(req_builder, &self.config);
            req_builder = transport::attach_web_search_eligible_header(req_builder, &self.config);
            if let Some(state) = turn_state.get() {
                req_builder = req_builder.header(X_CODEX_TURN_STATE_HEADER, state);
            }
            req_builder = req_builder
                .header("conversation_id", session_id_str.clone())
                .header("session_id", session_id_str.clone());

            if let Some(auth) = auth.as_ref()
                && auth.mode.is_chatgpt()
                && let Some(account_id) = auth.get_account_id()
            {
                req_builder = req_builder.header("chatgpt-account-id", account_id);
            }

            let header_snapshot = req_builder
                .try_clone()
                .and_then(|builder| builder.build().ok())
                .map(|req| sse::header_map_to_json(req.headers()));

            if request_id.is_empty()
                && let Ok(logger) = self.debug_logger.lock() {
                    request_id = logger
                        .start_request_log(&endpoint, &payload_json, header_snapshot.as_ref(), log_tag)
                        .unwrap_or_default();
                }

            let ws_headers = req_builder
                .try_clone()
                .and_then(|builder| builder.build().ok())
                .map(|req| req.headers().clone())
                .unwrap_or_else(HeaderMap::new);

            let mut ws_request = ws_endpoint
                .into_client_request()
                .map_err(|err| {
                    CodexErr::Stream(
                        format!("[ws] failed to build request: {err}"),
                        None,
                        Some(request_id.clone()),
                    )
                })?;
            ws_request.headers_mut().extend(ws_headers);
            // The Responses API websocket wire requires its own beta token (distinct from
            // `responses=v1` / `responses=experimental`).
            ws_request.headers_mut().insert(
                reqwest::header::HeaderName::from_static("openai-beta"),
                HeaderValue::from_static(match ws_version {
                    ResponsesWebsocketVersion::V2 => RESPONSES_WEBSOCKETS_BETA_HEADER_V2,
                    ResponsesWebsocketVersion::V1 => RESPONSES_WEBSOCKETS_BETA_HEADER_V1,
                }),
            );

            // Wrap the normal /responses request payload in the WebSocket envelope.
            let mut ws_payload = serde_json::Map::new();
            ws_payload.insert(
                "type".to_string(),
                serde_json::Value::String("response.create".to_string()),
            );
            if let Some(obj) = payload_json.as_object() {
                for (k, v) in obj {
                    ws_payload.insert(k.clone(), v.clone());
                }
            }
            let ws_payload_text = serde_json::to_string(&serde_json::Value::Object(ws_payload))?;

            let connect = tokio_tungstenite::connect_async(ws_request).await;
            match connect {
                Ok((mut ws_stream, response)) => {
                    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(1600);

                    if let Some(value) = response
                        .headers()
                        .get(X_CODEX_TURN_STATE_HEADER)
                        .and_then(|value| value.to_str().ok())
                    {
                        if let Some(existing) = turn_state.get()
                            && existing != value
                        {
                            warn!(
                                existing,
                                new = value,
                                "received unexpected x-codex-turn-state during websocket connect"
                            );
                        } else {
                            let _ = turn_state.set(value.to_string());
                        }
                    }

                    if let Some(snapshot) = sse::parse_rate_limit_snapshot(response.headers()) {
                        debug!(
                            "rate limit headers:\n{}",
                            sse::format_rate_limit_headers(response.headers())
                        );
                        if tx_event
                            .send(Ok(ResponseEvent::RateLimits(snapshot)))
                            .await
                            .is_err()
                        {
                            debug!("receiver dropped rate limit snapshot event");
                        }
                    }

                    let models_etag = response
                        .headers()
                        .get("X-Models-Etag")
                        .and_then(|value| value.to_str().ok())
                        .map(ToString::to_string);
                    if let Some(etag) = models_etag
                        && tx_event
                            .send(Ok(ResponseEvent::ModelsEtag(etag)))
                            .await
                            .is_err()
                        {
                            debug!("receiver dropped models etag event");
                        }

                    if response.headers().contains_key("x-reasoning-included")
                        && tx_event
                            .send(Ok(ResponseEvent::ServerReasoningIncluded(true)))
                            .await
                            .is_err()
                        {
                            debug!("receiver dropped server reasoning included event");
                        }

                    ws_stream
                        .send(Message::Text(ws_payload_text))
                        .await
                        .map_err(|err| {
                            CodexErr::Stream(
                                format!("[ws] failed to send websocket request: {err}"),
                                None,
                                Some(request_id.clone()),
                            )
                        })?;

                    // Keep websocket ingress bounded so a slow downstream consumer
                    // cannot cause unbounded buffering and memory growth.
                    let (tx_bytes, rx_bytes) =
                        mpsc::channel::<Result<Bytes>>(RESPONSES_WEBSOCKET_INGRESS_BUFFER);
                    let request_id_for_ws = request_id.clone();
                    let ws_reader_handle = tokio::spawn(async move {
                        loop {
                            let Some(next) = ws_stream.next().await else {
                                break;
                            };
                            match next {
                                Ok(Message::Text(text)) => {
                                    if let Some(error) = transport::parse_wrapped_websocket_error_event(&text)
                                        .and_then(transport::map_wrapped_websocket_error_event)
                                    {
                                        let _ = tx_bytes.send(Err(error)).await;
                                        break;
                                    }

                                    let chunk = format!("data: {text}\n\n");
                                    if tx_bytes.send(Ok(Bytes::from(chunk))).await.is_err() {
                                        break;
                                    }
                                }
                                Ok(Message::Ping(payload)) => {
                                    if ws_stream.send(Message::Pong(payload)).await.is_err() {
                                        break;
                                    }
                                }
                                Ok(Message::Pong(_)) => {}
                                Ok(Message::Close(_)) => break,
                                Ok(Message::Binary(_)) => {
                                    let _ = tx_bytes
                                        .send(Err(CodexErr::Stream(
                                            "[ws] unexpected binary websocket event".to_string(),
                                            None,
                                            Some(request_id_for_ws.clone()),
                                        )))
                                        .await;
                                    break;
                                }
                                Ok(_) => {}
                                Err(err) => {
                                    let _ = tx_bytes
                                        .send(Err(CodexErr::Stream(
                                            format!("[ws] websocket error: {err}"),
                                            None,
                                            Some(request_id_for_ws.clone()),
                                        )))
                                        .await;
                                    break;
                                }
                            }
                        }
                    });

                    let stream = ReceiverStream::new(rx_bytes);
                    let debug_logger = Arc::clone(&self.debug_logger);
                    let request_id_clone = request_id.clone();
                    let otel_event_manager = self.otel_event_manager.clone();
                    let stream_idle_timeout = self.provider.stream_idle_timeout();
                    tokio::spawn(async move {
                        sse::process_sse(
                            stream,
                            tx_event,
                            stream_idle_timeout,
                            debug_logger,
                            request_id_clone,
                            otel_event_manager,
                            Arc::new(RwLock::new(sse::StreamCheckpoint::default())),
                        )
                        .await;
                        // process_sse may finish before the server closes the websocket.
                        // Abort the websocket reader task to avoid lingering open sockets.
                        ws_reader_handle.abort();
                    });

                    return Ok(ResponseStream { rx_event });
                }
                Err(err) => {
                    if transport::websocket_connect_is_upgrade_required(&err) {
                        self.websockets_disabled.store(true, Ordering::Relaxed);
                        warn!("responses websocket upgrade required; falling back to HTTP responses transport");
                        return self.stream_responses(prompt, log_tag).await;
                    }

                    let err = CodexErr::Stream(
                        format!("[ws] failed to connect: {err}"),
                        None,
                        Some(request_id.clone()),
                    );
                    if (attempt as u64) < max_retries {
                        tokio::time::sleep(backoff(attempt as u64)).await;
                        continue;
                    }
                    self.websockets_disabled.store(true, Ordering::Relaxed);
                    return Err(err);
                }
            }
        }
    }

    /// Implementation for the OpenAI *Responses* experimental API.
    async fn stream_responses(&self, prompt: &Prompt, log_tag: Option<&str>) -> Result<ResponseStream> {
        if let Some(path) = &*CODEX_RS_SSE_FIXTURE {
            // short circuit for tests
            warn!(path, "Streaming from fixture");
            return sse::stream_from_fixture(path, self.provider.clone(), self.otel_event_manager.clone())
                .await;
        }

        let auth_manager = self.auth_manager.clone();

        let auth_mode = auth_manager
            .as_ref()
            .and_then(|m| m.auth())
            .as_ref()
            .map(|a| a.mode);

        // Use non-stored turns on all paths for stability.
        let store = false;
        let turn_state: Arc<OnceLock<String>> = Arc::new(OnceLock::new());

        let request_model = prompt
            .model_override
            .as_deref()
            .unwrap_or(self.config.model.as_str());
        let effective_effort = clamp_reasoning_effort_for_model(request_model, self.effort);
        let request_family = prompt
            .model_family_override
            .clone()
            .or_else(|| find_family_for_model(request_model))
            .unwrap_or_else(|| self.config.model_family.clone());

        let full_instructions = prompt.get_full_instructions(&request_family);
        let mut tools_json = create_tools_json_for_responses_api(&prompt.tools)?;
        if matches!(effective_effort, ReasoningEffortConfig::Minimal) {
            tools_json.retain(|tool| {
                tool.get("type")
                    .and_then(|value| value.as_str())
                    .map(|tool_type| tool_type != "web_search")
                    .unwrap_or(true)
                });
        }

        let mut input_with_instructions = prompt.get_formatted_input();
        replace_image_payloads_for_model(&mut input_with_instructions, request_model);

        // Build `text` parameter with conditional verbosity and optional format.
        // - Omit entirely for ChatGPT auth unless a `text.format` or output schema is present.
        // - Only include `text.verbosity` for GPT-5 family models; warn and ignore otherwise.
        // - When a structured `format` is present, still include `verbosity` so GPT-5 can honor it.
        let want_format = prompt.text_format.clone().or_else(|| {
            prompt.output_schema.as_ref().map(|schema| crate::client_common::TextFormat {
                r#type: "json_schema".to_string(),
                name: Some("code_output_schema".to_string()),
                strict: Some(true),
                schema: Some(schema.clone()),
            })
        });

        let effective_verbosity =
            transport::clamp_text_verbosity_for_model(request_model, self.verbosity);

        let verbosity = match &request_family.family {
            family if family == "gpt-5" || family == "gpt-5.1" => Some(effective_verbosity),
            _ => None,
        };

        let text_template = match (auth_mode, want_format, verbosity) {
            (Some(mode), None, _) if mode.is_chatgpt() => None,
            (_, Some(fmt), _) => Some(crate::client_common::Text {
                verbosity: effective_verbosity.into(),
                format: Some(fmt),
            }),
            (_, None, Some(_)) => Some(crate::client_common::Text {
                verbosity: effective_verbosity.into(),
                format: None,
            }),
            (_, None, None) => None,
        };

        // In general, we want to explicitly send `store: false` when using the Responses API,
        // but in practice, the Azure Responses API rejects `store: false`:
        //
        // - If store = false and id is sent an error is thrown that ID is not found
        // - If store = false and id is not sent an error is thrown that ID is required
        //
        // For Azure, we send `store: true` and preserve reasoning item IDs.
        let azure_workaround = self.provider.is_azure_responses_endpoint();

        let model_slug = request_model;

        let session_id = prompt
            .session_id_override
            .unwrap_or(self.session_id);
        let session_id_str = session_id.to_string();

        let mut attempt = 0;
        let max_retries = self.provider.request_max_retries();
        let mut request_id = String::new();
        let mut rate_limit_switch_state = crate::account_switching::RateLimitSwitchState::default();

        // Compute endpoint with the latest available auth (may be None at this point).
        let endpoint = self
            .provider
            .get_full_url(&auth_manager.as_ref().and_then(|m| m.auth()));

        loop {
            attempt += 1;

            let reasoning = self.current_reasoning_param(&request_family, effective_effort);
            // Request encrypted COT if we are not storing responses,
            // otherwise reasoning items will be referenced by ID
            let include: Vec<String> = if !store && reasoning.is_some() {
                vec!["reasoning.encrypted_content".to_string()]
            } else {
                Vec::new()
            };

            let text = text_template.clone();

            let payload = ResponsesApiRequest {
                model: model_slug,
                instructions: &full_instructions,
                input: &input_with_instructions,
                tools: &tools_json,
                tool_choice: "auto",
                parallel_tool_calls: request_family.supports_parallel_tool_calls,
                reasoning,
                text,
                store: azure_workaround,
                stream: true,
                include,
                // Use a stable per-process cache key (session id). With store=false this is inert.
                prompt_cache_key: Some(session_id_str.clone()),
            };

            let mut payload_json = serde_json::to_value(&payload)?;
            if let Some(model_value) = payload_json.get_mut("model") {
                *model_value = serde_json::Value::String(model_slug.to_string());
            }
            if azure_workaround {
                sse::attach_item_ids(&mut payload_json, &input_with_instructions);
            }
            if let Some(openrouter_cfg) = self.provider.openrouter_config()
                && let Some(obj) = payload_json.as_object_mut() {
                    if let Some(provider) = &openrouter_cfg.provider {
                        obj.insert(
                            "provider".to_string(),
                            serde_json::to_value(provider)?
                        );
                    }
                    if let Some(route) = &openrouter_cfg.route {
                        obj.insert("route".to_string(), route.clone());
                    }
                    for (key, value) in &openrouter_cfg.extra {
                        obj.entry(key.clone()).or_insert(value.clone());
                    }
                }
            let payload_body = serde_json::to_string(&payload_json)?;

            let mut auth_refresh_error: Option<RefreshTokenError> = None;

            // Always fetch the latest auth in case a prior attempt refreshed the token.
            let auth = auth_manager.as_ref().and_then(|m| m.auth());

            trace!(
                "POST to {}: {}",
                self.provider.get_full_url(&auth),
                payload_body.as_str()
            );

            let mut req_builder = self
                .provider
                .create_request_builder(&self.client, &auth)
                .await?;

            let has_beta_header = req_builder
                .try_clone()
                .and_then(|builder| builder.build().ok())
                .is_some_and(|req| req.headers().contains_key("OpenAI-Beta"));

            if !has_beta_header {
                let beta_value = if self.provider.is_public_openai_responses_endpoint() {
                    RESPONSES_BETA_HEADER_V1
                } else {
                    RESPONSES_BETA_HEADER_EXPERIMENTAL
                };
                req_builder = req_builder.header("OpenAI-Beta", beta_value);
            }

            req_builder = transport::attach_openai_subagent_header(req_builder);
            req_builder = transport::attach_codex_beta_features_header(req_builder, &self.config);
            req_builder = transport::attach_web_search_eligible_header(req_builder, &self.config);
            if let Some(state) = turn_state.get() {
                req_builder = req_builder.header(X_CODEX_TURN_STATE_HEADER, state);
            }

            req_builder = req_builder
                // Send `conversation_id`/`session_id` so the server can hit the prompt-cache.
                .header("conversation_id", session_id_str.clone())
                .header("session_id", session_id_str.clone())
                .header(reqwest::header::ACCEPT, "text/event-stream")
                .json(&payload_json);

            if let Some(auth) = auth.as_ref()
                && auth.mode.is_chatgpt()
                && let Some(account_id) = auth.get_account_id()
            {
                req_builder = req_builder.header("chatgpt-account-id", account_id);
            }

            if request_id.is_empty() {
                let endpoint_for_log = self.provider.get_full_url(&auth);
                let header_snapshot = req_builder
                    .try_clone()
                    .and_then(|builder| builder.build().ok())
                    .map(|req| sse::header_map_to_json(req.headers()));

                if let Ok(logger) = self.debug_logger.lock() {
                    request_id = logger
                        .start_request_log(
                            &endpoint_for_log,
                            &payload_json,
                            header_snapshot.as_ref(),
                            log_tag,
                        )
                        .unwrap_or_default();
                }
            }

            let res = if let Some(otel) = self.otel_event_manager.as_ref() {
                otel.log_request(attempt, || req_builder.send()).await
            } else {
                req_builder.send().await
            };
            if let Ok(resp) = &res {
                trace!(
                    "Response status: {}, request-id: {}",
                    resp.status(),
                    resp.headers()
                        .get("x-request-id")
                        .map(|v| v.to_str().unwrap_or_default())
                        .unwrap_or_default()
                );
            }

            match res {
                Ok(resp) if resp.status().is_success() => {
                    if let Some(value) = resp
                        .headers()
                        .get(X_CODEX_TURN_STATE_HEADER)
                        .and_then(|value| value.to_str().ok())
                    {
                        if let Some(existing) = turn_state.get()
                            && existing != value
                        {
                            warn!(
                                existing,
                                new = value,
                                "received unexpected x-codex-turn-state during responses request"
                            );
                        } else {
                            let _ = turn_state.set(value.to_string());
                        }
                    }

                    // Log successful response initiation
                    if let Ok(logger) = self.debug_logger.lock() {
                        let _ = logger.append_response_event(
                            &request_id,
                            "stream_initiated",
                            &serde_json::json!({
                                "status": "success",
                                "status_code": resp.status().as_u16(),
                                "x_request_id": resp.headers()
                                    .get("x-request-id")
                                    .and_then(|v| v.to_str().ok())
                                    .unwrap_or_default()
                            }),
                        );
                    }
                    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(1600);

                    if let Some(snapshot) = sse::parse_rate_limit_snapshot(resp.headers()) {
                        debug!(
                            "rate limit headers:\n{}",
                            sse::format_rate_limit_headers(resp.headers())
                        );

                        if tx_event
                            .send(Ok(ResponseEvent::RateLimits(snapshot)))
                            .await
                            .is_err()
                        {
                            debug!("receiver dropped rate limit snapshot event");
                        }
                    }

                    let models_etag = resp
                        .headers()
                        .get("X-Models-Etag")
                        .and_then(|value| value.to_str().ok())
                        .map(ToString::to_string);
                    if let Some(etag) = models_etag
                        && tx_event
                            .send(Ok(ResponseEvent::ModelsEtag(etag)))
                            .await
                            .is_err()
                        {
                            debug!("receiver dropped models etag event");
                        }

                    // spawn task to process SSE
                    let stream = resp.bytes_stream().map_err(CodexErr::Reqwest);
                    let debug_logger = Arc::clone(&self.debug_logger);
                    let request_id_clone = request_id.clone();
                    let otel_event_manager = self.otel_event_manager.clone();
                    tokio::spawn(sse::process_sse(
                        stream,
                        tx_event,
                        self.provider.stream_idle_timeout(),
                        debug_logger,
                        request_id_clone,
                        otel_event_manager,
                        Arc::new(RwLock::new(sse::StreamCheckpoint::default())),
                    ));

                    return Ok(ResponseStream { rx_event });
                }
                Ok(res) => {
                    let status = res.status();
                    let headers = res.headers().clone();
                    if let Some(value) = headers
                        .get(X_CODEX_TURN_STATE_HEADER)
                        .and_then(|value| value.to_str().ok())
                    {
                        if let Some(existing) = turn_state.get()
                            && existing != value
                        {
                            warn!(
                                existing,
                                new = value,
                                "received unexpected x-codex-turn-state during responses request"
                            );
                        } else {
                            let _ = turn_state.set(value.to_string());
                        }
                    }
                    // Capture x-request-id up-front in case we consume the response body later.
                    let x_request_id = headers
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok())
                        .map(std::string::ToString::to_string);
                    let now = Utc::now();

                    // Pull out Retry‑After header if present.
                    let retry_after_hint = headers
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|raw| sse::parse_retry_after_header(raw, now));

                    if status == StatusCode::UNAUTHORIZED {
                        if let Some(manager) = auth_manager.as_ref() {
                            match manager.refresh_token_classified().await {
                                Ok(Some(_)) => {}
                                Ok(None) => {
                                    auth_refresh_error = Some(RefreshTokenError::permanent(
                                        AUTH_REQUIRED_MESSAGE,
                                    ));
                                }
                                Err(err) => {
                                    auth_refresh_error = Some(err);
                                }
                            }
                        } else {
                            auth_refresh_error = Some(RefreshTokenError::permanent(
                                "Authentication manager unavailable; please log in again.",
                            ));
                        }
                    }

                    // Read the response body once for diagnostics across error branches.
                    let body_text = res.text().await.unwrap_or_default();
                    let body = serde_json::from_str::<ErrorResponse>(&body_text).ok();

                    if status == StatusCode::TOO_MANY_REQUESTS
                        && let Some(model) = headers
                            .get(MODEL_CAP_MODEL_HEADER)
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_string)
                        {
                            let reset_after_seconds = headers
                                .get(MODEL_CAP_RESET_AFTER_HEADER)
                                .and_then(|value| value.to_str().ok())
                                .and_then(|value| value.parse::<u64>().ok());
                            return Err(CodexErr::ModelCap(ModelCapError {
                                model,
                                reset_after_seconds,
                            }));
                        }

                    if status == StatusCode::TOO_MANY_REQUESTS
                        && self.config.auto_switch_accounts_on_rate_limit
                        && auth_manager.is_some()
                        && auth::read_code_api_key_from_env().is_none()
                    {
                        let current_account_id = auth
                            .as_ref()
                            .and_then(super::auth::CodexAuth::get_account_id)
                            .or_else(|| {
                                auth_accounts::get_active_account_id(self.code_home())
                                    .ok()
                                    .flatten()
                            });
                        if let Some(current_account_id) = current_account_id {
                            let mut retry_after_delay = retry_after_hint.clone();
                            if retry_after_delay.is_none()
                                && let Some(ErrorResponse { ref error }) = body {
                                    retry_after_delay = try_parse_retry_after(error, now);
                                }

                            let current_auth_mode = auth
                                .as_ref()
                                .map(|a| a.mode)
                                .unwrap_or(AuthMode::ApiKey);

                            let switch_reason = match body
                                .as_ref()
                                .and_then(|err| err.error.r#type.as_deref())
                            {
                                Some("usage_limit_reached") => "usage_limit_reached",
                                Some("usage_not_included") => "usage_not_included",
                                _ => "http_429",
                            };

                            let (blocked_until, should_record_usage_limit) = match body.as_ref() {
                                Some(ErrorResponse { error })
                                    if error.r#type.as_deref() == Some("usage_limit_reached") =>
                                {
                                    (
                                        error
                                            .resets_in_seconds
                                            .map(|seconds| now + ChronoDuration::seconds(seconds as i64)),
                                        true,
                                    )
                                }
                                _ => (retry_after_delay.as_ref().map(|info| info.resume_at), false),
                            };

                            rate_limit_switch_state.mark_limited(
                                &current_account_id,
                                current_auth_mode,
                                blocked_until,
                            );

                            if let Ok(Some(next_account_id)) =
                                crate::account_switching::select_next_account_id(
                                    self.code_home(),
                                    &rate_limit_switch_state,
                                    self.config.api_key_fallback_on_all_accounts_limited,
                                    now,
                                    Some(current_account_id.as_str()),
                                )
                            {
                                if should_record_usage_limit {
                                    let plan_type = body
                                        .as_ref()
                                        .and_then(|err| err.error.plan_type.as_deref())
                                        .map(std::string::ToString::to_string);
                                    let resets_in_seconds =
                                        body.as_ref().and_then(|err| err.error.resets_in_seconds);
                                    let code_home = self.code_home().to_path_buf();
                                    let account_id = current_account_id.clone();
                                    tokio::task::spawn_blocking(move || {
                                        let observed_at = Utc::now();
                                        if let Err(err) = account_usage::record_usage_limit_hint(
                                            &code_home,
                                            &account_id,
                                            plan_type.as_deref(),
                                            resets_in_seconds,
                                            observed_at,
                                        ) {
                                            tracing::warn!("Failed to persist usage limit hint: {err}");
                                        }
                                    });
                                }

                                tracing::info!(
                                    from_account_id = %current_account_id,
                                    to_account_id = %next_account_id,
                                    reason = switch_reason,
                                    "rate limit hit; auto-switching active account"
                                );

                                if let Ok(logger) = self.debug_logger.lock() {
                                    let _ = logger.append_response_event(
                                        &request_id,
                                        "account_switch",
                                        &serde_json::json!({
                                            "reason": switch_reason,
                                            "from_account_id": current_account_id.clone(),
                                            "to_account_id": next_account_id.clone(),
                                            "status": status.as_u16(),
                                        }),
                                    );
                                }

                                if let Err(err) = auth::activate_account_with_store_mode(
                                    self.code_home(),
                                    &next_account_id,
                                    self.auth_credentials_store_mode(),
                                ) {
                                    tracing::warn!(
                                        from_account_id = %current_account_id,
                                        to_account_id = %next_account_id,
                                        error = %err,
                                        "failed to activate account after rate limit"
                                    );
                                } else {
                                    if let Some(manager) = auth_manager.as_ref() {
                                        manager.reload();
                                    }
                                    attempt = 0;
                                    continue;
                                }
                            }
                        }
                    }

                    if status == StatusCode::BAD_REQUEST
                        && let Some(ErrorResponse { ref error }) = body
                            && !self.reasoning_summary_disabled.load(Ordering::Relaxed)
                                && is_reasoning_summary_rejected(error)
                            {
                                self.disable_reasoning_summary();

                                if let Ok(logger) = self.debug_logger.lock() {
                                    let _ = logger.append_response_event(
                                        &request_id,
                                        "reasoning_summary_disabled",
                                        &serde_json::json!({
                                            "status": status.as_u16(),
                                            "message": error.message.clone(),
                                            "code": error.code.clone(),
                                            "param": error.param.clone(),
                                        }),
                                    );
                                }

                                // Retry immediately with reasoning summaries removed.
                                attempt = 0;
                                continue;
                            }

                    // The OpenAI Responses endpoint returns structured JSON bodies even for 4xx/5xx
                    // errors. When we bubble early with only the HTTP status the caller sees an opaque
                    // "unexpected status 400 Bad Request" which makes debugging nearly impossible.
                    // Instead, read (and include) the response text so higher layers and users see the
                    // exact error message (e.g. "Unknown parameter: 'input[0].metadata'"). The body is
                    // small and this branch only runs on error paths so the extra allocation is
                    // negligible.
                    if !(status == StatusCode::TOO_MANY_REQUESTS
                        || status == StatusCode::UNAUTHORIZED
                        || status.is_server_error())
                    {
                        // Log error response
                        if let Ok(logger) = self.debug_logger.lock() {
                            let _ = logger.append_response_event(
                                &request_id,
                                "error",
                                &serde_json::json!({
                                    "status": status.as_u16(),
                                    "body": body_text
                                }),
                            );
                            let _ = logger.end_request_log(&request_id);
                        }
                        return Err(CodexErr::UnexpectedStatus(UnexpectedResponseError {
                            status,
                            body: body_text,
                            request_id: None,
                        }));
                    }

                    if let Some(ErrorResponse { ref error }) = body
                        && is_quota_exceeded_http_error(status, error) {
                            return Err(CodexErr::QuotaExceeded);
                        }

                    if status == StatusCode::UNAUTHORIZED
                        && let Some(error) =
                            map_unauthorized_outcome(auth.is_some(), auth_refresh_error.as_ref())
                        {
                            return Err(error);
                        }

                    if status == StatusCode::TOO_MANY_REQUESTS
                        && let Some(ErrorResponse { ref error }) = body {
                            if error.r#type.as_deref() == Some("usage_limit_reached") {
                                // Prefer the plan_type provided in the error message if present
                                // because it's more up to date than the one encoded in the auth
                                // token.
                                let plan_type = error
                                    .plan_type
                                    .clone()
                                    .or_else(|| auth.and_then(|a| a.get_plan_type()));
                                let resets_in_seconds = error.resets_in_seconds;
                                return Err(CodexErr::UsageLimitReached(UsageLimitReachedError {
                                    plan_type,
                                    resets_in_seconds,
                                }));
                            } else if error.r#type.as_deref() == Some("usage_not_included") {
                                return Err(CodexErr::UsageNotIncluded);
                            }
                        }

                    if attempt > max_retries {
                        // On final attempt, surface rich diagnostics for server errors.
                        // On final attempt, surface rich diagnostics for server errors.
                        if status.is_server_error() {
                            let (message, body_excerpt) =
                                match serde_json::from_str::<ErrorResponse>(&body_text) {
                                    Ok(ErrorResponse { error }) => {
                                        let msg = error
                                            .message
                                            .unwrap_or_else(|| "server error".to_string());
                                        (msg, None)
                                    }
                                    Err(_) => {
                                        let mut excerpt = body_text;
                                        const MAX: usize = 600;
                                        if excerpt.len() > MAX {
                                            excerpt.truncate(MAX);
                                        }
                                        (
                                            "server error".to_string(),
                                            if excerpt.is_empty() {
                                                None
                                            } else {
                                                Some(excerpt)
                                            },
                                        )
                                    }
                                };

                            // Build a single-line, actionable message for the UI and logs.
                            let mut msg = format!("server error {status}: {message}");
                            if let Some(id) = &x_request_id {
                                msg.push_str(&format!(" (request-id: {id})"));
                            }
                            if let Some(excerpt) = &body_excerpt {
                                msg.push_str(&format!(" | body: {excerpt}"));
                            }

                            // Log detailed context to the debug logger and close the request log.
                            if let Ok(logger) = self.debug_logger.lock() {
                                let _ = logger.append_response_event(
                                    &request_id,
                                    "server_error_on_retry_limit",
                                    &serde_json::json!({
                                        "status": status.as_u16(),
                                        "x_request_id": x_request_id,
                                        "message": message,
                                        "body_excerpt": body_excerpt,
                                    }),
                                );
                                let _ = logger.end_request_log(&request_id);
                            }

                            return Err(CodexErr::ServerError(msg));
                        }

                        return Err(CodexErr::RetryLimit(RetryLimitReachedError {
                            status,
                            request_id: None,
                            retryable: status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS,
                        }));
                    }

                    let mut retry_after_delay = retry_after_hint;
                    if retry_after_delay.is_none()
                        && let Some(ErrorResponse { ref error }) = body {
                            retry_after_delay = try_parse_retry_after(error, now);
                        }

                    let delay = retry_after_delay
                        .as_ref()
                        .map(|info| info.delay)
                        .unwrap_or_else(|| backoff(attempt));
                    tokio::time::sleep(delay).await;
                }
                Err(e) => {
                    let is_connectivity = e.is_connect() || e.is_timeout() || e.is_request();
                    if attempt > max_retries {
                        // Log network error before surfacing.
                        if let Ok(logger) = self.debug_logger.lock() {
                            let _ = logger.log_error(&endpoint, &format!("Network error: {e}"), log_tag);
                        }
                        if is_connectivity {
                            let req_id = (!request_id.is_empty()).then(|| request_id.clone());
                            return Err(CodexErr::Stream(
                                format!("[transport] network unavailable: {e}"),
                                None,
                                req_id,
                            ));
                        }
                        return Err(e.into());
                    }
                    let delay = backoff(attempt);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    pub fn get_provider(&self) -> ModelProviderInfo {
        self.provider.clone()
    }

    /// Returns the currently configured model slug.
    #[allow(dead_code)]
    pub fn get_model(&self) -> String {
        self.config.model.clone()
    }

    pub fn model_explicit(&self) -> bool {
        self.config.model_explicit
    }

    pub fn model_personality(&self) -> Option<crate::config_types::Personality> {
        self.config.model_personality
    }

    /// Returns the currently configured model family.
    #[allow(dead_code)]
    pub fn get_model_family(&self) -> ModelFamily {
        self.config.model_family.clone()
    }

    #[allow(dead_code)]
    pub fn get_model_context_window(&self) -> Option<u64> {
        self.config.model_context_window
    }

    #[allow(dead_code)]
    pub fn get_auth_manager(&self) -> Option<Arc<AuthManager>> {
        self.auth_manager.clone()
    }

    pub async fn compact_conversation_history(&self, prompt: &Prompt) -> Result<Vec<ResponseItem>> {
        if prompt.input.is_empty() {
            return Ok(Vec::new());
        }

        let auth_manager = self.auth_manager.clone();
        let mut rate_limit_switch_state = crate::account_switching::RateLimitSwitchState::default();

        let model_slug = prompt
            .model_override
            .as_deref()
            .unwrap_or(self.config.model.as_str());
        let family = prompt
            .model_family_override
            .clone()
            .or_else(|| find_family_for_model(model_slug))
            .unwrap_or_else(|| self.config.model_family.clone());
        let instructions = prompt.get_full_instructions(&family).into_owned();
        let payload = CompactHistoryRequest {
            model: model_slug,
            input: &prompt.input,
            instructions: instructions.clone(),
        };
        let payload_json = serde_json::json!({
            "model": payload.model,
            "input": payload.input,
            "instructions": instructions,
        });
        let mut request_id = String::new();

        loop {
            let auth = auth_manager.as_ref().and_then(|m| m.auth());
            let mut request = self
                .provider
                .create_compact_request_builder(&self.client, &auth)
                .await?;

            // Ensure Responses API beta header is present for compact calls. Mirror the
            // streaming path: use the public "responses=v1" header for the public OpenAI
            // endpoint and fall back to "responses=experimental" for other providers.
            let has_beta_header = request
                .try_clone()
                .and_then(|builder| builder.build().ok())
                .is_some_and(|req| req.headers().contains_key("OpenAI-Beta"));

            if !has_beta_header {
                let beta_value = if self.provider.is_public_openai_responses_endpoint() {
                    RESPONSES_BETA_HEADER_V1
                } else {
                    RESPONSES_BETA_HEADER_EXPERIMENTAL
                };
                request = request.header("OpenAI-Beta", beta_value);
            }

            request = transport::attach_openai_subagent_header(request);
            request = transport::attach_codex_beta_features_header(request, &self.config);
            request = transport::attach_web_search_eligible_header(request, &self.config);

            if let Some(auth) = auth.as_ref()
                && auth.mode.is_chatgpt()
                && let Some(account_id) = auth.get_account_id()
            {
                request = request.header("chatgpt-account-id", account_id);
            }

            request = request.json(&payload);

            let header_snapshot = request
                .try_clone()
                .and_then(|builder| builder.build().ok())
                .map(|req| sse::header_map_to_json(req.headers()));

            if request_id.is_empty()
                && let Ok(logger) = self.debug_logger.lock() {
                    let endpoint = self
                        .provider
                        .get_compact_url(&auth)
                        .unwrap_or_else(|| self.provider.get_full_url(&auth));
                    request_id = logger
                        .start_request_log(
                            &endpoint,
                            &payload_json,
                            header_snapshot.as_ref(),
                            Some("compact_remote"),
                        )
                        .unwrap_or_default();
                }

            let response = request.send().await?;
            let status = response.status();
            let body = response.text().await?;

            if status == StatusCode::TOO_MANY_REQUESTS
                && self.config.auto_switch_accounts_on_rate_limit
                && auth_manager.is_some()
                && auth::read_code_api_key_from_env().is_none()
            {
                let now = Utc::now();
                let current_account_id = auth
                    .as_ref()
                    .and_then(super::auth::CodexAuth::get_account_id)
                    .or_else(|| {
                        auth_accounts::get_active_account_id(self.code_home())
                            .ok()
                            .flatten()
                    });
                if let Some(current_account_id) = current_account_id {
                    let current_auth_mode = auth
                        .as_ref()
                        .map(|a| a.mode)
                        .unwrap_or(AuthMode::ApiKey);
                    rate_limit_switch_state.mark_limited(
                        &current_account_id,
                        current_auth_mode,
                        None,
                    );
                    if let Ok(Some(next_account_id)) =
                        crate::account_switching::select_next_account_id(
                            self.code_home(),
                            &rate_limit_switch_state,
                            self.config.api_key_fallback_on_all_accounts_limited,
                            now,
                            Some(current_account_id.as_str()),
                        )
                    {
                        tracing::info!(
                            from_account_id = %current_account_id,
                            to_account_id = %next_account_id,
                            "rate limit hit during compact; auto-switching active account"
                        );
                        if let Err(err) = auth::activate_account_with_store_mode(
                            self.code_home(),
                            &next_account_id,
                            self.auth_credentials_store_mode(),
                        ) {
                            tracing::warn!(
                                from_account_id = %current_account_id,
                                to_account_id = %next_account_id,
                                error = %err,
                                "failed to activate account after rate limit during compact"
                            );
                        } else {
                            if let Some(manager) = auth_manager.as_ref() {
                                manager.reload();
                            }
                            continue;
                        }
                    }
                }
            }

            if let Ok(logger) = self.debug_logger.lock() {
                let response_body: serde_json::Value = serde_json::from_str(&body)
                    .unwrap_or_else(|_| serde_json::json!({ "raw": body }));
                let _ = logger.append_response_event(
                    &request_id,
                    "compact_response",
                    &serde_json::json!({
                        "status_code": status.as_u16(),
                        "body": response_body,
                    }),
                );
                let _ = logger.end_request_log(&request_id);
            }

            if !status.is_success() {
                return Err(CodexErr::UnexpectedStatus(UnexpectedResponseError {
                    status,
                    body,
                    request_id: None,
                }));
            }

            let CompactHistoryResponse { output } = serde_json::from_str(&body)?;
            return Ok(output);
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests;
