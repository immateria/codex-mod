use super::*;

use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::bottom_pane::settings_ui::line_runs::selection_id_at;
use crate::ui_interaction::{
    route_selectable_list_mouse_with_config,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

impl ReviewSettingsView {
    pub(super) fn handle_mouse_event_direct_content_only(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let page = self.page();
        let Some(layout) = page.content_only().layout(area) else {
            return false;
        };
        self.handle_mouse_event_in_body(mouse_event, layout.body)
    }

    pub(super) fn handle_mouse_event_direct_framed(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let page = self.page();
        let Some(layout) = page.framed().layout(area) else {
            return false;
        };
        self.handle_mouse_event_in_body(mouse_event, layout.body)
    }

    fn handle_mouse_event_in_body(&mut self, mouse_event: MouseEvent, body: Rect) -> bool {
        let mut model = self.build_model();
        let total = model.selection_kinds.len();
        if total == 0 {
            return false;
        }

        self.state.clamp_selection(total);
        self.ensure_selected_visible(&model, body.height as usize);
        let scroll_top = self.state.scroll_top;

        let mut selected = self.state.selected_idx.unwrap_or(0);
        let result = {
            // Hit-testing is based on run geometry; selection-specific styling doesn't affect
            // line/rect boundaries, so we build runs without a selected row.
            let runs = self.build_runs(usize::MAX);
            route_selectable_list_mouse_with_config(
                mouse_event,
                &mut selected,
                total,
                |x, y| selection_id_at(body, x, y, scroll_top, &runs),
                SelectableListMouseConfig {
                    hover_select: false,
                    ..SelectableListMouseConfig::default()
                },
            )
        };
        self.state.selected_idx = Some(selected);

        if matches!(result, SelectableListMouseResult::Activated)
            && let Some(kind) = model.selection_kinds.get(selected).copied()
        {
            self.activate_selection_kind(kind);
        }
        if result.handled() {
            model = self.build_model();
            self.ensure_selected_visible(&model, body.height as usize);
        }

        result.handled()
    }
}

