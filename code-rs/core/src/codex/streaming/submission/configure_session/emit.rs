use super::*;

impl Runner<'_> {
    pub(super) async fn emit(&mut self, built: Built) {
        let Built {
            submission_id,
            model,
            mcp_connection_errors,
            restored_items,
            restored_history_snapshot,
            replay_history_items,
            resume_notice,
        } = built;

        let config = Arc::clone(&self.config);

        // Gather history metadata for SessionConfiguredEvent.
        let (history_log_id, history_entry_count) = crate::message_history::history_metadata(&config).await;

        // ack
        let Some(sess_arc) = self.sess.as_ref() else {
            self.send_no_session_event(&submission_id).await;
            return;
        };

        let events = std::iter::once(sess_arc.make_event(
            INITIAL_SUBMIT_ID,
            EventMsg::SessionConfigured(SessionConfiguredEvent {
                session_id: self.session_id,
                model,
                history_log_id,
                history_entry_count,
            }),
        ))
        .chain(mcp_connection_errors.into_iter().map(|message| {
            sess_arc.make_event(&submission_id, EventMsg::Error(ErrorEvent { message }))
        }));

        for event in events {
            if let Err(e) = self.tx_event.send(event).await {
                error!("failed to send event: {e:?}");
            }
        }

        // If we resumed from a rollout, replay the prior transcript into the UI.
        if replay_history_items.is_some()
            || restored_history_snapshot.is_some()
            || restored_items.is_some()
        {
            let items = replay_history_items.unwrap_or_default();
            let history_snapshot_value = restored_history_snapshot
                .as_ref()
                .and_then(|snapshot| serde_json::to_value(snapshot).ok());
            let event = sess_arc.make_event(
                &submission_id,
                EventMsg::ReplayHistory(crate::protocol::ReplayHistoryEvent {
                    items,
                    history_snapshot: history_snapshot_value,
                }),
            );
            if let Err(e) = self.tx_event.send(event).await {
                warn!("failed to send ReplayHistory event: {e}");
            }
        }

        if let Some(notice) = resume_notice {
            let event = sess_arc.make_event(
                &submission_id,
                EventMsg::BackgroundEvent(BackgroundEventEvent { message: notice }),
            );
            if let Err(e) = self.tx_event.send(event).await {
                warn!("failed to send resume notice event: {e}");
            }
        }

        spawn_bridge_listener(Arc::clone(sess_arc));
        sess_arc.run_session_hooks(ProjectHookEvent::SessionStart).await;

        // Initialize agent manager after SessionConfigured is sent
        if self.agent_manager_initialized {
            return;
        }

        let mut manager = AGENT_MANAGER.write().await;
        let (agent_tx, mut agent_rx) =
            tokio::sync::mpsc::unbounded_channel::<AgentStatusUpdatePayload>();
        manager.set_event_sender(agent_tx);
        drop(manager);

        let Some(sess_for_agents) = self.sess.as_ref().cloned() else {
            self.send_no_session_event(&submission_id).await;
            return;
        };

        // Forward agent events to the main event channel
        let tx_event_clone = self.tx_event.clone();
        tokio::spawn(async move {
            while let Some(payload) = agent_rx.recv().await {
                let wake_messages = {
                    let mut state = sess_for_agents.state.lock().unwrap();
                    agent_completion_wake_messages(&payload, &mut state.agent_completion_wake_batches)
                };
                if !wake_messages.is_empty() {
                    enqueue_agent_completion_wake(&sess_for_agents, wake_messages).await;
                }
                let status_event = sess_for_agents.make_event(
                    "agent_status",
                    EventMsg::AgentStatusUpdate(AgentStatusUpdateEvent {
                        agents: payload.agents.clone(),
                        context: payload.context.clone(),
                        task: payload.task.clone(),
                    }),
                );
                let _ = tx_event_clone.send(status_event).await;
            }
        });

        self.agent_manager_initialized = true;
    }
}

