use super::*;
use code_core::protocol::ExecCommandOutputDeltaEvent;
use code_core::protocol::McpToolCallBeginEvent;
use code_core::protocol::McpToolCallEndEvent;
use code_core::protocol::OrderMeta;

impl ChatWidget<'_> {
    pub(super) fn handle_exec_command_begin_event(
        &mut self,
        ev: ExecCommandBeginEvent,
        order: Option<OrderMeta>,
        seq: u64,
    ) {
        let om_begin = order.unwrap_or_else(|| {
            tracing::warn!("missing OrderMeta for ExecCommandBegin; using synthetic order");
            code_core::protocol::OrderMeta {
                request_ordinal: self.last_seen_request_index,
                output_index: Some(i32::MAX as u32),
                sequence_number: Some(seq),
            }
        });
        self.handle_exec_begin_ordered(ev, om_begin, seq);
    }

    pub(super) fn handle_exec_command_output_delta_event(
        &mut self,
        ev: ExecCommandOutputDeltaEvent,
    ) {
        let call_id = ExecCallId(ev.call_id.clone());
        if self.exec.running_commands.contains_key(&call_id) {
            self.ensure_spinner_for_activity("exec-output");
        }
        if let Some(running) = self.exec.running_commands.get_mut(&call_id) {
            let chunk = String::from_utf8_lossy(&ev.chunk).to_string();
            let chunk_len = chunk.len();
            let (stdout_chunk, stderr_chunk) = match ev.stream {
                ExecOutputStream::Stdout => {
                    let offset = running.stdout_offset;
                    running.stdout_offset = running.stdout_offset.saturating_add(chunk_len);
                    (
                        Some(crate::history::state::ExecStreamChunk {
                            offset,
                            content: chunk,
                        }),
                        None,
                    )
                }
                ExecOutputStream::Stderr => {
                    let offset = running.stderr_offset;
                    running.stderr_offset = running.stderr_offset.saturating_add(chunk_len);
                    (
                        None,
                        Some(crate::history::state::ExecStreamChunk {
                            offset,
                            content: chunk,
                        }),
                    )
                }
            };
            let history_id = running.history_id.or_else(|| {
                let mapped = self
                    .history_state
                    .history_id_for_exec_call(call_id.as_ref())
                    .or_else(|| {
                        running
                            .history_index
                            .and_then(|idx| self.history_cell_ids.get(idx).and_then(|slot| *slot))
                    });
                running.history_id = mapped;
                mapped
            });
            if let Some(history_id) = history_id
                && let Some(record_idx) = self.history_state.index_of(history_id)
            {
                let mutation =
                    self.history_state
                        .apply_domain_event(HistoryDomainEvent::UpdateExecStream {
                            index: record_idx,
                            stdout_chunk,
                            stderr_chunk,
                        });
                if let HistoryMutation::Replaced {
                    id,
                    record: HistoryRecord::Exec(exec_record),
                    ..
                } = mutation
                {
                    self.update_cell_from_record(id, HistoryRecord::Exec(exec_record));
                }
            }
            self.invalidate_height_cache();
            self.autoscroll_if_near_bottom();
            self.request_redraw();
        }
    }

    pub(super) fn handle_patch_apply_begin_event(
        &mut self,
        event: PatchApplyBeginEvent,
        order: Option<&OrderMeta>,
    ) {
        let PatchApplyBeginEvent {
            call_id,
            auto_approved,
            changes,
        } = event;
        let exec_call_id = ExecCallId(call_id);
        self.exec.suppress_exec_end(exec_call_id);
        // Store for session diff popup (clone before moving into history)
        self.diffs.session_patch_sets.push(changes.clone());
        // Capture/adjust baselines, including rename moves
        if let Some(last) = self.diffs.session_patch_sets.last() {
            for (src_path, chg) in last.iter() {
                match chg {
                    code_core::protocol::FileChange::Update {
                        move_path: Some(dest_path),
                        ..
                    } => {
                        // Prefer to carry forward existing baseline from src to dest.
                        if let Some(baseline) = self.diffs.baseline_file_contents.remove(src_path) {
                            self.diffs
                                .baseline_file_contents
                                .insert(dest_path.clone(), baseline);
                        } else if !self.diffs.baseline_file_contents.contains_key(dest_path) {
                            // Fallback: snapshot current contents of src (pre-apply) under dest key.
                            let baseline = std::fs::read_to_string(src_path).unwrap_or_default();
                            self.diffs
                                .baseline_file_contents
                                .insert(dest_path.clone(), baseline);
                        }
                    }
                    _ => {
                        if !self.diffs.baseline_file_contents.contains_key(src_path) {
                            let baseline = std::fs::read_to_string(src_path).unwrap_or_default();
                            self.diffs
                                .baseline_file_contents
                                .insert(src_path.clone(), baseline);
                        }
                    }
                }
            }
        }
        // Enable Ctrl+D footer hint now that we have diffs to show
        self.bottom_pane.set_diffs_hint(true);
        // Strict order
        let ok = match order {
            Some(om) => self.provider_order_key_from_order_meta(om),
            None => {
                tracing::warn!("missing OrderMeta on ExecEnd flush; using synthetic key");
                self.next_internal_key()
            }
        };
        let cell = history_cell::new_patch_event(PatchEventType::ApplyBegin { auto_approved }, changes);
        let _ = self.history_insert_with_key_global(Box::new(cell), ok);
    }

    pub(super) fn handle_patch_apply_end_event(&mut self, ev: PatchApplyEndEvent, seq: u64) {
        let ev2 = ev.clone();
        self.defer_or_handle(
            move |interrupts| interrupts.push_patch_end(seq, ev),
            |this| this.handle_patch_apply_end_now(ev2),
        );
    }

    pub(super) fn handle_exec_command_end_event(
        &mut self,
        ev: ExecCommandEndEvent,
        order: Option<OrderMeta>,
        seq: u64,
    ) {
        let ev2 = ev.clone();
        let order_meta_end = order.unwrap_or_else(|| {
            tracing::warn!("missing OrderMeta for ExecCommandEnd; using synthetic order");
            code_core::protocol::OrderMeta {
                request_ordinal: self.last_seen_request_index,
                output_index: Some(i32::MAX as u32),
                sequence_number: Some(seq),
            }
        });
        let om_for_send = order_meta_end.clone();
        self.defer_or_handle(
            move |interrupts| interrupts.push_exec_end(seq, ev, Some(om_for_send)),
            move |this| {
                tracing::info!("[order] ExecCommandEnd call_id={} seq={}", ev2.call_id, seq);
                this.enqueue_or_handle_exec_end(ev2, order_meta_end);
            },
        );
    }

    pub(super) fn handle_mcp_tool_call_begin_event(
        &mut self,
        ev: McpToolCallBeginEvent,
        order: Option<&OrderMeta>,
        seq: u64,
    ) {
        let order_ok = match order {
            Some(om) => self.provider_order_key_from_order_meta(om),
            None => {
                tracing::warn!("missing OrderMeta on McpBegin; using synthetic key");
                self.next_internal_key()
            }
        };
        self.finalize_active_stream();
        tracing::info!("[order] McpToolCallBegin call_id={} seq={}", ev.call_id, seq);
        self.ensure_spinner_for_activity("mcp-begin");
        tools::mcp_begin(self, ev, order_ok);
        if self.interrupts.has_queued() {
            self.flush_interrupt_queue();
        }
    }

    pub(super) fn handle_mcp_tool_call_end_event(
        &mut self,
        ev: McpToolCallEndEvent,
        order: Option<OrderMeta>,
        seq: u64,
    ) {
        let ev2 = ev.clone();
        let order_ok = match order.as_ref() {
            Some(om) => self.provider_order_key_from_order_meta(om),
            None => {
                tracing::warn!("missing OrderMeta on McpEnd; using synthetic key");
                self.next_internal_key()
            }
        };
        self.defer_or_handle(
            move |interrupts| interrupts.push_mcp_end(seq, ev, order),
            |this| {
                tracing::info!("[order] McpToolCallEnd call_id={} seq={}", ev2.call_id, seq);
                tools::mcp_end(this, ev2, order_ok)
            },
        );
    }

    pub(super) fn handle_view_image_tool_call_event(
        &mut self,
        call_id: String,
        path: std::path::PathBuf,
        order: Option<&OrderMeta>,
    ) {
        let ok = match order {
            Some(om) => self.provider_order_key_from_order_meta(om),
            None => {
                tracing::warn!("missing OrderMeta on ViewImageToolCall; using synthetic key");
                self.next_internal_key()
            }
        };
        if let Some(record) = image_record_from_path(&path) {
            let cell = Box::new(history_cell::ImageOutputCell::from_record(record));
            let _ = self.history_insert_with_key_global(cell, ok);
            self.tools_state.image_viewed_calls.insert(ToolCallId(call_id));
        }
    }
}
