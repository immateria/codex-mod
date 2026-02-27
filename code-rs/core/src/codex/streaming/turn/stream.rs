use super::*;

use super::handle_item::handle_response_item;
use super::latency::TurnLatencyGuard;

pub(super) async fn try_run_turn(
    sess: &Session,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: &str,
    prompt: &Prompt,
    attempt_req: u64,
) -> CodexResult<Vec<ProcessedResponseItem>> {
    // Ensure any pending tool calls from a previous interrupted attempt are paired with
    // an "aborted" output before we send a new request to the model.
    let missing_outputs = missing_tool_outputs_to_insert(&prompt.input);
    let prompt: Cow<Prompt> = if missing_outputs.is_empty() {
        Cow::Borrowed(prompt)
    } else {
        let mut input = prompt.input.clone();
        for (idx, output_item) in missing_outputs.into_iter().rev() {
            input.insert(idx + 1, output_item);
        }

        Cow::Owned(Prompt { input, ..prompt.clone() })
    };

    let enable_parallel_tool_calls = prompt
        .as_ref()
        .model_family_override
        .as_ref()
        .unwrap_or_else(|| sess.client.default_model_family())
        .supports_parallel_tool_calls;

    let mut turn_latency_guard = TurnLatencyGuard::new(sess, attempt_req, prompt.as_ref());
    let mut stream = match sess.client.clone().stream(&prompt).await {
        Ok(stream) => stream,
        Err(e) => {
            turn_latency_guard.mark_failed(Some(format!("stream_init_failed: {e}")));
            sess
                .notify_stream_error(
                    sub_id,
                    format!("[transport] failed to start stream: {e}"),
                )
                .await;
            return Err(e);
        }
    };

    let mut output = Vec::new();
    let mut pending_tool_calls: Vec<PendingToolCall> = Vec::new();
    loop {
        // Poll the next item from the model stream. We must inspect *both* Ok and Err
        // cases so that transient stream failures (e.g., dropped SSE connection before
        // `response.completed`) bubble up and trigger the caller's retry logic.
        let event = stream.next().await;
        let Some(event) = event else {
            // Channel closed without yielding a final Completed event or explicit error.
            // Treat as a disconnected stream so the caller can retry.
            turn_latency_guard
                .mark_failed(Some("stream_closed_before_completed".to_string()));
            return Err(CodexErr::Stream(
                "stream closed before response.completed".into(),
                None,
                None,
            ));
        };

        let event = match event {
            Ok(ev) => ev,
            Err(e) => {
                // Propagate the underlying stream error to the caller (run_turn), which
                // will apply the configured `stream_max_retries` policy.
                turn_latency_guard.mark_failed(Some(format!("stream_event_error: {e}")));
                return Err(e);
            }
        };

        match event {
            ResponseEvent::Created { .. } => {}
            ResponseEvent::ServerReasoningIncluded(_included) => {}
            ResponseEvent::OutputItemDone { item, sequence_number, output_index } => {
                let is_tool_call = matches!(
                    item,
                    ResponseItem::FunctionCall { .. }
                        | ResponseItem::LocalShellCall { .. }
                        | ResponseItem::CustomToolCall { .. }
                );

                if enable_parallel_tool_calls && is_tool_call {
                    let output_pos = output.len();
                    // Persist finalized tool call items so retries can re-seed them if the
                    // stream disconnects before `response.completed`.
                    sess.scratchpad_push(&item, &None, sub_id);
                    output.push(ProcessedResponseItem {
                        item,
                        response: None,
                    });
                    pending_tool_calls.push(PendingToolCall {
                        output_pos,
                        seq_hint: sequence_number,
                        output_index,
                    });
                } else {
                    let response = handle_response_item(
                        sess,
                        turn_diff_tracker,
                        sub_id,
                        item.clone(),
                        sequence_number,
                        output_index,
                        attempt_req,
                    )
                    .await?;

                    // Save into scratchpad so we can seed a retry if the stream drops later.
                    sess.scratchpad_push(&item, &response, sub_id);

                    // If this was a finalized assistant message, clear partial text buffer
                    if let ResponseItem::Message { .. } = &item {
                        sess.scratchpad_clear_partial_message();
                    }

                    output.push(ProcessedResponseItem { item, response });
                }
            }
            ResponseEvent::WebSearchCallBegin { call_id } => {
                // Stamp OrderMeta so the TUI can place the search block within
                // the correct request window instead of using an internal epilogue.
                let ctx = ToolCallCtx::new(sub_id.to_string(), call_id.clone(), None, None);
                let order = ctx.order_meta(attempt_req);
                let ev = sess.make_event_with_order(
                    sub_id,
                    EventMsg::WebSearchBegin(WebSearchBeginEvent { call_id, query: None }),
                    order,
                    None,
                );
                sess.send_event(ev).await;
            }
            ResponseEvent::WebSearchCallCompleted { call_id, query } => {
                let ctx = ToolCallCtx::new(sub_id.to_string(), call_id.clone(), None, None);
                let order = ctx.order_meta(attempt_req);
                let ev = sess.make_event_with_order(
                    sub_id,
                    EventMsg::WebSearchComplete(WebSearchCompleteEvent { call_id, query }),
                    order,
                    None,
                );
                sess.send_event(ev).await;
            }
            ResponseEvent::Completed {
                response_id: _,
                token_usage,
            } => {
                let (new_info, rate_limits, should_emit);
                {
                    let mut state = sess.state.lock().unwrap();
                    let info = TokenUsageInfo::new_or_append(
                        &state.token_usage_info,
                        &token_usage,
                        sess.client.get_model_context_window(),
                    );
                    let limits = state.latest_rate_limits.clone();
                    let emit = info.is_some() || limits.is_some();
                    state.token_usage_info = info.clone();
                    new_info = info;
                    rate_limits = limits;
                    should_emit = emit;
                }

                if should_emit {
                    let payload = TokenCountEvent {
                        info: new_info,
                        rate_limits,
                    };
                    sess.tx_event
                        .send(sess.make_event(sub_id, EventMsg::TokenCount(payload)))
                        .await
                        .ok();
                }

                if let Some(usage) = token_usage.as_ref()
                    && let Some(ctx) = account_usage_context(sess) {
                        let usage_home = ctx.code_home.clone();
                        let usage_account = ctx.account_id.clone();
                        let usage_plan = ctx.plan;
                        let usage_clone = usage.clone();
                        spawn_usage_task(move || {
                            if let Err(err) = account_usage::record_token_usage(
                                &usage_home,
                                &usage_account,
                                usage_plan.as_deref(),
                                &usage_clone,
                                Utc::now(),
                            ) {
                                warn!("Failed to persist token usage: {err}");
                            }
                        });
                    }

                if enable_parallel_tool_calls && !pending_tool_calls.is_empty() {
                    let results = crate::tools::scheduler::dispatch_pending_tool_calls(
                        sess,
                        turn_diff_tracker,
                        sub_id,
                        attempt_req,
                        &pending_tool_calls,
                        |pos| output.get(pos).map(|cell| &cell.item),
                    )
                    .await;

                    for (pos, resp) in results {
                        if let Some(cell) = output.get_mut(pos) {
                            cell.response = resp;
                            sess.scratchpad_push(&cell.item, &cell.response, sub_id);
                        }
                    }
                }

                let unified_diff = turn_diff_tracker.get_unified_diff();
                if let Ok(Some(unified_diff)) = unified_diff {
                    let msg = EventMsg::TurnDiff(TurnDiffEvent { unified_diff });
                    let _ = sess.tx_event.send(sess.make_event(sub_id, msg)).await;
                }

                turn_latency_guard.mark_completed(output.len(), token_usage.as_ref());
                return Ok(output);
            }
            ResponseEvent::OutputTextDelta { delta, item_id, sequence_number, output_index } => {
                // Don't append to history during streaming - only send UI events.
                // The complete message will be added to history when OutputItemDone arrives.
                // This ensures items are recorded in the correct chronological order.

                // Use the item_id if present and non-empty, otherwise fall back to sub_id.
                let event_id = item_id
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(|| sub_id.to_string());
                let order = crate::protocol::OrderMeta {
                    request_ordinal: attempt_req,
                    output_index,
                    sequence_number,
                };
                let stamped = sess.make_event_with_order(&event_id, EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta: delta.clone() }), order, sequence_number);
                sess.tx_event.send(stamped).await.ok();

                // Track partial assistant text in the scratchpad to help resume on retry.
                // Only accumulate when we have an item context or a single active stream.
                // We deliberately do not scope by item_id to keep implementation simple.
                sess.scratchpad_add_text_delta(&delta);
            }
            ResponseEvent::ReasoningSummaryDelta { delta, item_id, sequence_number, output_index, summary_index } => {
                // Use the item_id if present and non-empty, otherwise fall back to sub_id.
                let mut event_id = item_id
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(|| sub_id.to_string());
                if let Some(si) = summary_index { event_id = format!("{event_id}#s{si}"); }
                let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number };
                let stamped = sess.make_event_with_order(&event_id, EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta: delta.clone() }), order, sequence_number);
                sess.tx_event.send(stamped).await.ok();

                // Buffer reasoning summary so we can include a hint on retry.
                sess.scratchpad_add_reasoning_delta(&delta);
            }
            ResponseEvent::ReasoningSummaryPartAdded => {
                let stamped = sess.make_event(sub_id, EventMsg::AgentReasoningSectionBreak(AgentReasoningSectionBreakEvent {}));
                sess.tx_event.send(stamped).await.ok();
            }
            ResponseEvent::ReasoningContentDelta { delta, item_id, sequence_number, output_index, content_index } => {
                if sess.show_raw_agent_reasoning {
                    // Use the item_id if present and non-empty, otherwise fall back to sub_id.
                    let mut event_id = item_id
                        .filter(|id| !id.is_empty())
                        .unwrap_or_else(|| sub_id.to_string());
                    if let Some(ci) = content_index { event_id = format!("{event_id}#c{ci}"); }
                    let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number };
                    let stamped = sess.make_event_with_order(&event_id, EventMsg::AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent { delta }), order, sequence_number);
                    sess.tx_event.send(stamped).await.ok();
                }
            }
            ResponseEvent::ModelsEtag(etag) => {
                if let Some(remote) = sess.remote_models_manager.as_ref() {
                    remote.refresh_if_new_etag(etag).await;
                }
            }
            ResponseEvent::RateLimits(snapshot) => {
                let mut state = sess.state.lock().unwrap();
                state.latest_rate_limits = Some(snapshot.clone());
                if let Some(ctx) = account_usage_context(sess) {
                    let usage_home = ctx.code_home.clone();
                    let usage_account = ctx.account_id.clone();
                    let usage_plan = ctx.plan.clone();
                    let snapshot_clone = snapshot.clone();
                    spawn_usage_task(move || {
                        if let Err(err) = account_usage::record_rate_limit_snapshot(
                            &usage_home,
                            &usage_account,
                            usage_plan.as_deref(),
                            &snapshot_clone,
                            Utc::now(),
                        ) {
                            warn!("Failed to persist rate limit snapshot: {err}");
                        }
                    });
                }
            }
            // Note: ReasoningSummaryPartAdded handled above without scratchpad mutation.
        }
    }
}
