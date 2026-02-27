use super::*;

use super::super::turn::{ProcessedResponseItem, run_turn};
use super::review::{exit_review_mode, parse_review_output_event};

// Intentionally omit upstream review thread spawning; our fork handles review flows differently.
/// Takes a user message as input and runs a loop where, at each turn, the model
/// replies with either:
///
/// - requested function calls
/// - an assistant message
///
/// While it is possible for the model to return multiple of these items in a
/// single turn, in practice, we generally one item per turn:
///
/// - If the model requests a function call, we execute it and send the output
///   back to the model in the next turn.
/// - If the model sends only an assistant message, we record it in the
///   conversation history and consider the agent complete.
pub(super) async fn run_agent(sess: Arc<Session>, turn_context: Arc<TurnContext>, sub_id: String, input: Vec<InputItem>) {
    if input.is_empty() {
        return;
    }
    let event = sess.make_event(&sub_id, EventMsg::TaskStarted);
    if sess.tx_event.send(event).await.is_err() {
        return;
    }
    // Continue with our fork's history and input handling.

    let is_review_mode = turn_context.is_review_mode;
    let mut review_history: Vec<ResponseItem> = Vec::new();
    let mut review_messages: Vec<String> = Vec::new();
    let mut review_exit_emitted = false;

    let pending_only_turn = input.len() == 1
        && matches!(
            &input[0],
            InputItem::Text { text } if text == PENDING_ONLY_SENTINEL
        );

    // Debug logging for ephemeral images
    let ephemeral_count = input
        .iter()
        .filter(|item| matches!(item, InputItem::EphemeralImage { .. }))
        .count();

    if ephemeral_count > 0 {
        tracing::info!(
            "Processing {} ephemeral images in user input",
            ephemeral_count
        );
    }

    let mut initial_response_item: Option<ResponseItem> = None;

    if !pending_only_turn {
        // Convert input to ResponseInputItem
        let mut response_input = response_input_from_core_items(input.clone());
        sess.enforce_user_message_limits(&sub_id, &mut response_input);
        let response_item: ResponseItem = response_input.into();

        if is_review_mode {
            review_history.push(response_item.clone());
        } else {
            // Record to history but we'll handle ephemeral images separately
            sess.record_conversation_items(std::slice::from_ref(&response_item))
                .await;
        }
        initial_response_item = Some(response_item);
    }

    let mut last_task_message: Option<String> = None;
    // Although from the perspective of codex.rs, TurnDiffTracker has the lifecycle of a Agent which contains
    // many turns, from the perspective of the user, it is a single turn.
    let mut turn_diff_tracker = TurnDiffTracker::new();

    // Track if this is the first iteration - if so, include the initial input
    let mut first_iteration = true;

    // Track if we've done a proactive compaction in this iteration to prevent
    // infinite loops. As long as compaction works well in getting us way below
    // the token limit, we shouldn't need more than one compaction per iteration.
    let mut did_proactive_compact_this_iteration = false;
    let mut auto_compact_pending = false;

    loop {
        // Note that pending_input would be something like a message the user
        // submitted through the UI while the model was running. Though the UI
        // may support this, the model might not.
        // IMPORTANT: Do not inject queued user inputs into the review thread.
        // Doing so routes user messages (e.g., auto-resolve fix prompts) to the
        // review model, causing loops. Only include queued user inputs when not in
        // review mode. They will be picked up after TaskComplete via
        // pop_next_queued_user_input.
        let pending_input = if is_review_mode {
            sess.get_pending_input_filtered(false)
        } else {
            sess.get_pending_input()
        }
        .into_iter()
        .map(ResponseItem::from)
        .collect::<Vec<ResponseItem>>();
        let mut pending_input_tail = pending_input.clone();

        if initial_response_item.is_none() {
            if let Some(first_pending) = pending_input_tail.first().cloned() {
                pending_input_tail.remove(0);
                if is_review_mode {
                    review_history.push(first_pending.clone());
                } else {
                    sess.record_conversation_items(std::slice::from_ref(&first_pending))
                        .await;
                }
                initial_response_item = Some(first_pending);
            } else {
                tracing::warn!(
                    "pending-only turn had no queued input; skipping model invocation"
                );
                break;
            }
        }

        let compact_snapshot = if auto_compact_pending && !is_review_mode {
            Some(sess.turn_input_with_history(pending_input_tail.clone()))
        } else {
            None
        };

        // Do not duplicate the initial input in `pending_input`.
        // It is already recorded to history above; ephemeral items are appended separately.
        if first_iteration {
            first_iteration = false;
        } else {
            // Only record pending input to history on subsequent iterations
            sess.record_conversation_items(&pending_input).await;
        }

        if auto_compact_pending && !is_review_mode {
            let compacted_history = if compact::should_use_remote_compact_task(&sess).await {
                run_inline_remote_auto_compact_task(
                    Arc::clone(&sess),
                    Arc::clone(&turn_context),
                    Vec::new(),
                )
                .await
            } else {
                compact::run_inline_auto_compact_task(
                    Arc::clone(&sess),
                    Arc::clone(&turn_context),
                )
                .await
            };

            if !compacted_history.is_empty() {
                let mut rebuilt = compacted_history;
                if !pending_input_tail.is_empty() {
                    let previous_input_snapshot = compact_snapshot.unwrap_or_default();
                    let (missing_calls, filtered_outputs) = reconcile_pending_tool_outputs(
                        &pending_input_tail,
                        &rebuilt,
                        &previous_input_snapshot,
                    );
                    if !missing_calls.is_empty() {
                        rebuilt.extend(missing_calls);
                    }
                    if !filtered_outputs.is_empty() {
                        rebuilt.extend(filtered_outputs);
                    }
                }
                sess.replace_history(rebuilt);
                pending_input_tail.clear();
                did_proactive_compact_this_iteration = true;
            }
            auto_compact_pending = false;
        }

        // Construct the input that we will send to the model. When using the
        // Chat completions API (or ZDR clients), the model needs the full
        // conversation history on each turn. The rollout file, however, should
        // only record the new items that originated in this turn so that it
        // represents an append-only log without duplicates.
        let turn_input: Vec<ResponseItem> = if is_review_mode {
            if !pending_input_tail.is_empty() {
                review_history.extend(pending_input_tail.clone());
            }
            review_history.clone()
        } else {
            sess.turn_input_with_history(pending_input_tail.clone())
        };

        let turn_input_messages: Vec<String> = turn_input
            .iter()
            .filter_map(|item| match item {
                ResponseItem::Message { role, content, .. } if role == "user" => Some(content),
                _ => None,
            })
            .flat_map(|content| {
                content.iter().filter_map(|item| match item {
                    ContentItem::InputText { text } => Some(text.clone()),
                    _ => None,
                })
            })
            .collect();
        match run_turn(
            &sess,
            &turn_context,
            &mut turn_diff_tracker,
            sub_id.clone(),
            initial_response_item.clone(),
            pending_input_tail,
            turn_input,
        )
        .await
        {
            Ok(turn_output) => {
                let mut items_to_record_in_conversation_history = Vec::<ResponseItem>::new();
                let mut responses = Vec::<ResponseInputItem>::new();
                for processed_response_item in turn_output {
                    let ProcessedResponseItem { item, response } = processed_response_item;
                    match (&item, &response) {
                        (ResponseItem::Message { role, .. }, None) if role == "assistant" => {
                            // If the model returned a message, we need to record it.
                            items_to_record_in_conversation_history.push(item.clone());
                            if is_review_mode
                                && let ResponseItem::Message { content, .. } = &item {
                                    for ci in content {
                                        if let ContentItem::OutputText { text } = ci {
                                            review_messages.push(text.clone());
                                        }
                                    }
                                }
                        }
                        (
                            ResponseItem::LocalShellCall { .. },
                            Some(ResponseInputItem::FunctionCallOutput { call_id, output }),
                        ) => {
                            items_to_record_in_conversation_history.push(item.clone());
                            items_to_record_in_conversation_history.push(
                                ResponseItem::FunctionCallOutput {
                                    call_id: call_id.clone(),
                                    output: output.clone(),
                                },
                            );
                        }
                        (
                            ResponseItem::FunctionCall { .. },
                            Some(ResponseInputItem::FunctionCallOutput { call_id, output }),
                        ) => {
                            debug!(
                                "Recording function call and output for call_id: {}",
                                call_id
                            );
                            items_to_record_in_conversation_history.push(item.clone());
                            items_to_record_in_conversation_history.push(
                                ResponseItem::FunctionCallOutput {
                                    call_id: call_id.clone(),
                                    output: output.clone(),
                                },
                            );
                        }
                        (
                            ResponseItem::CustomToolCall { .. },
                            Some(ResponseInputItem::CustomToolCallOutput { call_id, output }),
                        ) => {
                            items_to_record_in_conversation_history.push(item.clone());
                            items_to_record_in_conversation_history.push(
                                ResponseItem::CustomToolCallOutput {
                                    call_id: call_id.clone(),
                                    output: output.clone(),
                                },
                            );
                        }
                        (
                            ResponseItem::FunctionCall { .. },
                            Some(ResponseInputItem::McpToolCallOutput { call_id, result }),
                        ) => {
                            items_to_record_in_conversation_history.push(item.clone());
                            let output =
                                convert_call_tool_result_to_function_call_output_payload(result);
                            items_to_record_in_conversation_history.push(
                                ResponseItem::FunctionCallOutput {
                                    call_id: call_id.clone(),
                                    output,
                                },
                            );
                        }
                        (
                            ResponseItem::Reasoning {
                                id,
                                summary,
                                content,
                                encrypted_content,
                            },
                            None,
                        ) => {
                            items_to_record_in_conversation_history.push(ResponseItem::Reasoning {
                                id: id.clone(),
                                summary: summary.clone(),
                                content: content.clone(),
                                encrypted_content: encrypted_content.clone(),
                            });
                        }
                        _ => {
                            warn!("Unexpected response item: {item:?} with response: {response:?}");
                        }
                    };
                    if let Some(response) = response {
                        responses.push(response);
                    }
                }

                // Only attempt to take the lock if there is something to record.
                if !items_to_record_in_conversation_history.is_empty() {
                    if is_review_mode {
                        review_history.extend(items_to_record_in_conversation_history.clone());
                    } else {
                        // Record items in their original chronological order to maintain
                        // proper sequence of events. This ensures function calls and their
                        // outputs appear in the correct order in conversation history.
                        sess.record_conversation_items(&items_to_record_in_conversation_history)
                            .await;
                    }
                }

                // Check whether we should proactively compact before queuing follow-up work.
                // Upstream codex-rs compacts as soon as usage hits the configured threshold,
                // which keeps us from hitting hard context-window errors mid-session.
                let limit = turn_context
                    .client
                    .get_auto_compact_token_limit()
                    .unwrap_or(i64::MAX);
                let most_recent_usage_tokens: Option<i64> = {
                    let state = sess.state.lock().unwrap();
                    state.token_usage_info.as_ref().and_then(|info| {
                        info.last_token_usage.total_tokens.try_into().ok()
                    })
                };
                // auto_compact_token_limit is defined relative to a single turn's
                // token usage (input + output). Using the cumulative total caused
                // the limit check to stay tripped permanently once crossed, even
                // after compacting history, which spammed repeated /compact runs.
                let token_limit_reached = most_recent_usage_tokens
                    .is_some_and(|tokens| tokens >= limit);

                // If there are responses, add them to pending input for the next iteration
                if !responses.is_empty() {
                    if !is_review_mode {
                        for response in &responses {
                            sess.add_pending_input(response.clone());
                        }
                    }
                    // Reset the proactive compact guard for the next iteration since we're
                    // about to process new tool calls and may need to compact again
                    did_proactive_compact_this_iteration = false;
                }

                // As long as compaction works well in getting us way below the token limit,
                // we shouldn't worry about being in an infinite loop. However, guard against
                // repeated compaction attempts within a single iteration.
                if token_limit_reached && !did_proactive_compact_this_iteration && !is_review_mode {
                    let attempt_req = sess.current_request_ordinal();
                    let order = sess.next_background_order(&sub_id, attempt_req, None);
                    sess
                        .notify_background_event_with_order(
                            &sub_id,
                            order,
                            "Token limit reached; running /compact and continuing…".to_string(),
                        )
                        .await;

                    if responses.is_empty() {
                        did_proactive_compact_this_iteration = true;
                        // Choose between local and remote compact based on auth mode,
                        // matching upstream codex-rs behavior
                        if compact::should_use_remote_compact_task(&sess).await {
                            let _ = run_inline_remote_auto_compact_task(
                                Arc::clone(&sess),
                                Arc::clone(&turn_context),
                                Vec::new(),
                            )
                            .await;
                        } else {
                            let _ = compact::run_inline_auto_compact_task(
                                Arc::clone(&sess),
                                Arc::clone(&turn_context),
                            )
                            .await;
                        }

                        // Restart this loop with the newly compacted history so the
                        // next turn can see the trimmed conversation state.
                        continue;
                    }

                    if !auto_compact_pending {
                        auto_compact_pending = true;
                    }
                }

                if responses.is_empty() {
                    debug!("Turn completed");
                    last_task_message = get_last_assistant_message_from_turn(
                        &items_to_record_in_conversation_history,
                    );
                    if let Some(m) = last_task_message.as_ref() {
                        tracing::info!("core.turn completed: last_assistant_message.len={}", m.len());
                    }
                    sess.maybe_notify(UserNotification::AgentTurnComplete {
                        turn_id: sub_id.clone(),
                        input_messages: turn_input_messages,
                        last_assistant_message: last_task_message.clone(),
                    });
                    break;
                }
            }
            Err(e) => {
                info!("Turn error: {e:#}");
                let event = sess.make_event(
                    &sub_id,
                    EventMsg::Error(ErrorEvent { message: e.to_string() }),
                );
                sess.tx_event.send(event).await.ok();
                if is_review_mode && !review_exit_emitted {
                    exit_review_mode(sess.clone(), sub_id.clone(), None).await;
                    review_exit_emitted = true;
                }
                // let the user continue the conversation
                break;
            }
        }
    }
    if is_review_mode && !review_exit_emitted {
        let combined = if !review_messages.is_empty() {
            review_messages.join("\n\n")
        } else {
            last_task_message.clone().unwrap_or_default()
        };
        let output = if combined.trim().is_empty() {
            None
        } else {
            Some(parse_review_output_event(&combined))
        };
        exit_review_mode(sess.clone(), sub_id.clone(), output).await;
    }

    sess.remove_task(&sub_id);
    let event = sess.make_event(
        &sub_id,
        EventMsg::TaskComplete(TaskCompleteEvent {
            last_agent_message: last_task_message,
        }),
    );
    if let EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message: Some(m) }) = &event.msg {
        tracing::info!("core.emit TaskComplete last_agent_message.len={}", m.len());
    }
    sess.tx_event.send(event).await.ok();

    if let Some(compact_sub_id) = sess.dequeue_manual_compact() {
        let turn_context = sess.make_turn_context();
        let prompt_text = sess.compact_prompt_text();
        compact::spawn_compact_task(
            Arc::clone(&sess),
            turn_context,
            compact_sub_id,
            vec![InputItem::Text {
                text: prompt_text,
            }],
        );
        return;
    }

    if let Some(queued) = sess.pop_next_queued_user_input() {
        let sess_clone = Arc::clone(&sess);
        tokio::spawn(async move {
            sess_clone.cleanup_old_status_items().await;
            let turn_context = sess_clone.make_turn_context();
            let submission_id = queued.submission_id;
            let items = queued.core_items;
            let agent = AgentTask::spawn(Arc::clone(&sess_clone), turn_context, submission_id, items);
            sess_clone.set_task(agent);
        });
    }
}
