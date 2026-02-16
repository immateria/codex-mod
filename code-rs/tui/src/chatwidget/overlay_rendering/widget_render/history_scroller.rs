use super::*;

mod render_pass;
mod scroll_layout;

impl ChatWidget<'_> {
    pub(super) fn render_history_scroller(
        &self,
        history_area: Rect,
        content_area: Rect,
        base_style: Style,
        streaming_cell: Option<crate::history_cell::StreamingContentCell>,
        queued_preview_cells: Vec<crate::history_cell::PlainHistoryCell>,
        buf: &mut Buffer,
    ) {
        self.ensure_render_request_cache();

        let extra_count = (self.active_exec_cell.is_some() as usize)
            .saturating_add(streaming_cell.is_some() as usize)
            .saturating_add(queued_preview_cells.len());
        let request_count = self.history_cells.len().saturating_add(extra_count);

        let mut render_requests_full: Option<Vec<RenderRequest>> = None;

        // Calculate total content height using prefix sums; build if needed
        let spacing = 1u16; // Standard spacing between cells
        const GUTTER_WIDTH: u16 = 2; // Same as in render loop
        let reasoning_visible = self.is_reasoning_shown();
        let cache_width = content_area.width.saturating_sub(GUTTER_WIDTH);

        // Opportunistically clear height cache if width changed
        self.history_render.handle_width_change(cache_width);

        // Perf: count a frame
        if self.perf_state.enabled {
            let mut p = self.perf_state.stats.borrow_mut();
            p.frames = p.frames.saturating_add(1);
        }

        let render_settings = RenderSettings::new(cache_width, self.render_theme_epoch, reasoning_visible);
        self.last_render_settings.set(render_settings);
        if self.history_frozen_count > 0
            && self.history_frozen_width != render_settings.width
            && !self.history_virtualization_sync_pending.get()
        {
            self.history_virtualization_sync_pending.set(true);
            self.app_event_tx.send(AppEvent::SyncHistoryVirtualization);
        }
        let perf_enabled = self.perf_state.enabled;
        let needs_prefix_rebuild =
            self.history_render
                .should_rebuild_prefix(content_area.width, request_count);
        let mut rendered_cells_full: Option<Vec<VisibleCell>> = None;
        if needs_prefix_rebuild {
            if render_requests_full.is_none() {
                let render_request_cache = self.render_request_cache.borrow();
                let mut render_requests = Vec::with_capacity(request_count);
                for (cell, seed) in self
                    .history_cells
                    .iter()
                    .zip(render_request_cache.iter())
                {
                    let assistant = cell
                        .as_any()
                        .downcast_ref::<crate::history_cell::AssistantMarkdownCell>();
                    render_requests.push(RenderRequest {
                        history_id: seed.history_id,
                        cell: Some(cell.as_ref()),
                        assistant,
                        use_cache: seed.use_cache,
                        fallback_lines: seed.fallback_lines.clone(),
                        kind: seed.kind,
                        config: &self.config,
                    });
                }

                if let Some(ref cell) = self.active_exec_cell {
                    render_requests.push(RenderRequest {
                        history_id: HistoryId::ZERO,
                        cell: Some(cell as &dyn HistoryCell),
                        assistant: None,
                        use_cache: false,
                        fallback_lines: None,
                        kind: RenderRequestKind::Legacy,
                        config: &self.config,
                    });
                }

                if let Some(ref cell) = streaming_cell {
                    render_requests.push(RenderRequest {
                        history_id: HistoryId::ZERO,
                        cell: Some(cell as &dyn HistoryCell),
                        assistant: None,
                        use_cache: false,
                        fallback_lines: None,
                        kind: RenderRequestKind::Legacy,
                        config: &self.config,
                    });
                }

                for c in &queued_preview_cells {
                    render_requests.push(RenderRequest {
                        history_id: HistoryId::ZERO,
                        cell: Some(c as &dyn HistoryCell),
                        assistant: None,
                        use_cache: false,
                        fallback_lines: None,
                        kind: RenderRequestKind::Legacy,
                        config: &self.config,
                    });
                }

                if perf_enabled {
                    let mut p = self.perf_state.stats.borrow_mut();
                    p.render_requests_full =
                        p.render_requests_full.saturating_add(render_requests.len() as u64);
                }

                render_requests_full = Some(render_requests);
            }

            let Some(render_requests) = render_requests_full.as_ref() else {
                return;
            };
            let mut used_fast_append = false;
            if self.try_append_prefix_fast(render_requests, render_settings, content_area.width) {
                used_fast_append = true;
                self.history_prefix_append_only.set(true);
            }
            if used_fast_append {
                // Prefix sums already updated; skip the full rebuild path.
                rendered_cells_full = None;
            } else {
            if perf_enabled {
                let mut p = self.perf_state.stats.borrow_mut();
                p.prefix_rebuilds = p.prefix_rebuilds.saturating_add(1);
            }

            let prefix_start = perf_enabled.then(std::time::Instant::now);
            let cells = self.history_render.visible_cells(
                &self.history_state,
                render_requests,
                render_settings,
            );

            let mut prefix: Vec<u16> = Vec::with_capacity(cells.len().saturating_add(1));
            prefix.push(0);
            let mut acc = 0u16;
            let content_width = content_area.width.saturating_sub(GUTTER_WIDTH);
            let mut spacing_ranges: Vec<(u16, u16)> = Vec::new();

            for (idx, vis) in cells.iter().enumerate() {
                let Some(cell) = vis.cell else {
                    continue;
                };
                let line_count = vis.height;
                if self.perf_state.enabled
                    && matches!(vis.height_source, history_render::HeightSource::DesiredHeight)
                {
                    let mut p = self.perf_state.stats.borrow_mut();
                    p.height_misses_render = p.height_misses_render.saturating_add(1);
                    if let Some(ns) = vis.height_measure_ns {
                        let label = self.perf_label_for_item(cell);
                        p.record_render((idx, content_width), label.as_str(), ns);
                    }
                }
                let cell_start = acc;
                acc = acc.saturating_add(line_count);
                let cell_end = acc;

                if cell
                    .as_any()
                    .is::<crate::history_cell::AssistantMarkdownCell>()
                    && line_count >= 2
                {
                    spacing_ranges.push((cell_start, cell_start.saturating_add(1)));
                    spacing_ranges.push((cell_end.saturating_sub(1), cell_end));
                }

                let mut should_add_spacing = idx < cells.len().saturating_sub(1) && line_count > 0;
                if should_add_spacing {
                    let prev_visible_idx = (0..idx).rev().find(|j| cells[*j].height > 0);
                    let next_visible_idx = ((idx + 1)..cells.len()).find(|j| cells[*j].height > 0);

                    if next_visible_idx.is_none() {
                        should_add_spacing = false;
                    } else {
                        let this_collapsed = cell
                            .as_any()
                            .downcast_ref::<crate::history_cell::CollapsibleReasoningCell>()
                            .map(crate::history_cell::CollapsibleReasoningCell::is_collapsed)
                            .unwrap_or(false);
                        if this_collapsed {
                            let prev_collapsed = prev_visible_idx
                                .and_then(|j| cells[j]
                                    .cell
                                    .and_then(|c| {
                                        c.as_any()
                                            .downcast_ref::<crate::history_cell::CollapsibleReasoningCell>()
                                            .map(crate::history_cell::CollapsibleReasoningCell::is_collapsed)
                                    }))
                                .unwrap_or(false);
                            let next_collapsed = next_visible_idx
                                .and_then(|j| cells[j]
                                    .cell
                                    .and_then(|c| {
                                        c.as_any()
                                            .downcast_ref::<crate::history_cell::CollapsibleReasoningCell>()
                                            .map(crate::history_cell::CollapsibleReasoningCell::is_collapsed)
                                    }))
                                .unwrap_or(false);
                            if prev_collapsed && next_collapsed {
                                should_add_spacing = false;
                            }
                        }
                    }
                }
                if should_add_spacing {
                    let spacing_start = acc;
                    acc = acc.saturating_add(spacing);
                    // Track the spacer interval so scroll adjustments can skip over it later.
                    spacing_ranges.push((spacing_start, acc));
                }
                prefix.push(acc);
            }

            let total_height = *prefix.last().unwrap_or(&0);
            if let (true, Some(t0)) = (perf_enabled, prefix_start) {
                let elapsed = t0.elapsed().as_nanos();
                let mut p = self.perf_state.stats.borrow_mut();
                p.ns_total_height = p.ns_total_height.saturating_add(elapsed);
            }
            self.history_render.update_prefix_cache(
                content_area.width,
                prefix,
                total_height,
                cells.len(),
                self.history_cells.len(),
            );
            self.history_render.update_spacing_ranges(spacing_ranges);
            rendered_cells_full = Some(cells);
            self.history_prefix_append_only.set(true);
            }
        }

        if self.history_virtualization_sync_pending.get()
            && !self.history_cells.is_empty()
            && render_settings.width > 0
            && content_area.height > 0
        {
            let prefix_ready = self.history_render.prefix_sums.borrow().len()
                > self.history_cells.len();
            if prefix_ready {
                self.history_virtualization_sync_pending.set(false);
                self.app_event_tx.send(AppEvent::SyncHistoryVirtualization);
            }
        }

        let scroll_layout = self.compute_history_scroll_layout(request_count, content_area);
        self.render_history_visible_window(
            render_pass::VisibleWindowRenderArgs {
                history_area,
                content_area,
                base_style,
                request_count,
                render_settings,
                render_requests_full: render_requests_full.as_ref(),
                rendered_cells_full: rendered_cells_full.as_ref(),
                streaming_cell: &streaming_cell,
                queued_preview_cells: queued_preview_cells.as_slice(),
                layout: scroll_layout,
            },
            buf,
        );
    }
}
