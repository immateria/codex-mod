impl ChatWidget<'_> {
    fn visible_history_cell_range_for_shortcuts(&self) -> Option<(usize, usize)> {
        let viewport_height = self.layout.last_history_viewport_height.get();
        if viewport_height == 0 {
            return None;
        }

        let ps_ref = self.history_render.prefix_sums.borrow();
        if ps_ref.len() < 2 {
            return None;
        }
        let request_count = ps_ref.len().saturating_sub(1);

        let history_len = self.history_cells.len();
        if history_len == 0 || request_count == 0 {
            return None;
        }

        let max_scroll = self.layout.last_max_scroll.get();
        let clamped_offset = self.layout.scroll_offset.get().min(max_scroll);

        // Reproduce the scroll-from-top calculation from the renderer so our
        // "visible" window matches what the user is actually seeing.
        let base_total_height = self.history_render.last_total_height();
        let total_height = if max_scroll > 0 {
            max_scroll.saturating_add(viewport_height)
        } else {
            base_total_height
        };
        let overscan_extra = total_height.saturating_sub(base_total_height);

        let mut scroll_pos = max_scroll.saturating_sub(clamped_offset);
        if overscan_extra > 0 && clamped_offset == 0 {
            scroll_pos = scroll_pos.saturating_sub(overscan_extra);
        }
        if clamped_offset > 0 {
            scroll_pos = self.history_render.adjust_scroll_to_content(scroll_pos);
        }

        let viewport_bottom = scroll_pos.saturating_add(viewport_height);
        let ps: &Vec<u16> = &ps_ref;

        let mut start_idx = match ps.binary_search(&scroll_pos) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        start_idx = start_idx.min(request_count);

        let mut end_idx = match ps.binary_search(&viewport_bottom) {
            Ok(i) => i,
            Err(i) => i,
        };
        end_idx = end_idx.saturating_add(1).min(request_count);
        drop(ps_ref);

        let start = start_idx.min(history_len);
        let end = end_idx.min(history_len);
        (start < end).then_some((start, end))
    }

    fn toggle_bottommost_exec_fold(&mut self) {
        use crate::history_cell::{
            ExecCell,
            JsReplCell,
            RunningToolCallCell,
            ToolCallCell,
            WebFetchToolCell,
        };

        let (start, end) = self
            .visible_history_cell_range_for_shortcuts()
            .unwrap_or((0, self.history_cells.len()));
        for idx in (start..end).rev() {
            let cell_box = &self.history_cells[idx];
            let cell = cell_box.as_ref();
            if let Some(exec_cell) = cell.as_any().downcast_ref::<ExecCell>()
                && exec_cell.output.is_some()
            {
                #[cfg(feature = "test-helpers")]
                if std::env::var("CODE_TUI_TEST_MODE").is_ok() {
                    eprintln!("toggle_bottommost_exec_fold: exec idx={idx} call_id={:?}", cell.call_id());
                }
                exec_cell.toggle_output_collapsed();
                self.invalidate_height_cache();
                self.request_redraw();
                return;
            }
            if let Some(js_cell) = cell.as_any().downcast_ref::<JsReplCell>()
                && js_cell.output.is_some()
            {
                #[cfg(feature = "test-helpers")]
                if std::env::var("CODE_TUI_TEST_MODE").is_ok() {
                    eprintln!("toggle_bottommost_exec_fold: js idx={idx} call_id={:?}", cell.call_id());
                }
                js_cell.toggle_output_collapsed();
                self.invalidate_height_cache();
                self.request_redraw();
                return;
            }
            if let Some(tool_cell) = cell.as_any().downcast_ref::<ToolCallCell>() {
                #[cfg(feature = "test-helpers")]
                if std::env::var("CODE_TUI_TEST_MODE").is_ok() {
                    eprintln!("toggle_bottommost_exec_fold: tool idx={idx} call_id={:?}", cell.call_id());
                }
                tool_cell.toggle_details_collapsed();
                self.invalidate_height_cache();
                self.request_redraw();
                return;
            }
            if let Some(tool_cell) = cell.as_any().downcast_ref::<RunningToolCallCell>() {
                #[cfg(feature = "test-helpers")]
                if std::env::var("CODE_TUI_TEST_MODE").is_ok() {
                    eprintln!("toggle_bottommost_exec_fold: running tool idx={idx} call_id={:?}", cell.call_id());
                }
                tool_cell.toggle_details_collapsed();
                self.invalidate_height_cache();
                self.request_redraw();
                return;
            }
            if let Some(web_fetch_cell) = cell.as_any().downcast_ref::<WebFetchToolCell>() {
                #[cfg(feature = "test-helpers")]
                if std::env::var("CODE_TUI_TEST_MODE").is_ok() {
                    eprintln!("toggle_bottommost_exec_fold: web idx={idx} call_id={:?}", cell.call_id());
                }
                web_fetch_cell.toggle_body_collapsed();
                self.invalidate_height_cache();
                self.request_redraw();
                return;
            }
        }
    }

    /// Toggle fold/collapse for a specific history cell by index (used by mouse clicks).
    pub(in crate::chatwidget) fn toggle_fold_at_index(&mut self, idx: usize) {
        use crate::history_cell::{
            ExecCell,
            JsReplCell,
            RunningToolCallCell,
            ToolCallCell,
            WebFetchToolCell,
        };

        let Some(cell_box) = self.history_cells.get(idx) else { return };
        let cell = cell_box.as_ref();
        if let Some(exec_cell) = cell.as_any().downcast_ref::<ExecCell>() {
            exec_cell.toggle_output_collapsed();
        } else if let Some(js_cell) = cell.as_any().downcast_ref::<JsReplCell>() {
            js_cell.toggle_output_collapsed();
        } else if let Some(tool_cell) = cell.as_any().downcast_ref::<ToolCallCell>() {
            tool_cell.toggle_details_collapsed();
        } else if let Some(tool_cell) = cell.as_any().downcast_ref::<RunningToolCallCell>() {
            tool_cell.toggle_details_collapsed();
        } else if let Some(web_fetch_cell) = cell.as_any().downcast_ref::<WebFetchToolCell>() {
            web_fetch_cell.toggle_body_collapsed();
        } else {
            return;
        }
        self.invalidate_height_cache();
        self.request_redraw();
    }

    fn toggle_bottommost_js_repl_code_fold(&mut self) {
        use crate::history_cell::JsReplCell;
        let (start, end) = self
            .visible_history_cell_range_for_shortcuts()
            .unwrap_or((0, self.history_cells.len()));
        for cell_box in self.history_cells[start..end].iter().rev() {
            let cell = cell_box.as_ref();
            if let Some(js_cell) = cell.as_any().downcast_ref::<JsReplCell>() {
                js_cell.toggle_code_collapsed();
                self.invalidate_height_cache();
                self.request_redraw();
                return;
            }
        }
    }
}
