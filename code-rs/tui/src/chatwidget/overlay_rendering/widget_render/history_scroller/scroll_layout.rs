use super::*;

#[derive(Clone, Copy)]
pub(super) struct HistoryScrollLayout {
    pub total_height: u16,
    pub start_y: u16,
    pub scroll_pos: u16,
}

impl ChatWidget<'_> {
    pub(super) fn compute_history_scroll_layout(
        &self,
        request_count: usize,
        content_area: Rect,
    ) -> HistoryScrollLayout {
        let mut total_height = self.history_render.last_total_height();
        let base_total_height = total_height;
        let viewport_rows = content_area.height;
        let mut requested_spacer_lines = 0u16;
        let mut remainder_for_log: Option<u16> = None;

        if total_height > 0 && viewport_rows > 0 && request_count > 0
            && base_total_height > viewport_rows {
                let remainder = base_total_height % viewport_rows;
                remainder_for_log = Some(remainder);
                if remainder == 0 {
                    requested_spacer_lines = if base_total_height == viewport_rows { 1 } else { 2 };
                } else if remainder <= 2 || remainder >= viewport_rows.saturating_sub(2) {
                    requested_spacer_lines = 1;
                }
            }

        let composer_rows = self.layout.last_bottom_reserved_rows.get();
        let ensure_footer_space = self.layout.scroll_offset.get() == 0
            && composer_rows > 0
            && base_total_height == viewport_rows
            && request_count > 0;
        if ensure_footer_space {
            // The command composer/header consumes several rows at the bottom of
            // the frame. When the history fits exactly, keep a small overscan
            // buffer so the last content row is never flush against the bottom.
            requested_spacer_lines = requested_spacer_lines.max(4);
        }

        let (spacer_lines, spacer_pending_shrink) = self
            .history_render
            .select_bottom_spacer_lines(requested_spacer_lines);

        if spacer_pending_shrink {
            // Force a follow-up frame so the spacer can settle back to the newly
            // requested height even if no additional history events arrive. Without
            // this, we'd keep the stale overscan row on-screen until the user types
            // or resizes the window again.
            self.app_event_tx.send(AppEvent::ScheduleFrameIn(
                HISTORY_ANIMATION_FRAME_INTERVAL,
            ));
        }

        if spacer_lines > 0 {
            total_height = total_height.saturating_add(spacer_lines);
            self.history_render
                .set_bottom_spacer_range(Some((base_total_height, total_height)));
            tracing::debug!(
                target: "code_tui::history_render",
                lines = spacer_lines,
                base_height = base_total_height,
                padded_height = total_height,
                viewport = viewport_rows,
                remainder = remainder_for_log,
                footer_padding = ensure_footer_space,
                "history overscan: adding bottom spacer",
            );
        } else {
            self.history_render.set_bottom_spacer_range(None);
        }
        let overscan_extra = total_height.saturating_sub(base_total_height);
        // Calculate scroll position and vertical alignment
        // Preserve a stable viewport anchor when history grows while the user is scrolled up.
        let prev_viewport_h = self.layout.last_history_viewport_height.get();
        let prev_max_scroll = self.layout.last_max_scroll.get();
        let prev_scroll_offset = self.layout.scroll_offset.get().min(prev_max_scroll);
        let prev_scroll_from_top = prev_max_scroll.saturating_sub(prev_scroll_offset);
        if prev_viewport_h == 0 {
            // Initialize on first render
            self.layout
                .last_history_viewport_height
                .set(content_area.height);
        }

        let (start_y, scroll_pos) = if total_height <= content_area.height {
            // Content fits - always align to bottom so "Popular commands" stays at the bottom
            let start_y = content_area.y + content_area.height.saturating_sub(total_height);
            // Update last_max_scroll cache
            self.layout.last_max_scroll.set(0);
            (start_y, 0u16) // No scrolling needed
        } else {
            // Content overflows - calculate scroll position
            // scroll_offset is measured from the bottom (0 = bottom/newest)
            // Convert to distance from the top for rendering math.
            let max_scroll = total_height.saturating_sub(content_area.height);
            if self.layout.scroll_offset.get() > 0 && max_scroll != prev_max_scroll {
                // If the user has scrolled up and the history height changes (e.g. new output
                // arrives while streaming), keep the same content anchored at the top of the
                // viewport by adjusting our bottom-anchored scroll offset.
                self.layout
                    .scroll_offset
                    .set(max_scroll.saturating_sub(prev_scroll_from_top));
            }

            // Update cache and clamp for display only.
            self.layout.last_max_scroll.set(max_scroll);
            let clamped_scroll_offset = self.layout.scroll_offset.get().min(max_scroll);
            let mut scroll_from_top = max_scroll.saturating_sub(clamped_scroll_offset);

            if overscan_extra > 0 && clamped_scroll_offset == 0 {
                scroll_from_top = scroll_from_top.saturating_sub(overscan_extra);
            }

            if clamped_scroll_offset == 0 && content_area.height == 1 {
                scroll_from_top = self
                    .history_render
                    .adjust_scroll_to_content(scroll_from_top);
            }

            // NOTE: when pinned to the bottom, avoid guessing at cell-internal padding.
            // Only skip known spacer intervals recorded by the history render cache.

            tracing::debug!(
                target: "code_tui::scrollback",
                total_height,
                base_total_height,
                viewport = content_area.height,
                overscan_extra,
                max_scroll,
                scroll_offset = clamped_scroll_offset,
                initial_scroll_from_top = scroll_from_top,
                "scrollback pre-adjust scroll position",
            );

            // If our scroll origin landed on a spacer row between cells, nudge it up so
            // the viewport starts with real content instead of an empty separator.
            let scroll_pos = if clamped_scroll_offset > 0 {
                let adjusted = self
                    .history_render
                    .adjust_scroll_to_content(scroll_from_top);
                tracing::debug!(
                    target: "code_tui::scrollback",
                    adjusted_scroll_from_top = adjusted,
                    scroll_from_top,
                    "scrollback adjusted scroll position",
                );
                adjusted
            } else {
                scroll_from_top
            };

            (content_area.y, scroll_pos)
        };

        // Record current viewport height for the next frame
        self.layout
            .last_history_viewport_height
            .set(content_area.height);
        HistoryScrollLayout {
            total_height,
            start_y,
            scroll_pos,
        }
    }
}
