use super::*;
use code_core::protocol::OrderMeta;

impl ChatWidget<'_> {
    pub(super) fn handle_agent_message_event(
        &mut self,
        order: Option<&OrderMeta>,
        event_seq: u64,
        id: String,
        message: String,
    ) {
        // If the user requested an interrupt, ignore late final answers.
        if self.stream_state.drop_streaming {
            tracing::debug!("Ignoring AgentMessage after interrupt");
            self.stop_spinner();
            return;
        }

        // Allow a fresh lingering-exec sweep even if the per-turn guard
        // was tripped before any commands started.
        self.cleared_lingering_execs_this_turn = false;
        self.ensure_lingering_execs_cleared();

        self.stream_state.seq_answer_final = Some(event_seq);
        if !id.trim().is_empty() {
            self.note_answer_stream_seen(&id);
            // Any Answer item that completes before TaskComplete is considered
            // mid-turn until we later determine it was the final Answer.
            if !self.active_task_ids.is_empty() {
                self.mid_turn_answer_ids_in_turn.insert(id.clone());
            }
        }
        // Strict order for the stream id.
        let ok = match order {
            Some(om) => self.provider_order_key_from_order_meta(om),
            None => {
                tracing::warn!("missing OrderMeta on AgentMessage; using synthetic key");
                self.next_internal_key()
            }
        };
        self.seed_stream_order_key(StreamKind::Answer, &id, ok);

        tracing::debug!(
            "AgentMessage final id={} bytes={} preview={:?}",
            id,
            message.len(),
            message.chars().take(80).collect::<String>()
        );

        // Route final message through streaming controller so AppEvent::InsertFinalAnswer
        // is the single source of truth for assistant content.
        let sink = AppEventHistorySink(self.app_event_tx.clone());
        streaming::begin(self, StreamKind::Answer, Some(id.clone()));
        let _ = self.stream.apply_final_answer(&message, &sink);

        // Track last message for potential dedup heuristics.
        let cleaned = Self::strip_context_sections(&message);
        self.last_assistant_message = (!cleaned.trim().is_empty()).then_some(cleaned);
        // Mark this Answer stream id as closed for the rest of the turn so any late
        // AgentMessageDelta for the same id is ignored. In the full App runtime,
        // the InsertFinalAnswer path also marks closed; setting it here makes
        // unit tests (which do not route AppEvents back) behave identically.
        self.stream_state
            .closed_answer_ids
            .insert(StreamId(id.clone()));
        // Do not quiesce the global spinner here. `AgentMessage` is emitted for every
        // completed assistant output item, and modern models may send multiple assistant
        // messages mid-turn (progress updates, tool interleavings, etc.). We only clear
        // the turn spinner on `TaskComplete` or when all other activity drains.
        // Important: do not advance Auto Drive here. The StreamController will emit
        // AppEvent::InsertFinalAnswer, and the App thread will finalize the assistant
        // cell slightly later. Advancing at this point can start the next Auto Drive
        // step before the final answer is actually inserted, which appears as a
        // mid-turn re-trigger. We instead advance immediately after insertion inside
        // insert_final_answer_with_id().
    }

    pub(super) fn handle_agent_message_delta_event(
        &mut self,
        order: Option<&OrderMeta>,
        id: String,
        delta: String,
    ) {
        tracing::debug!("AgentMessageDelta: {:?}", delta);
        // If the user requested an interrupt, ignore late deltas.
        if self.stream_state.drop_streaming {
            tracing::debug!("Ignoring Answer delta after interrupt");
            self.stop_spinner();
            return;
        }

        self.ensure_lingering_execs_cleared();

        if self.strict_stream_ids_enabled() && id.trim().is_empty() {
            self.warn_missing_stream_id("assistant answer delta");
            return;
        }
        // Ignore late deltas for ids that have already finalized in this turn.
        if self
            .stream_state
            .closed_answer_ids
            .contains(&StreamId(id.clone()))
        {
            tracing::debug!("Ignoring Answer delta for closed id={}", id);
            return;
        }
        self.note_answer_stream_seen(&id);
        // Seed/refresh order key for this Answer stream id (must have OrderMeta).
        let ok = match order {
            Some(om) => self.provider_order_key_from_order_meta(om),
            None => {
                tracing::warn!("missing OrderMeta on AgentMessageDelta; using synthetic key");
                self.next_internal_key()
            }
        };
        self.seed_stream_order_key(StreamKind::Answer, &id, ok);
        let visible = self.parse_answer_stream_chunk(&id, &delta);
        // Stream answer delta through StreamController.
        streaming::delta_text(
            self,
            StreamKind::Answer,
            id.clone(),
            visible,
            order.and_then(|o| o.sequence_number),
        );
        self.ensure_spinner_for_activity("assistant-delta");
        // Show responding state while assistant streams.
        self.bottom_pane
            .update_status_text("responding".to_string());
    }

    pub(super) fn handle_agent_reasoning_event(
        &mut self,
        order: Option<&OrderMeta>,
        id: String,
        text: String,
    ) {
        // Ignore late reasoning if we've dropped streaming due to interrupt.
        if self.stream_state.drop_streaming {
            tracing::debug!("Ignoring AgentReasoning after interrupt");
            self.stop_spinner();
            return;
        }
        tracing::debug!(
            "AgentReasoning event with text: {:?}...",
            text.chars().take(100).collect::<String>()
        );
        // Guard duplicates for this id within the task.
        if self
            .stream_state
            .closed_reasoning_ids
            .contains(&StreamId(id.clone()))
        {
            tracing::warn!("Ignoring duplicate AgentReasoning for closed id={}", id);
            return;
        }
        // Seed strict order key for this Reasoning stream.
        let ok = match order {
            Some(om) => self.provider_order_key_from_order_meta(om),
            None => {
                tracing::warn!("missing OrderMeta on AgentReasoning; using synthetic key");
                self.next_internal_key()
            }
        };
        tracing::info!("[order] EventMsg::AgentReasoning id={} key={:?}", id, ok);
        self.seed_stream_order_key(StreamKind::Reasoning, &id, ok);
        // Use StreamController for final reasoning.
        let sink = AppEventHistorySink(self.app_event_tx.clone());
        streaming::begin(self, StreamKind::Reasoning, Some(id.clone()));
        // The StreamController now properly handles duplicate detection and prevents
        // re-injecting content when we're already finishing a stream.
        let _finished = self.stream.apply_final_reasoning(&text, &sink);
        // Stream finishing is handled by StreamController.
        // Mark this id closed for further reasoning deltas in this turn.
        self.stream_state
            .closed_reasoning_ids
            .insert(StreamId(id.clone()));
        self.clear_latest_reasoning_in_progress_flag();
        self.mark_needs_redraw();
    }

    pub(super) fn handle_agent_reasoning_delta_event(
        &mut self,
        order: Option<&OrderMeta>,
        id: String,
        delta: String,
    ) {
        tracing::debug!("AgentReasoningDelta: {:?}", delta);
        if self.stream_state.drop_streaming {
            tracing::debug!("Ignoring Reasoning delta after interrupt");
            self.stop_spinner();
            return;
        }
        if self.strict_stream_ids_enabled() && id.trim().is_empty() {
            self.warn_missing_stream_id("assistant reasoning delta");
            return;
        }
        // Ignore late deltas for ids that have already finalized in this turn.
        if self
            .stream_state
            .closed_reasoning_ids
            .contains(&StreamId(id.clone()))
        {
            tracing::debug!("Ignoring Reasoning delta for closed id={}", id);
            return;
        }
        // Seed strict order key for this Reasoning stream.
        let ok = match order {
            Some(om) => self.provider_order_key_from_order_meta(om),
            None => {
                tracing::warn!("missing OrderMeta on AgentReasoningDelta; using synthetic key");
                self.next_internal_key()
            }
        };
        tracing::info!("[order] EventMsg::AgentReasoningDelta id={} key={:?}", id, ok);
        self.seed_stream_order_key(StreamKind::Reasoning, &id, ok);
        streaming::delta_text(
            self,
            StreamKind::Reasoning,
            id.clone(),
            delta,
            order.and_then(|o| o.sequence_number),
        );
        self.ensure_spinner_for_activity("reasoning-delta");
        // Show thinking state while reasoning streams.
        self.bottom_pane.update_status_text("thinking".to_string());
    }

    pub(super) fn handle_agent_reasoning_section_break_event(&mut self) {
        // Insert section break in reasoning stream.
        let sink = AppEventHistorySink(self.app_event_tx.clone());
        self.stream.insert_reasoning_section_break(&sink);
    }

    pub(super) fn handle_agent_reasoning_raw_content_delta_event(
        &mut self,
        order: Option<&OrderMeta>,
        id: String,
        delta: String,
    ) {
        if self.stream_state.drop_streaming {
            tracing::debug!("Ignoring RawContent delta after interrupt");
            self.stop_spinner();
            return;
        }
        if self.strict_stream_ids_enabled() && id.trim().is_empty() {
            self.warn_missing_stream_id("assistant raw reasoning delta");
            return;
        }
        // Treat raw reasoning content the same as summarized reasoning.
        if self
            .stream_state
            .closed_reasoning_ids
            .contains(&StreamId(id.clone()))
        {
            tracing::debug!("Ignoring RawContent delta for closed id={}", id);
            return;
        }
        // Seed strict order key for this reasoning stream id.
        let ok = match order {
            Some(om) => self.provider_order_key_from_order_meta(om),
            None => {
                tracing::warn!("missing OrderMeta on Tools::PlanUpdate; using synthetic key");
                self.next_internal_key()
            }
        };
        self.seed_stream_order_key(StreamKind::Reasoning, &id, ok);

        streaming::delta_text(
            self,
            StreamKind::Reasoning,
            id.clone(),
            delta,
            order.and_then(|o| o.sequence_number),
        );
    }

    pub(super) fn handle_agent_reasoning_raw_content_event(
        &mut self,
        order: Option<&OrderMeta>,
        id: String,
        text: String,
    ) {
        if self.stream_state.drop_streaming {
            tracing::debug!("Ignoring AgentReasoningRawContent after interrupt");
            self.stop_spinner();
            return;
        }
        tracing::debug!(
            "AgentReasoningRawContent event with text: {:?}...",
            text.chars().take(100).collect::<String>()
        );
        if self
            .stream_state
            .closed_reasoning_ids
            .contains(&StreamId(id.clone()))
        {
            tracing::warn!(
                "Ignoring duplicate AgentReasoningRawContent for closed id={}",
                id
            );
            return;
        }
        // Seed strict order key now so upcoming insert uses the correct key.
        let ok = match order {
            Some(om) => self.provider_order_key_from_order_meta(om),
            None => {
                tracing::warn!("missing OrderMeta on Tools::ReasoningBegin; using synthetic key");
                self.next_internal_key()
            }
        };
        self.seed_stream_order_key(StreamKind::Reasoning, &id, ok);
        // Use StreamController for final raw reasoning.
        let sink = AppEventHistorySink(self.app_event_tx.clone());
        streaming::begin(self, StreamKind::Reasoning, Some(id.clone()));
        let _finished = self.stream.apply_final_reasoning(&text, &sink);
        // Stream finishing is handled by StreamController.
        self.stream_state
            .closed_reasoning_ids
            .insert(StreamId(id.clone()));
        self.clear_latest_reasoning_in_progress_flag();
        self.mark_needs_redraw();
    }

    fn clear_latest_reasoning_in_progress_flag(&mut self) {
        if let Some(last) = self.history_cells.iter().rposition(|c| {
            c.as_any()
                .downcast_ref::<history_cell::CollapsibleReasoningCell>()
                .is_some()
        })
            && let Some(reason) = self.history_cells[last]
                .as_any()
                .downcast_ref::<history_cell::CollapsibleReasoningCell>()
        {
            reason.set_in_progress(false);
        }
    }
}
