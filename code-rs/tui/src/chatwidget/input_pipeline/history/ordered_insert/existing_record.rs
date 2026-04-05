impl ChatWidget<'_> {
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
            self.cell_order_seq.resize(
                self.history_cells.len(),
                OrderKey {
                    req: 0,
                    out: -1,
                    seq: 0,
                },
            );
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
        let req_dbg = key.req.to_string();
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
