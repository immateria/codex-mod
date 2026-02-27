use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use eventsource_stream::Eventsource;
use futures::{Stream, StreamExt, TryStreamExt};
use httpdate::parse_http_date;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_util::io::ReaderStream;
use tracing::{debug, trace};

use crate::client_common::{ResponseEvent, ResponseStream};
use crate::debug_logger::DebugLogger;
use crate::error::{CodexErr, RetryAfter, Result, UsageLimitReachedError};
use crate::model_provider_info::ModelProviderInfo;
use crate::protocol::{RateLimitSnapshotEvent, TokenUsage};
use code_otel::otel_event_manager::OtelEventManager;
use code_protocol::models::ResponseItem;

use super::{is_quota_exceeded_error, is_server_overloaded_error, try_parse_retry_after, Error};

#[derive(Default, Debug)]
pub(super) struct StreamCheckpoint {
    /// Highest sequence_number observed across attempts. Used to drop replayed deltas.
    last_sequence: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct SseEvent {
    #[serde(rename = "type")]
    kind: String,
    response: Option<Value>,
    item: Option<Value>,
    delta: Option<String>,
    // Present on delta events from the Responses API; used to correlate
    // streaming chunks with the final OutputItemDone.
    item_id: Option<String>,
    // Optional ordering metadata from the Responses API; used to filter
    // duplicates and out‑of‑order reasoning deltas.
    sequence_number: Option<u64>,
    output_index: Option<u32>,
    content_index: Option<u32>,
    summary_index: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ResponseCompleted {
    id: String,
    usage: Option<ResponseCompletedUsage>,
}

#[derive(Debug, Deserialize)]
struct ResponseDone {
    id: Option<String>,
    usage: Option<ResponseCompletedUsage>,
}

#[derive(Debug, Deserialize)]
struct ResponseCompletedUsage {
    input_tokens: u64,
    input_tokens_details: Option<ResponseCompletedInputTokensDetails>,
    output_tokens: u64,
    output_tokens_details: Option<ResponseCompletedOutputTokensDetails>,
    total_tokens: u64,
}

impl From<ResponseCompletedUsage> for TokenUsage {
    fn from(val: ResponseCompletedUsage) -> Self {
        TokenUsage {
            input_tokens: val.input_tokens,
            cached_input_tokens: val
                .input_tokens_details
                .map(|d| d.cached_tokens)
                .unwrap_or(0),
            output_tokens: val.output_tokens,
            reasoning_output_tokens: val
                .output_tokens_details
                .map(|d| d.reasoning_tokens)
                .unwrap_or(0),
            total_tokens: val.total_tokens,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ResponseCompletedInputTokensDetails {
    cached_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct ResponseCompletedOutputTokensDetails {
    reasoning_tokens: u64,
}

pub(super) fn attach_item_ids(payload_json: &mut Value, original_items: &[ResponseItem]) {
    let Some(input_value) = payload_json.get_mut("input") else {
        return;
    };
    let serde_json::Value::Array(items) = input_value else {
        return;
    };

    for (value, item) in items.iter_mut().zip(original_items.iter()) {
        if let ResponseItem::Reasoning { id, .. }
        | ResponseItem::Message { id: Some(id), .. }
        | ResponseItem::WebSearchCall { id: Some(id), .. }
        | ResponseItem::FunctionCall { id: Some(id), .. }
        | ResponseItem::LocalShellCall { id: Some(id), .. }
        | ResponseItem::CustomToolCall { id: Some(id), .. } = item
        {
            if id.is_empty() {
                continue;
            }

            if let Some(obj) = value.as_object_mut() {
                obj.insert("id".to_string(), Value::String(id.clone()));
            }
        }
    }
}

pub(super) fn parse_rate_limit_snapshot(headers: &HeaderMap) -> Option<RateLimitSnapshotEvent> {
    let primary_used_percent = parse_header_f64(headers, "x-codex-primary-used-percent")?;
    let secondary_used_percent = parse_header_f64(headers, "x-codex-secondary-used-percent")?;
    let primary_to_secondary_ratio_percent =
        parse_header_f64(headers, "x-codex-primary-over-secondary-limit-percent")?;
    let primary_window_minutes = parse_header_u64(headers, "x-codex-primary-window-minutes")?;
    let secondary_window_minutes = parse_header_u64(headers, "x-codex-secondary-window-minutes")?;
    let primary_reset_after_seconds =
        parse_header_u64(headers, "x-codex-primary-reset-after-seconds");
    let secondary_reset_after_seconds =
        parse_header_u64(headers, "x-codex-secondary-reset-after-seconds");

    Some(RateLimitSnapshotEvent {
        primary_used_percent,
        secondary_used_percent,
        primary_to_secondary_ratio_percent,
        primary_window_minutes,
        secondary_window_minutes,
        primary_reset_after_seconds,
        secondary_reset_after_seconds,
    })
}

pub(super) fn format_rate_limit_headers(headers: &HeaderMap) -> String {
    let mut pairs: Vec<String> = headers
        .iter()
        .map(|(name, value)| {
            let value_str = value.to_str().unwrap_or("<invalid>");
            format!("{name}: {value_str}")
        })
        .collect();
    pairs.sort();
    pairs.join("\n")
}

fn parse_header_f64(headers: &HeaderMap, name: &str) -> Option<f64> {
    parse_header_str(headers, name)?
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

fn parse_header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    parse_header_str(headers, name)?.parse::<u64>().ok()
}

fn parse_header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

pub(super) fn parse_retry_after_header(value: &str, now: DateTime<Utc>) -> Option<RetryAfter> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '<' | '>'))
        .trim();
    if normalized.is_empty() {
        return None;
    }

    if let Ok(secs) = normalized.parse::<u64>() {
        return Some(RetryAfter::from_duration(Duration::from_secs(secs), now));
    }
    if let Ok(float_secs) = normalized.parse::<f64>()
        && float_secs.is_finite()
        && !float_secs.is_sign_negative()
    {
        return Some(RetryAfter::from_duration(
            Duration::from_secs_f64(float_secs),
            now,
        ));
    }
    if let Ok(system_time) = parse_http_date(normalized) {
        let resume_at: DateTime<Utc> = system_time.into();
        return Some(RetryAfter::from_resume_at(resume_at, now));
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(normalized) {
        return Some(RetryAfter::from_resume_at(dt.with_timezone(&Utc), now));
    }
    if let Ok(dt) = DateTime::parse_from_rfc2822(normalized) {
        return Some(RetryAfter::from_resume_at(dt.with_timezone(&Utc), now));
    }
    if let Ok(dt) = DateTime::parse_from_str(normalized, "%a, %d %b %Y %H:%M:%S %z") {
        return Some(RetryAfter::from_resume_at(dt.with_timezone(&Utc), now));
    }

    None
}

pub(super) fn header_map_to_json(headers: &HeaderMap) -> Value {
    let mut ordered: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, value) in headers.iter() {
        let entry = ordered.entry(name.as_str().to_string()).or_default();
        entry.push(value.to_str().unwrap_or_default().to_string());
    }

    serde_json::to_value(ordered).unwrap_or(Value::Null)
}

async fn emit_completed_event(
    completed: ResponseCompleted,
    tx_event: &mpsc::Sender<Result<ResponseEvent>>,
    otel_event_manager: Option<&OtelEventManager>,
) {
    let ResponseCompleted { id, usage } = completed;
    if let (Some(usage), Some(manager)) = (&usage, otel_event_manager) {
        manager.sse_event_completed(
            usage.input_tokens,
            usage.output_tokens,
            usage.input_tokens_details.as_ref().map(|d| d.cached_tokens),
            usage.output_tokens_details.as_ref().map(|d| d.reasoning_tokens),
            usage.total_tokens,
        );
    }

    let event = ResponseEvent::Completed {
        response_id: id,
        token_usage: usage.map(Into::into),
    };
    let _ = tx_event.send(Ok(event)).await;
}

struct RequestLogGuard {
    debug_logger: Arc<Mutex<DebugLogger>>,
    request_id: String,
}

impl RequestLogGuard {
    fn new(debug_logger: Arc<Mutex<DebugLogger>>, request_id: String) -> Self {
        Self {
            debug_logger,
            request_id,
        }
    }
}

impl Drop for RequestLogGuard {
    fn drop(&mut self) {
        if let Ok(logger) = self.debug_logger.lock() {
            let _ = logger.end_request_log(&self.request_id);
        }
    }
}

pub(super) async fn process_sse<S>(
    stream: S,
    tx_event: mpsc::Sender<Result<ResponseEvent>>,
    idle_timeout: Duration,
    debug_logger: Arc<Mutex<DebugLogger>>,
    request_id: String,
    otel_event_manager: Option<OtelEventManager>,
    checkpoint: Arc<RwLock<StreamCheckpoint>>,
) where
    S: Stream<Item = Result<Bytes>> + Unpin,
{
    let mut stream = stream.eventsource();
    let _request_log_guard = RequestLogGuard::new(Arc::clone(&debug_logger), request_id.clone());

    // If the stream stays completely silent for an extended period treat it as disconnected.
    // The response id returned from the "complete" message.
    let mut response_completed: Option<ResponseCompleted> = None;
    let mut response_error: Option<CodexErr> = None;
    // Track the current item_id to include with delta events
    let mut current_item_id: Option<String> = None;

    // Monotonic sequence guards to drop duplicate/out‑of‑order deltas.
    // Keys are item_id strings.
    use std::collections::HashMap;
    // Track last sequence_number per (item_id, output_index[, content_index])
    // Default indices to 0 when absent for robustness across providers.
    let mut last_seq_reasoning_summary: HashMap<(String, u32, u32), u64> = HashMap::new();
    let mut last_seq_reasoning_content: HashMap<(String, u32, u32), u64> = HashMap::new();
    // Best-effort duplicate text guard when sequence_number is unavailable.
    let mut last_text_reasoning_summary: HashMap<(String, u32, u32), String> = HashMap::new();
    let mut last_text_reasoning_content: HashMap<(String, u32, u32), String> = HashMap::new();
    let mut global_last_seq: Option<u64> = checkpoint.read().ok().and_then(|c| c.last_sequence);

    loop {
        let next_event = if let Some(manager) = otel_event_manager.as_ref() {
            manager
                .log_sse_event(|| timeout(idle_timeout, stream.next()))
                .await
        } else {
            timeout(idle_timeout, stream.next()).await
        };

        let sse = match next_event {
            Ok(Some(Ok(sse))) => sse,
            Ok(Some(Err(e))) => {
                debug!("SSE Error: {e:#}");
                let event = CodexErr::Stream(
                    format!("[transport] {e}"),
                    None,
                    Some(request_id.clone()),
                );
                let _ = tx_event.send(Err(event)).await;
                return;
            }
            Ok(None) => {
                match response_completed {
                    Some(completed) => {
                        emit_completed_event(
                            completed,
                            &tx_event,
                            otel_event_manager.as_ref(),
                        )
                        .await;
                    }
                    None => {
                        let error = response_error.unwrap_or(CodexErr::Stream(
                            "stream closed before response.completed".into(),
                            None,
                            Some(request_id.clone()),
                        ));
                        if let Some(manager) = otel_event_manager.as_ref() {
                            manager.see_event_completed_failed(&error);
                        }
                        let _ = tx_event.send(Err(error)).await;
                    }
                }
                return;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(CodexErr::Stream(
                        "[idle] timeout waiting for SSE".into(),
                        None,
                        Some(request_id.clone()),
                    )))
                    .await;
                return;
            }
        };

        trace!(data = %sse.data, "SSE event");

        // Log the raw SSE event data
        if let Ok(logger) = debug_logger.lock()
            && let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&sse.data) {
                let _ = logger.append_response_event(&request_id, "sse_event", &json_value);
            }

        let event: SseEvent = match serde_json::from_str(&sse.data) {
            Ok(event) => event,
            Err(e) => {
                // Log parse error with data excerpt, and record it in the debug logger as well.
                let mut excerpt = sse.data.clone();
                const MAX: usize = 600;
                if excerpt.len() > MAX {
                    excerpt.truncate(MAX);
                }
                debug!("Failed to parse SSE event: {e}, data: {excerpt}");
                if let Ok(logger) = debug_logger.lock() {
                    let _ = logger.append_response_event(
                        &request_id,
                        "sse_parse_error",
                        &serde_json::json!({
                            "error": e.to_string(),
                            "data_excerpt": excerpt,
                        }),
                    );
                }
                continue;
            }
        };

        if let Some(seq) = event.sequence_number {
            if let Some(last) = global_last_seq
                && seq <= last {
                    continue;
                }
            global_last_seq = Some(seq);
            if let Ok(mut guard) = checkpoint.write() {
                guard.last_sequence = Some(seq);
            }
        }

        match event.kind.as_str() {
            // Individual output item finalised. Forward immediately so the
            // rest of the agent can stream assistant text/functions *live*
            // instead of waiting for the final `response.completed` envelope.
            //
            // IMPORTANT: We used to ignore these events and forward the
            // duplicated `output` array embedded in the `response.completed`
            // payload.  That produced two concrete issues:
            //   1. No real‑time streaming – the user only saw output after the
            //      entire turn had finished, which broke the "typing" UX and
            //      made long‑running turns look stalled.
            //   2. Duplicate `function_call_output` items – both the
            //      individual *and* the completed array were forwarded, which
            //      confused the backend and triggered 400
            //      "previous_response_not_found" errors because the duplicated
            //      IDs did not match the incremental turn chain.
            //
            // The fix is to forward the incremental events *as they come* and
            // drop the duplicated list inside `response.completed`.
            "response.output_item.done" => {
                let Some(item_val) = event.item else { continue };
                // Special-case: web_search_call completion -> synthesize a completion event
                if item_val
                    .get("type")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s == "web_search_call")
                {
                    let call_id = item_val
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let query = item_val
                        .get("action")
                        .and_then(|a| a.get("query"))
                        .and_then(|v| v.as_str())
                        .map(std::string::ToString::to_string);
                    let ev = ResponseEvent::WebSearchCallCompleted { call_id, query };
                    if tx_event.send(Ok(ev)).await.is_err() {
                        return;
                    }
                }
                let Ok(item) = serde_json::from_value::<ResponseItem>(item_val.clone()) else {
                    debug!("failed to parse ResponseItem from output_item.done");
                    continue;
                };

                // Extract item_id if present
                if let Some(id) = item_val.get("id").and_then(|v| v.as_str()) {
                    current_item_id = Some(id.to_string());
                } else {
                    // Check within the parsed item structure
                    match &item {
                        ResponseItem::Message { id, .. }
                        | ResponseItem::FunctionCall { id, .. }
                        | ResponseItem::LocalShellCall { id, .. } => {
                            if let Some(item_id) = id {
                                current_item_id = Some(item_id.clone());
                            }
                        }
                        ResponseItem::Reasoning { id, .. } => {
                            current_item_id = Some(id.clone());
                        }
                        _ => {}
                    }
                }

                let event = ResponseEvent::OutputItemDone { item, sequence_number: event.sequence_number, output_index: event.output_index };
                if tx_event.send(Ok(event)).await.is_err() {
                    return;
                }
            }
            "response.output_text.delta" => {
                if let Some(delta) = event.delta {
                    // Prefer the explicit item_id from the SSE event; fall back to last seen.
                    if let Some(ref id) = event.item_id {
                        current_item_id = Some(id.clone());
                    }
                    debug!(item_id = ?current_item_id, len = delta.len(), "sse.delta output_text");
                    let ev = ResponseEvent::OutputTextDelta {
                        delta,
                        item_id: event.item_id.or_else(|| current_item_id.clone()),
                        sequence_number: event.sequence_number,
                        output_index: event.output_index,
                    };
                    if tx_event.send(Ok(ev)).await.is_err() {
                        return;
                    }
                }
            }
            "response.reasoning_summary_text.delta" => {
                if let Some(delta) = event.delta {
                    if let Some(ref id) = event.item_id {
                        current_item_id = Some(id.clone());
                    }
                    // Compose key using item_id + output_index
                    let out_idx: u32 = event.output_index.unwrap_or(0);
                    let sum_idx: u32 = event.summary_index.unwrap_or(0);
                    if let Some(ref id) = current_item_id {
                        // Drop duplicates/out‑of‑order by sequence_number when available
                        if let Some(sn) = event.sequence_number {
                            let last = last_seq_reasoning_summary.entry((id.clone(), out_idx, sum_idx)).or_insert(0);
                            if *last >= sn { continue; }
                            *last = sn;
                        } else {
                            // Best-effort: drop exact duplicate text for same key when seq is missing
                            let key = (id.clone(), out_idx, sum_idx);
                            if last_text_reasoning_summary.get(&key) == Some(&delta) {
                                continue;
                            }
                            last_text_reasoning_summary.insert(key, delta.clone());
                        }
                    }
                    debug!(
                        item_id = ?current_item_id,
                        out_idx,
                        sum_idx,
                        len = delta.len(),
                        seq = ?event.sequence_number,
                        "sse.delta reasoning_summary",
                    );
                    let ev = ResponseEvent::ReasoningSummaryDelta {
                        delta,
                        item_id: event.item_id.or_else(|| current_item_id.clone()),
                        sequence_number: event.sequence_number,
                        output_index: event.output_index,
                        summary_index: event.summary_index,
                    };
                    if tx_event.send(Ok(ev)).await.is_err() {
                        return;
                    }
                }
            }
            "response.reasoning_text.delta" => {
                if let Some(delta) = event.delta {
                    if let Some(ref id) = event.item_id {
                        current_item_id = Some(id.clone());
                    }
                    // Compose key using item_id + output_index + content_index
                    let out_idx: u32 = event.output_index.unwrap_or(0);
                    let content_idx: u32 = event.content_index.unwrap_or(0);
                    if let Some(ref id) = current_item_id {
                        // Drop duplicates/out‑of‑order by sequence_number when available
                        if let Some(sn) = event.sequence_number {
                            let last = last_seq_reasoning_content.entry((id.clone(), out_idx, content_idx)).or_insert(0);
                            if *last >= sn { continue; }
                            *last = sn;
                        } else {
                            // Best-effort: drop exact duplicate text for same key when seq is missing
                            let key = (id.clone(), out_idx, content_idx);
                            if last_text_reasoning_content.get(&key) == Some(&delta) {
                                continue;
                            }
                            last_text_reasoning_content.insert(key, delta.clone());
                        }
                    }
                    debug!(
                        item_id = ?current_item_id,
                        out_idx,
                        content_idx,
                        len = delta.len(),
                        seq = ?event.sequence_number,
                        "sse.delta reasoning_content",
                    );
                    let ev = ResponseEvent::ReasoningContentDelta {
                        delta,
                        item_id: event.item_id.or_else(|| current_item_id.clone()),
                        sequence_number: event.sequence_number,
                        output_index: event.output_index,
                        content_index: event.content_index,
                    };
                    if tx_event.send(Ok(ev)).await.is_err() {
                        return;
                    }
                }
            }
            "response.created" => {
                if let Some(response) = event.response {
                    let response_id = response
                        .get("id")
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                    let response_model = response
                        .get("model")
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                    let _ = tx_event
                        .send(Ok(ResponseEvent::Created {
                            response_id,
                            response_model,
                        }))
                        .await;
                }
            }
            "response.failed" => {
                if let Some(resp_val) = event.response {
                    response_error = Some(CodexErr::Stream(
                        "response.failed event received".to_string(),
                        None,
                        Some(request_id.clone()),
                    ));

                    let error = resp_val.get("error");

                    if let Some(error) = error {
                        match serde_json::from_value::<Error>(error.clone()) {
                            Ok(error) => {
                                if error.r#type.as_deref() == Some("usage_limit_reached") {
                                    response_error = Some(CodexErr::UsageLimitReached(
                                        UsageLimitReachedError {
                                            plan_type: error.plan_type,
                                            resets_in_seconds: error.resets_in_seconds,
                                        },
                                    ));
                                } else if error.r#type.as_deref() == Some("usage_not_included") {
                                    response_error = Some(CodexErr::UsageNotIncluded);
                                } else if is_quota_exceeded_error(&error) {
                                    response_error = Some(CodexErr::QuotaExceeded);
                                } else if is_server_overloaded_error(&error) {
                                    response_error = Some(CodexErr::ServerOverloaded);
                                } else {
                                    let retry_after = try_parse_retry_after(&error, Utc::now());
                                    let message = error.message.unwrap_or_default();
                                    response_error = Some(CodexErr::Stream(
                                        message,
                                        retry_after,
                                        Some(request_id.clone()),
                                    ));
                                }
                            }
                            Err(e) => {
                                debug!("failed to parse ErrorResponse: {e}");
                            }
                        }
                    }

                    if let Some(error) = response_error.take() {
                        if let Some(manager) = otel_event_manager.as_ref() {
                            manager.see_event_completed_failed(&error);
                        }
                        let _ = tx_event.send(Err(error)).await;
                        return;
                    }
                }
            }
            "response.incomplete" => {
                let reason = event.response.as_ref().and_then(|response| {
                    response
                        .get("incomplete_details")
                        .and_then(|details| details.get("reason"))
                        .and_then(Value::as_str)
                });
                let reason = reason.unwrap_or("unknown");
                let message = format!("Incomplete response returned, reason: {reason}");
                let event = CodexErr::Stream(message, None, Some(request_id.clone()));
                let _ = tx_event.send(Err(event)).await;
                return;
            }
            // Final response completed – includes array of output items & id
            "response.completed" => {
                if let Some(resp_val) = event.response {
                    match serde_json::from_value::<ResponseCompleted>(resp_val) {
                        Ok(r) => {
                            response_completed = Some(r);
                        }
                        Err(e) => {
                            debug!("failed to parse ResponseCompleted: {e}");
                            continue;
                        }
                    };

                    if let Some(completed) = response_completed.take() {
                        emit_completed_event(
                            completed,
                            &tx_event,
                            otel_event_manager.as_ref(),
                        )
                        .await;
                        return;
                    }
                };
            }
            "response.done" => {
                if let Some(resp_val) = event.response {
                    match serde_json::from_value::<ResponseDone>(resp_val) {
                        Ok(r) => {
                            response_completed = Some(ResponseCompleted {
                                id: r.id.unwrap_or_default(),
                                usage: r.usage,
                            });
                        }
                        Err(e) => {
                            debug!("failed to parse ResponseDone: {e}");
                            continue;
                        }
                    };
                } else {
                    response_completed = Some(ResponseCompleted {
                        id: String::new(),
                        usage: None,
                    });
                }

                if let Some(completed) = response_completed.take() {
                    emit_completed_event(
                        completed,
                        &tx_event,
                        otel_event_manager.as_ref(),
                    )
                    .await;
                    return;
                }
            }
            "response.content_part.done"
            | "response.function_call_arguments.delta"
            | "response.custom_tool_call_input.delta"
            | "response.custom_tool_call_input.done" // also emitted as response.output_item.done
            | "response.in_progress"
            | "response.output_item.added"
            | "response.output_text.done" => {
                if event.kind == "response.output_item.added"
                    && let Some(item) = event.item.as_ref() {
                        // Detect web_search_call begin and forward a synthetic event upstream.
                        if let Some(ty) = item.get("type").and_then(|v| v.as_str())
                            && ty == "web_search_call" {
                                let call_id = item
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let ev = ResponseEvent::WebSearchCallBegin { call_id };
                                if tx_event.send(Ok(ev)).await.is_err() {
                                    return;
                                }
                            }
                    }
            }
            "response.reasoning_summary_part.added" => {
                // Boundary between reasoning summary sections (e.g., titles).
                let event = ResponseEvent::ReasoningSummaryPartAdded;
                if tx_event.send(Ok(event)).await.is_err() {
                    return;
                }
            }
            "response.reasoning_summary_text.done" => {}
            _ => {}
        }
    }
}

/// used in tests to stream from a text SSE file
pub(super) async fn stream_from_fixture(
    path: impl AsRef<Path>,
    provider: ModelProviderInfo,
    otel_event_manager: Option<OtelEventManager>,
) -> Result<ResponseStream> {
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(1600);
    let f = std::fs::File::open(path.as_ref())?;
    let lines = std::io::BufReader::new(f).lines();

    // insert \n\n after each line for proper SSE parsing
    let mut content = String::new();
    for line in lines {
        content.push_str(&line?);
        content.push_str("\n\n");
    }

    let rdr = std::io::Cursor::new(content);
    let stream = ReaderStream::new(rdr).map_err(CodexErr::Io);
    // Create a dummy debug logger for testing
    let debug_logger = Arc::new(Mutex::new(
        DebugLogger::new(false)
            .unwrap_or_else(|err| panic!("failed to create debug logger for fixture stream: {err}")),
    ));
    tokio::spawn(process_sse(
        stream,
        tx_event,
        provider.stream_idle_timeout(),
        debug_logger,
        String::new(), // Empty request_id for test fixture
        otel_event_manager,
        Arc::new(RwLock::new(StreamCheckpoint::default())),
    ));
    Ok(ResponseStream { rx_event })
}
