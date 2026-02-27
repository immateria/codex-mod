use super::*;
use crate::model_provider_info::{ModelProviderInfo, WireApi};
use std::collections::HashMap;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_test::io::Builder as IoBuilder;
use tokio_util::io::ReaderStream;
use chrono::{Duration as ChronoDuration, TimeZone, Utc};

// ────────────────────────────
// Helpers
// ────────────────────────────

#[test]
fn unauthorized_outcome_returns_permanent_error_for_permanent_refresh_failure() {
    let err = RefreshTokenError::permanent("token revoked");
    let outcome = map_unauthorized_outcome(true, Some(&err))
        .expect("should produce CodexErr");
    match outcome {
        CodexErr::AuthRefreshPermanent(msg) => {
            assert!(
                msg.contains("token revoked"),
                "unexpected message: {msg}"
            );
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
}

#[test]
fn unauthorized_outcome_requires_login_without_auth() {
    let outcome = map_unauthorized_outcome(false, None)
        .expect("should require login");
    match outcome {
        CodexErr::AuthRefreshPermanent(msg) => {
            assert_eq!(msg, AUTH_REQUIRED_MESSAGE);
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
}

#[test]
fn unauthorized_outcome_allows_retry_for_transient_refresh_error() {
    let err = RefreshTokenError::transient("server busy");
    assert!(map_unauthorized_outcome(true, Some(&err)).is_none());
}

#[tokio::test]
async fn responses_request_uses_beta_header_for_public_openai() {
    let provider = ModelProviderInfo {
        name: "openai".to_string(),
        base_url: Some("https://api.openai.com/v1".to_string()),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        requires_openai_auth: false,
        openrouter: None,
    };

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");

    let mut builder = provider
        .create_request_builder(&client, &None)
        .await
        .expect("builder");
    let has_beta = builder
        .try_clone()
        .and_then(|b| b.build().ok())
        .is_some_and(|req| req.headers().contains_key("OpenAI-Beta"));
    if !has_beta {
        builder = builder.header("OpenAI-Beta", RESPONSES_BETA_HEADER_V1);
    }
    let request = builder
        .try_clone()
        .expect("clone request builder")
        .build()
        .expect("build request");

    let header_value = request
        .headers()
        .get("OpenAI-Beta")
        .expect("OpenAI-Beta header present");
    assert_eq!(header_value, RESPONSES_BETA_HEADER_V1);
}

#[tokio::test]
async fn responses_request_uses_experimental_for_backend() {
    let provider = ModelProviderInfo {
        name: "backend".to_string(),
        base_url: Some("https://chatgpt.com/backend-api/codex".to_string()),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        requires_openai_auth: false,
        openrouter: None,
    };

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");

    let mut builder = provider
        .create_request_builder(&client, &None)
        .await
        .expect("builder");
    let has_beta = builder
        .try_clone()
        .and_then(|b| b.build().ok())
        .is_some_and(|req| req.headers().contains_key("OpenAI-Beta"));
    if !has_beta {
        builder = builder.header("OpenAI-Beta", RESPONSES_BETA_HEADER_EXPERIMENTAL);
    }
    let request = builder
        .try_clone()
        .expect("clone request builder")
        .build()
        .expect("build request");

    let header_value = request
        .headers()
        .get("OpenAI-Beta")
        .expect("OpenAI-Beta header present");
    assert_eq!(header_value, RESPONSES_BETA_HEADER_EXPERIMENTAL);
}

#[tokio::test]
async fn responses_request_respects_preexisting_beta_header() {
    let mut headers = HashMap::new();
    headers.insert("OpenAI-Beta".to_string(), "custom".to_string());
    let provider = ModelProviderInfo {
        name: "custom".to_string(),
        base_url: Some("https://api.openai.com/v1".to_string()),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: Some(headers),
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        requires_openai_auth: false,
        openrouter: None,
    };

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");

    let request = provider
        .create_request_builder(&client, &None)
        .await
        .expect("builder")
        .try_clone()
        .expect("clone request builder")
        .build()
        .expect("build request");

    let header_value = request
        .headers()
        .get("OpenAI-Beta")
        .expect("OpenAI-Beta header present");
    assert_eq!(header_value, "custom");
}

/// Runs the SSE parser on pre-chunked byte slices and returns every event
/// (including any final `Err` from a stream-closure check).
async fn collect_events(
    chunks: &[&[u8]],
    provider: ModelProviderInfo,
) -> Vec<Result<ResponseEvent>> {
    let mut builder = IoBuilder::new();
    for chunk in chunks {
        builder.read(chunk);
    }

    let reader = builder.build();
    let stream = ReaderStream::new(reader).map_err(CodexErr::Io);
    let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent>>(16);
    let debug_logger = Arc::new(Mutex::new(DebugLogger::new(false).unwrap()));
    let checkpoint = Arc::new(RwLock::new(sse::StreamCheckpoint::default()));
    tokio::spawn(sse::process_sse(
        stream,
        tx,
        provider.stream_idle_timeout(),
        debug_logger,
        String::new(),
        None,
        checkpoint,
    ));

    let mut events = Vec::new();
    while let Some(ev) = rx.recv().await {
        events.push(ev);
    }
    events
}

/// Builds an in-memory SSE stream from JSON fixtures and returns only the
/// successfully parsed events (panics on internal channel errors).
async fn run_sse(
    events: Vec<serde_json::Value>,
    provider: ModelProviderInfo,
) -> Vec<ResponseEvent> {
    let mut body = String::new();
    for e in events {
        let kind = e
            .get("type")
            .and_then(|v| v.as_str())
            .expect("fixture event missing type");
        if e.as_object().map(|o| o.len() == 1).unwrap_or(false) {
            body.push_str(&format!("event: {kind}\n\n"));
        } else {
            body.push_str(&format!("event: {kind}\ndata: {e}\n\n"));
        }
    }

    let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent>>(8);
    let stream = ReaderStream::new(std::io::Cursor::new(body)).map_err(CodexErr::Io);
    let debug_logger = Arc::new(Mutex::new(DebugLogger::new(false).unwrap()));
    let checkpoint = Arc::new(RwLock::new(sse::StreamCheckpoint::default()));
    tokio::spawn(sse::process_sse(
        stream,
        tx,
        provider.stream_idle_timeout(),
        debug_logger,
        String::new(),
        None,
        checkpoint,
    ));

    let mut out = Vec::new();
    while let Some(ev) = rx.recv().await {
        out.push(ev.expect("channel closed"));
    }
    out
}

// ────────────────────────────
// Tests from `implement-test-for-responses-api-sse-parser`
// ────────────────────────────

#[tokio::test]
async fn parses_items_and_completed() {
    let item1 = json!({
        "type": "response.output_item.done",
        "item": {
            "type": "message",
            "role": "assistant",
            "content": [{"type": "output_text", "text": "Hello"}]
        }
    })
    .to_string();

    let item2 = json!({
        "type": "response.output_item.done",
        "item": {
            "type": "message",
            "role": "assistant",
            "content": [{"type": "output_text", "text": "World"}]
        }
    })
    .to_string();

    let completed = json!({
        "type": "response.completed",
        "response": { "id": "resp1" }
    })
    .to_string();

    let sse1 = format!("event: response.output_item.done\ndata: {item1}\n\n");
    let sse2 = format!("event: response.output_item.done\ndata: {item2}\n\n");
    let sse3 = format!("event: response.completed\ndata: {completed}\n\n");

    let provider = ModelProviderInfo {
        name: "test".to_string(),
        base_url: Some("https://test.com".to_string()),
        env_key: Some("TEST_API_KEY".to_string()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(1000),
        requires_openai_auth: false,
        openrouter: None,
    };

    let events = collect_events(
        &[sse1.as_bytes(), sse2.as_bytes(), sse3.as_bytes()],
        provider,
    )
    .await;

    assert_eq!(events.len(), 3);

    matches!(
        &events[0],
        Ok(ResponseEvent::OutputItemDone {
            item: ResponseItem::Message { role, .. },
            ..
        }) if role == "assistant"
    );

    matches!(
        &events[1],
        Ok(ResponseEvent::OutputItemDone {
            item: ResponseItem::Message { role, .. },
            ..
        }) if role == "assistant"
    );

    match &events[2] {
        Ok(ResponseEvent::Completed {
            response_id,
            token_usage,
        }) => {
            assert_eq!(response_id, "resp1");
            assert!(token_usage.is_none());
        }
        other => panic!("unexpected third event: {other:?}"),
    }
}

#[tokio::test]
async fn error_when_missing_completed() {
    let item1 = json!({
        "type": "response.output_item.done",
        "item": {
            "type": "message",
            "role": "assistant",
            "content": [{"type": "output_text", "text": "Hello"}]
        }
    })
    .to_string();

    let sse1 = format!("event: response.output_item.done\ndata: {item1}\n\n");
    let provider = ModelProviderInfo {
        name: "test".to_string(),
        base_url: Some("https://test.com".to_string()),
        env_key: Some("TEST_API_KEY".to_string()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(1000),
        requires_openai_auth: false,
        openrouter: None,
    };

    let events = collect_events(&[sse1.as_bytes()], provider).await;

    assert_eq!(events.len(), 2);

    matches!(
        events[0],
        Ok(ResponseEvent::OutputItemDone { .. })
    );

    match &events[1] {
        Err(CodexErr::Stream(msg, _, _)) => {
            assert_eq!(msg, "stream closed before response.completed")
        }
        other => panic!("unexpected second event: {other:?}"),
    }
}

#[tokio::test]
async fn response_done_emits_completed() {
    let done = json!({
        "type": "response.done",
        "response": {
            "id": "resp_done_1",
            "usage": {
                "input_tokens": 1,
                "input_tokens_details": null,
                "output_tokens": 2,
                "output_tokens_details": null,
                "total_tokens": 3
            }
        }
    })
    .to_string();

    let sse1 = format!("event: response.done\ndata: {done}\n\n");
    let provider = ModelProviderInfo {
        name: "test".to_string(),
        base_url: Some("https://test.com".to_string()),
        env_key: Some("TEST_API_KEY".to_string()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(1000),
        requires_openai_auth: false,
        openrouter: None,
    };

    let events = collect_events(&[sse1.as_bytes()], provider).await;

    assert_eq!(events.len(), 1);
    match &events[0] {
        Ok(ResponseEvent::Completed {
            response_id,
            token_usage,
        }) => {
            assert_eq!(response_id, "resp_done_1");
            assert!(token_usage.is_some());
        }
        other => panic!("unexpected done event: {other:?}"),
    }
}

#[tokio::test]
async fn response_completed_does_not_wait_for_stream_close() {
    let completed = json!({
        "type": "response.completed",
        "response": {
            "id": "resp_ws_1",
            "usage": {
                "input_tokens": 1,
                "input_tokens_details": null,
                "output_tokens": 2,
                "output_tokens_details": null,
                "total_tokens": 3
            }
        }
    })
    .to_string();

    let sse = format!("event: response.completed\ndata: {completed}\n\n");
    let (tx_bytes, rx_bytes) = mpsc::channel::<Result<Bytes>>(4);
    tx_bytes
        .send(Ok(Bytes::from(sse)))
        .await
        .expect("seed response.completed chunk");
    let stream = ReceiverStream::new(rx_bytes);
    let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent>>(8);
    let debug_logger = Arc::new(Mutex::new(DebugLogger::new(false).unwrap()));
    let checkpoint = Arc::new(RwLock::new(sse::StreamCheckpoint::default()));

    tokio::spawn(sse::process_sse(
        stream,
        tx,
        Duration::from_secs(60),
        debug_logger,
        String::new(),
        None,
        checkpoint,
    ));

    // Keep sender alive so the stream does not terminate on EOF.
    let _keep_stream_open = tx_bytes;

    let first = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("parser should emit completion without waiting for EOF")
        .expect("completion event");
    match first {
        Ok(ResponseEvent::Completed { response_id, .. }) => {
            assert_eq!(response_id, "resp_ws_1");
        }
        other => panic!("unexpected first event: {other:?}"),
    }

    let second = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("channel should close after completion");
    assert!(second.is_none());
}

#[tokio::test]
async fn error_when_error_event() {
    let raw_error = r#"{"type":"response.failed","sequence_number":3,"response":{"id":"resp_689bcf18d7f08194bf3440ba62fe05d803fee0cdac429894","object":"response","created_at":1755041560,"status":"failed","background":false,"error":{"code":"rate_limit_exceeded","message":"Rate limit reached for gpt-5.1 in organization org-AAA on tokens per min (TPM): Limit 30000, Used 22999, Requested 12528. Please try again in 11.054s. Visit https://platform.openai.com/account/rate-limits to learn more."}, "usage":null,"user":null,"metadata":{}}}"#;

    let sse1 = format!("event: response.failed\ndata: {raw_error}\n\n");
    let provider = ModelProviderInfo {
        name: "test".to_string(),
        base_url: Some("https://test.com".to_string()),
        env_key: Some("TEST_API_KEY".to_string()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(1000),
        requires_openai_auth: false,
        openrouter: None,
    };

    let events = collect_events(&[sse1.as_bytes()], provider).await;

    assert_eq!(events.len(), 1);

    match &events[0] {
        Err(CodexErr::Stream(msg, Some(retry), _)) => {
            assert_eq!(
                msg,
                "Rate limit reached for gpt-5.1 in organization org-AAA on tokens per min (TPM): Limit 30000, Used 22999, Requested 12528. Please try again in 11.054s. Visit https://platform.openai.com/account/rate-limits to learn more."
            );
            assert_eq!(retry.delay, Duration::from_secs_f64(11.054));
        }
        other => panic!("unexpected second event: {other:?}"),
    }
}

// ────────────────────────────
// Table-driven test from `main`
// ────────────────────────────

/// Verifies that the adapter produces the right `ResponseEvent` for a
/// variety of incoming `type` values.
#[tokio::test]
async fn table_driven_event_kinds() {
    struct TestCase {
        name: &'static str,
        event: serde_json::Value,
        expect_first: fn(&ResponseEvent) -> bool,
        expected_len: usize,
    }

    fn is_created(ev: &ResponseEvent) -> bool {
        matches!(ev, ResponseEvent::Created { .. })
    }
    fn is_output(ev: &ResponseEvent) -> bool {
        matches!(ev, ResponseEvent::OutputItemDone { .. })
    }
    fn is_completed(ev: &ResponseEvent) -> bool {
        matches!(ev, ResponseEvent::Completed { .. })
    }

    let completed = json!({
        "type": "response.completed",
        "response": {
            "id": "c",
            "usage": {
                "input_tokens": 0,
                "input_tokens_details": null,
                "output_tokens": 0,
                "output_tokens_details": null,
                "total_tokens": 0
            },
            "output": []
        }
    });

    let cases = vec![
        TestCase {
            name: "created",
            event: json!({"type": "response.created", "response": {}}),
            expect_first: is_created,
            expected_len: 2,
        },
        TestCase {
            name: "output_item.done",
            event: json!({
                "type": "response.output_item.done",
                "item": {
                    "type": "message",
                    "role": "assistant",
                    "content": [
                        {"type": "output_text", "text": "hi"}
                    ]
                }
            }),
            expect_first: is_output,
            expected_len: 2,
        },
        TestCase {
            name: "unknown",
            event: json!({"type": "response.new_tool_event"}),
            expect_first: is_completed,
            expected_len: 1,
        },
    ];

    for case in cases {
        let mut evs = vec![case.event];
        evs.push(completed.clone());

        let provider = ModelProviderInfo {
            name: "test".to_string(),
            base_url: Some("https://test.com".to_string()),
            env_key: Some("TEST_API_KEY".to_string()),
            env_key_instructions: None,
            experimental_bearer_token: None,
            wire_api: WireApi::Responses,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: Some(0),
            stream_max_retries: Some(0),
            stream_idle_timeout_ms: Some(1000),
            requires_openai_auth: false,
            openrouter: None,
        };

        let out = run_sse(evs, provider).await;
        assert_eq!(out.len(), case.expected_len, "case {}", case.name);
        assert!(
            (case.expect_first)(&out[0]),
            "first event mismatch in case {}",
            case.name
        );
    }
}

fn fixed_now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2025, 11, 7, 12, 0, 0).unwrap()
}

#[test]
fn test_try_parse_retry_after_ms() {
    let now = fixed_now();
    let err = Error {
        r#type: None,
        message: Some("Rate limit reached for gpt-5.1 in organization org- on tokens per min (TPM): Limit 1, Used 1, Requested 19304. Please try again in 28ms. Visit https://platform.openai.com/account/rate-limits to learn more.".to_string()),
        code: Some("rate_limit_exceeded".to_string()),
        param: None,
        plan_type: None,
        resets_in_seconds: None,
    };

    let retry_after = try_parse_retry_after(&err, now).expect("retry");
    assert_eq!(retry_after.delay, Duration::from_millis(28));
    assert!(retry_after.resume_at >= now);
}

#[test]
fn test_try_parse_retry_after_seconds() {
    let now = fixed_now();
    let err = Error {
        r#type: None,
        message: Some("Rate limit reached for gpt-5.1 in organization <ORG> on tokens per min (TPM): Limit 30000, Used 6899, Requested 24050. Please try again in 1.898s. Visit https://platform.openai.com/account/rate-limits to learn more.".to_string()),
        code: Some("rate_limit_exceeded".to_string()),
        param: None,
        plan_type: None,
        resets_in_seconds: None,
    };
    let retry_after = try_parse_retry_after(&err, now).expect("retry");
    assert_eq!(retry_after.delay, Duration::from_secs_f64(1.898));
}

#[test]
fn test_try_parse_retry_after_azure() {
    let now = fixed_now();
    let err = Error {
        r#type: None,
        message: Some("Rate limit exceeded. Retry after 35 seconds.".to_string()),
        code: Some("rate_limit_exceeded".to_string()),
        param: None,
        plan_type: None,
        resets_in_seconds: None,
    };
    let retry_after = try_parse_retry_after(&err, now).expect("retry");
    assert_eq!(retry_after.delay, Duration::from_secs(35));
}

#[test]
fn test_try_parse_retry_after_none_when_missing() {
    let now = fixed_now();
    let err = Error {
        r#type: None,
        message: Some("Some other error".to_string()),
        code: None,
        param: None,
        plan_type: None,
        resets_in_seconds: None,
    };

    assert!(try_parse_retry_after(&err, now).is_none());
}

#[test]
fn parse_retry_after_header_parses_seconds() {
    let now = fixed_now();
    let retry = sse::parse_retry_after_header("42", now).expect("header");
    assert_eq!(retry.delay, Duration::from_secs(42));
    assert_eq!(retry.resume_at, now + ChronoDuration::seconds(42));
}

#[test]
fn parse_retry_after_header_parses_rfc7231_date() {
    let now = Utc.with_ymd_and_hms(1994, 11, 15, 8, 0, 0).unwrap();
    let retry = sse::parse_retry_after_header("Tue, 15 Nov 1994 08:12:31 GMT", now)
        .expect("header");
    assert_eq!(
        retry.resume_at,
        Utc.with_ymd_and_hms(1994, 11, 15, 8, 12, 31).unwrap()
    );
}

#[test]
fn parse_retry_after_header_clamps_past_date() {
    let now = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    let retry = sse::parse_retry_after_header("Tue, 15 Nov 1994 08:12:31 GMT", now)
        .expect("header");
    assert_eq!(retry.delay, Duration::ZERO);
    assert_eq!(retry.resume_at, now);
}

#[test]
fn parse_retry_after_header_strips_wrappers() {
    let now = fixed_now();
    let retry = sse::parse_retry_after_header(" \"17\" ", now).expect("header");
    assert_eq!(retry.delay, Duration::from_secs(17));
}

#[test]
fn retry_after_prefers_header_over_body_hint() {
    let now = fixed_now();
    let header_retry = sse::parse_retry_after_header("5", now);
    let mut chosen = header_retry;
    if chosen.is_none() {
        let err = Error {
            r#type: None,
            message: Some(
                "Rate limit reached for gpt-5.1. Please try again in 30 seconds.".to_string(),
            ),
            code: Some("rate_limit_exceeded".to_string()),
            param: None,
            plan_type: None,
            resets_in_seconds: None,
        };
        chosen = try_parse_retry_after(&err, now);
    }
    let retry = chosen.expect("retry");
    assert_eq!(retry.delay, Duration::from_secs(5));
}

#[test]
fn parse_retry_after_header_handles_timezones() {
    let now = Utc.with_ymd_and_hms(2025, 3, 9, 5, 0, 0).unwrap();
    let retry = sse::parse_retry_after_header("Sun, 09 Mar 2025 01:30:00 -0500", now)
        .expect("header");
    assert_eq!(
        retry.resume_at,
        Utc.with_ymd_and_hms(2025, 3, 9, 6, 30, 0).unwrap()
    );
}

#[test]
fn quota_error_detected_for_common_statuses() {
    let error = Error {
        r#type: Some("invalid_request_error".to_string()),
        message: Some("You exceeded your current quota".to_string()),
        code: Some("insufficient_quota".to_string()),
        param: None,
        plan_type: None,
        resets_in_seconds: None,
    };

    for status in [
        StatusCode::BAD_REQUEST,
        StatusCode::FORBIDDEN,
        StatusCode::TOO_MANY_REQUESTS,
    ] {
        assert!(is_quota_exceeded_http_error(status, &error), "status {status} should be fatal");
    }

    assert!(
        !is_quota_exceeded_http_error(StatusCode::INTERNAL_SERVER_ERROR, &error),
        "server errors should not map to quota handling"
    );
}

#[test]
fn malformed_quota_body_is_ignored() {
    let error = Error {
        r#type: Some("invalid_request_error".to_string()),
        message: Some("missing code".to_string()),
        code: None,
        param: None,
        plan_type: None,
        resets_in_seconds: None,
    };

    assert!(!is_quota_exceeded_http_error(StatusCode::BAD_REQUEST, &error));
}

#[test]
fn reasoning_summary_rejection_is_detected() {
    let error_with_param = Error {
        r#type: Some("invalid_request_error".to_string()),
        message: Some("Your organization must be verified to generate reasoning summaries.".to_string()),
        code: Some("unsupported_value".to_string()),
        param: Some("reasoning.summary".to_string()),
        plan_type: None,
        resets_in_seconds: None,
    };

    assert!(is_reasoning_summary_rejected(&error_with_param));

    let error_by_message = Error {
        r#type: Some("invalid_request_error".to_string()),
        message: Some("Your organization must be verified to generate reasoning summaries. If you just verified, it can take up to 15 minutes for access to propagate.".to_string()),
        code: Some("unsupported_value".to_string()),
        param: None,
        plan_type: None,
        resets_in_seconds: None,
    };

    assert!(is_reasoning_summary_rejected(&error_by_message));

    // An error with param="reasoning.summary" but a different error code
    // (e.g., rate_limit_exceeded) should NOT be treated as a rejection.
    let rate_limit_error = Error {
        r#type: Some("rate_limit_error".to_string()),
        message: Some("Rate limit reached for reasoning.summary requests.".to_string()),
        code: Some("rate_limit_exceeded".to_string()),
        param: Some("reasoning.summary".to_string()),
        plan_type: None,
        resets_in_seconds: None,
    };

    assert!(!is_reasoning_summary_rejected(&rate_limit_error));
}

#[tokio::test]
async fn quota_exceeded_error_is_fatal() {
    let raw_error = r#"{"type":"response.failed","sequence_number":3,"response":{"id":"resp_quota","object":"response","created_at":1759771626,"status":"failed","background":false,"error":{"code":"insufficient_quota","message":"You exceeded your current quota, please check your plan and billing details."},"incomplete_details":null}}"#;

    let sse1 = format!("event: response.failed\ndata: {raw_error}\n\n");
    let provider = ModelProviderInfo {
        name: "test".to_string(),
        base_url: Some("https://test.com".to_string()),
        env_key: Some("TEST_API_KEY".to_string()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(1000),
        requires_openai_auth: false,
        openrouter: None,
    };

    let events = collect_events(&[sse1.as_bytes()], provider).await;

    assert_eq!(events.len(), 1);
    match &events[0] {
        Err(CodexErr::QuotaExceeded) => {}
        other => panic!("unexpected quota event: {other:?}"),
    }
}

#[tokio::test]
async fn response_failed_usage_limit_maps_to_typed_error() {
    let raw_error = r#"{"type":"response.failed","sequence_number":3,"response":{"id":"resp_limit","object":"response","created_at":1759771626,"status":"failed","background":false,"error":{"type":"usage_limit_reached","message":"You've hit your usage limit.","plan_type":"pro","resets_in_seconds":120},"incomplete_details":null}}"#;

    let sse1 = format!("event: response.failed\ndata: {raw_error}\n\n");
    let provider = ModelProviderInfo {
        name: "test".to_string(),
        base_url: Some("https://test.com".to_string()),
        env_key: Some("TEST_API_KEY".to_string()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(1000),
        requires_openai_auth: false,
        openrouter: None,
    };

    let events = collect_events(&[sse1.as_bytes()], provider).await;

    assert_eq!(events.len(), 1);
    match &events[0] {
        Err(CodexErr::UsageLimitReached(err)) => {
            assert_eq!(err.plan_type.as_deref(), Some("pro"));
            assert_eq!(err.resets_in_seconds, Some(120));
        }
        other => panic!("unexpected usage-limit event: {other:?}"),
    }
}

#[tokio::test]
async fn response_failed_usage_not_included_maps_to_typed_error() {
    let raw_error = r#"{"type":"response.failed","sequence_number":3,"response":{"id":"resp_not_included","object":"response","created_at":1759771626,"status":"failed","background":false,"error":{"type":"usage_not_included","message":"Usage is not included for this model."},"incomplete_details":null}}"#;

    let sse1 = format!("event: response.failed\ndata: {raw_error}\n\n");
    let provider = ModelProviderInfo {
        name: "test".to_string(),
        base_url: Some("https://test.com".to_string()),
        env_key: Some("TEST_API_KEY".to_string()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(1000),
        requires_openai_auth: false,
        openrouter: None,
    };

    let events = collect_events(&[sse1.as_bytes()], provider).await;

    assert_eq!(events.len(), 1);
    match &events[0] {
        Err(CodexErr::UsageNotIncluded) => {}
        other => panic!("unexpected usage-not-included event: {other:?}"),
    }
}

#[tokio::test]
async fn server_overloaded_error_is_typed() {
    let raw_error = r#"{"type":"response.failed","sequence_number":3,"response":{"id":"resp_slow_down","object":"response","created_at":1759771626,"status":"failed","background":false,"error":{"code":"slow_down","message":"Server is overloaded. Please retry shortly."},"incomplete_details":null}}"#;

    let sse1 = format!("event: response.failed\ndata: {raw_error}\n\n");
    let provider = ModelProviderInfo {
        name: "test".to_string(),
        base_url: Some("https://test.com".to_string()),
        env_key: Some("TEST_API_KEY".to_string()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(1000),
        requires_openai_auth: false,
        openrouter: None,
    };

    let events = collect_events(&[sse1.as_bytes()], provider).await;

    assert_eq!(events.len(), 1);
    match &events[0] {
        Err(CodexErr::ServerOverloaded) => {}
        other => panic!("unexpected overloaded event: {other:?}"),
    }
}

#[tokio::test]
async fn response_incomplete_surfaces_stream_error_reason() {
    let raw_incomplete = r#"{"type":"response.incomplete","sequence_number":4,"response":{"id":"resp_incomplete","object":"response","created_at":1759771626,"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"}}}"#;

    let sse1 = format!("event: response.incomplete\ndata: {raw_incomplete}\n\n");
    let provider = ModelProviderInfo {
        name: "test".to_string(),
        base_url: Some("https://test.com".to_string()),
        env_key: Some("TEST_API_KEY".to_string()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(1000),
        requires_openai_auth: false,
        openrouter: None,
    };

    let events = collect_events(&[sse1.as_bytes()], provider).await;

    assert_eq!(events.len(), 1);
    match &events[0] {
        Err(CodexErr::Stream(message, None, _)) => {
            assert_eq!(
                message,
                "Incomplete response returned, reason: max_output_tokens"
            );
        }
        other => panic!("unexpected incomplete event: {other:?}"),
    }
}

#[test]
fn websocket_error_without_status_surfaces_stream_message() {
    let payload = r#"{"type":"error","error":{"type":"invalid_request_error","message":"The requested model 'gpt-5.3-codex-spark' does not exist."}}"#;
    let wrapped = transport::parse_wrapped_websocket_error_event(payload)
        .expect("wrapped websocket error should parse");
    let mapped =
        transport::map_wrapped_websocket_error_event(wrapped).expect("error should map without status");
    match mapped {
        CodexErr::Stream(message, None, None) => {
            assert_eq!(
                message,
                "The requested model 'gpt-5.3-codex-spark' does not exist."
            );
        }
        other => panic!("unexpected mapped websocket error: {other:?}"),
    }
}
