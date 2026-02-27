use super::prelude::*;

impl ChatWidget<'_> {
    /// Briefly show the vertical scrollbar and schedule a redraw to hide it.
    pub(in super::super) fn flash_scrollbar(&self) {
        layout_scroll::flash_scrollbar(self);
    }

    pub(in super::super) fn ensure_image_cell_picker(&self, cell: &dyn HistoryCell) {
        if let Some(image) = cell
            .as_any()
            .downcast_ref::<crate::history_cell::ImageOutputCell>()
        {
            let picker = self.terminal_info.picker.clone();
            let font_size = self.terminal_info.font_size;
            image.ensure_picker_initialized(picker, font_size);
        }
    }

    pub(in super::super) fn history_insert_with_key_global(
        &mut self,
        cell: Box<dyn HistoryCell>,
        key: OrderKey,
    ) -> usize {
        self.history_insert_with_key_global_tagged(cell, key, "untagged", None)
    }

    // Internal: same as above but with a short tag for debug overlays.
    pub(in super::super) fn history_insert_with_key_global_tagged(
        &mut self,
        cell: Box<dyn HistoryCell>,
        key: OrderKey,
        tag: &'static str,
        record: Option<HistoryDomainRecord>,
    ) -> usize {
        #[cfg(debug_assertions)]
        {
            let cell_kind = cell.kind();
            if cell_kind == HistoryCellType::BackgroundEvent {
                debug_assert!(
                    tag == "background",
                    "Background events must use the background helper (tag={tag})"
                );
            }
        }
        self.ensure_image_cell_picker(cell.as_ref());
        // Any ordered insert of a non-reasoning cell means reasoning is no longer the
        // bottom-most active block; drop the in-progress ellipsis on collapsed titles.
        let is_reasoning_cell = cell
            .as_any()
            .downcast_ref::<crate::history_cell::CollapsibleReasoningCell>()
            .is_some();
        if !is_reasoning_cell {
            self.clear_reasoning_in_progress();
        }
        let is_background_cell = matches!(cell.kind(), HistoryCellType::BackgroundEvent);
        let mut key = key;
        let mut key_bumped = false;
        if !is_background_cell
            && let Some(last) = self.last_assigned_order
                && key <= last {
                    key = Self::order_key_successor(last);
                    key_bumped = true;
                }

        // Determine insertion position across the entire history.
        // Most ordered inserts are monotonic tail-appends (we bump non-background
        // keys to keep them strictly increasing), so avoid an O(n) scan in the
        // common case.
        //
        // Exception: some early, non-background system cells (e.g. the context
        // summary) are inserted with a low order key before any ordering state
        // has been established. In that phase, we must still respect the order.
        let mut pos = self.history_cells.len();
        if is_background_cell || self.last_assigned_order.is_none() {
            for i in 0..self.history_cells.len() {
                if let Some(existing) = self.cell_order_seq.get(i)
                    && *existing > key {
                        pos = i;
                        break;
                    }
            }
        }

        // Keep auxiliary order vector in lockstep with history before inserting
        if self.cell_order_seq.len() < self.history_cells.len() {
            let missing = self.history_cells.len() - self.cell_order_seq.len();
            for _ in 0..missing {
                self.cell_order_seq.push(OrderKey {
                    req: 0,
                    out: -1,
                    seq: 0,
                });
            }
        }

        tracing::info!(
            "[order] insert: {} pos={} len_before={} order_len_before={} tag={}",
            Self::debug_fmt_order_key(key),
            pos,
            self.history_cells.len(),
            self.cell_order_seq.len(),
            tag
        );
        // If order overlay is enabled, compute a short, inline debug summary for
        // reasoning titles so we can spot mid‑word character drops quickly.
        // We intentionally do this before inserting so we can attach the
        // composed string alongside the standard order debug info.
        let reasoning_title_dbg: Option<String> = if self.show_order_overlay {
            // CollapsibleReasoningCell shows a collapsed "title" line; extract
            // the first visible line and summarize its raw text/lengths.
            if let Some(rc) = cell
                .as_any()
                .downcast_ref::<crate::history_cell::CollapsibleReasoningCell>()
            {
                let lines = rc.display_lines_trimmed();
                let first = lines.first();
                if let Some(line) = first {
                    // Collect visible text and basic metrics
                    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                    let bytes = text.len();
                    let chars = text.chars().count();
                    let width = unicode_width::UnicodeWidthStr::width(text.as_str());
                    let spans = line.spans.len();
                    // Per‑span byte lengths to catch odd splits inside words
                    let span_lens: Vec<usize> =
                        line.spans.iter().map(|s| s.content.len()).collect();
                    // Truncate preview to avoid overflow in narrow panes
                    let mut preview = text;
                    // Truncate preview by display width, not bytes, to avoid splitting
                    // a multi-byte character at an invalid boundary.
                    {
                        use unicode_width::UnicodeWidthStr as _;
                        let maxw = 120usize;
                        if preview.width() > maxw {
                            preview = format!(
                                "{}…",
                                crate::live_wrap::take_prefix_by_width(
                                    &preview,
                                    maxw.saturating_sub(1)
                                )
                                .0
                            );
                        }
                    }
                    Some(format!(
                        "title='{preview}' bytes={bytes} chars={chars} width={width} spans={spans} span_bytes={span_lens:?}"
                    ))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let mut cell = cell;

        let mutation = if let Some(domain_record) = record {
            let record_index = if pos == self.history_cells.len() {
                self.history_state.records.len()
            } else {
                self.record_index_for_position(pos)
            };
            let event = match domain_record {
                HistoryDomainRecord::Exec(ref exec_record) => {
                    HistoryDomainEvent::StartExec {
                        index: record_index,
                        call_id: exec_record.call_id.clone(),
                        command: exec_record.command.clone(),
                        parsed: exec_record.parsed.clone(),
                        action: exec_record.action,
                        started_at: exec_record.started_at,
                        working_dir: exec_record.working_dir.clone(),
                        env: exec_record.env.clone(),
                        tags: exec_record.tags.clone(),
                    }
                }
                other => HistoryDomainEvent::Insert {
                    index: record_index,
                    record: other,
                },
            };
            Some(self.history_state.apply_domain_event(event))
        } else if let Some(record) = history_cell::record_from_cell(cell.as_ref()) {
            let record_index = if pos == self.history_cells.len() {
                self.history_state.records.len()
            } else {
                self.record_index_for_position(pos)
            };
            let event = match HistoryDomainRecord::from(record) {
                HistoryDomainRecord::Exec(exec_record) => HistoryDomainEvent::StartExec {
                    index: record_index,
                    call_id: exec_record.call_id.clone(),
                    command: exec_record.command.clone(),
                    parsed: exec_record.parsed.clone(),
                    action: exec_record.action,
                    started_at: exec_record.started_at,
                    working_dir: exec_record.working_dir.clone(),
                    env: exec_record.env.clone(),
                    tags: exec_record.tags,
                },
                other => HistoryDomainEvent::Insert {
                    index: record_index,
                    record: other,
                },
            };
            Some(self.history_state.apply_domain_event(event))
        } else {
            None
        };

        let mut maybe_id = None;
        if let Some(mutation) = mutation
            && let Some(id) = self.apply_mutation_to_cell(&mut cell, mutation) {
                maybe_id = Some(id);
            }

        let append = pos == self.history_cells.len();
        if !append {
            self.history_prefix_append_only.set(false);
        }
        if append {
            self.history_cells.push(cell);
            self.history_cell_ids.push(maybe_id);
        } else {
            self.history_cells.insert(pos, cell);
            self.history_cell_ids.insert(pos, maybe_id);
        }
        // In terminal mode, App mirrors history lines into the native buffer.
        // Ensure order vector is also long enough for position after cell insert
        if self.cell_order_seq.len() < pos {
            self.cell_order_seq.resize(
                pos,
                OrderKey {
                    req: 0,
                    out: -1,
                    seq: 0,
                },
            );
        }
        if append {
            self.cell_order_seq.push(key);
        } else {
            self.cell_order_seq.insert(pos, key);
        }
        if key_bumped
            && let Some(stream) = self.history_cells[pos]
                .as_any()
                .downcast_ref::<crate::history_cell::StreamingContentCell>()
            {
                self.stream_order_seq
                    .insert((StreamKind::Answer, stream.state().stream_id.clone()), key);
            }
        self.last_assigned_order = Some(match self.last_assigned_order {
            Some(prev) => prev.max(key),
            None => key,
        });
        // Insert debug info aligned with cell insert
        let ordered = "ordered";
        let req_dbg = format!("{}", key.req);
        let dbg = if let Some(tdbg) = reasoning_title_dbg {
            format!(
                "insert: {} req={} key={} {} pos={} tag={} | {}",
                ordered,
                req_dbg,
                0,
                Self::debug_fmt_order_key(key),
                pos,
                tag,
                tdbg
            )
        } else {
            format!(
                "insert: {} req={} {} pos={} tag={}",
                ordered,
                req_dbg,
                Self::debug_fmt_order_key(key),
                pos,
                tag
            )
        };
        if self.cell_order_dbg.len() < pos {
            self.cell_order_dbg.resize(pos, None);
        }
        if append {
            self.cell_order_dbg.push(Some(dbg));
        } else {
            self.cell_order_dbg.insert(pos, Some(dbg));
        }
        if let Some(id) = maybe_id {
            if id != HistoryId::ZERO {
                self.history_render.invalidate_history_id(id);
            } else {
                self.history_render.invalidate_prefix_only();
            }
        } else {
            self.history_render.invalidate_prefix_only();
        }
        self.mark_render_requests_dirty();
        self.autoscroll_if_near_bottom();
        self.bottom_pane.set_has_chat_history(true);
        self.process_animation_cleanup();
        // Maintain input focus when new history arrives unless a modal overlay owns it
        if !self.agents_terminal.active {
            self.bottom_pane.ensure_input_focus();
        }
        self.app_event_tx.send(AppEvent::RequestRedraw);
        self.refresh_explore_trailing_flags();
        self.refresh_reasoning_collapsed_visibility();
        self.mark_history_dirty();
        pos
    }

    pub(in super::super) fn history_insert_existing_record(
        &mut self,
        mut cell: Box<dyn HistoryCell>,
        mut key: OrderKey,
        tag: &'static str,
        id: HistoryId,
    ) -> usize {
        #[cfg(debug_assertions)]
        {
            let cell_kind = cell.kind();
            if cell_kind == HistoryCellType::BackgroundEvent {
                debug_assert!(
                    tag == "background",
                    "Background events must use the background helper (tag={tag})"
                );
            }
        }

        let is_reasoning_cell = cell
            .as_any()
            .downcast_ref::<crate::history_cell::CollapsibleReasoningCell>()
            .is_some();
        if !is_reasoning_cell {
                self.clear_reasoning_in_progress();
        }

        let is_background_cell = matches!(cell.kind(), HistoryCellType::BackgroundEvent);
        let mut key_bumped = false;
        if !is_background_cell
            && let Some(last) = self.last_assigned_order
                && key <= last {
                    key = Self::order_key_successor(last);
                    key_bumped = true;
                }

        let mut pos = self.history_cells.len();
        if is_background_cell || self.last_assigned_order.is_none() {
            for i in 0..self.history_cells.len() {
                if let Some(existing) = self.cell_order_seq.get(i)
                    && *existing > key {
                        pos = i;
                        break;
                    }
            }
        }

        if self.cell_order_seq.len() < self.history_cells.len() {
            let missing = self.history_cells.len() - self.cell_order_seq.len();
            for _ in 0..missing {
                self.cell_order_seq.push(OrderKey {
                    req: 0,
                    out: -1,
                    seq: 0,
                });
            }
        }

        tracing::info!(
            "[order] insert(existing): {} pos={} len_before={} order_len_before={} tag={}",
            Self::debug_fmt_order_key(key),
            pos,
            self.history_cells.len(),
            self.cell_order_seq.len(),
            tag
        );

        let reasoning_title_dbg: Option<String> = if self.show_order_overlay {
            if let Some(rc) = cell
                .as_any()
                .downcast_ref::<crate::history_cell::CollapsibleReasoningCell>()
            {
                let lines = rc.display_lines_trimmed();
                if let Some(line) = lines.first() {
                    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                    let bytes = text.len();
                    let chars = text.chars().count();
                    let width = unicode_width::UnicodeWidthStr::width(text.as_str());
                    let spans = line.spans.len();
                    let span_lens: Vec<usize> =
                        line.spans.iter().map(|s| s.content.len()).collect();
                    let mut preview = text;
                    {
                        use unicode_width::UnicodeWidthStr as _;
                        let maxw = 120usize;
                        if preview.width() > maxw {
                            preview = format!(
                                "{}…",
                                crate::live_wrap::take_prefix_by_width(
                                    &preview,
                                    maxw.saturating_sub(1)
                                )
                                .0
                            );
                        }
                    }
                    Some(format!(
                        "title='{preview}' bytes={bytes} chars={chars} width={width} spans={spans} span_bytes={span_lens:?}"
                    ))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        Self::assign_history_id_inner(&mut cell, id);

        let append = pos == self.history_cells.len();
        if !append {
            self.history_prefix_append_only.set(false);
        }
        if append {
            self.history_cells.push(cell);
            self.history_cell_ids.push(Some(id));
        } else {
            self.history_cells.insert(pos, cell);
            self.history_cell_ids.insert(pos, Some(id));
        }
        if self.cell_order_seq.len() < pos {
            self.cell_order_seq.resize(
                pos,
                OrderKey {
                    req: 0,
                    out: -1,
                    seq: 0,
                },
            );
        }
        if append {
            self.cell_order_seq.push(key);
        } else {
            self.cell_order_seq.insert(pos, key);
        }
        if key_bumped
            && let Some(stream) = self.history_cells[pos]
                .as_any()
                .downcast_ref::<crate::history_cell::StreamingContentCell>()
            {
                self.stream_order_seq
                    .insert((StreamKind::Answer, stream.state().stream_id.clone()), key);
            }
        self.last_assigned_order = Some(match self.last_assigned_order {
            Some(prev) => prev.max(key),
            None => key,
        });

        let ordered = "existing";
        let req_dbg = format!("{}", key.req);
        let dbg = if let Some(tdbg) = reasoning_title_dbg {
            format!(
                "insert: {} req={} {} pos={} tag={} | {}",
                ordered,
                req_dbg,
                Self::debug_fmt_order_key(key),
                pos,
                tag,
                tdbg
            )
        } else {
            format!(
                "insert: {} req={} {} pos={} tag={}",
                ordered,
                req_dbg,
                Self::debug_fmt_order_key(key),
                pos,
                tag
            )
        };
        if self.cell_order_dbg.len() < pos {
            self.cell_order_dbg.resize(pos, None);
        }
        if append {
            self.cell_order_dbg.push(Some(dbg));
        } else {
            self.cell_order_dbg.insert(pos, Some(dbg));
        }
        self.history_render.invalidate_history_id(id);
        self.mark_render_requests_dirty();
        self.autoscroll_if_near_bottom();
        self.bottom_pane.set_has_chat_history(true);
        self.process_animation_cleanup();
        if !self.agents_terminal.active {
            self.bottom_pane.ensure_input_focus();
        }
        self.app_event_tx.send(AppEvent::RequestRedraw);
        self.refresh_explore_trailing_flags();
        self.refresh_reasoning_collapsed_visibility();
        self.mark_history_dirty();
        pos
    }

    pub(in super::super) fn append_wait_pairs(target: &mut Vec<(String, bool)>, additions: &[(String, bool)]) {
        for (text, is_error) in additions {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            if target
                .last()
                .map(|(existing, existing_err)| existing == trimmed && *existing_err == *is_error)
                .unwrap_or(false)
            {
                continue;
            }
            target.push((trimmed.to_string(), *is_error));
        }
    }

    pub(in super::super) fn wait_pairs_from_exec_notes(notes: &[ExecWaitNote]) -> Vec<(String, bool)> {
        notes
            .iter()
            .map(|note| {
                (
                    note.message.clone(),
                    matches!(note.tone, TextTone::Error),
                )
            })
            .collect()
    }

    pub(in super::super) fn update_exec_wait_state_with_pairs(
        &mut self,
        history_id: HistoryId,
        total_wait: Option<Duration>,
        wait_active: bool,
        notes: &[(String, bool)],
    ) -> bool {
        let Some(record_idx) = self.history_state.index_of(history_id) else {
            return false;
        };
        let note_records: Vec<ExecWaitNote> = notes
            .iter()
            .filter_map(|(text, is_error)| {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(ExecWaitNote {
                        message: trimmed.to_string(),
                        tone: if *is_error {
                            TextTone::Error
                        } else {
                            TextTone::Info
                        },
                        timestamp: SystemTime::now(),
                    })
                }
            })
            .collect();
        let mutation = self.history_state.apply_domain_event(HistoryDomainEvent::UpdateExecWait {
            index: record_idx,
            total_wait,
            wait_active,
            notes: note_records,
        });
        match mutation {
            HistoryMutation::Replaced {
                id,
                record: HistoryRecord::Exec(exec_record),
                ..
            }
            | HistoryMutation::Inserted {
                id,
                record: HistoryRecord::Exec(exec_record),
                ..
            } => {
                self.update_cell_from_record(id, HistoryRecord::Exec(exec_record));
                self.mark_history_dirty();
                true
            }
            _ => false,
        }
    }

    pub(in super::super) fn merge_tool_arguments(existing: &mut Vec<ToolArgument>, updates: Vec<ToolArgument>) {
        for update in updates {
            if let Some(existing_arg) = existing.iter_mut().find(|arg| arg.name == update.name) {
                *existing_arg = update;
            } else {
                existing.push(update);
            }
        }
    }

    pub(in super::super) fn apply_custom_tool_update(
        &mut self,
        call_id: &str,
        parameters: Option<serde_json::Value>,
    ) {
        let Some(params) = parameters else {
            return;
        };
        let updates = history_cell::arguments_from_json(&params);
        if updates.is_empty() {
            return;
        }

        let running_entry = self
            .tools_state
            .running_custom_tools
            .get(&ToolCallId(call_id.to_string()))
            .copied();
        let resolved_idx = running_entry
            .as_ref()
            .and_then(|entry| running_tools::resolve_entry_index(self, entry, call_id))
            .or_else(|| running_tools::find_by_call_id(self, call_id));

        let Some(idx) = resolved_idx else {
            return;
        };
        if idx >= self.history_cells.len() {
            return;
        }
        let Some(running_cell) = self.history_cells[idx]
            .as_any()
            .downcast_ref::<history_cell::RunningToolCallCell>()
        else {
            return;
        };

        let mut state = running_cell.state().clone();
        Self::merge_tool_arguments(&mut state.arguments, updates);
        let mut updated_cell = history_cell::RunningToolCallCell::from_state(state.clone());
        updated_cell.state_mut().call_id = Some(call_id.to_string());
        self.history_replace_with_record(
            idx,
            Box::new(updated_cell),
            HistoryDomainRecord::from(state),
        );
    }

    pub(in super::super) fn hydrate_cell_from_record(
        &self,
        cell: &mut Box<dyn HistoryCell>,
        record: &HistoryRecord,
    ) -> bool {
        Self::hydrate_cell_from_record_inner(cell, record, &self.config)
    }

    pub(in super::super) fn hydrate_cell_from_record_inner(
        cell: &mut Box<dyn HistoryCell>,
        record: &HistoryRecord,
        config: &Config,
    ) -> bool {
        match record {
            HistoryRecord::PlainMessage(state) => {
                if let Some(plain) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::PlainHistoryCell>()
                {
                    *plain.state_mut() = state.clone();
                    return true;
                }
            }
            HistoryRecord::WaitStatus(state) => {
                if let Some(wait) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::WaitStatusCell>()
                {
                    *wait.state_mut() = state.clone();
                    return true;
                }
            }
            HistoryRecord::Loading(state) => {
                if let Some(loading) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::LoadingCell>()
                {
                    *loading.state_mut() = state.clone();
                    return true;
                }
            }
            HistoryRecord::BackgroundEvent(state) => {
                if let Some(background) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::BackgroundEventCell>()
                {
                    *background.state_mut() = state.clone();
                    return true;
                }
            }
            HistoryRecord::Exec(state) => {
                if let Some(exec) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::ExecCell>()
                {
                    exec.sync_from_record(state);
                    return true;
                }
            }
            HistoryRecord::AssistantStream(state) => {
                if let Some(stream) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::StreamingContentCell>()
                {
                    stream.set_state(state.clone());
                    stream.update_context(config.file_opener, &config.cwd);
                    return true;
                }
            }
            HistoryRecord::RateLimits(state) => {
                if let Some(rate_limits) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::RateLimitsCell>()
                {
                    *rate_limits.record_mut() = state.clone();
                    return true;
                }
            }
            HistoryRecord::Patch(state) => {
                if let Some(patch) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::PatchSummaryCell>()
                {
                    patch.update_record(state.clone());
                    return true;
                }
            }
            HistoryRecord::Image(state) => {
                if let Some(image) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::ImageOutputCell>()
                {
                    *image.record_mut() = state.clone();
                    return true;
                }
            }
            HistoryRecord::Context(state) => {
                if let Some(context) = cell
                    .as_any_mut()
                    .downcast_mut::<crate::history_cell::ContextCell>()
                {
                    context.update(state.clone());
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    pub(in super::super) fn build_cell_from_record(&self, record: &HistoryRecord) -> Option<Box<dyn HistoryCell>> {
        use crate::history_cell;

        match record {
            HistoryRecord::PlainMessage(state) => Some(Box::new(
                history_cell::PlainHistoryCell::from_state(state.clone()),
            )),
            HistoryRecord::WaitStatus(state) => {
                Some(Box::new(history_cell::WaitStatusCell::from_state(state.clone())))
            }
            HistoryRecord::Loading(state) => {
                Some(Box::new(history_cell::LoadingCell::from_state(state.clone())))
            }
            HistoryRecord::RunningTool(state) => Some(Box::new(
                history_cell::RunningToolCallCell::from_state(state.clone()),
            )),
            HistoryRecord::ToolCall(state) => Some(Box::new(
                history_cell::ToolCallCell::from_state(state.clone()),
            )),
            HistoryRecord::PlanUpdate(state) => Some(Box::new(
                history_cell::PlanUpdateCell::from_state(state.clone()),
            )),
            HistoryRecord::UpgradeNotice(state) => Some(Box::new(
                history_cell::UpgradeNoticeCell::from_state(state.clone()),
            )),
            HistoryRecord::Reasoning(state) => Some(Box::new(
                history_cell::CollapsibleReasoningCell::from_state(state.clone()),
            )),
            HistoryRecord::Exec(state) => {
                Some(Box::new(history_cell::ExecCell::from_record(state.clone())))
            }
            HistoryRecord::MergedExec(state) => Some(Box::new(
                history_cell::MergedExecCell::from_state(state.clone()),
            )),
            HistoryRecord::AssistantStream(state) => Some(Box::new(
                history_cell::StreamingContentCell::from_state(
                    state.clone(),
                    self.config.file_opener,
                    self.config.cwd.clone(),
                ),
            )),
            HistoryRecord::AssistantMessage(state) => Some(Box::new(
                history_cell::AssistantMarkdownCell::from_state(state.clone(), &self.config),
            )),
            HistoryRecord::ProposedPlan(state) => Some(Box::new(
                history_cell::ProposedPlanCell::from_state(state.clone(), &self.config),
            )),
            HistoryRecord::Diff(state) => {
                Some(Box::new(history_cell::DiffCell::from_record(state.clone())))
            }
            HistoryRecord::Patch(state) => {
                Some(Box::new(history_cell::PatchSummaryCell::from_record(state.clone())))
            }
            HistoryRecord::Explore(state) => {
                Some(Box::new(history_cell::ExploreAggregationCell::from_record(state.clone())))
            }
            HistoryRecord::RateLimits(state) => Some(Box::new(
                history_cell::RateLimitsCell::from_record(state.clone()),
            )),
            HistoryRecord::BackgroundEvent(state) => {
                Some(Box::new(history_cell::BackgroundEventCell::new(state.clone())))
            }
            HistoryRecord::Image(state) => {
                let cell = history_cell::ImageOutputCell::from_record(state.clone());
                self.ensure_image_cell_picker(&cell);
                Some(Box::new(cell))
            }
            HistoryRecord::Context(state) => Some(Box::new(
                history_cell::ContextCell::new(state.clone()),
            )),
            HistoryRecord::Notice(state) => Some(Box::new(
                history_cell::PlainHistoryCell::from_notice_record(state.clone()),
            )),
        }
    }

    pub(in super::super) fn apply_mutation_to_cell(
        &self,
        cell: &mut Box<dyn HistoryCell>,
        mutation: HistoryMutation,
    ) -> Option<HistoryId> {
        match mutation {
            HistoryMutation::Inserted { id, record, .. }
            | HistoryMutation::Replaced { id, record, .. } => {
                if let Some(mut new_cell) = self.build_cell_from_record(&record) {
                    self.assign_history_id(&mut new_cell, id);
                    *cell = new_cell;
                } else if !self.hydrate_cell_from_record(cell, &record) {
                    self.assign_history_id(cell, id);
                }
                Some(id)
            }
            _ => None,
        }
    }

    pub(in super::super) fn apply_mutation_to_cell_index(
        &mut self,
        idx: usize,
        mutation: HistoryMutation,
    ) -> Option<HistoryId> {
        if idx >= self.history_cells.len() {
            return None;
        }
        match mutation {
            HistoryMutation::Inserted { id, record, .. }
            | HistoryMutation::Replaced { id, record, .. } => {
                self.update_cell_from_record(id, record);
                Some(id)
            }
            _ => None,
        }
    }

    pub(in super::super) fn cell_index_for_history_id(&self, id: HistoryId) -> Option<usize> {
        if let Some(idx) = self
            .history_cell_ids
            .iter()
            .position(|maybe| maybe.map(|stored| stored == id).unwrap_or(false))
        {
            return Some(idx);
        }

        self.history_cells.iter().enumerate().find_map(|(idx, cell)| {
        history_cell::record_from_cell(cell.as_ref())
                .map(|record| record.id() == id)
                .filter(|matched| *matched)
                .map(|_| idx)
        })
    }

    pub(in super::super) fn update_cell_from_record(&mut self, id: HistoryId, record: HistoryRecord) {
        if id == HistoryId::ZERO {
            tracing::debug!("skip update_cell_from_record: zero id");
            return;
        }

        self.history_render.invalidate_history_id(id);

        if let Some(idx) = self.cell_index_for_history_id(id) {
            if let Some(mut rebuilt) = self.build_cell_from_record(&record) {
                Self::assign_history_id_inner(&mut rebuilt, id);
                self.history_cells[idx] = rebuilt;
            } else if let Some(cell_slot) = self.history_cells.get_mut(idx)
                && !Self::hydrate_cell_from_record_inner(cell_slot, &record, &self.config) {
                    Self::assign_history_id_inner(cell_slot, id);
                }

            if idx < self.history_cell_ids.len() {
                self.history_cell_ids[idx] = Some(id);
            }
            self.invalidate_height_cache();
            self.request_redraw();
        } else {
            tracing::warn!(
                "history-state mismatch: unable to locate cell for id {:?}",
                id
            );
        }
    }

    pub(in super::super) fn assign_history_id(&self, cell: &mut Box<dyn HistoryCell>, id: HistoryId) {
        Self::assign_history_id_inner(cell, id);
    }

    pub(in super::super) fn assign_history_id_inner(cell: &mut Box<dyn HistoryCell>, id: HistoryId) {
        if let Some(tool_call) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::ToolCallCell>()
        {
            tool_call.state_mut().id = id;
        } else if let Some(running_tool) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::RunningToolCallCell>()
        {
            running_tool.state_mut().id = id;
        } else if let Some(plan) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::PlanUpdateCell>()
        {
            plan.state_mut().id = id;
        } else if let Some(upgrade) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::UpgradeNoticeCell>()
        {
            upgrade.state_mut().id = id;
        } else if let Some(reasoning) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::CollapsibleReasoningCell>()
        {
            reasoning.set_history_id(id);
        } else if let Some(exec) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::ExecCell>()
        {
            exec.record.id = id;
        } else if let Some(merged) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::MergedExecCell>()
        {
            merged.set_history_id(id);
        } else if let Some(stream) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::StreamingContentCell>()
        {
            stream.state_mut().id = id;
        } else if let Some(assistant) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::AssistantMarkdownCell>()
        {
            assistant.state_mut().id = id;
        } else if let Some(diff) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::DiffCell>()
        {
            diff.record_mut().id = id;
        } else if let Some(image) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::ImageOutputCell>()
        {
            image.record_mut().id = id;
        } else if let Some(patch) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::PatchSummaryCell>()
        {
            patch.record_mut().id = id;
        } else if let Some(explore) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::ExploreAggregationCell>()
        {
            explore.record_mut().id = id;
        } else if let Some(rate_limits) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::RateLimitsCell>()
        {
            rate_limits.record_mut().id = id;
        } else if let Some(plain) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::PlainHistoryCell>()
        {
            plain.state_mut().id = id;
        } else if let Some(wait) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::WaitStatusCell>()
        {
            wait.state_mut().id = id;
        } else if let Some(loading) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::LoadingCell>()
        {
            loading.state_mut().id = id;
        } else if let Some(background) = cell
            .as_any_mut()
            .downcast_mut::<crate::history_cell::BackgroundEventCell>()
        {
            background.state_mut().id = id;
        }
    }

    /// Push a cell using a synthetic global order key at the bottom of the current request.
    pub(crate) fn history_push(&mut self, cell: impl HistoryCell + 'static) {
        #[cfg(debug_assertions)]
        {
            debug_assert!(
                cell.kind() != HistoryCellType::BackgroundEvent,
                "Background events must use push_background_* helpers"
            );
        }
        let key = self.next_internal_key();
        let _ = self.history_insert_with_key_global_tagged(Box::new(cell), key, "epilogue", None);
    }

    pub(in super::super) fn history_insert_plain_state_with_key(
        &mut self,
        state: PlainMessageState,
        key: OrderKey,
        tag: &'static str,
    ) -> usize {
        let cell = crate::history_cell::PlainHistoryCell::from_state(state.clone());
        self.history_insert_with_key_global_tagged(
            Box::new(cell),
            key,
            tag,
            Some(HistoryDomainRecord::Plain(state)),
        )
    }

    pub(crate) fn history_push_plain_state(&mut self, state: PlainMessageState) {
        let key = self.next_internal_key();
        let _ = self.history_insert_plain_state_with_key(state, key, "epilogue");
    }

    pub(in super::super) fn history_push_plain_paragraphs<I, S>(
        &mut self,
        kind: PlainMessageKind,
        lines: I,
    ) where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let role = history_cell::plain_role_for_kind(kind);
        let state = history_cell::plain_message_state_from_paragraphs(kind, role, lines);
        self.history_push_plain_state(state);
    }

    pub(in super::super) fn history_push_diff(&mut self, title: Option<String>, diff_output: String) {
        let record = history_cell::diff_record_from_string(
            title.unwrap_or_default(),
            &diff_output,
        );
        let key = self.next_internal_key();
        let _ = self.history_insert_with_key_global_tagged(
            Box::new(history_cell::DiffCell::from_record(record.clone())),
            key,
            "diff",
            Some(HistoryDomainRecord::Diff(record)),
        );
    }
    /// Insert a background event near the top of the current request so it appears
    /// before imminent provider output (e.g. Exec begin).
    pub(crate) fn insert_background_event_early(&mut self, message: String) {
        let ticket = self.make_background_before_next_output_ticket();
        self.insert_background_event_with_placement(
            message,
            BackgroundPlacement::BeforeNextOutput,
            Some(ticket.next_order()),
        );
    }
    /// Insert a background event using the specified placement semantics.
    pub(crate) fn insert_background_event_with_placement(
        &mut self,
        message: String,
        placement: BackgroundPlacement,
        order: Option<code_core::protocol::OrderMeta>,
    ) {
        if order.is_none() {
            if matches!(placement, BackgroundPlacement::Tail) {
                tracing::error!(
                    target: "code_order",
                    "missing order metadata for tail background event; dropping message"
                );
                return;
            } else {
                tracing::warn!(
                    target: "code_order",
                    "background event without order metadata placement={:?}",
                    placement
                );
            }
        }
        let system_placement = match placement {
            BackgroundPlacement::Tail => SystemPlacement::Tail,
            BackgroundPlacement::BeforeNextOutput => {
                if self.pending_user_prompts_for_next_turn > 0 {
                    SystemPlacement::Early
                } else {
                    SystemPlacement::PrePrompt
                }
            }
        };
        let cell = history_cell::new_background_event(message);
        let record = HistoryDomainRecord::BackgroundEvent(cell.state().clone());
        self.push_system_cell(
            Box::new(cell),
            system_placement,
            None,
            order.as_ref(),
            "background",
            Some(record),
        );
    }

    pub(crate) fn push_background_tail(&mut self, message: impl Into<String>) {
        let ticket = self.make_background_tail_ticket();
        self.insert_background_event_with_placement(
            message.into(),
            BackgroundPlacement::Tail,
            Some(ticket.next_order()),
        );
    }

    pub(crate) fn push_background_before_next_output(&mut self, message: impl Into<String>) {
        let ticket = self.make_background_before_next_output_ticket();
        self.insert_background_event_with_placement(
            message.into(),
            BackgroundPlacement::BeforeNextOutput,
            Some(ticket.next_order()),
        );
    }

    pub(in super::super) fn history_debug(&self, message: impl Into<String>) {
        if !history_cell_logging_enabled() {
            return;
        }
        let message = message.into();
        tracing::trace!(target: "code_history", "{message}");
        if let Some(buffer) = &self.history_debug_events {
            buffer.borrow_mut().push(message);
        }
    }

    pub(in super::super) fn rehydrate_system_order_cache(&mut self, preserved: &[(String, HistoryId)]) {
        let prev = self.system_cell_by_id.len();
        self.system_cell_by_id.clear();

        for (key, hid) in preserved {
            if let Some(idx) = self
                .history_cell_ids
                .iter()
                .position(|maybe| maybe.map(|stored| stored == *hid).unwrap_or(false))
            {
                self.system_cell_by_id.insert(key.clone(), idx);
            }
        }

        self.history_debug(format!(
            "system_order_cache.rehydrate prev={} restored={} entries={}",
            prev,
            preserved.len(),
            self.system_cell_by_id.len()
        ));
    }

    /// Push a cell using a synthetic key at the TOP of the NEXT request.
    pub(in super::super) fn history_push_top_next_req(&mut self, cell: impl HistoryCell + 'static) {
        let key = self.next_req_key_top();
        let _ = self.history_insert_with_key_global_tagged(Box::new(cell), key, "prelude", None);
    }
    pub(in super::super) fn history_replace_with_record(
        &mut self,
        idx: usize,
        mut cell: Box<dyn HistoryCell>,
        record: HistoryDomainRecord,
    ) {
        if idx >= self.history_cells.len() {
            return;
        }

        let record_idx = self
            .record_index_for_cell(idx)
            .unwrap_or_else(|| self.record_index_for_position(idx));

        let mutation = self.history_state.apply_domain_event(HistoryDomainEvent::Replace {
            index: record_idx,
            record,
        });

        if let Some(id) = self.apply_mutation_to_cell(&mut cell, mutation)
            && idx < self.history_cell_ids.len() {
                self.history_cell_ids[idx] = Some(id);
            }

        self.ensure_image_cell_picker(cell.as_ref());
        self.history_cells[idx] = cell;
        self.invalidate_height_cache();
        self.request_redraw();
        self.refresh_explore_trailing_flags();
        self.mark_history_dirty();
    }

    pub(in super::super) fn history_replace_at(&mut self, idx: usize, mut cell: Box<dyn HistoryCell>) {
        if idx >= self.history_cells.len() {
            return;
        }

        let old_id = self.history_cell_ids.get(idx).and_then(|id| *id);
        let record = history_cell::record_from_cell(cell.as_ref());
        let mut maybe_id = None;

        match (record.map(HistoryDomainRecord::from), self.record_index_for_cell(idx)) {
            (Some(record), Some(record_idx)) => {
                let mutation = self
                    .history_state
                    .apply_domain_event(HistoryDomainEvent::Replace {
                        index: record_idx,
                        record,
                    });
                if let Some(id) = self.apply_mutation_to_cell(&mut cell, mutation) {
                    maybe_id = Some(id);
                }
            }
            (Some(record), None) => {
                let record_idx = self.record_index_for_position(idx);
                let mutation = self
                    .history_state
                    .apply_domain_event(HistoryDomainEvent::Insert {
                        index: record_idx,
                        record,
                    });
                if let Some(id) = self.apply_mutation_to_cell(&mut cell, mutation) {
                    maybe_id = Some(id);
                }
            }
            (None, Some(record_idx)) => {
                let _ = self
                    .history_state
                    .apply_domain_event(HistoryDomainEvent::Remove { index: record_idx });
            }
            (None, None) => {}
        }

        self.ensure_image_cell_picker(cell.as_ref());
        self.history_cells[idx] = cell;
        if idx < self.history_cell_ids.len() {
            self.history_cell_ids[idx] = maybe_id;
        }
        if let Some(id) = old_id {
            self.history_render.invalidate_history_id(id);
        } else {
            self.history_render.invalidate_prefix_only();
        }
        if let Some(id) = maybe_id
            && Some(id) != old_id {
                self.history_render.invalidate_history_id(id);
            }
        self.mark_render_requests_dirty();
        self.history_prefix_append_only.set(false);
        self.request_redraw();
        self.refresh_explore_trailing_flags();
        // Keep debug info for this cell index as-is.
        self.mark_history_dirty();
    }

    pub(in super::super) fn history_remove_at(&mut self, idx: usize) {
        if idx >= self.history_cells.len() {
            return;
        }

        let removed_id = self.history_cell_ids.get(idx).and_then(|id| *id);
        if let Some(record_idx) = self.record_index_for_cell(idx) {
            let _ = self
                .history_state
                .apply_domain_event(HistoryDomainEvent::Remove { index: record_idx });
        }

        self.history_cells.remove(idx);
        if idx < self.history_cell_ids.len() {
            self.history_cell_ids.remove(idx);
        }
        if idx < self.cell_order_seq.len() {
            self.cell_order_seq.remove(idx);
        }
        if idx < self.cell_order_dbg.len() {
            self.cell_order_dbg.remove(idx);
        }
        if let Some(id) = removed_id {
            self.history_render.invalidate_history_id(id);
        } else {
            self.history_render.invalidate_prefix_only();
        }
        self.mark_render_requests_dirty();
        self.history_prefix_append_only.set(false);
        self.request_redraw();
        self.refresh_explore_trailing_flags();
        self.mark_history_dirty();
    }

    pub(in super::super) fn history_replace_and_maybe_merge(&mut self, idx: usize, cell: Box<dyn HistoryCell>) {
        // Replace at index, then attempt standard exec merge with previous cell.
        self.history_replace_at(idx, cell);
        // Merge only if the new cell is an Exec with output (completed) or a MergedExec.
        crate::chatwidget::exec_tools::try_merge_completed_exec_at(self, idx);
    }

    // Merge adjacent tool cells with the same header (e.g., successive Web Search blocks)
    #[allow(dead_code)]
    pub(in super::super) fn history_maybe_merge_tool_with_previous(&mut self, idx: usize) {
        if idx == 0 || idx >= self.history_cells.len() {
            return;
        }
        let new_lines = self.history_cells[idx].display_lines();
        let new_header = new_lines
            .first()
            .and_then(|l| l.spans.first())
            .map(|s| s.content.clone().to_string())
            .unwrap_or_default();
        if new_header.is_empty() {
            return;
        }
        let prev_lines = self.history_cells[idx - 1].display_lines();
        let prev_header = prev_lines
            .first()
            .and_then(|l| l.spans.first())
            .map(|s| s.content.clone().to_string())
            .unwrap_or_default();
        if new_header != prev_header {
            return;
        }
        let mut combined = prev_lines.clone();
        while combined
            .last()
            .map(|l| crate::render::line_utils::is_blank_line_trim(l))
            .unwrap_or(false)
        {
            combined.pop();
        }
        let mut body: Vec<ratatui::text::Line<'static>> = new_lines.into_iter().skip(1).collect();
        while body
            .first()
            .map(|l| crate::render::line_utils::is_blank_line_trim(l))
            .unwrap_or(false)
        {
            body.remove(0);
        }
        while body
            .last()
            .map(|l| crate::render::line_utils::is_blank_line_trim(l))
            .unwrap_or(false)
        {
            body.pop();
        }
        if let Some(first_line) = body.first_mut()
            && let Some(first_span) = first_line.spans.get_mut(0)
                && (first_span.content == "  └ " || first_span.content == "└ ") {
                    first_span.content = "  ".into();
                }
        combined.extend(body);
        let state = history_cell::plain_message_state_from_lines(
            combined,
            crate::history_cell::HistoryCellType::Plain,
        );
        self.history_replace_with_record(
            idx - 1,
            Box::new(crate::history_cell::PlainHistoryCell::from_state(state.clone())),
            HistoryDomainRecord::Plain(state),
        );
        self.history_remove_at(idx);
    }

    pub(in super::super) fn record_index_for_position(&self, ui_index: usize) -> usize {
        if let Some(Some(id)) = self.history_cell_ids.get(ui_index)
            && let Some(idx) = self.history_state.index_of(*id) {
                return idx;
            }
        self.history_cell_ids
            .iter()
            .take(ui_index)
            .filter(|entry| entry.is_some())
            .count()
    }

    pub(in super::super) fn record_index_for_cell(&self, idx: usize) -> Option<usize> {
        self.history_cell_ids
            .get(idx)
            .and_then(|entry| entry.map(|_| self.record_index_for_position(idx)))
    }

    /// Clean up faded-out animation cells
    pub(in super::super) fn process_animation_cleanup(&mut self) {
        // With trait-based cells, we can't easily detect and clean up specific cell types
        // Animation cleanup is now handled differently
    }

    pub(in super::super) fn refresh_explore_trailing_flags(&mut self) -> bool {
        let mut updated = false;
        for idx in 0..self.history_cells.len() {
            let is_explore = self.history_cells[idx]
                .as_any()
                .downcast_ref::<history_cell::ExploreAggregationCell>()
                .is_some();
            if !is_explore {
                continue;
            }

            let hold_title = self.rendered_explore_should_hold(idx);

            if let Some(explore_cell) = self.history_cells[idx]
                .as_any_mut()
                .downcast_mut::<history_cell::ExploreAggregationCell>()
                && explore_cell.set_force_exploring_header(hold_title) {
                    updated = true;
                    if let Some(Some(id)) = self.history_cell_ids.get(idx) {
                        self.history_render.invalidate_history_id(*id);
                    }
                }
        }

        if updated {
            self.invalidate_height_cache();
            self.request_redraw();
        }

        updated
    }

    pub(in super::super) fn rendered_explore_should_hold(&self, idx: usize) -> bool {
        if idx >= self.history_cells.len() {
            return true;
        }

        let mut next = idx + 1;
        while next < self.history_cells.len() {
            let cell = &self.history_cells[next];

            if cell.should_remove() {
                next += 1;
                continue;
            }

            match cell.kind() {
                history_cell::HistoryCellType::Reasoning
                | history_cell::HistoryCellType::Loading
                | history_cell::HistoryCellType::PlanUpdate => {
                    next += 1;
                    continue;
                }
                _ => {}
            }

            if cell
                .as_any()
                .downcast_ref::<history_cell::WaitStatusCell>()
                .is_some()
            {
                next += 1;
                continue;
            }

            if self.cell_lines_trimmed_is_empty(next, cell.as_ref()) {
                next += 1;
                continue;
            }

            return false;
        }

        true
    }
}
