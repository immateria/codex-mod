use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test;
use crate::ui_interaction::{
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

use super::SettingsOverviewView;

impl SettingsOverviewView {
    pub(super) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let page = self.page();
        let Some(layout) = page.framed().layout(area) else {
            return false;
        };

        if self.rows.is_empty() || layout.body.width == 0 || layout.body.height == 0 {
            return false;
        }

        let visible_rows = layout.body.height as usize;
        self.viewport_rows.set(visible_rows.max(1));
        let rows = self.menu_rows();
        let mut scroll = self.scroll;
        let outcome = route_scroll_state_mouse_with_hit_test(
            mouse_event,
            &mut scroll,
            self.rows.len(),
            visible_rows.max(1),
            |x, y, scroll_top| {
                crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage::selection_menu_id_in_body(
                    layout.body,
                    x,
                    y,
                    scroll_top,
                    &rows,
                )
            },
            SelectableListMouseConfig {
                hover_select: false,
                require_pointer_hit_for_scroll: true,
                scroll_behavior: ScrollSelectionBehavior::Clamp,
                ..SelectableListMouseConfig::default()
            },
        );
        self.scroll = scroll;

        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            self.open_selected();
        }
        outcome.changed
    }
}
