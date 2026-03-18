use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::ChromeMode;
use crate::ui_interaction::{step_index_by_delta, ScrollSelectionBehavior};

use super::super::{
    McpPaneHit,
    McpSettingsFocus,
    McpSettingsMode,
    McpSettingsView,
    McpToolHoverPart,
    McpViewLayout,
    SUMMARY_HORIZONTAL_SCROLL_STEP,
    SUMMARY_SCROLL_STEP,
};

impl McpSettingsView {
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

    pub(super) fn handle_mouse_event_direct_impl(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
        chrome: ChromeMode,
    ) -> bool {
        if !matches!(self.mode, McpSettingsMode::Main) {
            return match chrome {
                ChromeMode::Framed => self.handle_policy_editor_mouse_framed(mouse_event, area),
                ChromeMode::ContentOnly => {
                    self.handle_policy_editor_mouse_content_only(mouse_event, area)
                }
            };
        }

        let Some(layout) = (match chrome {
            ChromeMode::Framed => {
                McpViewLayout::from_area_with_scroll(area, self.stacked_scroll_top)
            }
            ChromeMode::ContentOnly => {
                McpViewLayout::from_content_area_with_scroll(area, self.stacked_scroll_top)
            }
        }) else {
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
            MouseEventKind::Up(MouseButton::Left) => self.clear_scrollbar_drag(),
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

