use crossterm::event::MouseEvent;

use super::super::{
    McpPaneHit,
    McpSettingsFocus,
    McpSettingsView,
    McpToolHoverPart,
    McpViewLayout,
};

impl McpSettingsView {
    pub(super) fn set_list_hover_index(&mut self, list_index: Option<usize>) -> bool {
        if self.hovered_list_index == list_index {
            false
        } else {
            self.hovered_list_index = list_index;
            true
        }
    }

    pub(super) fn clear_list_hover(&mut self) -> bool {
        self.set_list_hover_index(None)
    }

    pub(super) fn update_list_hover_from_mouse(
        &mut self,
        layout: McpViewLayout,
        mouse_event: MouseEvent,
    ) -> bool {
        let idx = self.server_index_at_mouse_row(layout.list_inner, mouse_event.row);
        self.set_list_hover_index(idx)
    }

    pub(super) fn set_tool_hover_state(
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

    pub(super) fn clear_tool_hover(&mut self) -> bool {
        self.set_tool_hover_state(None, None)
    }

    pub(super) fn tool_hover_part_from_rel_x(rel_x: u16) -> McpToolHoverPart {
        if (2..=4).contains(&rel_x) {
            McpToolHoverPart::Toggle
        } else if rel_x == 6 {
            McpToolHoverPart::Expand
        } else {
            McpToolHoverPart::Label
        }
    }

    pub(super) fn update_tool_hover_from_mouse(
        &mut self,
        layout: McpViewLayout,
        mouse_event: MouseEvent,
    ) -> bool {
        let Some(idx) = self.tool_index_at_mouse_row(layout.tools_inner, mouse_event.row) else {
            return self.clear_tool_hover();
        };
        let rel_x = mouse_event.column.saturating_sub(layout.tools_inner.x);
        let part = Self::tool_hover_part_from_rel_x(rel_x);
        self.set_tool_hover_state(Some(idx), Some(part))
    }

    pub(super) fn clear_server_row_click_arm(&mut self) {
        self.armed_server_row_click = None;
    }

    pub(super) fn activate_server_row_on_click(&mut self, row_index: usize) -> bool {
        if self.armed_server_row_click == Some(row_index) {
            self.armed_server_row_click = None;
            true
        } else {
            self.armed_server_row_click = Some(row_index);
            false
        }
    }

    pub(super) fn set_hovered_pane(&mut self, pane: McpPaneHit) -> bool {
        if self.hovered_pane == pane {
            false
        } else {
            self.hovered_pane = pane;
            true
        }
    }

    pub(super) fn pane_hit_at(&self, layout: McpViewLayout, x: u16, y: u16) -> McpPaneHit {
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

    pub(super) fn apply_focus_from_hit(&mut self, hit: McpPaneHit) -> bool {
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
}

