impl ChatWidget<'_> {
    pub(crate) fn insert_history_lines(&mut self, lines: Vec<ratatui::text::Line<'static>>) {
        let kind = self.stream_state.current_kind.unwrap_or(StreamKind::Answer);
        self.insert_history_lines_with_kind(kind, None, lines);
    }

    pub(crate) fn insert_history_lines_with_kind(
        &mut self,
        kind: StreamKind,
        id: Option<String>,
        lines: Vec<ratatui::text::Line<'static>>,
    ) {
        // No debug logging: we rely on preserving span modifiers end-to-end.
        // Insert all lines as a single streaming content cell to preserve spacing
        if lines.is_empty() {
            return;
        }

        if let Some(first_line) = lines.first() {
            let first_line_text: String = first_line
                .spans
                .iter()
                .map(|s| s.content.to_string())
                .collect();
            tracing::debug!("First line content: {:?}", first_line_text);
        }

        match kind {
            StreamKind::Reasoning => {
                // This reasoning block is the bottom-most; show progress indicator here only
                self.clear_reasoning_in_progress();
                // Ensure footer shows Ctrl+R hint when reasoning content is present
                self.bottom_pane.set_reasoning_hint(true);
                // Update footer label to reflect current visibility state
                self.bottom_pane
                    .set_reasoning_state(self.is_reasoning_shown());
                // Route by id when provided to avoid splitting reasoning across cells.
                // Be defensive: the cached index may be stale after inserts/removals; validate it.
                if let Some(ref rid) = id
                    && let Some(&idx) = self.reasoning_index.get(rid) {
                        if idx < self.history_cells.len()
                            && let Some(reasoning_cell) = self.history_cells[idx]
                                .as_any_mut()
                                .downcast_mut::<history_cell::CollapsibleReasoningCell>(
                            ) {
                                tracing::debug!(
                                    "Appending {} lines to Reasoning(id={})",
                                    lines.len(),
                                    rid
                                );
                                reasoning_cell.append_lines_dedup(lines);
                                reasoning_cell.set_in_progress(true);
                                self.invalidate_height_cache();
                                self.autoscroll_if_near_bottom();
                                self.request_redraw();
                                self.refresh_reasoning_collapsed_visibility();
                                return;
                            }
                        // Cached index was stale or wrong type — try to locate by scanning.
                        if let Some(found_idx) = self.history_cells.iter().rposition(|c| {
                            c.as_any()
                                .downcast_ref::<history_cell::CollapsibleReasoningCell>()
                                .map(|rc| rc.matches_id(rid))
                                .unwrap_or(false)
                        }) {
                            if let Some(reasoning_cell) = self.history_cells[found_idx]
                                .as_any_mut()
                                .downcast_mut::<history_cell::CollapsibleReasoningCell>()
                            {
                                // Refresh the cache with the corrected index
                                self.reasoning_index.insert(rid.clone(), found_idx);
                                tracing::debug!(
                                    "Recovered stale reasoning index; appending at {} for id={}",
                                    found_idx,
                                    rid
                                );
                                reasoning_cell.append_lines_dedup(lines);
                                reasoning_cell.set_in_progress(true);
                                self.invalidate_height_cache();
                                self.autoscroll_if_near_bottom();
                                self.request_redraw();
                                self.refresh_reasoning_collapsed_visibility();
                                return;
                            }
                        } else {
                            // No matching cell remains; drop the stale cache entry.
                            self.reasoning_index.remove(rid);
                        }
                    }

                tracing::debug!("Creating new CollapsibleReasoningCell id={:?}", id);
                let cell = history_cell::CollapsibleReasoningCell::new_with_id(lines, id.clone());
                if self.config.tui.show_reasoning {
                    cell.set_collapsed(false);
                } else {
                    cell.set_collapsed(true);
                }
                cell.set_in_progress(true);

                // Use pre-seeded key for this stream id when present; otherwise synthesize.
                let key = match id.as_deref() {
                    Some(rid) => self.try_stream_order_key(kind, rid).unwrap_or_else(|| {
                        tracing::warn!(
                            "missing stream order key for Reasoning id={}; using synthetic key",
                            rid
                        );
                        self.next_internal_key()
                    }),
                    None => {
                        tracing::warn!("missing stream id for Reasoning; using synthetic key");
                        self.next_internal_key()
                    }
                };
                tracing::info!(
                    "[order] insert Reasoning new id={:?} {}",
                    id,
                    Self::debug_fmt_order_key(key)
                );
                let idx = self.history_insert_with_key_global(Box::new(cell), key);
                if let Some(rid) = id {
                    self.reasoning_index.insert(rid, idx);
                }
                // Auto Drive status updates are handled via coordinator decisions.
            }
            StreamKind::Answer => {
                tracing::debug!(
                    "history.insert Answer id={:?} incoming_lines={}",
                    id,
                    lines.len()
                );
                self.clear_reasoning_in_progress();

                let explicit_id = id.clone();
                let stream_identifier = explicit_id.clone().unwrap_or_else(|| {
                    self.stream
                        .current_stream_id()
                        .map(std::string::ToString::to_string)
                        .unwrap_or_else(|| "stream-preview".to_string())
                });

                let fallback_preview = self
                    .synthesize_stream_state_from_lines(Some(stream_identifier.as_str()), &lines, true)
                    .preview_markdown;
                let preview_markdown = self
                    .stream
                    .preview_source_for_kind(StreamKind::Answer)
                    .unwrap_or(fallback_preview);

                let mutation = self.history_state.apply_domain_event(
                    HistoryDomainEvent::UpsertAssistantStream {
                        stream_id: stream_identifier,
                        preview_markdown,
                        delta: None,
                        metadata: None,
                    },
                );

                match mutation {
                    HistoryMutation::Inserted { id: history_id, record, .. } => {
                        let insert_key = match explicit_id.as_deref() {
                            Some(rid) => self.try_stream_order_key(kind, rid).unwrap_or_else(|| {
                                tracing::warn!(
                                    "missing stream order key for Answer id={}; using synthetic key",
                                    rid
                                );
                                self.next_internal_key()
                            }),
                            None => {
                                tracing::warn!(
                                    "missing stream id for Answer; using synthetic key"
                                );
                                self.next_internal_key()
                            }
                        };

                        if let Some(mut cell) = self.build_cell_from_record(&record) {
                            self.assign_history_id(&mut cell, history_id);
                            let new_idx = self.history_insert_existing_record(
                                cell,
                                insert_key,
                                "stream-begin",
                                history_id,
                            );
                            tracing::debug!(
                                "history.new StreamingContentCell at idx={} id={:?}",
                                new_idx,
                                explicit_id
                            );
                        } else {
                            tracing::warn!("assistant stream record could not build cell");
                        }
                    }
                    HistoryMutation::Replaced { id: history_id, record, .. } => {
                        if self.cell_index_for_history_id(history_id).is_some() {
                            self.update_cell_from_record(history_id, record);
                        } else if let Some(mut cell) = self.build_cell_from_record(&record) {
                            // The stream state may have been inserted by a delta
                            // before the stream controller emitted any history
                            // lines. Insert the cell now using the same ordering
                            // key as the normal Inserted path.
                            let insert_key = match explicit_id.as_deref() {
                                Some(rid) => self.try_stream_order_key(kind, rid).unwrap_or_else(|| {
                                    tracing::warn!(
                                        "missing stream order key for Answer id={}; using synthetic key",
                                        rid
                                    );
                                    self.next_internal_key()
                                }),
                                None => {
                                    tracing::warn!(
                                        "missing stream id for Answer; using synthetic key"
                                    );
                                    self.next_internal_key()
                                }
                            };
                            self.assign_history_id(&mut cell, history_id);
                            let new_idx = self.history_insert_existing_record(
                                cell,
                                insert_key,
                                "stream-begin",
                                history_id,
                            );
                            tracing::debug!(
                                "history.new StreamingContentCell at idx={} id={:?}",
                                new_idx,
                                explicit_id
                            );
                        } else {
                            tracing::warn!("assistant stream record could not build cell");
                        }
                        self.mark_history_dirty();
                    }
                    HistoryMutation::Noop => {}
                    other => tracing::debug!(
                        "unexpected streaming mutation {:?} for id={:?}",
                        other,
                        explicit_id
                    ),
                }
            }
        }

        // Auto-follow if near bottom so new inserts are visible
        self.autoscroll_if_near_bottom();
        self.request_redraw();
        self.flush_history_snapshot_if_needed(false);
    }

    fn synthesize_stream_state_from_lines(
        &self,
        stream_id: Option<&str>,
        lines: &[ratatui::text::Line<'static>],
        in_progress: bool,
    ) -> AssistantStreamState {
        let mut preview = String::new();
        for (idx, line) in lines.iter().enumerate() {
            let flat: String = line
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect();
            if idx == 0 && flat.trim().eq_ignore_ascii_case("codex") {
                continue;
            }
            if !preview.is_empty() {
                preview.push('\n');
            }
            preview.push_str(&flat);
        }
        if !preview.is_empty() && !preview.ends_with('\n') {
            preview.push('\n');
        }
        let mut stream_id_string = stream_id
            .map(str::to_owned)
            .unwrap_or_else(|| "stream-preview".to_string());
        if stream_id_string.is_empty() {
            stream_id_string = "stream-preview".to_string();
        }
        AssistantStreamState {
            id: HistoryId::ZERO,
            stream_id: stream_id_string,
            preview_markdown: preview,
            deltas: Vec::new(),
            citations: Vec::new(),
            metadata: None,
            in_progress,
            last_updated_at: SystemTime::now(),
            truncated_prefix_bytes: 0,
        }
    }

    fn refresh_streaming_cell_for_stream_id(
        &mut self,
        stream_id: &str,
        state: AssistantStreamState,
    ) {
        if state.id != HistoryId::ZERO {
            // Streaming deltas may update history_state before the streaming
            // sink has inserted the corresponding cell. Only attempt a cell
            // update when the cell is present to avoid warning noise.
            if self.cell_index_for_history_id(state.id).is_some() {
                self.update_cell_from_record(
                    state.id,
                    HistoryRecord::AssistantStream(state),
                );
                self.autoscroll_if_near_bottom();
            }
            return;
        }

        if let Some(existing) = self
            .history_state
            .assistant_stream_state(stream_id)
            .cloned()
            && existing.id != HistoryId::ZERO {
                if self.cell_index_for_history_id(existing.id).is_some() {
                    self.update_cell_from_record(
                        existing.id,
                        HistoryRecord::AssistantStream(existing),
                    );
                    self.autoscroll_if_near_bottom();
                }
        }
    }

    fn answer_stream_metadata(
        &self,
        stream_id: &str,
        token_usage_override: Option<code_core::protocol::TokenUsage>,
    ) -> Option<MessageMetadata> {
        let existing_metadata = self
            .history_state
            .assistant_stream_state(stream_id)
            .and_then(|state| state.metadata.clone());

        let mut citations = existing_metadata
            .as_ref()
            .map(|meta| meta.citations.clone())
            .unwrap_or_default();
        if let Some(state) = self.stream_state.answer_markup.get(stream_id) {
            Self::merge_citations_dedup_case_sensitive(&mut citations, state.citations.clone());
        }

        let token_usage = token_usage_override
            .or_else(|| existing_metadata.and_then(|meta| meta.token_usage));

        if citations.is_empty() && token_usage.is_none() {
            None
        } else {
            Some(MessageMetadata {
                citations,
                token_usage,
            })
        }
    }

    fn parse_answer_stream_chunk(&mut self, stream_id: &str, chunk: &str) -> String {
        let plan_mode = self.collaboration_mode == code_core::protocol::CollaborationModeKind::Plan;
        let state = self
            .stream_state
            .answer_markup
            .entry(stream_id.to_string())
            .or_insert_with(|| internals::state::AnswerMarkupState {
                parser: stream_parser::AssistantTextStreamParser::new(plan_mode),
                citations: Vec::new(),
                plan_markdown: String::new(),
            });

        let stream_parser::AssistantTextChunk {
            visible_text,
            citations,
            plan_segments,
        } = state.parser.push_str(chunk);
        Self::merge_citations_dedup_case_sensitive(&mut state.citations, citations);
        Self::apply_proposed_plan_segments(state, plan_segments);
        visible_text
    }

    fn take_answer_stream_markup(
        &mut self,
        stream_id: Option<&str>,
    ) -> (Vec<String>, Option<String>) {
        let key = if let Some(stream_id) = stream_id {
            Some(stream_id.to_string())
        } else if self.stream_state.answer_markup.len() == 1 {
            self.stream_state.answer_markup.keys().next().cloned()
        } else {
            None
        };

        let Some(key) = key else {
            return (Vec::new(), None);
        };

        let Some(mut state) = self.stream_state.answer_markup.remove(&key) else {
            return (Vec::new(), None);
        };

        let stream_parser::AssistantTextChunk {
            citations,
            plan_segments,
            ..
        } = state.parser.finish();
        Self::merge_citations_dedup_case_sensitive(&mut state.citations, citations);
        Self::apply_proposed_plan_segments(&mut state, plan_segments);

        let plan = (!state.plan_markdown.trim().is_empty()).then_some(state.plan_markdown);
        (state.citations, plan)
    }

    fn clear_answer_stream_markup_tracking(&mut self) {
        self.stream_state.answer_markup.clear();
    }

    fn merge_citations_dedup_case_sensitive(existing: &mut Vec<String>, incoming: Vec<String>) {
        for citation in incoming {
            if !existing.iter().any(|current| current == &citation) {
                existing.push(citation);
            }
        }
    }

    fn apply_proposed_plan_segments(
        state: &mut internals::state::AnswerMarkupState,
        segments: Vec<stream_parser::ProposedPlanSegment>,
    ) {
        for segment in segments {
            match segment {
                stream_parser::ProposedPlanSegment::ProposedPlanStart => {
                    state.plan_markdown.clear();
                }
                stream_parser::ProposedPlanSegment::ProposedPlanDelta(delta) => {
                    state.plan_markdown.push_str(&delta);
                }
                stream_parser::ProposedPlanSegment::Normal(_)
                | stream_parser::ProposedPlanSegment::ProposedPlanEnd => {}
            }
        }
    }

    fn update_stream_token_usage_metadata(&mut self) {
        let Some(stream_id) = self.stream.current_stream_id().map(str::to_owned) else {
            return;
        };
        let Some(preview) = self
            .stream
            .preview_source_for_kind(StreamKind::Answer)
        else {
            return;
        };
        let metadata = self
            .answer_stream_metadata(&stream_id, Some(self.last_token_usage.clone()));
        self
            .history_state
            .upsert_assistant_stream_state(&stream_id, preview, None, metadata.as_ref());
        if let Some(state) = self
            .history_state
            .assistant_stream_state(&stream_id)
            .cloned()
        {
            self.refresh_streaming_cell_for_stream_id(&stream_id, state);
        }
    }

    fn track_answer_stream_delta(&mut self, stream_id: &str, delta: &str, seq: Option<u64>) {
        let preview = self
            .stream
            .preview_source_for_kind(StreamKind::Answer)
            .unwrap_or_default();
        let delta = if delta.is_empty() {
            None
        } else {
            Some(AssistantStreamDelta {
                delta: delta.to_string(),
                sequence: seq,
                received_at: SystemTime::now(),
            })
        };
        let metadata = self.answer_stream_metadata(stream_id, None);
        let mutation = self.history_state.apply_domain_event(
            HistoryDomainEvent::UpsertAssistantStream {
                stream_id: stream_id.to_string(),
                preview_markdown: preview,
                delta,
                metadata,
            },
        );

        match mutation {
            HistoryMutation::Inserted {
                record: HistoryRecord::AssistantStream(state),
                ..
            } => {
                // Inserting an assistant stream record can happen before the
                // UI cell has been inserted via the streaming sink. Only update
                // an existing cell to avoid history-state mismatch warnings.
                if self.cell_index_for_history_id(state.id).is_some() {
                    self.update_cell_from_record(
                        state.id,
                        HistoryRecord::AssistantStream(state),
                    );
                }
                self.mark_history_dirty();
            }
            HistoryMutation::Replaced { id, record, .. } => {
                if matches!(record, HistoryRecord::AssistantStream(_))
                    && self.cell_index_for_history_id(id).is_some()
                {
                    self.update_cell_from_record(id, record);
                    self.mark_history_dirty();
                }
            }
            _ => {}
        }
    }

    fn note_answer_stream_seen(&mut self, new_stream_id: &str) {
        let prev = self.last_seen_answer_stream_id_in_turn.clone();
        if let Some(prev) = prev
            && prev != new_stream_id {
                self.mid_turn_answer_ids_in_turn.insert(prev.clone());
                self.maybe_mark_finalized_answer_mid_turn(&prev);
            }
        self.last_seen_answer_stream_id_in_turn = Some(new_stream_id.to_string());
    }

    fn maybe_mark_finalized_answer_mid_turn(&mut self, prev_stream_id: &str) {
        let Some(last_final_id) = self.last_answer_stream_id_in_turn.as_deref() else {
            return;
        };
        if last_final_id != prev_stream_id {
            return;
        }
        let Some(prev_history_id) = self.last_answer_history_id_in_turn else {
            return;
        };

        let mut changed = false;
        if let Some(idx) = self
            .history_cell_ids
            .iter()
            .rposition(|hid| *hid == Some(prev_history_id))
            && let Some(cell) = self.history_cells[idx]
                .as_any_mut()
                .downcast_mut::<history_cell::AssistantMarkdownCell>()
                && !cell.state().mid_turn {
                    cell.set_mid_turn(true);
                    changed = true;
                }

        if let Some(record) = self.history_state.record_mut(prev_history_id)
            && let HistoryRecord::AssistantMessage(state) = record
                && !state.mid_turn {
                    state.mid_turn = true;
                    changed = true;
                }

        if changed {
            self.mark_history_dirty();
            self.request_redraw();
        }
    }

    fn apply_mid_turn_flag(&self, stream_id: Option<&str>, state: &mut AssistantMessageState) {
        if let Some(sid) = stream_id
            && self.mid_turn_answer_ids_in_turn.contains(sid) {
                state.mid_turn = true;
            }
    }

    fn maybe_clear_mid_turn_for_last_answer(&mut self, stream_id: &str) {
        let Some(last_history_id) = self.last_answer_history_id_in_turn else {
            return;
        };

        let mut changed = false;

        if let Some(record) = self.history_state.record_mut(last_history_id)
            && let HistoryRecord::AssistantMessage(state) = record
                && state.stream_id.as_deref() == Some(stream_id) && state.mid_turn {
                    state.mid_turn = false;
                    changed = true;
                }

        if let Some(idx) = self
            .history_cell_ids
            .iter()
            .rposition(|hid| *hid == Some(last_history_id))
            && let Some(cell) = self.history_cells[idx]
                .as_any_mut()
                .downcast_mut::<history_cell::AssistantMarkdownCell>()
                && cell.stream_id() == Some(stream_id) && cell.state().mid_turn {
                    cell.set_mid_turn(false);
                    changed = true;
                }

        if changed {
            self.mark_history_dirty();
            self.request_redraw();
        }
    }

    fn finalize_answer_stream_state(
        &mut self,
        stream_id: Option<&str>,
        source: &str,
        citations: Vec<String>,
    ) -> AssistantMessageState {
        let mut metadata = stream_id.and_then(|sid| {
            self.history_state
                .assistant_stream_state(sid)
                .and_then(|state| state.metadata.clone())
        });

        if !citations.is_empty() {
            if let Some(meta) = metadata.as_mut() {
                meta.citations = citations.clone();
            } else {
                metadata = Some(MessageMetadata {
                    citations: citations.clone(),
                    token_usage: None,
                });
            }
        }

        let should_attach_token_usage = self.last_token_usage.total_tokens > 0;
        if should_attach_token_usage {
            if let Some(meta) = metadata.as_mut() {
                if meta.token_usage.is_none() {
                    meta.token_usage = Some(self.last_token_usage.clone());
                }
            } else {
                metadata = Some(MessageMetadata {
                    citations,
                    token_usage: Some(self.last_token_usage.clone()),
                });
            }
        }

        let token_usage = if should_attach_token_usage {
            Some(self.last_token_usage.clone())
        } else {
            None
        };

        
        self.history_state.finalize_assistant_stream_state(
            stream_id,
            source.to_string(),
            metadata.as_ref(),
            token_usage.as_ref(),
        )
    }

    fn strip_hidden_assistant_markup(
        &self,
        text: &str,
    ) -> (String, Vec<String>, Option<String>) {
        let plan_mode = self.collaboration_mode == code_core::protocol::CollaborationModeKind::Plan;
        let (without_citations, citations) = stream_parser::strip_citations(text);
        if !plan_mode {
            return (without_citations, citations, None);
        }

        let plan_text = stream_parser::extract_proposed_plan_text(&without_citations)
            .filter(|plan| !plan.trim().is_empty());
        let cleaned = stream_parser::strip_proposed_plan_blocks(&without_citations);
        (cleaned, citations, plan_text)
    }

    fn maybe_insert_proposed_plan(
        &mut self,
        plan_markdown: Option<String>,
        after_key: OrderKey,
    ) {
        let Some(plan_markdown) = plan_markdown else {
            return;
        };
        if plan_markdown.trim().is_empty() {
            return;
        }

        let already_present = self.history_cells.iter().rev().take(8).any(|cell| {
            cell.as_any()
                .downcast_ref::<history_cell::ProposedPlanCell>()
                .is_some_and(|existing| existing.markdown().trim() == plan_markdown.trim())
        });
        if already_present {
            return;
        }

        let mut state = code_core::history::ProposedPlanState {
            id: HistoryId::ZERO,
            markdown: plan_markdown,
            created_at: std::time::SystemTime::now(),
        };
        let plan_id = self
            .history_state
            .push(code_core::history::HistoryRecord::ProposedPlan(state.clone()));
        state.id = plan_id;
        let cell = history_cell::ProposedPlanCell::from_state(state, &self.config);
        let key = Self::order_key_successor(after_key);
        self.history_insert_existing_record(Box::new(cell), key, "proposed-plan", plan_id);
    }

    /// Replace the in-progress streaming assistant cell with a final markdown cell that
    /// stores raw markdown for future re-rendering.
    pub(crate) fn insert_final_answer_with_id(
        &mut self,
        id: Option<String>,
        lines: Vec<ratatui::text::Line<'static>>,
        source: String,
    ) {
        tracing::debug!(
            "insert_final_answer_with_id id={:?} source_len={} lines={}",
            id,
            source.len(),
            lines.len()
        );
        tracing::info!("[order] final Answer id={:?}", id);
        let raw_source = source;
        let (final_source, citations, proposed_plan) =
            self.strip_hidden_assistant_markup(&raw_source);
        let mut citations = citations;
        let mut proposed_plan = proposed_plan;
        let (pending_citations, pending_plan) = self.take_answer_stream_markup(id.as_deref());
        Self::merge_citations_dedup_case_sensitive(&mut citations, pending_citations);
        if proposed_plan.is_none() {
            proposed_plan = pending_plan;
        }

        if self.auto_state.pending_stop_message.is_some() {
            match serde_json::from_str::<code_auto_drive_diagnostics::CompletionCheck>(&raw_source)
            {
                Ok(check) => {
                    if check.complete {
                        let explanation = check.explanation.trim();
                        if explanation.is_empty() {
                            self.auto_state.last_completion_explanation = None;
                        } else {
                            self.auto_state.last_completion_explanation =
                                Some(explanation.to_string());
                        }
                        let pending = self.auto_state.pending_stop_message.take();
                        if let Some(idx) = self.history_cells.iter().rposition(|c| {
                            c.as_any()
                                .downcast_ref::<history_cell::StreamingContentCell>()
                                .and_then(|sc| sc.id.as_ref())
                                .map(|existing| Some(existing.as_str()) == id.as_deref())
                                .unwrap_or(false)
                        }) {
                            self.history_remove_at(idx);
                        }
                        if let Some(ref stream_id) = id {
                            let _ = self.history_state.finalize_assistant_stream_state(
                                Some(stream_id.as_str()),
                                String::new(),
                                None,
                                None,
                            );
                            self.stream_state
                                .closed_answer_ids
                                .insert(StreamId(stream_id.clone()));
                        }
                        self.auto_stop(pending);
                        self.stop_spinner();
                        return;
                    } else {
                        self.auto_state.last_completion_explanation = None;
                        let goal = self
                            .auto_state
                            .goal
                            .as_deref()
                            .unwrap_or("(goal unavailable)");
                    let follow_up = format!(
                        "The primary goal has not been met. Please continue working on this.\nPrimary Goal: {goal}\nExplanation: {explanation}",
                        explanation = check.explanation
                    );
                    let mut conversation = self.rebuild_auto_history();
                    if let Some(user_item) = Self::auto_drive_make_user_message(follow_up) {
                        conversation.push(user_item.clone());
                        self.auto_history.append_raw(std::slice::from_ref(&user_item));
                    }
                    self.auto_state.pending_stop_message = None;
                    // Re-run the conversation through the normal decision pipeline so the
                    // coordinator produces a full finish_status/progress/cli turn rather than
                    // falling back to the user-response schema.
                    self.auto_state.set_phase(AutoRunPhase::Active);
                    self.auto_send_conversation_force();
                    self.stop_spinner();
                    return;
                }
                }
                Err(err) => {
                    tracing::warn!(
                        "failed to parse diagnostics completion check: {}",
                        err
                    );
                    self.auto_state.last_completion_explanation = None;
                    let pending = self.auto_state.pending_stop_message.take();
                    self.auto_stop(pending);
                }
            }
        }

        self.last_assistant_message = Some(final_source.clone());

        if self.is_review_flow_active() {
            if let Some(ref want) = id {
                if !self
                    .stream_state
                    .closed_answer_ids
                    .insert(StreamId(want.clone()))
                {
                    tracing::debug!(
                        "InsertFinalAnswer(review): dropping duplicate final for id={}",
                        want
                    );
                    self.maybe_hide_spinner();
                    return;
                }
                if let Some(idx) = self.history_cells.iter().rposition(|c| {
                    c.as_any()
                        .downcast_ref::<history_cell::StreamingContentCell>()
                        .and_then(|sc| sc.id.as_ref())
                        .map(|existing| existing == want)
                        .unwrap_or(false)
                }) {
                    self.history_remove_at(idx);
                }
            } else if let Some(idx) = self.history_cells.iter().rposition(|c| {
                c.as_any()
                    .downcast_ref::<history_cell::StreamingContentCell>()
                    .is_some()
            }) {
                self.history_remove_at(idx);
            }
            let mut state = self.finalize_answer_stream_state(
                id.as_deref(),
                &final_source,
                std::mem::take(&mut citations),
            );
            self.apply_mid_turn_flag(id.as_deref(), &mut state);
            let history_id = state.id;
            let mut key = match id.as_deref() {
                Some(rid) => self.try_stream_order_key(StreamKind::Answer, rid).unwrap_or_else(|| {
                    tracing::warn!(
                        "missing stream order key for final Answer id={}; using synthetic key",
                        rid
                    );
                    self.next_internal_key()
                }),
                None => {
                    tracing::warn!("missing stream id for final Answer; using synthetic key");
                    self.next_internal_key()
                }
            };

            if let Some(last) = self.last_assigned_order
                && key <= last {
                    key = Self::order_key_successor(last);
                    if let Some(ref want) = id {
                        self.stream_order_seq
                            .insert((StreamKind::Answer, want.clone()), key);
                    }
                }

            let cell = history_cell::AssistantMarkdownCell::from_state(state, &self.config);
            self.history_insert_existing_record(Box::new(cell), key, "answer-review", history_id);
            self.last_answer_stream_id_in_turn = id.clone();
            self.last_answer_history_id_in_turn = Some(history_id);
            // Advance Auto Drive after the assistant message has been finalized.
            self.auto_on_assistant_final();
            self.maybe_insert_proposed_plan(proposed_plan.take(), key);
            self.maybe_hide_spinner();
            return;
        }
        // If we already finalized this id in the current turn with identical content,
        // drop this event to avoid duplicates (belt-and-suspenders against upstream repeats).
        if let Some(ref want) = id
            && self
                .stream_state
                .closed_answer_ids
                .contains(&StreamId(want.clone()))
                && let Some(existing_idx) = self.history_cells.iter().rposition(|c| {
                    c.as_any()
                        .downcast_ref::<history_cell::AssistantMarkdownCell>()
                        .map(|amc| amc.stream_id() == Some(want.as_str()))
                        .unwrap_or(false)
                })
                    && let Some(amc) = self.history_cells[existing_idx]
                        .as_any()
                        .downcast_ref::<history_cell::AssistantMarkdownCell>()
                    {
                        let prev = Self::normalize_text(amc.markdown());
                        let newn = Self::normalize_text(&final_source);
                        if prev == newn {
                            tracing::debug!(
                                "InsertFinalAnswer: dropping duplicate final for id={}",
                                want
                            );
                            if let Some(after_key) = self.cell_order_seq.get(existing_idx).copied()
                            {
                                self.maybe_insert_proposed_plan(proposed_plan.take(), after_key);
                            }
                            self.maybe_hide_spinner();
                            return;
                        }
                    }

        // Replace the matching StreamingContentCell if one exists for this id; else fallback to most recent.
        // NOTE (dup‑guard): This relies on `StreamingContentCell::as_any()` returning `self`.
        // If that impl is removed, downcast_ref will fail and we won't find the streaming cell,
        // causing the final to append a new Assistant cell (duplicate).
        let streaming_idx = if let Some(ref want) = id {
            // Only replace a streaming cell if its id matches this final.
            self.history_cells.iter().rposition(|c| {
                if let Some(sc) = c
                    .as_any()
                    .downcast_ref::<history_cell::StreamingContentCell>()
                {
                    sc.id.as_ref() == Some(want)
                } else {
                    false
                }
            })
        } else {
            None
        };
        if let Some(idx) = streaming_idx {
            tracing::debug!(
                "final-answer: replacing StreamingContentCell at idx={} by id match",
                idx
            );
            let after_key = self
                .cell_order_seq
                .get(idx)
                .copied()
                .unwrap_or_else(|| self.next_internal_key());
            let mut state = self.finalize_answer_stream_state(
                id.as_deref(),
                &final_source,
                std::mem::take(&mut citations),
            );
            self.apply_mid_turn_flag(id.as_deref(), &mut state);
            let history_id = state.id;
            let cell = history_cell::AssistantMarkdownCell::from_state(state, &self.config);
            self.history_replace_at(idx, Box::new(cell));
            if let Some(ref want) = id {
                self.stream_state
                    .closed_answer_ids
                    .insert(StreamId(want.clone()));
            }
            self.autoscroll_if_near_bottom();
            self.last_answer_stream_id_in_turn = id.clone();
            self.last_answer_history_id_in_turn = Some(history_id);
            // Final cell committed via replacement; now advance Auto Drive.
            self.auto_on_assistant_final();
            self.maybe_insert_proposed_plan(proposed_plan.take(), after_key);
            self.maybe_hide_spinner();
            return;
        }

        // No streaming cell found. First, try to replace a finalized assistant cell
        // that was created for the same stream id (e.g., we already finalized due to
        // a lifecycle event and this InsertFinalAnswer arrived slightly later).
        if let Some(ref want) = id
            && let Some(idx) = self.history_cells.iter().rposition(|c| {
                if let Some(amc) = c
                    .as_any()
                    .downcast_ref::<history_cell::AssistantMarkdownCell>()
                {
                    amc.stream_id() == Some(want.as_str())
                } else {
                    false
                }
            }) {
                tracing::debug!(
                    "final-answer: replacing existing AssistantMarkdownCell at idx={} by id match",
                    idx
                );
                let after_key = self
                    .cell_order_seq
                    .get(idx)
                    .copied()
                    .unwrap_or_else(|| self.next_internal_key());
                let mut state =
                    self.finalize_answer_stream_state(id.as_deref(), &final_source, std::mem::take(&mut citations));
                self.apply_mid_turn_flag(id.as_deref(), &mut state);
                let history_id = state.id;
                let cell = history_cell::AssistantMarkdownCell::from_state(state, &self.config);
                self.history_replace_at(idx, Box::new(cell));
                self.stream_state
                    .closed_answer_ids
                    .insert(StreamId(want.clone()));
                self.autoscroll_if_near_bottom();
                self.last_answer_stream_id_in_turn = id.clone();
                self.last_answer_history_id_in_turn = Some(history_id);
                // Final cell replaced in-place; advance Auto Drive now.
                self.auto_on_assistant_final();
                self.maybe_insert_proposed_plan(proposed_plan.take(), after_key);
                self.maybe_hide_spinner();
                return;
            }

        // Otherwise, if a finalized assistant cell exists at the tail,
        // replace it in place to avoid duplicate assistant messages when a second
        // InsertFinalAnswer (e.g., from an AgentMessage event) arrives after we already
        // finalized due to a side event.
        if let Some(idx) = self.history_cells.iter().rposition(|c| {
            c.as_any()
                .downcast_ref::<history_cell::AssistantMarkdownCell>()
                .is_some()
        }) {
            // Replace the tail finalized assistant cell if the new content is identical OR
            // a small revision that merely adds leading/trailing context. Otherwise append a
            // new assistant message so distinct replies remain separate.
            let should_replace = self.history_cells[idx]
                .as_any()
                .downcast_ref::<history_cell::AssistantMarkdownCell>()
                .map(|amc| {
                    let prev = Self::normalize_text(amc.markdown());
                    let newn = Self::normalize_text(&final_source);
                    let identical = prev == newn;
                    if identical || prev.is_empty() {
                        return identical;
                    }
                    let is_prefix_expansion = newn.starts_with(&prev);
                    let is_suffix_expansion = newn.ends_with(&prev);
                    let is_large_superset = prev.len() >= 80 && newn.contains(&prev);
                    identical || is_prefix_expansion || is_suffix_expansion || is_large_superset
                })
                .unwrap_or(false);
            if should_replace {
                tracing::debug!(
                    "final-answer: replacing tail AssistantMarkdownCell via heuristic identical/expansion"
                );
                let after_key = self
                    .cell_order_seq
                    .get(idx)
                    .copied()
                    .unwrap_or_else(|| self.next_internal_key());
                let mut state =
                    self.finalize_answer_stream_state(id.as_deref(), &final_source, std::mem::take(&mut citations));
                self.apply_mid_turn_flag(id.as_deref(), &mut state);
                let history_id = state.id;
                let cell = history_cell::AssistantMarkdownCell::from_state(state, &self.config);
                self.history_replace_at(idx, Box::new(cell));
                self.autoscroll_if_near_bottom();
                self.last_answer_stream_id_in_turn = id.clone();
                self.last_answer_history_id_in_turn = Some(history_id);
                // Final assistant content revised; advance Auto Drive now.
                self.auto_on_assistant_final();
                self.maybe_insert_proposed_plan(proposed_plan.take(), after_key);
                self.maybe_hide_spinner();
                return;
            }
        }

        // Fallback: no prior assistant cell found; insert at stable sequence position.
        tracing::debug!(
            "final-answer: ordered insert new AssistantMarkdownCell id={:?}",
            id
        );
        let mut key = match id.as_deref() {
            Some(rid) => self
                .try_stream_order_key(StreamKind::Answer, rid)
                .unwrap_or_else(|| {
                    tracing::warn!(
                        "missing stream order key for final Answer id={}; using synthetic key",
                        rid
                    );
                    self.next_internal_key()
                }),
            None => {
                tracing::warn!("missing stream id for final Answer; using synthetic key");
                self.next_internal_key()
            }
        };
        if let Some(last) = self.last_assigned_order
            && key <= last {
                // Background notices anchor themselves at out = i32::MAX. If a final answer arrives
                // after those notices we still want it to appear at the bottom, so bump the key
                // just past the most-recently assigned slot.
                key = Self::order_key_successor(last);
                if let Some(ref want) = id {
                    self.stream_order_seq
                        .insert((StreamKind::Answer, want.clone()), key);
                }
            }
        tracing::info!(
            "[order] final Answer ordered insert id={:?} {}",
            id,
            Self::debug_fmt_order_key(key)
        );
        let mut state =
            self.finalize_answer_stream_state(id.as_deref(), &final_source, std::mem::take(&mut citations));
        self.apply_mid_turn_flag(id.as_deref(), &mut state);
        let history_id = state.id;
        let cell = history_cell::AssistantMarkdownCell::from_state(state, &self.config);
        self.history_insert_existing_record(
            Box::new(cell),
            key,
            "answer-final",
            history_id,
        );
        if let Some(ref want) = id {
            self.stream_state
                .closed_answer_ids
                .insert(StreamId(want.clone()));
        }
        self.last_answer_stream_id_in_turn = id.clone();
        self.last_answer_history_id_in_turn = Some(history_id);
        // Ordered insert completed; advance Auto Drive now that the assistant
        // message is present in history.
        self.auto_on_assistant_final();
        self.maybe_insert_proposed_plan(proposed_plan.take(), key);
        self.maybe_hide_spinner();
    }

    // Assign or fetch a stable sequence for a stream kind+id within its originating turn
    // removed legacy ensure_stream_order_key; strict variant is used instead

}
