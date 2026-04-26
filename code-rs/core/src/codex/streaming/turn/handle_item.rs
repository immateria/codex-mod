use super::*;
use crate::protocol::{ImageGenerationBeginEvent, ImageGenerationEndEvent};

pub(super) async fn handle_response_item(
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
                .unwrap_or_else(|| sub_id.to_owned());
            for item in content {
                if let ContentItem::OutputText { text } = item {
                    let order = crate::protocol::OrderMeta {
                        request_ordinal: attempt_req,
                        output_index,
                        sequence_number: seq_hint,
                    };
                    let stamped = sess.make_event_with_order(
                        &event_id,
                        EventMsg::AgentMessage(AgentMessageEvent { message: text }),
                        order,
                        seq_hint,
                    );
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
            let event_id = if id.is_empty() {
                sub_id.to_owned()
            } else {
                id.clone()
            };
            for (i, item) in summary.into_iter().enumerate() {
                let text = match item {
                    ReasoningItemReasoningSummary::SummaryText { text } => text,
                };
                let eid = format!("{event_id}#s{i}");
                let order = crate::protocol::OrderMeta {
                    request_ordinal: attempt_req,
                    output_index,
                    sequence_number: seq_hint,
                };
                let stamped = sess.make_event_with_order(
                    &eid,
                    EventMsg::AgentReasoning(AgentReasoningEvent { text }),
                    order,
                    seq_hint,
                );
                sess.tx_event.send(stamped).await.ok();
            }
            if sess.show_raw_agent_reasoning
                && let Some(content) = content
            {
                for item in content {
                    let text = match item {
                        ReasoningItemContent::ReasoningText { text }
                        | ReasoningItemContent::Text { text } => text,
                    };
                    let order = crate::protocol::OrderMeta {
                        request_ordinal: attempt_req,
                        output_index,
                        sequence_number: seq_hint,
                    };
                    let stamped = sess.make_event_with_order(
                        &event_id,
                        EventMsg::AgentReasoningRawContent(AgentReasoningRawContentEvent { text }),
                        order,
                        seq_hint,
                    );
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
                sess.maybe_mark_memories_polluted("web_search_call");
                let call_id = id.unwrap_or_default();
                let query = web_search_query(query.as_ref(), queries.as_ref());
                let event = sess.make_event_with_hint(
                    sub_id,
                    EventMsg::WebSearchComplete(WebSearchCompleteEvent { call_id, query }),
                    seq_hint,
                );
                sess.tx_event.send(event).await.ok();
            }
            None
        }
        ResponseItem::ImageGenerationCall {
            id,
            status,
            revised_prompt,
            result,
        } => {
            let call_id = id;
            let order = crate::protocol::OrderMeta {
                request_ordinal: attempt_req,
                output_index,
                sequence_number: seq_hint,
            };

            // Emit begin event.
            let begin_event = sess.make_event_with_order(
                sub_id,
                EventMsg::ImageGenerationBegin(ImageGenerationBeginEvent {
                    call_id: call_id.clone(),
                }),
                order.clone(),
                seq_hint,
            );
            sess.tx_event.send(begin_event).await.ok();

            // Save the generated image to disk.
            let saved_path = save_image_generation_artifact(
                sess.client.code_home(),
                sess.id,
                &call_id,
                &result,
            );

            // Emit end event.
            let end_event = sess.make_event_with_order(
                sub_id,
                EventMsg::ImageGenerationEnd(ImageGenerationEndEvent {
                    call_id: call_id.clone(),
                    status,
                    revised_prompt,
                    result,
                    saved_path,
                }),
                order,
                seq_hint,
            );
            sess.tx_event.send(end_event).await.ok();

            None
        }
        ResponseItem::ToolSearchCall { .. }
        | ResponseItem::ToolSearchOutput { .. }
        | ResponseItem::GhostSnapshot { .. }
        | ResponseItem::Other => None,
    };
    Ok(output)
}

fn web_search_query(query: Option<&String>, queries: Option<&Vec<String>>) -> Option<String> {
    if let Some(value) = query.filter(|q| !q.is_empty()).cloned() {
        return Some(value);
    }

    let first = queries
        .and_then(|queries| queries.first())
        .cloned()
        .unwrap_or_default();
    if first.is_empty() {
        return None;
    }
    if queries.is_some_and(|queries| queries.len() > 1) {
        Some(format!("{first} ..."))
    } else {
        Some(first)
    }
}

/// Save a base64-encoded image generation result to disk and return the path.
fn save_image_generation_artifact(
    code_home: &std::path::Path,
    session_id: uuid::Uuid,
    call_id: &str,
    result: &str,
) -> Option<code_utils_absolute_path::AbsolutePathBuf> {
    use base64::Engine as _;
    use code_utils_absolute_path::AbsolutePathBuf;
    fn sanitize(value: &str) -> String {
        let sanitized: String = value
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        if sanitized.is_empty() {
            "generated_image".to_string()
        } else {
            sanitized
        }
    }

    // Skip data-URI results (inline images, not pure base64).
    if result.starts_with("data:") {
        return None;
    }

    let dir = code_home
        .join("generated_images")
        .join(sanitize(&session_id.to_string()));
    let path = dir.join(format!("{}.png", sanitize(call_id)));

    if path.exists() {
        return AbsolutePathBuf::from_absolute_path(&path).ok();
    }

    let bytes = match base64::engine::general_purpose::STANDARD.decode(result.trim().as_bytes()) {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!("failed to decode image generation result: {err}");
            return None;
        }
    };

    if let Err(err) = std::fs::create_dir_all(&dir) {
        tracing::warn!("failed to create generated image dir {}: {err}", dir.display());
        return None;
    }
    if let Err(err) = std::fs::write(&path, bytes) {
        tracing::warn!("failed to write image generation artifact {}: {err}", path.display());
        return None;
    }

    AbsolutePathBuf::from_absolute_path(&path).ok()
}
