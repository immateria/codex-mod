use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test;
use crate::ui_interaction::{
    contains_point,
    SelectableListMouseResult,
    SETTINGS_LIST_MOUSE_CONFIG,
};

use super::SettingsOverviewView;

impl SettingsOverviewView {
    pub(super) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let page = self.page();
        let Some(layout) = page.layout_in_chrome(ChromeMode::Framed, area) else {
            return false;
        };

        if self.rows.is_empty() || layout.body.width == 0 || layout.body.height == 0 {
            return false;
        }

        let visible_rows = layout.body.height as usize;
        self.viewport_rows.set(visible_rows.max(1));
        let total = self.rows.len();
        let rows = self.menu_rows();
        let mut scroll = self.scroll;
        let kind = mouse_event.kind;
        let outcome = route_scroll_state_mouse_with_hit_test(
            mouse_event,
            &mut scroll,
            total,
            visible_rows.max(1),
            |x, y, scroll_top| {
                if matches!(kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown) {
                    if !contains_point(layout.body, x, y) {
                        return None;
                    }
                    let rel = y.saturating_sub(layout.body.y) as usize;
                    Some(scroll_top.saturating_add(rel).min(total.saturating_sub(1)))
                } else {
                    crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage::selection_menu_id_in_body(
                        layout.body,
                        x,
                        y,
                        scroll_top,
                        &rows,
                    )
                }
            },
            SETTINGS_LIST_MOUSE_CONFIG,
        );
        self.scroll = scroll;

        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            self.open_selected();
        }
        outcome.changed
    }
}
