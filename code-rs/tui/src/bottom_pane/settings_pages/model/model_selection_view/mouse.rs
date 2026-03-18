use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::line_runs::selection_id_at;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test_no_ensure_visible;
use crate::components::scroll_state::ScrollState;
use crate::ui_interaction::{SelectableListMouseConfig, SelectableListMouseResult};

use crate::bottom_pane::ConditionalUpdate;

use super::ModelSelectionView;

impl ModelSelectionView {
    pub(super) fn hit_test_in_body(&self, body: Rect, x: u16, y: u16) -> Option<usize> {
        selection_id_at(body, x, y, self.scroll_offset, &self.build_render_runs())
    }

    fn handle_mouse_event_shared(&mut self, mouse_event: MouseEvent, body: Rect) -> ConditionalUpdate {
        let mut state = ScrollState {
            selected_idx: Some(self.selected_index),
            scroll_top: 0,
        };
        let outcome = route_scroll_state_mouse_with_hit_test_no_ensure_visible(
            mouse_event,
            &mut state,
            self.entry_count(),
            |x, y, _scroll_top| self.hit_test_in_body(body, x, y),
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );
        self.selected_index = state.selected_idx.unwrap_or(0);

        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            self.select_item(self.selected_index);
            return ConditionalUpdate::NeedsRedraw;
        }

        match mouse_event.kind {
            MouseEventKind::ScrollUp => {
                self.scroll_up();
                return ConditionalUpdate::NeedsRedraw;
            }
            MouseEventKind::ScrollDown => {
                self.scroll_down();
                return ConditionalUpdate::NeedsRedraw;
            }
            _ => {}
        }

        if outcome.changed {
            ConditionalUpdate::NeedsRedraw
        } else {
            ConditionalUpdate::NoRedraw
        }
    }

    pub(super) fn handle_mouse_event_direct_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        let Some(layout) = self.page().layout_in_chrome(chrome, area) else {
            return ConditionalUpdate::NoRedraw;
        };
        self.handle_mouse_event_shared(mouse_event, layout.body)
    }
}

