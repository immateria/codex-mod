use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders};

use crate::ui_interaction::{
    next_scroll_top_with_delta,
    scroll_top_to_keep_visible,
    step_index_by_delta,
    vertical_scrollbar_metrics,
    vertical_scrollbar_position_for_thumb_drag,
    vertical_scrollbar_right_hit_test,
    ScrollSelectionBehavior,
    VerticalScrollbarHit,
};

use super::{
    McpPaneHit,
    McpScrollbarDragState,
    McpScrollbarTarget,
    McpSettingsFocus,
    McpSettingsView,
    McpToolHoverPart,
    McpViewLayout,
    SUMMARY_HORIZONTAL_SCROLL_STEP,
    SUMMARY_SCROLL_STEP,
};

impl McpSettingsView {
    fn ensure_stacked_focus_visible_from_last_render(&mut self) -> bool {
        let Some(area) = self.last_render_area.get() else {
            return false;
        };
        let Some(layout) = McpViewLayout::from_area_with_scroll(area, self.stacked_scroll_top) else {
            return false;
        };
        self.stacked_scroll_top = layout.stack_scroll_top;
        self.ensure_stacked_focus_visible(layout)
    }

    fn ensure_stacked_focus_visible(&mut self, layout: McpViewLayout) -> bool {
        if !layout.stacked || layout.stack_max_scroll == 0 || layout.stack_viewport.height == 0 {
            return false;
        }

        let (pane_top, pane_h) = match self.focus {
            McpSettingsFocus::Servers => (layout.stack_list_top, layout.stack_list_h),
            McpSettingsFocus::Summary => (layout.stack_summary_top, layout.stack_summary_h),
            McpSettingsFocus::Tools => (layout.stack_tools_top, layout.stack_tools_h),
        };
        if pane_h == 0 {
            return false;
        }

        let viewport_h = layout.stack_viewport.height as usize;
        let next = scroll_top_to_keep_visible(
            self.stacked_scroll_top,
            layout.stack_max_scroll,
            viewport_h,
            pane_top,
            pane_h,
        );
        if self.stacked_scroll_top == next {
            false
        } else {
            self.stacked_scroll_top = next;
            true
        }
    }

    fn scroll_stacked_column(&mut self, layout: McpViewLayout, delta: isize) -> bool {
        if !layout.stacked || layout.stack_max_scroll == 0 {
            return false;
        }

        let next = next_scroll_top_with_delta(
            layout.stack_scroll_top,
            layout.stack_max_scroll,
            delta,
        );

        if self.stacked_scroll_top == next {
            false
        } else {
            self.stacked_scroll_top = next;
            true
        }
    }

    fn set_list_hover_index(&mut self, list_index: Option<usize>) -> bool {
        if self.hovered_list_index == list_index {
            false
        } else {
            self.hovered_list_index = list_index;
            true
        }
    }

    fn clear_list_hover(&mut self) -> bool {
        self.set_list_hover_index(None)
    }

    fn update_list_hover_from_mouse(&mut self, layout: McpViewLayout, mouse_event: MouseEvent) -> bool {
        let idx = self.server_index_at_mouse_row(layout.list_inner, mouse_event.row);
        self.set_list_hover_index(idx)
    }

    fn set_tool_hover_state(
        &mut self,
        tool_index: Option<usize>,
        tool_part: Option<McpToolHoverPart>,
    ) -> bool {
        if self.hovered_tool_index == tool_index && self.hovered_tool_part == tool_part {
            false
        } else {
            self.hovered_tool_index = tool_index;
            self.hovered_tool_part = tool_part;
            true
        }
    }

    fn clear_tool_hover(&mut self) -> bool {
        self.set_tool_hover_state(None, None)
    }

    fn tool_hover_part_from_rel_x(rel_x: u16) -> McpToolHoverPart {
        if (2..=4).contains(&rel_x) {
            McpToolHoverPart::Toggle
        } else if rel_x == 6 {
            McpToolHoverPart::Expand
        } else {
            McpToolHoverPart::Label
        }
    }

