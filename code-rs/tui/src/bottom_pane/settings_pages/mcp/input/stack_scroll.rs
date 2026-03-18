use crate::bottom_pane::ChromeMode;
use crate::ui_interaction::{next_scroll_top_with_delta, scroll_top_to_keep_visible};

use super::super::{McpSettingsFocus, McpSettingsView, McpViewLayout};

impl McpSettingsView {
    pub(super) fn ensure_stacked_focus_visible_from_last_render(&mut self) -> bool {
        let Some((area, chrome)) = self.last_render.get() else {
            return false;
        };
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

    pub(super) fn scroll_stacked_column(&mut self, layout: McpViewLayout, delta: isize) -> bool {
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
}

