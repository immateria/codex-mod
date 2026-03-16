use super::*;

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::fields::BorderedField;
use crate::bottom_pane::settings_ui::line_runs::selection_id_at as selection_run_id_at;
use crate::ui_interaction::{
    route_selectable_list_mouse_with_config,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

impl ShellSelectionView {
    pub(super) fn update_action_hover(&mut self, area: Rect, mouse_pos: (u16, u16)) -> bool {
        if !self.custom_input_mode {
            return false;
        }

        let page = self.edit_page();
        let Some(layout) = page.framed().layout(area) else {
            return false;
        };
        let buttons = self.edit_buttons();
        let hovered =
            page.standard_action_at_end(&layout, mouse_pos.0, mouse_pos.1, &buttons);
        if hovered == self.hovered_action {
            return false;
        }
        self.hovered_action = hovered;
        true
    }

    fn handle_mouse_event_direct_in_chrome(
        &mut self,
        chrome: ChromeMode,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        if self.custom_input_mode {
            let page = self.edit_page();
            let Some(layout) = page.layout_in_chrome(chrome, area) else {
                return false;
            };
            let buttons = self.edit_buttons();
            return match mouse_event.kind {
                MouseEventKind::Moved => {
                    let hovered = page.standard_action_at_end(
                        &layout,
                        mouse_event.column,
                        mouse_event.row,
                        &buttons,
                    );
                    if hovered == self.hovered_action {
                        return false;
                    }
                    self.hovered_action = hovered;
                    true
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(action) = page.standard_action_at_end(
                        &layout,
                        mouse_event.column,
                        mouse_event.row,
                        &buttons,
                    ) {
                        self.selected_action = action;
                        self.edit_focus = EditFocus::Actions;
                        self.activate_edit_action(action);
                        return true;
                    }

                    let field_outer = Rect::new(layout.body.x, layout.body.y, layout.body.width, 3);
                    if field_outer.contains(ratatui::layout::Position {
                        x: mouse_event.column,
                        y: mouse_event.row,
                    }) {
                        let focus_changed = self.edit_focus != EditFocus::Field;
                        self.edit_focus = EditFocus::Field;
                        self.hovered_action = None;
                        let inner = BorderedField::new(
                            "Shell command",
                            matches!(self.edit_focus, EditFocus::Field),
                        )
                        .inner(field_outer);
                        let handled = self
                            .custom_field
                            .handle_mouse_click(mouse_event.column, mouse_event.row, inner);
                        return focus_changed || handled;
                    }

                    false
                }
                _ => false,
            };
        }

        let page = self.list_page();
        let Some(layout) = page.layout_in_chrome(chrome, area) else {
            return false;
        };
        let runs = self.list_runs();

        let mut selected = self.selected_index;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            self.item_count(),
            |x, y| selection_run_id_at(layout.body, x, y, 0, &runs),
            SelectableListMouseConfig {
                hover_select: false,
                scroll_behavior: ScrollSelectionBehavior::Wrap,
                ..SelectableListMouseConfig::default()
            },
        );

        let mut handled = false;
        if selected != self.selected_index {
            self.selected_index = selected;
            handled = true;
        }

        if matches!(result, SelectableListMouseResult::Activated) {
            self.select_item(self.selected_index);
            handled = true;
        }

        handled || result.handled()
    }

    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_in_chrome(ChromeMode::ContentOnly, mouse_event, area)
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_in_chrome(ChromeMode::Framed, mouse_event, area)
    }
}
