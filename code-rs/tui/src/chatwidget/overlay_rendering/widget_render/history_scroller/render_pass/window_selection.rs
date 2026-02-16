use super::*;

pub(super) enum VisibleRequestsWindow<'a> {
    Borrowed(&'a [RenderRequest<'a>]),
    Owned(Vec<RenderRequest<'a>>),
}

impl<'a> VisibleRequestsWindow<'a> {
    pub(super) fn as_slice(&self) -> &[RenderRequest<'a>] {
        match self {
            Self::Borrowed(slice) => slice,
            Self::Owned(vec) => vec.as_slice(),
        }
    }
}

pub(super) enum VisibleCellsWindow<'a> {
    Borrowed(&'a [VisibleCell<'a>]),
    Owned(Vec<VisibleCell<'a>>),
}

impl<'a> VisibleCellsWindow<'a> {
    pub(super) fn as_slice(&self) -> &[VisibleCell<'a>] {
        match self {
            Self::Borrowed(slice) => slice,
            Self::Owned(vec) => vec.as_slice(),
        }
    }

    pub(super) fn is_owned(&self) -> bool {
        matches!(self, Self::Owned(_))
    }
}

pub(super) struct WindowSelection<'a> {
    pub start_idx: usize,
    pub visible_requests: VisibleRequestsWindow<'a>,
    pub visible_cells: VisibleCellsWindow<'a>,
}

pub(super) struct WindowSelectionRequest<'a> {
    pub request_count: usize,
    pub scroll_pos: u16,
    pub viewport_height: u16,
    pub render_settings: RenderSettings,
    pub render_requests_full: Option<&'a Vec<RenderRequest<'a>>>,
    pub rendered_cells_full: Option<&'a Vec<VisibleCell<'a>>>,
    pub streaming_cell: &'a Option<crate::history_cell::StreamingContentCell>,
    pub queued_preview_cells: &'a [crate::history_cell::PlainHistoryCell],
}

impl ChatWidget<'_> {
    pub(super) fn build_window_selection<'a>(
        &'a self,
        request: WindowSelectionRequest<'a>,
    ) -> WindowSelection<'a> {
        let WindowSelectionRequest {
            request_count,
            scroll_pos,
            viewport_height,
            render_settings,
            render_requests_full,
            rendered_cells_full,
            streaming_cell,
            queued_preview_cells,
        } = request;

        let viewport_bottom = scroll_pos.saturating_add(viewport_height);
        let ps_ref = self.history_render.prefix_sums.borrow();
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

        let history_len = self.history_cells.len();
        let visible_requests = if let Some(full_requests) = render_requests_full {
            VisibleRequestsWindow::Borrowed(&full_requests[start_idx..end_idx])
        } else {
            let render_request_cache = self.render_request_cache.borrow();
            let mut requests = Vec::with_capacity(end_idx.saturating_sub(start_idx));
            for idx in start_idx..end_idx {
                if idx < history_len {
                    let cell = &self.history_cells[idx];
                    let seed = &render_request_cache[idx];
                    let assistant = cell
                        .as_any()
                        .downcast_ref::<crate::history_cell::AssistantMarkdownCell>();
                    requests.push(RenderRequest {
                        history_id: seed.history_id,
                        cell: Some(cell.as_ref()),
                        assistant,
                        use_cache: seed.use_cache,
                        fallback_lines: seed.fallback_lines.clone(),
                        kind: seed.kind,
                        config: &self.config,
                    });
                    continue;
                }

                let extra_idx = idx.saturating_sub(history_len);
                let mut extra_cursor = 0usize;
                if let Some(ref cell) = self.active_exec_cell {
                    if extra_idx == extra_cursor {
                        requests.push(RenderRequest {
                            history_id: HistoryId::ZERO,
                            cell: Some(cell as &dyn HistoryCell),
                            assistant: None,
                            use_cache: false,
                            fallback_lines: None,
                            kind: RenderRequestKind::Legacy,
                            config: &self.config,
                        });
                        continue;
                    }
                    extra_cursor = extra_cursor.saturating_add(1);
                }

                if let Some(cell) = streaming_cell {
                    if extra_idx == extra_cursor {
                        requests.push(RenderRequest {
                            history_id: HistoryId::ZERO,
                            cell: Some(cell as &dyn HistoryCell),
                            assistant: None,
                            use_cache: false,
                            fallback_lines: None,
                            kind: RenderRequestKind::Legacy,
                            config: &self.config,
                        });
                        continue;
                    }
                    extra_cursor = extra_cursor.saturating_add(1);
                }

                let queued_idx = extra_idx.saturating_sub(extra_cursor);
                if let Some(cell) = queued_preview_cells.get(queued_idx) {
                    requests.push(RenderRequest {
                        history_id: HistoryId::ZERO,
                        cell: Some(cell as &dyn HistoryCell),
                        assistant: None,
                        use_cache: false,
                        fallback_lines: None,
                        kind: RenderRequestKind::Legacy,
                        config: &self.config,
                    });
                }
            }
            VisibleRequestsWindow::Owned(requests)
        };

        let visible_cells = if let Some(full) = rendered_cells_full {
            VisibleCellsWindow::Borrowed(&full[start_idx..end_idx])
        } else {
            let cells = self.history_render.visible_cells(
                &self.history_state,
                visible_requests.as_slice(),
                render_settings,
            );
            VisibleCellsWindow::Owned(cells)
        };

        WindowSelection {
            start_idx,
            visible_requests,
            visible_cells,
        }
    }
}
