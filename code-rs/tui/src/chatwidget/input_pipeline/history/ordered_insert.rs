use super::super::prelude::*;

impl ChatWidget<'_> {
    /// Briefly show the vertical scrollbar and schedule a redraw to hide it.
    pub(in crate::chatwidget) fn flash_scrollbar(&self) {
        layout_scroll::flash_scrollbar(self);
    }

    pub(in crate::chatwidget) fn ensure_image_cell_picker(&self, cell: &dyn HistoryCell) {
        if let Some(image) = cell
            .as_any()
            .downcast_ref::<crate::history_cell::ImageOutputCell>()
        {
            let picker = self.terminal_info.picker.clone();
            let font_size = self.terminal_info.font_size;
            image.ensure_picker_initialized(picker, font_size);
        }
    }

    pub(in crate::chatwidget) fn history_insert_with_key_global(
        &mut self,
        cell: Box<dyn HistoryCell>,
        key: OrderKey,
    ) -> usize {
        self.history_insert_with_key_global_tagged(cell, key, "untagged", None)
    }

    // Internal: same as above but with a short tag for debug overlays.
    pub(in crate::chatwidget) fn history_insert_with_key_global_tagged(
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

    pub(in crate::chatwidget) fn history_insert_existing_record(
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
}
