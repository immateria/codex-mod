use super::*;

use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_in_body;
use crate::ui_interaction::{
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

        let page = self.overview_page();
        let Some(layout) = page.layout_in_chrome(chrome, area) else {
            return false;
        };
        let visible_rows = layout.body.height.max(1) as usize;
        self.list_viewport_rows.set(visible_rows);

        let mut state = self.list_state.get();
        let outcome = route_scroll_state_mouse_in_body(
            mouse_event,
            layout.body,
            &mut state,
            total,
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

