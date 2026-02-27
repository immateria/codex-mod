use super::*;

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
                .unwrap_or_else(|| sub_id.to_string());
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
            let event_id = if !id.is_empty() {
                id.clone()
            } else {
                sub_id.to_string()
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
                for item in content.into_iter() {
                    let text = match item {
                        ReasoningItemContent::ReasoningText { text } => text,
                        ReasoningItemContent::Text { text } => text,
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
                let call_id = id.unwrap_or_else(|| "".to_string());
                let query = web_search_query(&query, &queries);
                let event = sess.make_event_with_hint(
                    sub_id,
                    EventMsg::WebSearchComplete(WebSearchCompleteEvent { call_id, query }),
                    seq_hint,
                );
                sess.tx_event.send(event).await.ok();
            }
            None
        }
        ResponseItem::GhostSnapshot { .. } => None,
        ResponseItem::Other => None,
    };
    Ok(output)
}

fn web_search_query(query: &Option<String>, queries: &Option<Vec<String>>) -> Option<String> {
    if let Some(value) = query.clone().filter(|q| !q.is_empty()) {
        return Some(value);
    }

    let items = queries.as_ref();
    let first = items
        .and_then(|queries| queries.first())
        .cloned()
        .unwrap_or_default();
    if first.is_empty() {
        return None;
    }
    if items.is_some_and(|queries| queries.len() > 1) {
        Some(format!("{first} ..."))
    } else {
        Some(first)
    }
}

