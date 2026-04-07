use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::StatusLineSetupView;

impl StatusLineSetupView {
    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        match mouse_event.kind {
            MouseEventKind::ScrollUp => {
                self.move_selection_up();
                true
            }
            MouseEventKind::ScrollDown => {
                self.move_selection_down();
                true
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let status_x = area.x.saturating_add(2);
                let status_width = area.width.saturating_sub(4);
                let within_status_x = mouse_event.column >= status_x
                    && mouse_event.column < status_x.saturating_add(status_width);

                // Account for scroll offset and block border (1 line top border)
                let scroll = self.scroll_offset.get();
                let inner_y = area.y.saturating_add(1); // block top border

                let lane_row = inner_y.saturating_add(2).saturating_sub(scroll);
                if within_status_x && mouse_event.row == lane_row && lane_row >= inner_y {
                    self.switch_active_lane();
                    return true;
                }

                let primary_row = inner_y.saturating_add(3).saturating_sub(scroll);
                if within_status_x && mouse_event.row == primary_row && primary_row >= inner_y {
                    self.toggle_primary_lane();
                    return true;
                }

                // Item rows start at line 5 (header_lines) in the virtual document
                let header_lines: u16 = 5;
                for idx in 0..self.choices_for_active_lane().len() {
                    let virtual_y = header_lines + idx as u16;
                    if virtual_y < scroll {
                        continue;
                    }
                    let screen_y = inner_y.saturating_add(virtual_y - scroll);
                    if screen_y >= area.y.saturating_add(area.height).saturating_sub(1) {
                        break; // below visible area
                    }
                    if mouse_event.row == screen_y && within_status_x {
                        if self.selected_index_for_active_lane() == idx {
                            self.toggle_selected();
                        } else {
                            self.set_selected_index_for_active_lane(idx);
                        }
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }
}

