use super::*;

mod history_scroller;
mod overlay_stack;
mod pending_command;
mod terminal_overlay;
mod widget_helpers;

impl ChatWidget<'_> {
    pub(super) fn render_widget_ref(&self, area: Rect, buf: &mut Buffer) {
        // Top-level widget render timing
        let _perf_widget_start = if self.perf_state.enabled {
            Some(std::time::Instant::now())
        } else {
            None
        };

        // Safety clear for non-standard mode: keep a stable background even
        // when downstream widgets intentionally skip unchanged regions.
        if !self.standard_terminal_mode {
            let bg_style = Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text());
            fill_rect(buf, area, None, bg_style);
        }

        // Remember full frame size for layout and hit testing.
        self.layout.last_frame_height.set(area.height);
        self.layout.last_frame_width.set(area.width);

        let layout_areas = self.layout_areas(area);
        let status_bar_area = layout_areas.first().copied().unwrap_or(area);
        let history_area = layout_areas.get(1).copied().unwrap_or(area);
        let bottom_pane_area = layout_areas.get(2).copied().unwrap_or(area);
        self.layout
            .last_bottom_reserved_rows
            .set(bottom_pane_area.height);
        self.layout.last_bottom_pane_area.set(bottom_pane_area);

        if !self.standard_terminal_mode {
            self.render_status_bar(status_bar_area, buf);
        }

        if self.standard_terminal_mode {
            ratatui::widgets::WidgetRef::render_ref(&(&self.bottom_pane), bottom_pane_area, buf);
            self.clear_backgrounds_in(buf, bottom_pane_area);
            return;
        }

        let padding = 1u16;
        let content_area = Rect {
            x: history_area.x + padding,
            y: history_area.y,
            width: history_area.width.saturating_sub(padding * 2),
            height: history_area.height,
        };

        self.update_welcome_height_hint(content_area.width, content_area.height);

        let base_style = Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text());
        // Clear the full history viewport once so reused rows/gutters do not
        // retain stale paint from previous frames.
        fill_rect(buf, history_area, Some(' '), base_style);

        let streaming_lines = self
            .live_builder
            .display_rows()
            .into_iter()
            .map(|r| ratatui::text::Line::from(r.text))
            .collect::<Vec<_>>();
        let streaming_cell = if !streaming_lines.is_empty() {
            let state = self.synthesize_stream_state_from_lines(None, &streaming_lines, true);
            Some(history_cell::new_streaming_content(state, &self.config))
        } else {
            None
        };

        let mut queued_preview_cells: Vec<crate::history_cell::PlainHistoryCell> =
            Vec::with_capacity(self.queued_user_messages.len());
        if !self.queued_user_messages.is_empty() {
            for qm in &self.queued_user_messages {
                let state = history_cell::new_queued_user_prompt(qm.display_text.clone());
                queued_preview_cells.push(crate::history_cell::PlainHistoryCell::from_state(state));
            }
        }

        self.render_history_scroller(
            history_area,
            content_area,
            base_style,
            streaming_cell,
            queued_preview_cells,
            buf,
        );

        self.render_terminal_overlay_and_bottom_pane(area, history_area, bottom_pane_area, buf);
        self.render_overlay_stack(area, history_area, bottom_pane_area, buf);

        if let Some(t0) = _perf_widget_start {
            let dt = t0.elapsed().as_nanos();
            let mut p = self.perf_state.stats.borrow_mut();
            p.ns_widget_render_total = p.ns_widget_render_total.saturating_add(dt);
        }
    }
}
