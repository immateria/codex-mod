use super::*;

mod handle_item;
mod latency;
mod stream;

pub(super) async fn run_turn(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: String,
    initial_user_item: Option<ResponseItem>,
    pending_input_tail: Vec<ResponseItem>,
    mut input: Vec<ResponseItem>,
) -> CodexResult<Vec<ProcessedResponseItem>> {
    // Check if browser is enabled
    let browser_enabled = code_browser::global::get_browser_manager().await.is_some();

    let tc = &**turn_context;
    let agents_active = {
        let manager = AGENT_MANAGER.read().await;
        manager.has_active_agents()
    };

    let mut retries = 0;
    let mut rate_limit_switch_state = RateLimitSwitchState::default();
    let collaboration_mode_instructions =
        render_collaboration_mode_instructions(tc.collaboration_mode);
    // Ensure we only auto-compact once per turn to avoid loops
    let mut did_auto_compact = false;
    // Attempt input starts as the provided input, and may be augmented with
    // items from a previous dropped stream attempt so we don't lose progress.
    let _mcp_turn_allow_guard =
        mcp_access::McpTurnAllowGuard::new(Arc::clone(sess), sub_id.clone());
    mcp_access::preflight_turn_skill_input(
        sess,
        turn_context,
        sub_id.as_str(),
        initial_user_item.as_ref(),
        pending_input_tail.as_slice(),
        &mut input,
    )
    .await;

    let mut attempt_input: Vec<ResponseItem> = input.clone();
    loop {
        // Each loop iteration corresponds to a single provider HTTP request.
        // Increment the attempt ordinal first and capture its value so all
        // OrderMeta emitted during this attempt share the same `req`, even if
        // later attempts start before all events have been delivered.
        sess.begin_http_attempt();
        let attempt_req = sess.current_request_ordinal();
        // Build status items (screenshots, system status) fresh for each attempt
        let status_items = build_turn_status_items(sess).await;

        let mut prepend_developer_messages: Vec<String> = tc
            .demo_developer_message
            .clone()
            .into_iter()
            .collect();
        let trimmed_mode_instructions = collaboration_mode_instructions.trim();
        if !trimmed_mode_instructions.is_empty() {
            prepend_developer_messages.push(trimmed_mode_instructions.to_string());
        }
        if let Some(shell_style) = sess.user_shell.script_style() {
            prepend_developer_messages.push(shell_style.developer_instruction().to_string());
        }
        prepend_developer_messages.extend(
            sess
                .shell_style_profile_messages
                .iter()
                .filter_map(|message| {
                    let trimmed = message.trim();
                    (!trimmed.is_empty()).then(|| trimmed.to_string())
                }),
        );
        if should_inject_html_sanitizer_guardrails(&attempt_input) {
            prepend_developer_messages.push(HTML_SANITIZER_GUARDRAILS_MESSAGE.to_string());
        }

        let mut prompt = Prompt {
            input: attempt_input.clone(),
            store: !sess.disable_response_storage,
            user_instructions: tc.user_instructions.clone(),
            environment_context: Some(EnvironmentContext::new(
                Some(tc.cwd.clone()),
                Some(tc.approval_policy),
                Some(tc.sandbox_policy.clone()),
                Some(sess.user_shell.clone()),
            )),
            tools: Vec::new(),
            status_items, // Include status items with this request
            base_instructions_override: tc.base_instructions.clone(),
            include_additional_instructions: true,
            prepend_developer_messages,
            text_format: tc.text_format_override.clone(),
            model_override: None,
            model_family_override: None,
            output_schema: tc.final_output_json_schema.clone(),
            log_tag: Some("codex/turn".to_string()),
            session_id_override: None,
            model_descriptions: sess.model_descriptions.clone(),
        };

        sess.apply_remote_model_overrides(&mut prompt).await;

        let effective_family = prompt
            .model_family_override
            .as_ref()
            .unwrap_or_else(|| tc.client.default_model_family());
        let tools_config = tc.client.build_tools_config_with_sandbox_for_family(
            tc.sandbox_policy.clone(),
            effective_family,
        );
        let mcp_access = sess.mcp_access_snapshot();
        let mcp_tools = if tools_config.search_tool {
            let selection = sess.mcp_tool_selection_snapshot().unwrap_or_default();
            if selection.is_empty() {
                None
            } else {
                let selection_lower: std::collections::HashSet<String> = selection
                    .iter()
                    .map(|tool| tool.to_ascii_lowercase())
                    .collect();
                let session_deny: std::collections::HashSet<String> = mcp_access
                    .session_deny_servers
                    .iter()
                    .map(|name| name.to_ascii_lowercase())
                    .collect();
                let mut selected = std::collections::HashMap::new();
                for (qualified_name, server_name, tool) in sess
                    .mcp_connection_manager
                    .list_all_tools_with_server_names()
                {
                    if session_deny.contains(&server_name.to_ascii_lowercase()) {
                        continue;
                    }
                    if !selection_lower.contains(&qualified_name.to_ascii_lowercase()) {
                        continue;
                    }
                    selected.insert(qualified_name, tool);
                }
                (!selected.is_empty()).then_some(selected)
            }
        } else {
            Some(crate::mcp::policy::filter_tools_for_turn(
                &sess.mcp_connection_manager,
                &mcp_access,
                sub_id.as_str(),
            ))
        };
        prompt.tools = get_openai_tools(
            &tools_config,
            mcp_tools,
            browser_enabled,
            agents_active,
            sess.dynamic_tools.as_slice(),
        );
        if should_inject_search_tool_developer_instructions(&prompt.tools) {
            let search_tool_instructions = SEARCH_TOOL_DEVELOPER_INSTRUCTIONS.trim();
            if !search_tool_instructions.is_empty()
                && !prompt
                    .prepend_developer_messages
                    .iter()
                    .any(|message| message.trim() == search_tool_instructions)
            {
                prompt
                    .prepend_developer_messages
                    .push(search_tool_instructions.to_string());
            }
        }

        // Start a new scratchpad for this HTTP attempt
        sess.begin_attempt_scratchpad();

        match stream::try_run_turn(sess, turn_diff_tracker, &sub_id, &prompt, attempt_req).await {
            Ok(output) => {
                // Record status items to conversation history after successful turn
                // This ensures they persist for future requests in the right chronological order
                if !prompt.status_items.is_empty() {
                    sess.record_conversation_items(&prompt.status_items).await;
                }
                // Commit successful attempt – scratchpad is no longer needed.
                sess.clear_scratchpad();
                return Ok(output);
            }
            Err(CodexErr::Interrupted) => return Err(CodexErr::Interrupted),
            Err(CodexErr::EnvVar(var)) => return Err(CodexErr::EnvVar(var)),
            Err(CodexErr::UsageLimitReached(limit_err)) => {
                if let Some(ctx) = account_usage_context(sess) {
                    let usage_home = ctx.code_home.clone();
                    let usage_account = ctx.account_id.clone();
                    let usage_plan = ctx.plan.clone();
                    let resets = limit_err.resets_in_seconds;
                    spawn_usage_task(move || {
                        if let Err(err) = account_usage::record_usage_limit_hint(
                            &usage_home,
                            &usage_account,
                            usage_plan.as_deref(),
                            resets,
                            Utc::now(),
                        ) {
                            warn!("Failed to persist usage limit hint: {err}");
                        }
                    });
                }

                let mut switched = false;
                if sess.client.auto_switch_accounts_on_rate_limit()
                    && auth::read_code_api_key_from_env().is_none()
                    && let Some(auth_manager) = sess.client.get_auth_manager() {
                        let auth = auth_manager.auth();
                        let current_account_id = auth
                            .as_ref()
                            .and_then(crate::auth::CodexAuth::get_account_id)
                            .or_else(|| {
                                auth_accounts::get_active_account_id(sess.client.code_home())
                                    .ok()
                                    .flatten()
                            });
                        if let Some(current_account_id) = current_account_id {
                            let now = Utc::now();
                            let blocked_until = limit_err.resets_in_seconds.map(|seconds| {
                                now + chrono::Duration::seconds(seconds as i64)
                            });
                            let current_auth_mode = auth
                                .as_ref()
                                .map(|current| current.mode)
                                .unwrap_or(AppAuthMode::ApiKey);
                            match crate::account_switching::switch_active_account_on_rate_limit(
                                crate::account_switching::SwitchActiveAccountOnRateLimitParams {
                                    code_home: sess.client.code_home(),
                                    auth_credentials_store_mode: sess
                                        .client
                                        .auth_credentials_store_mode(),
                                    state: &mut rate_limit_switch_state,
                                    allow_api_key_fallback: sess
                                        .client
                                        .api_key_fallback_on_all_accounts_limited(),
                                    now,
                                    current_account_id: current_account_id.as_str(),
                                    current_mode: current_auth_mode,
                                    blocked_until,
                                },
                            ) {
                                Ok(Some(next_account_id)) => {
                                    let next_label = auth_accounts::find_account(
                                        sess.client.code_home(),
                                        &next_account_id,
                                    )
                                    .ok()
                                    .flatten()
                                    .and_then(|account| account.label)
                                    .unwrap_or_else(|| next_account_id.clone());
                                    tracing::info!(
                                        from_account_id = %current_account_id,
                                        to_account_id = %next_account_id,
                                        reason = "usage_limit_reached",
                                        "rate limit hit; auto-switching active account"
                                    );
                                    auth_manager.reload();
                                    let order = sess.next_background_order(&sub_id, attempt_req, None);
                                    let notice = format!(
                                        "Auto-switch: now using {next_label} due to usage limit."
                                    );
                                    sess
                                        .notify_background_event_with_order(
                                            &sub_id,
                                            order,
                                            notice,
                                        )
                                        .await;
                                    switched = true;
                                }
                                Ok(None) => {}
                                Err(err) => {
                                    tracing::warn!(
                                        from_account_id = %current_account_id,
                                        error = %err,
                                        "failed to activate account after usage limit"
                                    );
                                }
                            }
                        }
                    }

                if switched {
                    retries = 0;
                    continue;
                }

                let now = Utc::now();
                let retry_after = limit_err
                    .retry_after(now)
                    .unwrap_or_else(|| RetryAfter::from_duration(std::time::Duration::from_secs(5 * 60), now));
                let eta = format_retry_eta(&retry_after);
                let mut retry_message = format!("{limit_err} Auto-retrying");
                if let Some(eta) = eta {
                    retry_message.push_str(&format!(" at {eta}"));
                }
                retry_message.push('…');
                sess.notify_stream_error(&sub_id, retry_message).await;
                tokio::time::sleep(retry_after.delay).await;
                retries = 0;
                continue;
            }
            Err(CodexErr::UsageNotIncluded) => return Err(CodexErr::UsageNotIncluded),
            Err(CodexErr::QuotaExceeded) => return Err(CodexErr::QuotaExceeded),
            Err(e) => {
                // Detect context-window overflow and auto-run a compact summarization once
                if !did_auto_compact
                    && let CodexErr::Stream(msg, _maybe_delay, _req_id) = &e {
                        let lower = msg.to_ascii_lowercase();
                        let looks_like_context_overflow =
                            lower.contains("exceeds the context window")
                                || lower.contains("exceed the context window")
                                || lower.contains("context length exceeded")
                                || lower.contains("maximum context length")
                                || (lower.contains("context window")
                                    && (lower.contains("exceed")
                                        || lower.contains("exceeded")
                                        || lower.contains("full")
                                        || lower.contains("too long")));

                        if looks_like_context_overflow {
                            did_auto_compact = true;
                            sess
                                .notify_stream_error(
                                    &sub_id,
                                    "Model hit context-window limit; running /compact and retrying…"
                                        .to_string(),
                                )
                                .await;

                            let previous_input_snapshot = input.clone();
                            let compacted_history = if compact::should_use_remote_compact_task(sess).await {
                                run_inline_remote_auto_compact_task(
                                    Arc::clone(sess),
                                    Arc::clone(turn_context),
                                    Vec::new(),
                                )
                                .await
                            } else {
                                compact::run_inline_auto_compact_task(
                                    Arc::clone(sess),
                                    Arc::clone(turn_context),
                                )
                                .await
                            };

                            // Reset any partial attempt state and rebuild the request payload using the
                            // newly compacted history plus the current user turn items.
                            sess.clear_scratchpad();

                            if compacted_history.is_empty() {
                                attempt_input = input.clone();
                            } else {
                                let mut rebuilt = compacted_history;
                                if let Some(initial_item) = initial_user_item.clone() {
                                    rebuilt.push(initial_item);
                                }
                                if !pending_input_tail.is_empty() {
                                    let (missing_calls, filtered_outputs) =
                                        reconcile_pending_tool_outputs(&pending_input_tail, &rebuilt, &previous_input_snapshot);
                                    if !missing_calls.is_empty() {
                                        rebuilt.extend(missing_calls);
                                    }
                                    if !filtered_outputs.is_empty() {
                                        rebuilt.extend(filtered_outputs);
                                    }
                                }
                                input = rebuilt.clone();
                                attempt_input = rebuilt;
                            }
                            continue;
                        }
                    }

                // Use the configured provider-specific stream retry budget.
                let max_retries = tc.client.get_provider().stream_max_retries();
                let req_id = match &e {
                    CodexErr::Stream(_, _, req) => req.clone(),
                    _ => None,
                };
                let is_connectivity = is_connectivity_error(&e);
                let drain_scratchpad_into_attempt = |attempt_input: &mut Vec<ResponseItem>| {
                    if let Some(sp) = sess.take_scratchpad() {
                        inject_scratchpad_into_attempt_input(attempt_input, sp);
                    }
                };

                if is_connectivity && retries >= max_retries {
                    let probe = tc.client.get_provider().base_url_for_probe();
                    let wait_message = format!(
                        "Network unavailable; waiting to reconnect to {probe} ({e})"
                    );
                    sess.notify_stream_error(&sub_id, wait_message).await;
                    drain_scratchpad_into_attempt(&mut attempt_input);
                    wait_for_connectivity(&probe).await;
                    retries = 0;
                    continue;
                }

                if retries < max_retries {
                    retries += 1;
                    let (delay, retry_eta) = match e {
                        CodexErr::Stream(_, Some(ref retry_after), _) => {
                            let eta = format_retry_eta(retry_after);
                            (retry_after.delay, eta)
                        }
                        _ => (backoff(retries), None),
                    };
                    warn!(
                        error = %e,
                        request_id = req_id.as_deref(),
                        "stream disconnected - retrying turn in {delay:?} (attempt {retries}/{max_retries})",
                    );

                    // Surface retry information to any UI/front‑end so the
                    // user understands what is happening instead of staring
                    // at a seemingly frozen screen.
                    let mut retry_message =
                        format!("stream error: {e}; retrying in {delay:?}");
                    if let Some(eta) = retry_eta {
                        retry_message.push_str(&format!(" (next attempt at {eta})"));
                    }
                    retry_message.push('…');
                    sess.notify_stream_error(&sub_id, retry_message.clone()).await;
                    // Pull any partial progress from this attempt and append to
                    // the next request's input so we do not lose tool progress
                    // or already-finalized items.
                    drain_scratchpad_into_attempt(&mut attempt_input);

                    tokio::time::sleep(delay).await;
                } else {
                    error!(
                        retries,
                        max_retries,
                        auto_compact_attempted = did_auto_compact,
                        request_id = req_id.as_deref(),
                        error = %e,
                        "stream disconnected - retries exhausted"
                    );
                    return Err(e);
                }
            }
        }
    }
}

/// When the model is prompted, it returns a stream of events. Some of these
/// events map to a `ResponseItem`. A `ResponseItem` may need to be
/// "handled" such that it produces a `ResponseInputItem` that needs to be
/// sent back to the model on the next turn.
#[derive(Debug)]
pub(super) struct ProcessedResponseItem {
    pub(super) item: ResponseItem,
    pub(super) response: Option<ResponseInputItem>,
}

