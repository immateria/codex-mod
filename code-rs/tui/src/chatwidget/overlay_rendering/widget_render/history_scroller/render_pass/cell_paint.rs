use super::*;

pub(super) struct PaintVisibleCellsArgs<'a> {
    pub history_area: Rect,
    pub content_area: Rect,
    pub request_count: usize,
    pub start_idx: usize,
    pub start_y: u16,
    pub scroll_pos: u16,
    pub visible_slice: &'a [VisibleCell<'a>],
    pub visible_requests_slice: &'a [RenderRequest<'a>],
    pub rendered_cells_from_subset: bool,
    pub ps: &'a [u16],
    pub buf: &'a mut Buffer,
}

impl ChatWidget<'_> {
    pub(super) fn paint_visible_cells_window<'a>(
        &'a self,
        args: PaintVisibleCellsArgs<'a>,
    ) -> (u16, bool) {
        let PaintVisibleCellsArgs {
            history_area,
            content_area,
            request_count,
            start_idx,
            start_y,
            scroll_pos,
            visible_slice,
            visible_requests_slice: _visible_requests_slice,
            rendered_cells_from_subset,
            ps,
            buf,
        } = args;
        let mut screen_y = start_y;
        let mut has_visible_animation = false;
        let spacing = 1u16;
        let history_len = self.history_cells.len();
        const GUTTER_WIDTH: u16 = 4; // icon (2 cols) + gap (2 cols)
        let viewport_bottom = content_area.y.saturating_add(content_area.height);
        let history_right = history_area.x.saturating_add(history_area.width);
        let logging_enabled = history_cell_logging_enabled();

        // Cache theme colors for the gutter symbol coloring closure to avoid
        // repeated RwLock reads + Arc clones per visible cell.
        let c_success = crate::colors::success();
        let c_error = crate::colors::error();
        let c_text = crate::colors::text();
        let c_text_dim = crate::colors::text_dim();
        let c_text_bright = crate::colors::text_bright();
        let c_primary = crate::colors::primary();
        let c_info = crate::colors::info();

        let render_loop_start = if self.perf_state.enabled {
            Some(std::time::Instant::now())
        } else {
            None
        };

        #[cfg(debug_assertions)]
        #[derive(Debug)]
        struct HeightMismatch {
            history_id: HistoryId,
            cached: u16,
            recomputed: u16,
            idx: usize,
            preview: String,
        }

        #[cfg(debug_assertions)]
        let mut height_mismatches: Vec<HeightMismatch> = Vec::new();

        let is_collapsed_reasoning_at = |idx: usize| {
            if idx >= request_count {
                return false;
            }
            if idx < history_len {
                return self.history_cells[idx]
                    .as_any()
                    .downcast_ref::<crate::history_cell::CollapsibleReasoningCell>()
                    .is_some_and(crate::history_cell::CollapsibleReasoningCell::is_collapsed);
            }
            false
        };

        // Running counter for 1-indexed reply numbers. Count assistant
        // cells before the visible window up front, then increment inline.
        let mut reply_counter: usize = self.history_cells[..start_idx]
            .iter()
            .filter(|c| matches!(c.kind(), crate::history_cell::HistoryCellType::Assistant))
            .count();

        // Hoist RefCell borrows outside the per-cell loop to avoid
        // repeated lock/unlock overhead (up to 6 borrow_mut per cell).
        let hovered_action_ref = self.hovered_clickable_action.borrow();
        let mut regions = self.clickable_regions.borrow_mut();
        // Ensure regions has capacity for the visible window to avoid
        // repeated reallocation during the loop. Each cell typically adds
        // 1-3 clickable regions (fold toggle, copy, scroll-to-top).
        let needed = visible_slice.len().saturating_mul(3);
        let current_cap = regions.capacity();
        if current_cap < needed {
            regions.reserve(needed - current_cap);
        }

        for (offset, visible) in visible_slice.iter().enumerate() {
            let idx = start_idx + offset;
            let Some(item) = visible.cell else {
                continue;
            };

            let item_kind = item.kind();
            let content_width = content_area.width.saturating_sub(GUTTER_WIDTH);

            // Cache common downcasts to avoid repeated vtable lookups.
            let cached_assistant: Option<&crate::history_cell::AssistantMarkdownCell> =
                if matches!(item_kind, crate::history_cell::HistoryCellType::Assistant) {
                    item.as_any().downcast_ref()
                } else {
                    None
                };
            let cached_exec: Option<&crate::history_cell::ExecCell> = match item_kind {
                crate::history_cell::HistoryCellType::Exec { .. } => item.as_any().downcast_ref(),
                _ => None,
            };
            let cached_reasoning: Option<&crate::history_cell::CollapsibleReasoningCell> =
                match item_kind {
                    crate::history_cell::HistoryCellType::Reasoning => {
                        item.as_any().downcast_ref()
                    }
                    _ => None,
                };

            // Accumulate animation flag to avoid a separate full-scan pass.
            has_visible_animation |= item.is_animating();

            // Set reply number on assistant cells so collapsed summaries show "R #N".
            if let Some(assistant) = cached_assistant {
                reply_counter += 1;
                assistant.set_reply_number(reply_counter);
            }

            let mut layout_for_render: Option<Rc<CachedLayout>> = visible
                .layout
                .as_ref()
                .map(super::history_render::LayoutRef::layout);

            let item_height = visible.height;
            #[cfg(debug_assertions)]
            if content_area.width > 0
                && let Some(req) = _visible_requests_slice.get(offset)
                && req.history_id != HistoryId::ZERO
                && matches!(item_kind, history_cell::HistoryCellType::Reasoning)
            {
                if item_height == 0 && content_width == 0 {
                    continue;
                }

                let mut preview: Option<String> = None;
                let fresh = item.desired_height(content_width);
                if fresh != item_height {
                    if preview.is_none() {
                        let lines = item.display_lines_trimmed();
                        if !lines.is_empty() {
                            preview = Some(ChatWidget::reasoning_preview(&lines));
                        }
                    }
                    height_mismatches.push(HeightMismatch {
                        history_id: req.history_id,
                        cached: item_height,
                        recomputed: fresh,
                        idx,
                        preview: preview.unwrap_or_default(),
                    });
                }
            }

            if self.perf_state.enabled
                && rendered_cells_from_subset
                && matches!(visible.height_source, history_render::HeightSource::DesiredHeight)
            {
                let mut p = self.perf_state.stats.borrow_mut();
                p.height_misses_render = p.height_misses_render.saturating_add(1);
                if let Some(ns) = visible.height_measure_ns {
                    let label = self.perf_label_for_item(item);
                    p.record_render((idx, content_width), label.as_str(), ns);
                }
            }

            let content_y = ps[idx];
            let skip_top = scroll_pos.saturating_sub(content_y);
            if screen_y >= viewport_bottom {
                break;
            }

            let available_height = viewport_bottom.saturating_sub(screen_y);
            let visible_height = item_height.saturating_sub(skip_top).min(available_height);

            if visible_height > 0 {
                let gutter_area = Rect {
                    x: content_area.x,
                    y: screen_y,
                    width: GUTTER_WIDTH.min(content_area.width),
                    height: visible_height,
                };

                let item_area = Rect {
                    x: content_area.x + GUTTER_WIDTH.min(content_area.width),
                    y: screen_y,
                    width: content_area.width.saturating_sub(GUTTER_WIDTH),
                    height: visible_height,
                };

                if logging_enabled {
                    let is_streaming = item
                        .as_any()
                        .downcast_ref::<crate::history_cell::StreamingContentCell>()
                        .is_some();
                    let row_start = item_area.y;
                    let row_end = item_area
                        .y
                        .saturating_add(visible_height)
                        .saturating_sub(1);
                    let cache_hit = layout_for_render.is_some();
                    tracing::info!(
                        target: "code_tui::history_cells",
                        idx,
                        kind = ?item_kind,
                        row_start,
                        row_end,
                        height = visible_height,
                        width = item_area.width,
                        skip_rows = skip_top,
                        item_height,
                        content_y,
                        cache_hit,
                        assistant = cached_assistant.is_some(),
                        streaming = is_streaming,
                        custom = item.has_custom_render(),
                        animating = item.is_animating(),
                        "history cell render",
                    );
                }

                let is_assistant = matches!(item_kind, crate::history_cell::HistoryCellType::Assistant);
                let is_auto_review = ChatWidget::is_auto_review_cell(item);
                let auto_review_bg = crate::history_cell::PlainHistoryCell::auto_review_bg();
                let gutter_bg = if is_assistant {
                    crate::colors::assistant_bg()
                } else if is_auto_review {
                    auto_review_bg
                } else {
                    crate::colors::background()
                };

                if (is_assistant || is_auto_review) && gutter_area.width > 0 && gutter_area.height > 0 {
                    let _perf_gutter_start = if self.perf_state.enabled {
                        Some(std::time::Instant::now())
                    } else {
                        None
                    };
                    let style = Style::default().bg(gutter_bg);
                    let mut tint_x = gutter_area.x;
                    let mut tint_width = gutter_area.width;
                    if content_area.x > history_area.x {
                        tint_x = content_area.x.saturating_sub(1);
                        tint_width = tint_width.saturating_add(1);
                    }
                    let tint_rect = Rect::new(tint_x, gutter_area.y, tint_width, gutter_area.height);
                    fill_bg(buf, tint_rect, style);
                    let right_col_x = content_area.x.saturating_add(content_area.width);
                    if right_col_x < history_right {
                        let right_rect = Rect::new(right_col_x, item_area.y, 1, item_area.height);
                        fill_bg(buf, right_rect, style);
                    }
                    if let Some(t0) = _perf_gutter_start {
                        let dt = t0.elapsed().as_nanos();
                        let mut p = self.perf_state.stats.borrow_mut();
                        p.ns_gutter_paint = p.ns_gutter_paint.saturating_add(dt);
                        let area_cells: u64 =
                            (gutter_area.width as u64).saturating_mul(gutter_area.height as u64);
                        p.cells_gutter_paint = p.cells_gutter_paint.saturating_add(area_cells);
                    }
                }

                let base_symbol = item.gutter_symbol();
                let parent_call_id = item.parent_call_id();
                let (left_symbol, right_symbol) = if parent_call_id.is_some() {
                    (Some("↳"), base_symbol)
                } else {
                    (base_symbol, None)
                };
                let color_for_symbol = |symbol: &str| {
                    if is_auto_review {
                        c_success
                    } else if symbol == "↳" {
                        match item_kind {
                            crate::history_cell::HistoryCellType::Tool { status } => match status {
                                crate::history_cell::ToolCellStatus::Running => c_info,
                                crate::history_cell::ToolCellStatus::Success => c_success,
                                crate::history_cell::ToolCellStatus::Failed => c_error,
                            },
                            crate::history_cell::HistoryCellType::Exec {
                                kind: crate::history_cell::ExecKind::Run,
                                status: _,
                            } => {
                                if let Some(exec) = cached_exec {
                                    match &exec.output {
                                        None => c_text,
                                        Some(o) if o.exit_code == 0 => c_text,
                                        Some(_) => c_error,
                                    }
                                } else {
                                    c_text
                                }
                            }
                            crate::history_cell::HistoryCellType::Patch { kind } => match kind {
                                crate::history_cell::PatchKind::ApplySuccess => c_success,
                                crate::history_cell::PatchKind::ApplyBegin => c_success,
                                crate::history_cell::PatchKind::Proposed => c_primary,
                                crate::history_cell::PatchKind::ApplyFailure => c_error,
                            },
                            _ => c_text_dim,
                        }
                    } else if crate::icons::is_exec_prompt(symbol) {
                        if let Some(exec) = cached_exec {
                            match &exec.output {
                                None => c_text,
                                Some(o) if o.exit_code == 0 => c_text,
                                Some(_) => c_error,
                            }
                        } else {
                            match item_kind {
                                crate::history_cell::HistoryCellType::Exec {
                                    kind: crate::history_cell::ExecKind::Run,
                                    status: crate::history::state::ExecStatus::Success,
                                } => c_text,
                                crate::history_cell::HistoryCellType::Exec {
                                    kind: crate::history_cell::ExecKind::Run,
                                    status: crate::history::state::ExecStatus::Error,
                                } => c_error,
                                crate::history_cell::HistoryCellType::Exec { .. } => c_text,
                                _ => c_text,
                            }
                        }
                    } else if crate::icons::is_patch(symbol) {
                        match item_kind {
                            crate::history_cell::HistoryCellType::Patch {
                                kind: crate::history_cell::PatchKind::ApplySuccess,
                            } => c_success,
                            crate::history_cell::HistoryCellType::Patch {
                                kind: crate::history_cell::PatchKind::ApplyBegin,
                            } => c_success,
                            crate::history_cell::HistoryCellType::Patch {
                                kind: crate::history_cell::PatchKind::Proposed,
                            } => c_primary,
                            crate::history_cell::HistoryCellType::Patch {
                                kind: crate::history_cell::PatchKind::ApplyFailure,
                            } => c_error,
                            _ => c_primary,
                        }
                    } else if crate::icons::is_spinner(symbol)
                        && item
                            .as_any()
                            .downcast_ref::<crate::history_cell::RunningToolCallCell>()
                            .is_some_and(|cell| cell.has_title("Waiting"))
                    {
                        c_text_bright
                    } else if crate::icons::is_progress(symbol) {
                        if let Some(plan_cell) = item
                            .as_any()
                            .downcast_ref::<crate::history_cell::PlanUpdateCell>()
                        {
                            if plan_cell.is_complete() {
                                c_success
                            } else {
                                c_info
                            }
                        } else {
                            c_success
                        }
                    } else if crate::icons::is_user(symbol) {
                        c_text
                    } else if symbol == "⋮" {
                        c_primary
                    } else if crate::icons::is_assistant(symbol) {
                        c_text_bright
                    } else if crate::icons::is_running(symbol) {
                        c_info
                    } else if crate::icons::is_success(symbol) {
                        c_success
                    } else if crate::icons::is_failure(symbol) {
                        c_error
                    } else if crate::icons::is_notice(symbol) {
                        c_text_bright
                    } else {
                        c_text_dim
                    }
                };

                // Track where the fold icon was drawn so the click target
                // aligns with the visual affordance.
                let mut fold_icon_y: Option<u16> = None;

                if gutter_area.width >= 2 && (left_symbol.is_some() || right_symbol.is_some()) {
                    let anchor_offset: u16 = match item_kind {
                        crate::history_cell::HistoryCellType::Assistant => 1,
                        _ if is_auto_review => {
                            crate::history_cell::PlainHistoryCell::auto_review_padding().0
                        }
                        _ => 0,
                    };

                    // If the cell header is scrolled out of view (skip_top > anchor_offset),
                    // paint the gutter symbol at the first visible row so jump/fold click
                    // targets stay discoverable on tight viewports.
                    let rel = anchor_offset.saturating_sub(skip_top);
                    let symbol_y = gutter_area.y.saturating_add(rel);
                    if symbol_y < gutter_area.y.saturating_add(gutter_area.height) {
                        let symbol_x = gutter_area.x;
                        if let Some(symbol) = left_symbol {
                            let symbol_style =
                                Style::default().fg(color_for_symbol(symbol)).bg(gutter_bg);
                            buf.set_string(symbol_x, symbol_y, symbol, symbol_style);
                        }
                        if let Some(symbol) = right_symbol {
                            let symbol_style =
                                Style::default().fg(color_for_symbol(symbol)).bg(gutter_bg);
                            buf.set_string(
                                symbol_x.saturating_add(1),
                                symbol_y,
                                symbol,
                                symbol_style,
                            );
                        }
                        if let Some(parent_call_id) = parent_call_id {
                            regions.push(
                                crate::chatwidget::ClickableRegion {
                                    rect: Rect::new(
                                        symbol_x,
                                        symbol_y,
                                        gutter_area.width.min(2),
                                        1,
                                    ),
                                    action: crate::chatwidget::ClickableAction::JumpToCallId(
                                        parent_call_id.to_string(),
                                    ),
                                },
                            );
                        }
                        // Show fold indicator for foldable cells.
                        if item.is_fold_toggleable() {
                            let collapsed = item.is_collapsed();
                            let fold_icon = if collapsed {
                                crate::icons::collapse_closed()
                            } else {
                                crate::icons::collapse_open()
                            };
                            let fold_style = Style::default()
                                .fg(crate::colors::text_dim())
                                .bg(gutter_bg);
                            if collapsed {
                                // Collapsed: show ▶ in the gutter gap (after the icon, same row).
                                let gap_x = symbol_x.saturating_add(2);
                                if gap_x < gutter_area.x.saturating_add(gutter_area.width) {
                                    buf.set_string(gap_x, symbol_y, fold_icon, fold_style);
                                }
                                fold_icon_y = Some(symbol_y);
                            } else if visible_height > 1 {
                                // Expanded: show ▼ on the row below the gutter symbol.
                                let fold_y = symbol_y.saturating_add(1);
                                if fold_y < gutter_area.y.saturating_add(gutter_area.height) {
                                    buf.set_string(symbol_x, fold_y, fold_icon, fold_style);
                                    fold_icon_y = Some(fold_y);
                                }
                            }
                        }
                    }
                }

                let skip_rows = skip_top;
                let is_animating = item.is_animating();
                let has_custom = item.has_custom_render();
                if is_animating || has_custom {
                    tracing::debug!(
                        ">>> RENDERING ANIMATION Cell[{}]: area={:?}, skip_rows={}",
                        idx,
                        item_area,
                        skip_rows
                    );
                }

                let mut handled_assistant = false;
                if let Some(plan) = visible.assistant_plan.as_ref()
                    && let Some(assistant) = cached_assistant
                    && !assistant.is_collapsed()
                {
                    if skip_rows < plan.total_rows() && item_area.height > 0 {
                        assistant.render_with_layout(plan.as_ref(), item_area, buf, skip_rows);
                    }
                    handled_assistant = true;
                    layout_for_render = None;
                }

                if !handled_assistant {
                    if let Some(layout_rc) = layout_for_render.as_ref() {
                        self.render_cached_lines(
                            item,
                            layout_rc.as_ref(),
                            item_area,
                            buf,
                            skip_rows,
                        );
                    } else {
                        item.render_with_skip(item_area, buf, skip_rows);
                    }
                }

                if let Some(js_cell) = item
                    .as_any()
                    .downcast_ref::<crate::history_cell::JsReplCell>()
                    && let Some((call_id, line_idx, start_col, width)) =
                        js_cell.spawned_click_target(item_area.width)
                {
                    let rel = line_idx.saturating_sub(skip_rows as usize);
                    if rel < visible_height as usize
                        && start_col < item_area.width
                        && width > 0
                    {
                        let x = item_area.x.saturating_add(start_col);
                        let y = item_area.y.saturating_add(rel as u16);
                        let w = width.min(item_area.width.saturating_sub(start_col));
                        if w > 0 {
                            regions.push(
                                crate::chatwidget::ClickableRegion {
                                    rect: Rect::new(x, y, w, 1),
                                    action: crate::chatwidget::ClickableAction::JumpToCallId(
                                        call_id,
                                    ),
                                },
                            );
                        }
                    }
                }

                // Register fold toggle click target for foldable cells.
                // The target covers the gutter plus generous content width,
                // anchored at the fold icon's actual screen row so the click
                // target aligns with the visual ▶/▼ indicator.
                if item.is_fold_toggleable() && visible_height > 0 {
                    let gutter_x = content_area.x;
                    let fold_click_width = GUTTER_WIDTH.saturating_add(
                        item_area.width.min(20)
                    );
                    // Anchor the click region at the fold icon row when known,
                    // otherwise fall back to the first visible row.
                    let click_y = fold_icon_y.unwrap_or(item_area.y);
                    let max_y = item_area.y.saturating_add(visible_height);
                    let fold_click_height = max_y.saturating_sub(click_y).clamp(1, 2);
                    regions.push(
                        crate::chatwidget::ClickableRegion {
                            rect: Rect::new(
                                gutter_x,
                                click_y,
                                fold_click_width,
                                fold_click_height,
                            ),
                            action: crate::chatwidget::ClickableAction::ToggleFoldAtIndex(idx),
                        },
                    );
                }

                // Background events can be noisy (e.g. transient MCP failures); let the user
                // dismiss them with a small close affordance.
                if matches!(
                    item_kind,
                    crate::history_cell::HistoryCellType::BackgroundEvent
                ) && visible_height > 0
                    && item_area.width >= 3
                {
                    let label = crate::icons::dismiss();
                    let label_width = unicode_width::UnicodeWidthStr::width(label) as u16;
                    let x = item_area
                        .x
                        .saturating_add(item_area.width.saturating_sub(label_width));
                    let y = item_area.y;
                    let action = crate::chatwidget::ClickableAction::DismissHistoryCellAtIndex(idx);
                    let hovered = hovered_action_ref.as_ref() == Some(&action);
                    let style = if hovered {
                        Style::default()
                            .bg(crate::colors::background())
                            .fg(crate::colors::error())
                    } else {
                        Style::default()
                            .bg(crate::colors::background())
                            .fg(crate::colors::text_dim())
                    };
                    buf.set_string(x, y, label, style);
                    regions.push(
                        crate::chatwidget::ClickableRegion {
                            rect: Rect::new(x, y, label_width, 1),
                            action,
                        },
                    );
                }

                // Copy-as-markdown button: only visible when mouse hovers over
                // this cell's area. Pushed down an extra row when the
                // scroll-to-top arrow occupies row 0 (skip_top > 0).
                if visible_height > 2
                    && item_area.width >= 8
                    && item.has_copyable_content()
                {
                    let mouse_in_cell = self.last_mouse_pos.get().is_some_and(|(mx, my)| {
                        mx >= item_area.x
                            && mx < item_area.x.saturating_add(item_area.width)
                            && my >= item_area.y
                            && my < item_area.y.saturating_add(visible_height)
                    });
                    // When the scroll-to-top arrow is visible (skip_top > 0),
                    // push the copy button down by 1 extra row to leave a gap.
                    let copy_offset: u16 = if skip_top > 0 { 2 } else { 1 };
                    let btn_y = item_area.y.saturating_add(copy_offset);
                    let btn_visible = btn_y < item_area.y.saturating_add(visible_height);

                    if mouse_in_cell && btn_visible {
                        let label = crate::icons::copy_content();
                        let label_w = {
                            use unicode_width::UnicodeWidthStr as _;
                            label.width() as u16
                        };
                        // Inset: 2 cols from right.
                        let btn_x = item_area
                            .x
                            .saturating_add(item_area.width)
                            .saturating_sub(label_w + 2);
                        let action = crate::chatwidget::ClickableAction::CopyMarkdownAtIndex(idx);
                        let hovered = hovered_action_ref.as_ref() == Some(&action);
                        let style = if hovered {
                            Style::default()
                                .bg(crate::colors::background())
                                .fg(crate::colors::primary())
                        } else {
                            Style::default()
                                .bg(crate::colors::background())
                                .fg(crate::colors::text_dim())
                        };
                        buf.set_string(btn_x, btn_y, label, style);
                        regions.push(
                            crate::chatwidget::ClickableRegion {
                                rect: Rect::new(btn_x, btn_y, label_w.max(1), 1),
                                action,
                            },
                        );
                    }
                }

                // Scroll-to-top arrow: rendered when the cell's header is
                // scrolled above the viewport (skip_top > 0). Uses the
                // scroll_to_top icon with a subtle highlight background.
                if skip_top > 0 && visible_height > 1 && item_area.width >= 6 {
                    let icon = crate::icons::scroll_to_top();
                    let icon_w = {
                        use unicode_width::UnicodeWidthStr as _;
                        icon.width() as u16
                    };
                    // Right-aligned, 2 cols from the right edge, on the first
                    // visible row of the cell.
                    let px = item_area
                        .x
                        .saturating_add(item_area.width)
                        .saturating_sub(icon_w + 2);
                    let py = item_area.y;
                    let scroll_action = crate::chatwidget::ClickableAction::ScrollToTopOfCell(idx);
                    let scroll_hovered = hovered_action_ref.as_ref() == Some(&scroll_action);
                    let icon_style = if scroll_hovered {
                        Style::default()
                            .fg(crate::colors::primary())
                    } else {
                        Style::default()
                            .fg(crate::colors::text_bright())
                    };
                    buf.set_string(px, py, icon, icon_style);
                    regions.push(
                        crate::chatwidget::ClickableRegion {
                            rect: Rect::new(px, py, icon_w.max(1), 1),
                            action: scroll_action,
                        },
                    );
                }

                if self.show_order_overlay
                    && let Some(Some(info)) = self.cell_order_dbg.get(idx)
                {
                    let mut text = format!("⟦{info}⟧");
                    if let Some(rc) = cached_reasoning {
                        let snap = rc.debug_title_overlay();
                        text.push_str(" | ");
                        text.push_str(&snap);
                    }
                    let style = crate::colors::style_text_dim();
                    let below_y = item_area.y.saturating_add(visible_height);
                    let bottom_y = viewport_bottom;
                    let maxw = item_area.width as usize;
                    let draw_text = {
                        use unicode_width::UnicodeWidthStr as _;
                        if text.width() > maxw {
                            crate::live_wrap::take_prefix_by_width(&text, maxw).0
                        } else {
                            text
                        }
                    };
                    if item_area.width > 0 {
                        if below_y < bottom_y {
                            buf.set_string(item_area.x, below_y, &draw_text, style);
                        } else if item_area.y > content_area.y {
                            let above_y = item_area.y.saturating_sub(1);
                            buf.set_string(item_area.x, above_y, &draw_text, style);
                        }
                    }
                }

                screen_y += visible_height;
            }

            if idx == request_count.saturating_sub(1) {
                let viewport_top = content_area.y;
                let viewport_bottom = content_area.y.saturating_add(content_area.height);
                tracing::debug!(
                    target: "code_tui::scrollback",
                    idx,
                    request_count,
                    content_y,
                    scroll_pos,
                    viewport_top,
                    viewport_bottom,
                    skip_top,
                    item_height,
                    available_height,
                    visible_height,
                    screen_y,
                    spacing,
                    "last visible history cell metrics"
                );
            }

            let mut should_add_spacing = idx < request_count.saturating_sub(1) && visible_height > 0;
            if should_add_spacing {
                let this_is_collapsed_reasoning = cached_reasoning
                    .is_some_and(crate::history_cell::CollapsibleReasoningCell::is_collapsed);
                if this_is_collapsed_reasoning {
                    let prev_is_collapsed_reasoning = idx
                        .checked_sub(1)
                        .is_some_and(is_collapsed_reasoning_at);
                    let next_is_collapsed_reasoning = is_collapsed_reasoning_at(idx + 1);
                    if prev_is_collapsed_reasoning && next_is_collapsed_reasoning {
                        should_add_spacing = false;
                    }
                }
            }
            if should_add_spacing {
                let bottom = viewport_bottom;
                if screen_y < bottom {
                    let spacing_rows = spacing.min(bottom.saturating_sub(screen_y));
                    screen_y = screen_y.saturating_add(spacing_rows);
                }
            }
        }

        // Release hoisted RefCell borrows before post-loop work.
        drop(regions);
        drop(hovered_action_ref);

        #[cfg(debug_assertions)]
        if let Some(first) = height_mismatches.first() {
            for mismatch in &height_mismatches {
                tracing::error!(
                    target: "code_tui::history_cells",
                    history_id = ?mismatch.history_id,
                    idx = mismatch.idx,
                    cached = mismatch.cached,
                    recomputed = mismatch.recomputed,
                    preview = %mismatch.preview,
                    "History cell height mismatch detected; aborting to capture repro",
                );
            }
            panic!(
                "history cell height mismatch ({} cases); first id={:?} cached={} recomputed={} preview={}",
                height_mismatches.len(),
                first.history_id,
                first.cached,
                first.recomputed,
                first.preview
            );
        }

        if let Some(start) = render_loop_start && self.perf_state.enabled {
            let elapsed = start.elapsed().as_nanos();
            let pending_scroll = self.perf_state.pending_scroll_rows.get();
            {
                let mut p = self.perf_state.stats.borrow_mut();
                p.ns_render_loop = p.ns_render_loop.saturating_add(elapsed);
                if pending_scroll > 0 {
                    p.record_scroll_render(pending_scroll, elapsed);
                }
            }
            if pending_scroll > 0 {
                self.perf_state.pending_scroll_rows.set(0);
            }
        }

        (screen_y, has_visible_animation)
    }
}
