use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::ui_interaction::{
    route_selectable_list_mouse_with_config,
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
        let mut selected = self.selected_index();
        let scroll_top = self.scroll.scroll_top.min(self.rows.len().saturating_sub(1));
        let rows = self.menu_rows();

        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            self.rows.len(),
            |x, y| {
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

        match result {
            SelectableListMouseResult::Ignored => false,
            SelectableListMouseResult::SelectionChanged => {
                self.scroll.selected_idx = Some(selected);
                self.scroll.ensure_visible(self.rows.len(), visible_rows.max(1));
                true
            }
            SelectableListMouseResult::Activated => {
                self.scroll.selected_idx = Some(selected);
                self.open_selected();
                true
            }
        }
    }
}

