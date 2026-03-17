use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test_no_ensure_visible;
use crate::ui_interaction::{SelectableListMouseConfig, SelectableListMouseResult};

use super::VerbositySelectionView;

impl VerbositySelectionView {
    pub(super) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let rows = self.menu_rows();
        let Some(layout) = self.page().layout_in_chrome(ChromeMode::Framed, area) else {
            return false;
        };

        let outcome = route_scroll_state_mouse_with_hit_test_no_ensure_visible(
            mouse_event,
            &mut self.state,
            rows.len(),
            |x, y, _scroll_top| {
                crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage::selection_menu_id_in_body(
                    layout.body,
                    x,
                    y,
                    0,
                    &rows,
                )
            },
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );

        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            self.confirm_selection();
            return true;
        }

        outcome.changed
    }
}
