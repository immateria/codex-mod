use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use super::model::{AgentsOverviewState, AgentsSettingsContent};

impl AgentsSettingsContent {
    pub(super) fn overview_selection_at(
        state: &AgentsOverviewState,
        area: Rect,
        mouse_event: MouseEvent,
    ) -> Option<usize> {
        if area.width == 0 || area.height == 0 {
            return None;
        }
        if mouse_event.column < area.x
            || mouse_event.column >= area.x.saturating_add(area.width)
            || mouse_event.row < area.y
            || mouse_event.row >= area.y.saturating_add(area.height)
        {
            return None;
        }

        let rel_y = mouse_event.row.saturating_sub(area.y) as usize;
        let rows_len = state.rows.len();
        let command_len = state.commands.len();

        if rel_y >= 1 && rel_y < 1 + rows_len {
            return Some(rel_y - 1);
        }

        let add_agent_line = rows_len + 2;
        if rel_y == add_agent_line {
            return Some(rows_len);
        }

        let command_start = rows_len + 5;
        if rel_y >= command_start && rel_y < command_start + command_len {
            return Some(rows_len + 1 + (rel_y - command_start));
        }

        let add_command_line = command_start + command_len;
        if rel_y == add_command_line {
            return Some(rows_len + 1 + command_len);
        }

        None
    }
}

