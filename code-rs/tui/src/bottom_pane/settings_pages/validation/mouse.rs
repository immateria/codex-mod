use super::*;

use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::line_runs::selection_id_at;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test_no_ensure_visible;
use crate::bottom_pane::BottomPane;
use crate::ui_interaction::{
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

impl ValidationSettingsView {
    pub(super) fn handle_mouse_event_internal(
        &mut self,
        pane: Option<&mut BottomPane<'_>>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let page = self.page();
        let Some(layout) = page.layout_in_chrome(ChromeMode::Framed, area) else {
            return false;
        };
        self.handle_mouse_event_in_body(pane, mouse_event, layout.body)
    }

    fn handle_mouse_event_internal_content(
        &mut self,
        pane: Option<&mut BottomPane<'_>>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let page = self.page();
        let Some(layout) = page.layout_in_chrome(ChromeMode::ContentOnly, area) else {
            return false;
        };
        self.handle_mouse_event_in_body(pane, mouse_event, layout.body)
    }

    fn handle_mouse_event_in_body(
        &mut self,
        mut pane: Option<&mut BottomPane<'_>>,
        mouse_event: MouseEvent,
        body: Rect,
    ) -> bool {
        let mut model = self.build_model();
        let total = model.selection_kinds.len();
        if total == 0 {
            return false;
        }

        self.ensure_selected_visible(&model, body.height as usize);
        let (outcome, next_state) = {
            // Hit-testing is based on run geometry; selection-specific styling doesn't affect
            // line/rect boundaries, so we build runs without a selected row.
            let runs = self.build_runs(usize::MAX);
            let mut state = self.state;
            let outcome = route_scroll_state_mouse_with_hit_test_no_ensure_visible(
                mouse_event,
                &mut state,
                total,
                |x, y, scroll_top| selection_id_at(body, x, y, scroll_top, &runs),
                SelectableListMouseConfig {
                    hover_select: false,
                    ..SelectableListMouseConfig::default()
                },
            );
            (outcome, state)
        };
        self.state = next_state;

        if matches!(outcome.result, SelectableListMouseResult::Activated)
            && let Some(selected) = self.state.selected_idx
            && let Some(kind) = model.selection_kinds.get(selected).copied()
        {
            self.activate_selection(pane.take(), kind);
        }

        if outcome.changed {
            model = self.build_model();
            let total = model.selection_kinds.len();
            if total == 0 {
                self.state.selected_idx = None;
                self.state.scroll_top = 0;
            } else {
                self.ensure_selected_visible(&model, body.height as usize);
            }
        }
        outcome.changed
    }

    pub(super) fn handle_mouse_event_direct_content_only(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.handle_mouse_event_internal_content(None, mouse_event, area)
    }
}
