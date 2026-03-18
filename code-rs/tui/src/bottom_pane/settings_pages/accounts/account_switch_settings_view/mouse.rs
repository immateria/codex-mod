use super::{AccountSwitchSettingsView, ViewMode};

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::line_runs::selection_id_at as selection_run_id_at;
use crate::bottom_pane::settings_ui::menu_rows::selection_id_at as selection_menu_id_at;
use crate::bottom_pane::settings_ui::selectable_list_mouse::route_scroll_state_mouse_with_hit_test_no_ensure_visible;
use crate::ui_interaction::{SelectableListMouseConfig, SelectableListMouseResult};
use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

impl AccountSwitchSettingsView {
    pub(super) fn handle_mouse_event_direct_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        match self.view_mode {
            ViewMode::Main => {
                let page = self.main_page();
                let Some(layout) = page.layout_in_chrome(chrome, area) else {
                    return false;
                };
                self.handle_mouse_event_main_in_body(mouse_event, layout.body)
            }
            ViewMode::ConfirmStoreChange { target } => {
                let page = self.confirm_page(target);
                let Some(layout) = page.layout_in_chrome(chrome, area) else {
                    return false;
                };
                self.handle_mouse_event_confirm_in_body(mouse_event, layout.body)
            }
        }
    }

    fn handle_mouse_event_main_in_body(&mut self, mouse_event: MouseEvent, body: Rect) -> bool {
        let runs = self.main_runs(None);
        let mut state = self.main_state;
        let outcome = route_scroll_state_mouse_with_hit_test_no_ensure_visible(
            mouse_event,
            &mut state,
            Self::MAIN_OPTION_COUNT,
            |x, y, _scroll_top| selection_run_id_at(body, x, y, 0, &runs),
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );
        self.main_state = state;

        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            self.activate_selected_main();
        }
        outcome.changed
    }

    fn handle_mouse_event_confirm_in_body(&mut self, mouse_event: MouseEvent, body: Rect) -> bool {
        let rows = self.confirm_rows();
        let mut state = self.confirm_state;
        let outcome = route_scroll_state_mouse_with_hit_test_no_ensure_visible(
            mouse_event,
            &mut state,
            Self::CONFIRM_OPTION_COUNT,
            |x, y, _scroll_top| selection_menu_id_at(body, x, y, 0, &rows),
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );
        self.confirm_state = state;

        if matches!(outcome.result, SelectableListMouseResult::Activated) {
            self.activate_selected_confirm();
        }
        outcome.changed
    }
}
