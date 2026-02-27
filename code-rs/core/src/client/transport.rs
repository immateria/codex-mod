use reqwest::StatusCode;
use reqwest::header::HeaderValue;
use serde::Deserialize;

use crate::config::Config;
use crate::config_types::TextVerbosity as TextVerbosityConfig;
use crate::error::{CodexErr, UnexpectedResponseError, UsageLimitReachedError};

use super::Error;
use tokio_tungstenite::tungstenite::Error as WsError;

const CODE_OPENAI_SUBAGENT_ENV: &str = "CODE_OPENAI_SUBAGENT";
const WEB_SEARCH_ELIGIBLE_HEADER: &str = "x-oai-web-search-eligible";

#[derive(Debug, Deserialize)]
pub(super) struct WrappedWebsocketErrorEvent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(alias = "status_code")]
    status: Option<u16>,
    #[serde(default)]
    error: Option<Error>,
}

pub(super) fn attach_codex_beta_features_header(
    builder: reqwest::RequestBuilder,
    config: &Config,
) -> reqwest::RequestBuilder {
    let Some(value) = codex_beta_features_header_value(config) else {
        return builder;
    };

    let has_header = builder
        .try_clone()
        .and_then(|builder| builder.build().ok())
        .is_some_and(|req| req.headers().contains_key("x-codex-beta-features"));
    if has_header {
        return builder;
    }

    builder.header("x-codex-beta-features", value)
}

pub(super) fn parse_wrapped_websocket_error_event(payload: &str) -> Option<WrappedWebsocketErrorEvent> {
    let event: WrappedWebsocketErrorEvent = serde_json::from_str(payload).ok()?;
    if event.kind != "error" {
        return None;
    }
    Some(event)
}

pub(super) fn map_wrapped_websocket_error_event(event: WrappedWebsocketErrorEvent) -> Option<CodexErr> {
    let status = match event.status.and_then(|value| StatusCode::from_u16(value).ok()) {
        Some(status) => status,
        None => {
            if let Some(error) = event.error {
                let message = error
                    .message
                    .unwrap_or_else(|| "websocket returned an error event".to_string());
                return Some(CodexErr::Stream(message, None, None));
            }
            return Some(CodexErr::Stream(
                "websocket returned an error event".to_string(),
                None,
                None,
            ));
        }
    };
    if status.is_success() {
        return None;
    }

    let body = if let Some(error) = event.error {
        if status == StatusCode::TOO_MANY_REQUESTS {
            if error.r#type.as_deref() == Some("usage_limit_reached") {
                return Some(CodexErr::UsageLimitReached(UsageLimitReachedError {
                    plan_type: error.plan_type,
                    resets_in_seconds: error.resets_in_seconds,
                }));
            }

            if error.r#type.as_deref() == Some("usage_not_included") {
                return Some(CodexErr::UsageNotIncluded);
            }
        }

        if super::is_quota_exceeded_error(&error) {
            return Some(CodexErr::QuotaExceeded);
        }

        if super::is_server_overloaded_error(&error) {
            return Some(CodexErr::ServerOverloaded);
        }

        serde_json::json!({
            "error": {
                "type": error.r#type,
                "code": error.code,
                "param": error.param,
                "message": error.message,
                "plan_type": error.plan_type,
                "resets_in_seconds": error.resets_in_seconds,
            }
        })
        .to_string()
    } else {
        serde_json::json!({
            "error": {
                "message": "websocket returned an error event"
            }
        })
        .to_string()
    };

    Some(CodexErr::UnexpectedStatus(UnexpectedResponseError {
        status,
        body,
        request_id: None,
    }))
}

pub(super) fn attach_web_search_eligible_header(
    builder: reqwest::RequestBuilder,
    config: &Config,
) -> reqwest::RequestBuilder {
    let has_header = builder
        .try_clone()
        .and_then(|builder| builder.build().ok())
        .is_some_and(|req| req.headers().contains_key(WEB_SEARCH_ELIGIBLE_HEADER));
    if has_header {
        return builder;
    }

    let value = if config.tools_web_search_request {
        "true"
    } else {
        "false"
    };
    builder.header(WEB_SEARCH_ELIGIBLE_HEADER, HeaderValue::from_static(value))
}

pub(super) fn websocket_connect_is_upgrade_required(error: &WsError) -> bool {
    matches!(
        error,
        WsError::Http(response)
            if response.status().as_u16() == 426
    )
}

fn codex_beta_features_header_value(config: &Config) -> Option<HeaderValue> {
    let mut enabled: Vec<&'static str> = Vec::new();

    if config.skills_enabled {
        enabled.push("skills");
    }
    if config.tools_web_search_request {
        enabled.push("web_search_request");
    }

    let value = enabled.join(",");
    if value.is_empty() {
        return None;
    }

    HeaderValue::from_str(value.as_str()).ok()
}

pub(super) fn attach_openai_subagent_header(builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    let Some(value) = openai_subagent_header_value() else {
        return builder;
    };

    let has_header = builder
        .try_clone()
        .and_then(|builder| builder.build().ok())
        .is_some_and(|req| req.headers().contains_key("x-openai-subagent"));
    if has_header {
        return builder;
    }

    builder.header("x-openai-subagent", value)
}

fn openai_subagent_header_value() -> Option<HeaderValue> {
    let subagent = std::env::var(CODE_OPENAI_SUBAGENT_ENV).ok()?;
    let subagent = subagent.trim();
    if subagent.is_empty() {
        return None;
    }
    HeaderValue::from_str(subagent).ok()
}

pub(super) fn clamp_text_verbosity_for_model(
    model: &str,
    requested: TextVerbosityConfig,
) -> TextVerbosityConfig {
    let allowed = supported_text_verbosity_for_model(model);
    if allowed.iter().any(|v| v == &requested) {
        return requested;
    }

    if let Some(medium) = allowed.iter().find(|v| matches!(v, TextVerbosityConfig::Medium)) {
        tracing::debug!(
            model,
            requested = ?requested,
            fallback = ?medium,
            "text verbosity clamped to supported value for model",
        );
        return *medium;
    }

    let fallback = *allowed.first().unwrap_or(&TextVerbosityConfig::Medium);
    tracing::debug!(
        model,
        requested = ?requested,
        fallback = ?fallback,
        "text verbosity clamped to first supported value for model",
    );
    fallback
}

fn supported_text_verbosity_for_model(model: &str) -> &'static [TextVerbosityConfig] {
    if model.eq_ignore_ascii_case("gpt-5.1-codex-max") {
        return &[TextVerbosityConfig::Medium];
    }

    const ALL: &[TextVerbosityConfig] = &[
        TextVerbosityConfig::Low,
        TextVerbosityConfig::Medium,
        TextVerbosityConfig::High,
    ];
    ALL
}