    fn update_tool_hover_from_mouse(&mut self, layout: McpViewLayout, mouse_event: MouseEvent) -> bool {
        let Some(idx) = self.tool_index_at_mouse_row(layout.tools_inner, mouse_event.row) else {
            return self.clear_tool_hover();
        };
        let rel_x = mouse_event.column.saturating_sub(layout.tools_inner.x);
        let part = Self::tool_hover_part_from_rel_x(rel_x);
        self.set_tool_hover_state(Some(idx), Some(part))
    }

    fn clear_server_row_click_arm(&mut self) {
        self.armed_server_row_click = None;
    }

    fn activate_server_row_on_click(&mut self, row_index: usize) -> bool {
        if self.armed_server_row_click == Some(row_index) {
            self.armed_server_row_click = None;
            true
        } else {
            self.armed_server_row_click = Some(row_index);
            false
        }
    }

    fn set_hovered_pane(&mut self, pane: McpPaneHit) -> bool {
        if self.hovered_pane == pane {
            false
        } else {
            self.hovered_pane = pane;
            true
        }
    }

    fn pane_hit_at(&self, layout: McpViewLayout, x: u16, y: u16) -> McpPaneHit {
        if layout.contains_list(x, y) {
            McpPaneHit::Servers
        } else if layout.contains_summary(x, y) {
            McpPaneHit::Summary
        } else if layout.contains_tools(x, y) {
            McpPaneHit::Tools
        } else {
            McpPaneHit::Outside
        }
    }

    fn apply_focus_from_hit(&mut self, hit: McpPaneHit) -> bool {
        let next_focus = match hit {
            McpPaneHit::Servers => Some(McpSettingsFocus::Servers),
            McpPaneHit::Summary => Some(McpSettingsFocus::Summary),
            McpPaneHit::Tools => Some(McpSettingsFocus::Tools),
            McpPaneHit::Outside => None,
        };
        let Some(next_focus) = next_focus else {
            return false;
        };
        let changed = self.focus != next_focus;
        self.set_focus(next_focus);
        changed
    }

    fn set_scrollbar_drag(&mut self, drag: Option<McpScrollbarDragState>) -> bool {
        if self.scrollbar_drag == drag {
            false
        } else {
            self.scrollbar_drag = drag;
            true
        }
    }

