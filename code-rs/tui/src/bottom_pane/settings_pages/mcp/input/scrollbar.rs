use crossterm::event::MouseEvent;

use crate::ui_interaction::{
    vertical_scrollbar_metrics,
    vertical_scrollbar_position_for_thumb_drag,
    vertical_scrollbar_right_hit_test,
    VerticalScrollbarHit,
};

use super::super::{McpPaneHit, McpScrollbarDragState, McpScrollbarTarget, McpSettingsFocus, McpSettingsView, McpViewLayout};

impl McpSettingsView {
    fn set_scrollbar_drag(&mut self, drag: Option<McpScrollbarDragState>) -> bool {
        if self.scrollbar_drag == drag {
            false
        } else {
            self.scrollbar_drag = drag;
            true
        }
    }

    pub(super) fn clear_scrollbar_drag(&mut self) -> bool {
        self.set_scrollbar_drag(None)
    }

    fn begin_scrollbar_drag(&mut self, target: McpScrollbarTarget, offset_in_thumb: usize) -> bool {
        self.set_scrollbar_drag(Some(McpScrollbarDragState {
            target,
            offset_in_thumb,
        }))
    }

    fn tools_set_scroll_top_from_scrollbar(&mut self, scroll_top: usize, viewport_len: usize) {
        let entries_len = self.tool_entries().len();
        if entries_len == 0 {
            self.tools_selected = 0;
            return;
        }
        let half = viewport_len / 2;
        self.tools_selected = scroll_top
            .saturating_add(half)
            .min(entries_len.saturating_sub(1));
    }

    pub(super) fn handle_scrollbar_mouse_down(
        &mut self,
        layout: McpViewLayout,
        mouse_event: MouseEvent,
    ) -> bool {
        let x = mouse_event.column;
        let y = mouse_event.row;

        // In stacked mode, the outer scrollbar sits on the rightmost column of the whole viewport.
        if layout.stacked && layout.stack_max_scroll > 0 {
            let content_length = layout.stack_max_scroll.saturating_add(1);
            let viewport_len = layout.stack_viewport.height as usize;
            if let Some(metrics) = vertical_scrollbar_metrics(
                layout.stack_viewport,
                content_length,
                layout.stack_scroll_top,
                viewport_len,
            ) && let Some(hit) =
                vertical_scrollbar_right_hit_test(layout.stack_viewport, x, y, metrics)
            {
                match hit {
                    VerticalScrollbarHit::BeginArrow => {
                        self.stacked_scroll_top = layout.stack_scroll_top.saturating_sub(1);
                        return true;
                    }
                    VerticalScrollbarHit::EndArrow => {
                        self.stacked_scroll_top =
                            (layout.stack_scroll_top + 1).min(layout.stack_max_scroll);
                        return true;
                    }
                    VerticalScrollbarHit::TrackAboveThumb => {
                        self.stacked_scroll_top =
                            layout.stack_scroll_top.saturating_sub(viewport_len);
                        return true;
                    }
                    VerticalScrollbarHit::TrackBelowThumb => {
                        self.stacked_scroll_top = layout
                            .stack_scroll_top
                            .saturating_add(viewport_len)
                            .min(layout.stack_max_scroll);
                        return true;
                    }
                    VerticalScrollbarHit::Thumb { offset_in_thumb } => {
                        self.begin_scrollbar_drag(
                            McpScrollbarTarget::Stacked,
                            offset_in_thumb,
                        );
                        return true;
                    }
                }
            }
        }

        // Summary scrollbar (details pane).
        let summary_scrollbar_area = layout.summary_scrollbar_area();
        let summary_metrics = self.summary_metrics_for_viewport(layout.summary_inner);
        if summary_metrics.visible_lines > 0
            && summary_metrics.total_lines > summary_metrics.visible_lines
        {
            let max_scroll =
                summary_metrics.total_lines.saturating_sub(summary_metrics.visible_lines);
            let position = self.summary_scroll_top.min(max_scroll);
            let content_length = max_scroll.saturating_add(1);
            if let Some(metrics) = vertical_scrollbar_metrics(
                summary_scrollbar_area,
                content_length,
                position,
                summary_metrics.visible_lines,
            ) && let Some(hit) =
                vertical_scrollbar_right_hit_test(summary_scrollbar_area, x, y, metrics)
            {
                self.set_focus(McpSettingsFocus::Summary);
                self.set_hovered_pane(McpPaneHit::Summary);
                self.clear_tool_hover();
                self.clear_list_hover();
                match hit {
                    VerticalScrollbarHit::BeginArrow => self.scroll_summary_lines(-1),
                    VerticalScrollbarHit::EndArrow => self.scroll_summary_lines(1),
                    VerticalScrollbarHit::TrackAboveThumb => self
                        .scroll_summary_lines(-(summary_metrics.visible_lines as isize)),
                    VerticalScrollbarHit::TrackBelowThumb => self
                        .scroll_summary_lines(summary_metrics.visible_lines as isize),
                    VerticalScrollbarHit::Thumb { offset_in_thumb } => {
                        self.begin_scrollbar_drag(
                            McpScrollbarTarget::Summary,
                            offset_in_thumb,
                        );
                    }
                }
                return true;
            }
        }

        // Tools scrollbar.
        let tools_scrollbar_area = layout.tools_scrollbar_area();
        let entries_len = self.tool_entries().len();
        let viewport_len = layout.tools_inner.height as usize;
        if viewport_len > 0 && entries_len > viewport_len {
            let max_scroll = entries_len.saturating_sub(viewport_len);
            let scroll_top = self
                .tools_scroll_top_for_entries_len(layout.tools_inner.height, entries_len)
                .min(max_scroll);
            let content_length = max_scroll.saturating_add(1);
            if let Some(metrics) = vertical_scrollbar_metrics(
                tools_scrollbar_area,
                content_length,
                scroll_top,
                viewport_len,
            ) && let Some(hit) =
                vertical_scrollbar_right_hit_test(tools_scrollbar_area, x, y, metrics)
            {
                self.set_focus(McpSettingsFocus::Tools);
                self.set_hovered_pane(McpPaneHit::Tools);
                self.clear_list_hover();
                self.clear_server_row_click_arm();
                match hit {
                    VerticalScrollbarHit::BeginArrow => {
                        self.tools_selected = self.tools_selected.saturating_sub(1);
                    }
                    VerticalScrollbarHit::EndArrow => {
                        let len = self.tool_entries().len();
                        if len > 0 {
                            self.tools_selected = (self.tools_selected + 1)
                                .min(len.saturating_sub(1));
                        }
                    }
                    VerticalScrollbarHit::TrackAboveThumb => {
                        self.tools_selected = self.tools_selected.saturating_sub(viewport_len);
                    }
                    VerticalScrollbarHit::TrackBelowThumb => {
                        let len = self.tool_entries().len();
                        if len > 0 {
                            self.tools_selected = self
                                .tools_selected
                                .saturating_add(viewport_len)
                                .min(len.saturating_sub(1));
                        }
                    }
                    VerticalScrollbarHit::Thumb { offset_in_thumb } => {
                        self.begin_scrollbar_drag(
                            McpScrollbarTarget::Tools,
                            offset_in_thumb,
                        );
                    }
                }
                return true;
            }
        }

        false
    }

