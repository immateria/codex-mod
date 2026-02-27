use super::*;

mod configure_session;
mod skills;

pub(in crate::codex) async fn submission_loop(
    mut session_id: Uuid,
    config: Arc<Config>,
    auth_manager: Option<Arc<AuthManager>>,
    rx_sub: Receiver<Submission>,
    tx_event: Sender<Event>,
) {
    let mut config = config;
    let mut sess: Option<Arc<Session>> = None;
    let mut agent_manager_initialized = false;

    let file_watcher = crate::file_watcher::FileWatcher::new(config.code_home.clone())
        .unwrap_or_else(|err| {
            warn!("failed to start file watcher: {err}");
            crate::file_watcher::FileWatcher::noop()
        });
    file_watcher.register_config(config.as_ref());
    let mut file_watcher_rx = file_watcher.subscribe();
    let mut file_watcher_enabled = true;
    // shorthand - send an event when there is no active session
    let send_no_session_event = |sub_id: String| async {
        let event = Event {
            id: sub_id,
            event_seq: 0,
            msg: EventMsg::Error(ErrorEvent { message: "No session initialized, expected 'ConfigureSession' as first Op".to_string() }),
            order: None,
        };
        tx_event.send(event).await.ok();
    };

    // To break out of this loop, send Op::Shutdown.
    loop {
        tokio::select! {
            sub = rx_sub.recv() => {
                let sub = match sub {
                    Ok(sub) => sub,
                    Err(_) => break,
                };

                debug!(?sub, "Submission");
                match sub.op {
            Op::Interrupt => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess.clone(),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                tokio::spawn(async move {
                    sess.notify_wait_interrupted(WaitInterruptReason::SessionAborted);
                    sess.abort();
                });
            }
            Op::CancelAgents { batch_ids, agent_ids } => {
                let sess_arc = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                let mut manager = AGENT_MANAGER.write().await;
                let mut seen_batches: HashSet<String> = HashSet::new();
                let mut seen_agents: HashSet<String> = HashSet::new();
                let mut cancelled = 0usize;

                for batch in batch_ids {
                    let trimmed = batch.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if !seen_batches.insert(trimmed.to_string()) {
                        continue;
                    }
                    cancelled += manager.cancel_batch(trimmed).await;
                }

                for agent_id in agent_ids {
                    let trimmed = agent_id.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if !seen_agents.insert(trimmed.to_string()) {
                        continue;
                    }
                    if manager.cancel_agent(trimmed).await {
                        cancelled += 1;
                    }
                }

                drop(manager);

                send_agent_status_update(&sess_arc).await;

                let message = if cancelled == 0 {
                    "No running agents to cancel.".to_string()
                } else {
                    let suffix = if cancelled == 1 { "" } else { "s" };
                    format!("Cancelled {cancelled} running agent{suffix}.")
                };

                let event = sess_arc.make_event(
                    &sub.id,
                    EventMsg::AgentMessage(AgentMessageEvent { message }),
                );
                sess_arc.send_event(event).await;
            }
            Op::AddPendingInputDeveloper { text } => {
                let sess = match sess.as_ref() { Some(s) => s.clone(), None => { send_no_session_event(sub.id).await; continue; } };
                let dev_msg = ResponseInputItem::Message { role: "developer".to_string(), content: vec![ContentItem::InputText { text }] };
                let should_start_turn = sess.enqueue_out_of_turn_item(dev_msg);
                if should_start_turn {
                    sess.cleanup_old_status_items().await;
                    let turn_context = sess.make_turn_context();
                    let sub_id = sess.next_internal_sub_id();
                    let sentinel_input = vec![InputItem::Text {
                        text: PENDING_ONLY_SENTINEL.to_string(),
                    }];
                    let agent = AgentTask::spawn(Arc::clone(&sess), turn_context, sub_id, sentinel_input);
                    sess.set_task(agent);
                }
            }
            op @ Op::ConfigureSession { .. } => {
                let state = configure_session::ConfigureSessionState {
                    session_id,
                    config,
                    sess,
                    agent_manager_initialized,
                };

                let (state, control) = configure_session::handle_configure_session(
                    state,
                    auth_manager.clone(),
                    &tx_event,
                    &file_watcher,
                    sub.id,
                    op,
                )
                .await;

                session_id = state.session_id;
                config = state.config;
                sess = state.sess;
                agent_manager_initialized = state.agent_manager_initialized;

                if matches!(control, configure_session::ConfigureSessionControl::Exit) {
                    return;
                }
            }
            Op::UserInput {
                items,
                final_output_json_schema,
            } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                // Clean up old status items when new user input arrives
                // This prevents token buildup from old screenshots/status messages
                sess.cleanup_old_status_items().await;

                // Abort synchronously here to avoid a race that can kill the
                // newly spawned agent if the async abort runs after set_task.
                sess.notify_wait_interrupted(WaitInterruptReason::UserMessage);
                sess.abort();

                // Spawn a new agent for this user input.
                let turn_context = sess.make_turn_context_with_schema(final_output_json_schema);
                let agent = AgentTask::spawn(Arc::clone(sess), turn_context, sub.id.clone(), items);
                sess.set_task(agent);
            }
            Op::QueueUserInput { items } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                if sess.has_running_task() {
                    let mut response_item = response_input_from_core_items(items.clone());
                    sess.enforce_user_message_limits(&sub.id, &mut response_item);
                    sess.notify_wait_interrupted(WaitInterruptReason::UserMessage);
                    let queued = QueuedUserInput {
                        submission_id: sub.id.clone(),
                        response_item,
                        core_items: items,
                    };
                    sess.queue_user_input(queued);
                } else {
                    // No task running: treat this as immediate user input without aborting.
                    sess.cleanup_old_status_items().await;
                    let turn_context = sess.make_turn_context();
                    let agent = AgentTask::spawn(Arc::clone(sess), turn_context, sub.id.clone(), items);
                    sess.set_task(agent);
                }
            }
            Op::ExecApproval { id, decision, .. } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                match decision {
                    ReviewDecision::Abort => {
                        sess.notify_wait_interrupted(WaitInterruptReason::SessionAborted);
                        sess.abort();
                    }
                    other => sess.notify_approval(&id, other),
                }
            }
            Op::UserInputAnswer { id, response } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                sess.notify_user_input_response(&id, response);
            }
            Op::DynamicToolResponse { id, response } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                sess.notify_dynamic_tool_response(&id, response);
            }
            Op::RegisterApprovedCommand {
                command,
                match_kind,
                semantic_prefix,
            } => {
                if command.is_empty() {
                    continue;
                }
                if let Some(sess) = sess.as_ref() {
                    sess.add_approved_command(ApprovedCommandPattern::new(
                        command,
                        match_kind,
                        semantic_prefix,
                    ));
                } else {
                    send_no_session_event(sub.id).await;
                }
            }
            Op::PatchApproval { id, decision } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                match decision {
                    ReviewDecision::Abort => {
                        sess.notify_wait_interrupted(WaitInterruptReason::SessionAborted);
                        sess.abort();
                    }
                    other => sess.notify_approval(&id, other),
                }
            }
            Op::UpdateValidationTool { name, enable } => {
                if let Some(sess) = sess.as_ref() {
                    sess.update_validation_tool(&name, enable);
                } else {
                    send_no_session_event(sub.id).await;
                }
            }
            Op::UpdateValidationGroup { group, enable } => {
                if let Some(sess) = sess.as_ref() {
                    sess.update_validation_group(group, enable);
                } else {
                    send_no_session_event(sub.id).await;
                }
            }
            Op::AddToHistory { text } => {
                // TODO: What should we do if we got AddToHistory before ConfigureSession?
                // currently, if ConfigureSession has resume path, this history will be ignored
                let id = session_id;
                let config = config.clone();
                tokio::spawn(async move {
                    if let Err(e) = crate::message_history::append_entry(&text, &id, &config).await
                    {
                        warn!("failed to append to message history: {e}");
                    }
                });
            }

            Op::PersistHistorySnapshot { snapshot } => {
                let Some(sess) = sess.as_ref() else {
                    send_no_session_event(sub.id).await;
                    continue;
                };
                if let Some(recorder) = sess.clone_rollout_recorder() {
                    tokio::spawn(async move {
                        if let Err(e) = recorder.set_history_snapshot(snapshot).await {
                            warn!("failed to persist history snapshot: {e}");
                        }
                    });
                }
            }

            Op::RunProjectCommand { name } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                let mut tracker = TurnDiffTracker::new();
                let attempt_req = sess.current_request_ordinal();
                sess.run_project_command(&mut tracker, &sub.id, &name, attempt_req)
                    .await;
            }

            Op::GetHistoryEntryRequest { offset, log_id } => {
                let config = config.clone();
                let tx_event = tx_event.clone();
                let sub_id = sub.id.clone();

                tokio::spawn(async move {
                    // Run lookup in blocking thread because it does file IO + locking.
                    let entry_opt = tokio::task::spawn_blocking(move || {
                        crate::message_history::lookup(log_id, offset, &config)
                    })
                    .await
                    .unwrap_or(None);

                    let event = Event {
                        id: sub_id,
                        event_seq: 0,
                        msg: EventMsg::GetHistoryEntryResponse(
                            crate::protocol::GetHistoryEntryResponseEvent {
                                offset,
                                log_id,
                                entry: entry_opt,
                            },
                        ),
                        order: None,
                    };

                    if let Err(e) = tx_event.send(event).await {
                        warn!("failed to send GetHistoryEntryResponse event: {e}");
                    }
                });
            }
            Op::ListMcpTools => {
                let sess = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                let tools = sess
                    .mcp_connection_manager
                    .list_all_tools()
                    .into_iter()
                    .filter_map(|(name, tool)| {
                        let value = match serde_json::to_value(tool) {
                            Ok(value) => value,
                            Err(err) => {
                                warn!("failed to serialize MCP tool {name}: {err}");
                                return None;
                            }
                        };
                        match code_protocol::mcp::Tool::from_mcp_value(value) {
                            Ok(converted) => Some((name, converted)),
                            Err(err) => {
                                warn!("failed to convert MCP tool {name}: {err}");
                                None
                            }
                        }
                    })
                    .collect();
                let server_tools = sess.mcp_connection_manager.list_tools_by_server();
                let server_disabled_tools =
                    sess.mcp_connection_manager.list_disabled_tools_by_server();
                let server_failures = sess.mcp_connection_manager.list_server_failures();
                let resources =
                    convert_mcp_resources_by_server(sess.mcp_connection_manager.list_resources_by_server().await);
                let resource_templates = convert_mcp_resource_templates_by_server(
                    sess.mcp_connection_manager
                        .list_resource_templates_by_server()
                        .await,
                );
                let auth_statuses = sess.mcp_connection_manager.list_auth_statuses().await;

                let event = Event {
                    id: sub.id.clone(),
                    event_seq: 0,
                    msg: EventMsg::McpListToolsResponse(McpListToolsResponseEvent {
                        tools,
                        server_tools: Some(server_tools),
                        server_disabled_tools: Some(server_disabled_tools),
                        server_failures: Some(server_failures),
                        resources,
                        resource_templates,
                        auth_statuses,
                    }),
                    order: None,
                };

                if let Err(e) = tx_event.send(event).await {
                    warn!("failed to send McpListToolsResponse event: {e}");
                }
            }
            Op::RefreshMcpTools => {
                let sess = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                sess.mcp_connection_manager.refresh_tools().await;

                let tools = sess
                    .mcp_connection_manager
                    .list_all_tools()
                    .into_iter()
                    .filter_map(|(name, tool)| {
                        let value = match serde_json::to_value(tool) {
                            Ok(value) => value,
                            Err(err) => {
                                warn!("failed to serialize MCP tool {name}: {err}");
                                return None;
                            }
                        };
                        match code_protocol::mcp::Tool::from_mcp_value(value) {
                            Ok(converted) => Some((name, converted)),
                            Err(err) => {
                                warn!("failed to convert MCP tool {name}: {err}");
                                None
                            }
                        }
                    })
                    .collect();
                let server_tools = sess.mcp_connection_manager.list_tools_by_server();
                let server_disabled_tools =
                    sess.mcp_connection_manager.list_disabled_tools_by_server();
                let server_failures = sess.mcp_connection_manager.list_server_failures();
                let resources =
                    convert_mcp_resources_by_server(sess.mcp_connection_manager.list_resources_by_server().await);
                let resource_templates = convert_mcp_resource_templates_by_server(
                    sess.mcp_connection_manager
                        .list_resource_templates_by_server()
                        .await,
                );
                let auth_statuses = sess.mcp_connection_manager.list_auth_statuses().await;

                let event = Event {
                    id: sub.id.clone(),
                    event_seq: 0,
                    msg: EventMsg::McpListToolsResponse(McpListToolsResponseEvent {
                        tools,
                        server_tools: Some(server_tools),
                        server_disabled_tools: Some(server_disabled_tools),
                        server_failures: Some(server_failures),
                        resources,
                        resource_templates,
                        auth_statuses,
                    }),
                    order: None,
                };

                if let Err(e) = tx_event.send(event).await {
                    warn!("failed to send McpListToolsResponse event: {e}");
                }
            }
            Op::SetMcpToolEnabled {
                server,
                tool,
                enable,
            } => {
                let sess = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                sess.mcp_connection_manager
                    .set_tool_enabled(&server, &tool, enable)
                    .await;

                let tools = sess
                    .mcp_connection_manager
                    .list_all_tools()
                    .into_iter()
                    .filter_map(|(name, tool)| {
                        let value = match serde_json::to_value(tool) {
                            Ok(value) => value,
                            Err(err) => {
                                warn!("failed to serialize MCP tool {name}: {err}");
                                return None;
                            }
                        };
                        match code_protocol::mcp::Tool::from_mcp_value(value) {
                            Ok(converted) => Some((name, converted)),
                            Err(err) => {
                                warn!("failed to convert MCP tool {name}: {err}");
                                None
                            }
                        }
                    })
                    .collect();
                let server_tools = sess.mcp_connection_manager.list_tools_by_server();
                let server_disabled_tools =
                    sess.mcp_connection_manager.list_disabled_tools_by_server();
                let server_failures = sess.mcp_connection_manager.list_server_failures();
                let resources =
                    convert_mcp_resources_by_server(sess.mcp_connection_manager.list_resources_by_server().await);
                let resource_templates = convert_mcp_resource_templates_by_server(
                    sess.mcp_connection_manager
                        .list_resource_templates_by_server()
                        .await,
                );
                let auth_statuses = sess.mcp_connection_manager.list_auth_statuses().await;

                let event = Event {
                    id: sub.id.clone(),
                    event_seq: 0,
                    msg: EventMsg::McpListToolsResponse(McpListToolsResponseEvent {
                        tools,
                        server_tools: Some(server_tools),
                        server_disabled_tools: Some(server_disabled_tools),
                        server_failures: Some(server_failures),
                        resources,
                        resource_templates,
                        auth_statuses,
                    }),
                    order: None,
                };

                if let Err(e) = tx_event.send(event).await {
                    warn!("failed to send McpListToolsResponse event: {e}");
                }
            }
            Op::ListCustomPrompts => {
                let sess = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                let custom_prompts: Vec<code_protocol::custom_prompts::CustomPrompt> =
                    if let Some(dir) = crate::custom_prompts::default_prompts_dir() {
                        crate::custom_prompts::discover_prompts_in(&dir).await
                    } else {
                        Vec::new()
                    };

                let event = Event {
                    id: sub.id.clone(),
                    event_seq: 0,
                    msg: EventMsg::ListCustomPromptsResponse(ListCustomPromptsResponseEvent {
                        custom_prompts,
                    }),
                    order: None,
                };

                sess.send_event(event).await;
            }
            Op::ListSkills => {
                let sess = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                let inventory = skills::load_skills_inventory_and_refresh_session(
                    &sess,
                    Arc::clone(&config),
                )
                .await;

                let skills: Vec<code_protocol::skills::Skill> = inventory
                    .skills
                    .iter()
                    .map(|skill| code_protocol::skills::Skill {
                        name: skill.name.clone(),
                        description: skill.description.clone(),
                        path: skill.path.clone(),
                        scope: match skill.scope {
                            crate::skills::model::SkillScope::Repo => {
                                code_protocol::skills::SkillScope::Repo
                            }
                            crate::skills::model::SkillScope::User => {
                                code_protocol::skills::SkillScope::User
                            }
                            crate::skills::model::SkillScope::System => {
                                code_protocol::skills::SkillScope::System
                            }
                            crate::skills::model::SkillScope::Admin => {
                                code_protocol::skills::SkillScope::System
                            }
                        },
                        content: skill.content.clone(),
                    })
                    .collect();

                let event = Event {
                    id: sub.id.clone(),
                    event_seq: 0,
                    msg: EventMsg::ListSkillsResponse(ListSkillsResponseEvent { skills }),
                    order: None,
                };

                sess.send_event(event).await;
            }
            Op::Compact => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                let prompt_text = sess.compact_prompt_text();
                // Attempt to inject input into current task
                if let Err(items) = sess.inject_input(vec![InputItem::Text {
                    text: prompt_text,
                }]) {
                    let turn_context = sess.make_turn_context();
                    compact::spawn_compact_task(sess.clone(), turn_context, sub.id.clone(), items);
                } else {
                    let was_empty = sess.enqueue_manual_compact(sub.id.clone());
                    let message = if was_empty {
                        "Manual compact queued; it will run after the current response finishes.".to_string()
                    } else {
                        "Manual compact already queued; waiting for the current response to finish.".to_string()
                    };
                    let event = sess.make_event(
                        &sub.id,
                        EventMsg::AgentMessage(AgentMessageEvent { message }),
                    );
                    sess.send_event(event).await;
                }
            }
            Op::Review { review_request } => {
                let sess = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                let config = Arc::clone(&config);
                let sub_id = sub.id.clone();
                super::agent::spawn_review_thread(sess, config, sub_id, review_request).await;
            }
            Op::SetNextTextFormat { format } => {
                let sess_arc = match sess.as_ref() {
                    Some(sess) => Arc::clone(sess),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                *sess_arc.next_turn_text_format.lock().unwrap() = Some(format);
            }
            Op::Shutdown => {
                info!("Shutting down Codex instance");

                // Ensure any running agent is aborted so streaming stops promptly.
                if let Some(sess_arc) = sess.as_ref() {
                    let s2 = sess_arc.clone();
                    tokio::spawn(async move {
                        s2.notify_wait_interrupted(WaitInterruptReason::SessionAborted);
                        s2.abort();
                    });
                }

                // Gracefully flush and shutdown rollout recorder on session end so tests
                // that inspect the rollout file do not race with the background writer.
                if let Some(ref sess_arc) = sess {
                    let recorder_opt = sess_arc.rollout.lock().unwrap().take();
                    if let Some(rec) = recorder_opt
                        && let Err(e) = rec.shutdown().await {
                            warn!("failed to shutdown rollout recorder: {e}");
                            let event = sess_arc.make_event(
                                &sub.id,
                                EventMsg::Error(ErrorEvent {
                                    message: "Failed to shutdown rollout recorder".to_string(),
                                }),
                            );
                            if let Err(e) = tx_event.send(event).await {
                                warn!("failed to send error message: {e:?}");
                            }
                        }
                }
                if let Some(ref sess_arc) = sess {
                    sess_arc.run_session_hooks(ProjectHookEvent::SessionEnd).await;
                }
                let event = match sess {
                    Some(ref sess_arc) => sess_arc.make_event(&sub.id, EventMsg::ShutdownComplete),
                    None => Event {
                        id: sub.id.clone(),
                        event_seq: 0,
                        msg: EventMsg::ShutdownComplete,
                        order: None,
                    },
                };
                if let Err(e) = tx_event.send(event).await {
                    warn!("failed to send Shutdown event: {e}");
                }
                break;
            }
                }
            }
            watcher_event = file_watcher_rx.recv(), if file_watcher_enabled => {
                match watcher_event {
                    Ok(crate::file_watcher::FileWatcherEvent::SkillsChanged { .. }) => {
                        let Some(sess_arc) = sess.as_ref() else {
                            continue;
                        };
                        let sess_arc = Arc::clone(sess_arc);
                        let config_snapshot = Arc::clone(&config);
                        tokio::spawn(async move {
                            let _ = skills::load_skills_inventory_and_refresh_session(
                                &sess_arc,
                                config_snapshot,
                            )
                            .await;
                        });
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        warn!("file watcher channel closed; disabling");
                        file_watcher_enabled = false;
                    }
                }
            }
        }
    }
    debug!("Agent loop exited");
}

