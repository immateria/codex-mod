use super::super::prelude::*;

impl ChatWidget<'_> {
    /// Push a cell using a synthetic key at the TOP of the NEXT request.
    pub(in crate::chatwidget) fn history_push_top_next_req(&mut self, cell: impl HistoryCell + 'static) {
        let key = self.next_req_key_top();
        let _ = self.history_insert_with_key_global_tagged(Box::new(cell), key, "prelude", None);
    }
    pub(in crate::chatwidget) fn history_replace_with_record(
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

    pub(in crate::chatwidget) fn history_replace_at(&mut self, idx: usize, mut cell: Box<dyn HistoryCell>) {
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

    pub(in crate::chatwidget) fn history_remove_at(&mut self, idx: usize) {
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

    pub(in crate::chatwidget) fn history_replace_and_maybe_merge(&mut self, idx: usize, cell: Box<dyn HistoryCell>) {
        // Replace at index, then attempt standard exec merge with previous cell.
        self.history_replace_at(idx, cell);
        // Merge only if the new cell is an Exec with output (completed) or a MergedExec.
        crate::chatwidget::exec_tools::try_merge_completed_exec_at(self, idx);
    }

    // Merge adjacent tool cells with the same header (e.g., successive Web Search blocks)
    #[allow(dead_code)]
    pub(in crate::chatwidget) fn history_maybe_merge_tool_with_previous(&mut self, idx: usize) {
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

    pub(in crate::chatwidget) fn record_index_for_position(&self, ui_index: usize) -> usize {
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

    pub(in crate::chatwidget) fn record_index_for_cell(&self, idx: usize) -> Option<usize> {
        self.history_cell_ids
            .get(idx)
            .and_then(|entry| entry.map(|_| self.record_index_for_position(idx)))
    }
}
