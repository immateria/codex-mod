use super::*;
use super::scroll_layout::HistoryScrollLayout;

mod cell_paint;
mod post_paint;
mod window_selection;

pub(super) struct VisibleWindowRenderArgs<'a> {
    pub history_area: Rect,
    pub content_area: Rect,
    pub base_style: Style,
    pub request_count: usize,
    pub render_settings: RenderSettings,
    pub render_requests_full: Option<&'a Vec<RenderRequest<'a>>>,
    pub rendered_cells_full: Option<&'a Vec<VisibleCell<'a>>>,
    pub streaming_cell: &'a Option<crate::history_cell::StreamingContentCell>,
    pub queued_preview_cells: &'a [crate::history_cell::PlainHistoryCell],
    pub layout: HistoryScrollLayout,
}

impl ChatWidget<'_> {
    pub(super) fn render_history_visible_window<'a>(
        &'a self,
        args: VisibleWindowRenderArgs<'a>,
        buf: &mut Buffer,
    ) {
        let VisibleWindowRenderArgs {
            history_area,
            content_area,
            base_style,
            request_count,
            render_settings,
            render_requests_full,
            rendered_cells_full,
            streaming_cell,
            queued_preview_cells,
            layout,
        } = args;

        let HistoryScrollLayout {
            total_height,
            start_y,
            scroll_pos,
        } = layout;

        let selection = self.build_window_selection(window_selection::WindowSelectionRequest {
            request_count,
            scroll_pos,
            viewport_height: content_area.height,
            render_settings,
            render_requests_full,
            rendered_cells_full,
            streaming_cell,
            queued_preview_cells,
        });

        let visible_requests_slice = selection.visible_requests.as_slice();
        if self.perf_state.enabled {
            let mut p = self.perf_state.stats.borrow_mut();
            p.render_requests_visible = p
                .render_requests_visible
                .saturating_add(visible_requests_slice.len() as u64);
        }

        let visible_slice = selection.visible_cells.as_slice();

        let ps_ref = self.history_render.prefix_sums.borrow();
        let ps: &[u16] = &ps_ref;
        let (screen_y, has_visible_animation) =
            self.paint_visible_cells_window(cell_paint::PaintVisibleCellsArgs {
                history_area,
                content_area,
                request_count,
                start_idx: selection.start_idx,
                start_y,
                scroll_pos,
                visible_slice,
                visible_requests_slice,
                rendered_cells_from_subset: selection.visible_cells.is_owned(),
                ps,
                buf,
            });
        drop(ps_ref);

        // Schedule next frame if any visible cell is animating (flag accumulated
        // during the paint loop to avoid a separate O(n) scan).
        if has_visible_animation && !ChatWidget::auto_reduced_motion_preference() {
            tracing::debug!("Visible animation detected, scheduling next frame");
            self.app_event_tx
                .send(AppEvent::ScheduleFrameIn(HISTORY_ANIMATION_FRAME_INTERVAL));
        }

        self.render_history_post_paint(
            post_paint::PostPaintArgs {
                history_area,
                content_area,
                base_style,
                total_height,
                scroll_pos,
                screen_y,
            },
            buf,
        );

        // Cache geometry for scrollbar hit-testing in mouse handler.
        self.layout.last_history_area.set(history_area);
        self.layout.last_total_height.set(total_height);
    }
}