    fn clear_scrollbar_drag(&mut self) -> bool {
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

    fn handle_scrollbar_mouse_down(&mut self, layout: McpViewLayout, mouse_event: MouseEvent) -> bool {
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
        if summary_metrics.visible_lines > 0 && summary_metrics.total_lines > summary_metrics.visible_lines {
            let max_scroll = summary_metrics.total_lines.saturating_sub(summary_metrics.visible_lines);
            let position = self.summary_scroll_top.min(max_scroll);
            let content_length = max_scroll.saturating_add(1);
            if let Some(metrics) = vertical_scrollbar_metrics(
                summary_scrollbar_area,
                content_length,
                position,
                summary_metrics.visible_lines,
            ) && let Some(hit) = vertical_scrollbar_right_hit_test(summary_scrollbar_area, x, y, metrics)
            {
                self.set_focus(McpSettingsFocus::Summary);
                self.set_hovered_pane(McpPaneHit::Summary);
                self.clear_tool_hover();
                self.clear_list_hover();
                match hit {
                    VerticalScrollbarHit::BeginArrow => self.scroll_summary_lines(-1),
                    VerticalScrollbarHit::EndArrow => self.scroll_summary_lines(1),
                    VerticalScrollbarHit::TrackAboveThumb => {
                        self.scroll_summary_lines(-(summary_metrics.visible_lines as isize))
                    }
                    VerticalScrollbarHit::TrackBelowThumb => {
                        self.scroll_summary_lines(summary_metrics.visible_lines as isize)
                    }
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
            let scroll_top = self.tools_scroll_top(layout.tools_inner.height).min(max_scroll);
            let content_length = max_scroll.saturating_add(1);
            if let Some(metrics) = vertical_scrollbar_metrics(
                tools_scrollbar_area,
                content_length,
                scroll_top,
                viewport_len,
            ) && let Some(hit) = vertical_scrollbar_right_hit_test(tools_scrollbar_area, x, y, metrics)
            {
                self.set_focus(McpSettingsFocus::Tools);
                self.set_hovered_pane(McpPaneHit::Tools);
                self.clear_list_hover();
                self.clear_server_row_click_arm();
                match hit {
                    VerticalScrollbarHit::BeginArrow => {
                        self.tools_selected = step_index_by_delta(
                            self.tools_selected,
                            entries_len,
                            -1,
                            ScrollSelectionBehavior::Clamp,
                        );
                    }
                    VerticalScrollbarHit::EndArrow => {
                        self.tools_selected = step_index_by_delta(
                            self.tools_selected,
                            entries_len,
                            1,
                            ScrollSelectionBehavior::Clamp,
                        );
                    }
                    VerticalScrollbarHit::TrackAboveThumb => {
                        let next = scroll_top.saturating_sub(viewport_len);
                        self.tools_set_scroll_top_from_scrollbar(next, viewport_len);
                    }
                    VerticalScrollbarHit::TrackBelowThumb => {
                        let next = scroll_top.saturating_add(viewport_len).min(max_scroll);
                        self.tools_set_scroll_top_from_scrollbar(next, viewport_len);
                    }
                    VerticalScrollbarHit::Thumb { offset_in_thumb } => {
                        self.begin_scrollbar_drag(
                            McpScrollbarTarget::Tools,
                            offset_in_thumb,
                        );
                    }
                }
                self.update_tool_hover_from_mouse(layout, mouse_event);
                return true;
            }
        }

        false
    }

    fn handle_scrollbar_mouse_drag(&mut self, layout: McpViewLayout, mouse_event: MouseEvent) -> bool {
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
                let Some(metrics) =
                    vertical_scrollbar_metrics(layout.stack_viewport, content_length, layout.stack_scroll_top, viewport_len)
                else {
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
                if summary_metrics.visible_lines == 0 || summary_metrics.total_lines <= summary_metrics.visible_lines {
                    return self.clear_scrollbar_drag();
                }
                let max_scroll = summary_metrics.total_lines.saturating_sub(summary_metrics.visible_lines);
                let position = self.summary_scroll_top.min(max_scroll);
                let content_length = max_scroll.saturating_add(1);
                let Some(metrics) =
                    vertical_scrollbar_metrics(
                        summary_scrollbar_area,
                        content_length,
                        position,
                        summary_metrics.visible_lines,
                    )
                else {
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
                let scroll_top = self.tools_scroll_top(layout.tools_inner.height).min(max_scroll);
                let content_length = max_scroll.saturating_add(1);
                let Some(metrics) =
                    vertical_scrollbar_metrics(
                        tools_scrollbar_area,
                        content_length,
                        scroll_top,
                        viewport_len,
                    )
                else {
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

    fn handle_mouse_move_routed(&mut self, layout: McpViewLayout, mouse_event: MouseEvent) -> bool {
        let hit = self.pane_hit_at(layout, mouse_event.column, mouse_event.row);
        if hit != McpPaneHit::Servers {
            self.clear_server_row_click_arm();
        }
        match hit {
            McpPaneHit::Servers => {
                let pane_changed = self.set_hovered_pane(hit);
                let list_changed = self.update_list_hover_from_mouse(layout, mouse_event);
                let tool_cleared = self.clear_tool_hover();
                pane_changed || list_changed || tool_cleared
            }
            McpPaneHit::Tools => {
                let pane_changed = self.set_hovered_pane(hit);
                let tool_changed = self.update_tool_hover_from_mouse(layout, mouse_event);
                let list_cleared = self.clear_list_hover();
                pane_changed || tool_changed || list_cleared
            }
            _ => {
                let pane_changed = self.set_hovered_pane(hit);
                let tool_cleared = self.clear_tool_hover();
                let list_cleared = self.clear_list_hover();
                pane_changed || tool_cleared || list_cleared
            }
        }
    }

    fn handle_mouse_left_click_routed(
        &mut self,
        layout: McpViewLayout,
        mouse_event: MouseEvent,
    ) -> bool {
        let hit = self.pane_hit_at(layout, mouse_event.column, mouse_event.row);
        let handled = self.set_hovered_pane(hit);
        match hit {
            McpPaneHit::Servers => {
                self.clear_tool_hover();
                self.set_focus(McpSettingsFocus::Servers);
                let Some(next) = self.server_index_at_mouse_position(
                    layout.list_inner,
                    mouse_event.column,
                    mouse_event.row,
                ) else {
                    self.clear_server_row_click_arm();
                    let list_cleared = self.clear_list_hover();
                    return handled || list_cleared;
                };
                self.set_list_hover_index(Some(next));
                self.set_selected(next);
                if next < self.rows.len() {
                    if self.activate_server_row_on_click(next) {
                        self.on_enter_server_selection();
                    }
                } else {
                    self.clear_server_row_click_arm();
                    self.on_enter_server_selection();
                }
                true
            }
            McpPaneHit::Summary => {
                self.clear_server_row_click_arm();
                self.clear_tool_hover();
                let focus_changed = self.apply_focus_from_hit(hit);
                let list_cleared = self.clear_list_hover();
                handled || focus_changed || list_cleared
            }
            McpPaneHit::Tools => {
                self.clear_server_row_click_arm();
                self.clear_list_hover();
                self.set_focus(McpSettingsFocus::Tools);
                let Some(idx) = self.tool_index_at_mouse_position(
                    layout.tools_inner,
                    mouse_event.column,
                    mouse_event.row,
                ) else {
                    let tool_cleared = self.clear_tool_hover();
                    return handled || tool_cleared;
                };
                let rel_x = mouse_event.column.saturating_sub(layout.tools_inner.x);
                let part = Self::tool_hover_part_from_rel_x(rel_x);
                self.set_tool_hover_state(Some(idx), Some(part));
                let was_selected = self.tools_selected == idx;
                self.tools_selected = idx;
                match part {
                    McpToolHoverPart::Toggle => self.toggle_selected_tool(),
                    McpToolHoverPart::Expand => self.toggle_selected_tool_expansion(),
                    McpToolHoverPart::Label => {
                        if was_selected {
                            self.toggle_selected_tool_expansion();
                        }
                    }
                }
                true
            }
            McpPaneHit::Outside => {
                self.clear_server_row_click_arm();
                let tool_cleared = self.clear_tool_hover();
                let list_cleared = self.clear_list_hover();
                handled || tool_cleared || list_cleared
            }
        }
    }

    fn handle_mouse_wheel_vertical_routed(
        &mut self,
        layout: McpViewLayout,
        mouse_event: MouseEvent,
        delta: isize,
    ) -> bool {
        let hit = self.pane_hit_at(layout, mouse_event.column, mouse_event.row);
        if hit != McpPaneHit::Servers {
            self.clear_server_row_click_arm();
        }
        let hover_changed = self.set_hovered_pane(hit);
        if layout.stacked && layout.stack_max_scroll > 0 {
            let focus_changed = self.apply_focus_from_hit(hit);
            let hover_details_changed = match hit {
                McpPaneHit::Servers => {
                    self.clear_tool_hover();
                    self.update_list_hover_from_mouse(layout, mouse_event)
                }
                McpPaneHit::Tools => {
                    self.clear_list_hover();
                    self.update_tool_hover_from_mouse(layout, mouse_event)
                }
                McpPaneHit::Summary | McpPaneHit::Outside => {
                    let tool_cleared = self.clear_tool_hover();
                    let list_cleared = self.clear_list_hover();
                    tool_cleared || list_cleared
                }
            };
            let scrolled = self.scroll_stacked_column(layout, delta * SUMMARY_SCROLL_STEP as isize);
            return scrolled || hover_changed || focus_changed || hover_details_changed;
        }
        match hit {
            McpPaneHit::Summary => {
                self.clear_tool_hover();
                self.clear_list_hover();
                self.set_focus(McpSettingsFocus::Summary);
                self.scroll_summary_with_wheel(
                    delta * SUMMARY_SCROLL_STEP as isize,
                    layout.summary_inner,
                    mouse_event.modifiers,
                );
                true
            }
            McpPaneHit::Tools => {
                self.clear_list_hover();
                self.set_focus(McpSettingsFocus::Tools);
                self.update_tool_hover_from_mouse(layout, mouse_event);
                let len = self.tool_entries().len();
                if len > 0 {
                    self.tools_selected = step_index_by_delta(
                        self.tools_selected,
                        len,
                        delta,
                        ScrollSelectionBehavior::Clamp,
                    );
                }
                true
            }
            McpPaneHit::Servers => {
                self.clear_tool_hover();
                self.set_focus(McpSettingsFocus::Servers);
                self.update_list_hover_from_mouse(layout, mouse_event);
                if delta < 0 {
                    self.move_selection_up();
                } else {
                    self.move_selection_down();
                }
                true
            }
            McpPaneHit::Outside => {
                let tool_cleared = self.clear_tool_hover();
                let list_cleared = self.clear_list_hover();
                hover_changed || tool_cleared || list_cleared
            }
        }
    }

    pub(super) fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        self.clear_server_row_click_arm();
        self.clear_list_hover();
        let handled = match key_event {
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.cycle_focus(false);
                true
            }
            KeyEvent {
                code: KeyCode::BackTab,
                ..
            } => {
                self.cycle_focus(true);
                true
            }
            KeyEvent { code: KeyCode::Up, .. } => {
                match self.focus {
                    McpSettingsFocus::Servers => self.move_selection_up(),
                    McpSettingsFocus::Summary => self.scroll_summary_lines(-1),
                    McpSettingsFocus::Tools => {
                        let len = self.tool_entries().len();
                        if len > 0 {
                            if self.tools_selected == 0 {
                                self.tools_selected = len - 1;
                            } else {
                                self.tools_selected -= 1;
                            }
                        }
                    }
                }
                true
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => {
                match self.focus {
                    McpSettingsFocus::Servers => self.move_selection_down(),
                    McpSettingsFocus::Summary => self.scroll_summary_lines(1),
                    McpSettingsFocus::Tools => {
                        let len = self.tool_entries().len();
                        if len > 0 {
                            self.tools_selected = (self.tools_selected + 1) % len;
                        }
                    }
                }
                true
            }
            KeyEvent {
                code: KeyCode::Left,
                ..
            } => {
                if self.focus == McpSettingsFocus::Servers {
                    self.on_toggle_server();
                } else if self.focus == McpSettingsFocus::Tools {
                    self.set_expanded_tool_for_selected_server(None);
                } else if self.focus == McpSettingsFocus::Summary && !self.summary_wrap {
                    self.shift_summary_hscroll(-SUMMARY_HORIZONTAL_SCROLL_STEP);
                }
                true
            }
            KeyEvent {
                code: KeyCode::Right,
                ..
            } => {
                if self.focus == McpSettingsFocus::Servers {
                    self.on_toggle_server();
                } else if self.focus == McpSettingsFocus::Tools {
                    self.toggle_selected_tool_expansion();
                } else if self.focus == McpSettingsFocus::Summary && !self.summary_wrap {
                    self.shift_summary_hscroll(SUMMARY_HORIZONTAL_SCROLL_STEP);
                }
                true
            }
            KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                match self.focus {
                    McpSettingsFocus::Servers => self.on_toggle_server(),
                    McpSettingsFocus::Tools => self.toggle_selected_tool(),
                    McpSettingsFocus::Summary => {}
                }
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => {
                match self.focus {
                    McpSettingsFocus::Servers => self.on_enter_server_selection(),
                    McpSettingsFocus::Tools => self.toggle_selected_tool_expansion(),
                    McpSettingsFocus::Summary => {}
                }
                true
            }
            KeyEvent {
                code: KeyCode::PageUp,
                ..
            } => {
                self.page_summary(-1);
                true
            }
            KeyEvent {
                code: KeyCode::PageDown,
                ..
            } => {
                self.page_summary(1);
                true
            }
            KeyEvent {
                code: KeyCode::Home,
                ..
            } => {
                match self.focus {
                    McpSettingsFocus::Servers => self.set_selected(0),
                    McpSettingsFocus::Tools => self.tools_selected = 0,
                    McpSettingsFocus::Summary => self.summary_scroll_top = 0,
                }
                true
            }
            KeyEvent {
                code: KeyCode::End, ..
            } => {
                match self.focus {
                    McpSettingsFocus::Servers => self.set_selected(self.len().saturating_sub(1)),
                    McpSettingsFocus::Tools => {
                        let len = self.tool_entries().len();
                        self.tools_selected = len.saturating_sub(1);
                    }
                    McpSettingsFocus::Summary => self.summary_scroll_top = usize::MAX,
                }
                true
            }
            KeyEvent {
                code: KeyCode::Char('r' | 'R'),
                modifiers,
                ..
            } if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
                self.request_refresh();
                true
            }
            KeyEvent {
                code: KeyCode::Char('s' | 'S'),
                modifiers,
                ..
            } if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
                self.queue_status_report();
                true
            }
            KeyEvent {
                code: KeyCode::Char('w' | 'W'),
                modifiers,
                ..
            } if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
                self.toggle_summary_wrap_mode();
                true
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.is_complete = true;
                true
            }
            _ => false,
        }
        ;

        if handled {
            self.ensure_stacked_focus_visible_from_last_render();
        }

        handled
    }

    pub(super) fn content_rect(area: Rect) -> Rect {
        let inner = Block::default().borders(Borders::ALL).inner(area);
        Rect {
            x: inner.x.saturating_add(1),
            y: inner.y,
            width: inner.width.saturating_sub(2),
            height: inner.height,
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.process_key_event(key_event)
    }

    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let Some(layout) = McpViewLayout::from_area_with_scroll(area, self.stacked_scroll_top) else {
            return false;
        };
        self.stacked_scroll_top = layout.stack_scroll_top;

        match mouse_event.kind {
            MouseEventKind::Moved => {
                if self.scrollbar_drag.is_some() {
                    self.handle_scrollbar_mouse_drag(layout, mouse_event)
                } else {
                    self.handle_mouse_move_routed(layout, mouse_event)
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if self.handle_scrollbar_mouse_down(layout, mouse_event) {
                    true
                } else {
                    self.handle_mouse_left_click_routed(layout, mouse_event)
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                self.handle_scrollbar_mouse_drag(layout, mouse_event)
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.clear_scrollbar_drag()
            }
            MouseEventKind::ScrollUp => {
                self.handle_mouse_wheel_vertical_routed(layout, mouse_event, -1)
            }
            MouseEventKind::ScrollDown => {
                self.handle_mouse_wheel_vertical_routed(layout, mouse_event, 1)
            }
            MouseEventKind::ScrollLeft => {
                let hit = self.pane_hit_at(layout, mouse_event.column, mouse_event.row);
                let hover_changed = self.set_hovered_pane(hit);
                if hit == McpPaneHit::Summary {
                    self.clear_tool_hover();
                    self.clear_list_hover();
                    self.set_focus(McpSettingsFocus::Summary);
                    self.shift_summary_hscroll(-SUMMARY_HORIZONTAL_SCROLL_STEP);
                    return true;
                }
                let tool_cleared = self.clear_tool_hover();
                let list_cleared = self.clear_list_hover();
                hover_changed || tool_cleared || list_cleared
            }
            MouseEventKind::ScrollRight => {
                let hit = self.pane_hit_at(layout, mouse_event.column, mouse_event.row);
                let hover_changed = self.set_hovered_pane(hit);
                if hit == McpPaneHit::Summary {
                    self.clear_tool_hover();
                    self.clear_list_hover();
                    self.set_focus(McpSettingsFocus::Summary);
                    self.shift_summary_hscroll(SUMMARY_HORIZONTAL_SCROLL_STEP);
                    return true;
                }
                let tool_cleared = self.clear_tool_hover();
                let list_cleared = self.clear_list_hover();
                hover_changed || tool_cleared || list_cleared
            }
            _ => false,
        }
    }
}
