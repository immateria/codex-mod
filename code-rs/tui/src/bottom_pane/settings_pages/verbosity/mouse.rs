use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::ui_interaction::{
    route_selectable_list_mouse_with_config,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

use super::VerbositySelectionView;

impl VerbositySelectionView {
    pub(super) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let rows = self.menu_rows();
        let Some(layout) = self.page().framed().layout(area) else {
            return false;
        };

        let mut selected_idx = self.selected_idx;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected_idx,
            rows.len(),
            |x, y| {
                crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage::selection_menu_id_in_body(
                    layout.body, x, y, 0, &rows,
                )
            },
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );
        self.set_selected_index(selected_idx);

        if matches!(result, SelectableListMouseResult::Activated) {
            self.confirm_selection();
            return true;
        }

        result.handled()
    }
}