    pub(super) fn handle_scrollbar_mouse_drag(
        &mut self,
        layout: McpViewLayout,
        mouse_event: MouseEvent,
    ) -> bool {
        let Some(drag) = self.scrollbar_drag else {
            return false;
        };
        let y = mouse_event.row;

        match drag.target {
            McpScrollbarTarget::Stacked => {
                if !layout.stacked || layout.stack_max_scroll == 0 {
                    return self.clear_scrollbar_drag();
                }
                let content_length = layout.stack_max_scroll.saturating_add(1);
                let viewport_len = layout.stack_viewport.height as usize;
                let Some(metrics) = vertical_scrollbar_metrics(
                    layout.stack_viewport,
                    content_length,
                    layout.stack_scroll_top,
                    viewport_len,
                ) else {
                    return false;
                };
                let pos = vertical_scrollbar_position_for_thumb_drag(
                    layout.stack_viewport,
                    y,
                    metrics,
                    drag.offset_in_thumb,
                );
                self.stacked_scroll_top = pos.min(layout.stack_max_scroll);
                true
            }
            McpScrollbarTarget::Summary => {
                let summary_scrollbar_area = layout.summary_scrollbar_area();
                let summary_metrics = self.summary_metrics_for_viewport(layout.summary_inner);
                if summary_metrics.visible_lines == 0
                    || summary_metrics.total_lines <= summary_metrics.visible_lines
                {
                    return self.clear_scrollbar_drag();
                }
                let max_scroll =
                    summary_metrics.total_lines.saturating_sub(summary_metrics.visible_lines);
                let position = self.summary_scroll_top.min(max_scroll);
                let content_length = max_scroll.saturating_add(1);
                let Some(metrics) = vertical_scrollbar_metrics(
                    summary_scrollbar_area,
                    content_length,
                    position,
                    summary_metrics.visible_lines,
                ) else {
                    return false;
                };
                let pos = vertical_scrollbar_position_for_thumb_drag(
                    summary_scrollbar_area,
                    y,
                    metrics,
                    drag.offset_in_thumb,
                );
                self.summary_scroll_top = pos.min(max_scroll);
                true
            }
            McpScrollbarTarget::Tools => {
                let tools_scrollbar_area = layout.tools_scrollbar_area();
                let entries_len = self.tool_entries().len();
                let viewport_len = layout.tools_inner.height as usize;
                if viewport_len == 0 || entries_len <= viewport_len {
                    return self.clear_scrollbar_drag();
                }
                let max_scroll = entries_len.saturating_sub(viewport_len);
                let scroll_top = self
                    .tools_scroll_top_for_entries_len(layout.tools_inner.height, entries_len)
                    .min(max_scroll);
                let content_length = max_scroll.saturating_add(1);
                let Some(metrics) = vertical_scrollbar_metrics(
                    tools_scrollbar_area,
                    content_length,
                    scroll_top,
                    viewport_len,
                ) else {
                    return false;
                };
                let pos = vertical_scrollbar_position_for_thumb_drag(
                    tools_scrollbar_area,
                    y,
                    metrics,
                    drag.offset_in_thumb,
                );
                self.tools_set_scroll_top_from_scrollbar(pos.min(max_scroll), viewport_len);
                true
            }
        }
    }
}

