use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::StatusLineSetupView;

impl StatusLineSetupView {
    fn active_item_row_bounds(area: Rect, row_index: usize) -> Option<Rect> {
        let y = area.y.saturating_add(5).saturating_add(
            u16::try_from(row_index).unwrap_or(u16::MAX),
        );
        if y >= area.y.saturating_add(area.height) {
            return None;
        }
        Some(Rect {
            x: area.x.saturating_add(2),
            y,
            width: area.width.saturating_sub(4),
            height: 1,
        })
    }

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

                let lane_row = area.y.saturating_add(2);
                if within_status_x && mouse_event.row == lane_row {
                    self.switch_active_lane();
                    return true;
                }

                let primary_row = area.y.saturating_add(3);
                if within_status_x && mouse_event.row == primary_row {
                    self.toggle_primary_lane();
                    return true;
                }

                for idx in 0..self.choices_for_active_lane().len() {
                    let Some(row) = Self::active_item_row_bounds(area, idx) else {
                        continue;
                    };
                    let within_x = mouse_event.column >= row.x
                        && mouse_event.column < row.x.saturating_add(row.width);
                    let within_y = mouse_event.row == row.y;
                    if !within_x || !within_y {
                        continue;
                    }

                    if self.selected_index_for_active_lane() == idx {
                        self.toggle_selected();
                    } else {
                        self.set_selected_index_for_active_lane(idx);
                    }
                    return true;
                }
                false
            }
            _ => false,
        }
    }
}

