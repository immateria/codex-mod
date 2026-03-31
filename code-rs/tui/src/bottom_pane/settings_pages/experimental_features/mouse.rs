use super::*;

use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::menu_rows::selection_id_at as selection_menu_id_at;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test;
use crate::ui_interaction::{
    contains_point,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

impl ExperimentalFeaturesSettingsView {
    fn handle_mouse_event_direct_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let total = self.feature_count();
        if total == 0 {
            return false;
        }

        let rows = self.overview_rows();
        let page = self.overview_page();
        let Some(layout) = page.layout_in_chrome(chrome, area) else {
            return false;
        };
        let body_layout = page.menu_body_layout(layout.body);
        let visible_rows = body_layout.list.height.max(1) as usize;
        self.list_viewport_rows.set(visible_rows);

        let mut state = self.list_state.get();
        let kind = mouse_event.kind;
        let outcome = route_scroll_state_mouse_with_hit_test(
            mouse_event,
            &mut state,
            total,
            visible_rows,
            |x, y, scroll_top| {
                if matches!(kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown) {
                    if !contains_point(body_layout.list, x, y) {
                        return None;
                    }
                    let rel = y.saturating_sub(body_layout.list.y) as usize;
                    Some(scroll_top.saturating_add(rel).min(total.saturating_sub(1)))
                } else {
                    selection_menu_id_at(body_layout.list, x, y, scroll_top, &rows)
                }
            },
            SelectableListMouseConfig {
                hover_select: false,
                require_pointer_hit_for_scroll: true,
                scroll_behavior: ScrollSelectionBehavior::Clamp,
                ..SelectableListMouseConfig::default()
            },
        );
        let selected = state.selected_idx.unwrap_or(0);
        self.list_state.set(state);

        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            let _ = selected;
            self.toggle_selected();
        }

        outcome.changed
    }

    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_in_chrome(ChromeMode::ContentOnly, mouse_event, area)
    }

    pub(super) fn handle_mouse_event_direct_framed(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.handle_mouse_event_direct_in_chrome(ChromeMode::Framed, mouse_event, area)
    }
}
