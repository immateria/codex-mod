use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders};

use super::super::{McpSettingsMode, McpSettingsView};

use super::model::{centered_overlay_rect, SERVER_ROWS, TOOL_ROWS};

impl McpSettingsView {
    pub(in crate::bottom_pane::settings_pages::mcp) fn handle_policy_editor_mouse_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        let outer = Block::default().borders(Borders::ALL).inner(area);
        self.handle_policy_editor_mouse_in_outer(mouse_event, outer)
    }

    pub(in crate::bottom_pane::settings_pages::mcp) fn handle_policy_editor_mouse_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_policy_editor_mouse_in_outer(mouse_event, area)
    }

    fn handle_policy_editor_mouse_in_outer(&mut self, mouse_event: MouseEvent, outer: Rect) -> bool {
        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {}
            _ => return false,
        }

        let overlay = centered_overlay_rect(outer, 76, 14);
        let inner = Block::default().borders(Borders::ALL).inner(overlay);
        let (row_start_y, row_count) = match &self.mode {
            McpSettingsMode::EditToolScheduling(_) => (inner.y.saturating_add(2), TOOL_ROWS.len()),
            McpSettingsMode::EditServerScheduling(_) => (inner.y.saturating_add(1), SERVER_ROWS.len()),
            McpSettingsMode::Main => return false,
        };
        let rows_end_y = inner.y.saturating_add(inner.height).saturating_sub(1);
        if mouse_event.row < row_start_y || mouse_event.row >= rows_end_y {
            return false;
        }
        let idx = mouse_event.row.saturating_sub(row_start_y) as usize;
        if idx >= row_count {
            return false;
        }

        let mut activate = false;
        let mode = std::mem::replace(&mut self.mode, McpSettingsMode::Main);
        let (handled, next_mode) = match mode {
            McpSettingsMode::EditServerScheduling(mut editor) => {
                let was_selected = editor.selected_row == idx;
                editor.set_selected_row(idx);
                activate = was_selected;
                (true, McpSettingsMode::EditServerScheduling(editor))
            }
            McpSettingsMode::EditToolScheduling(mut editor) => {
                let was_selected = editor.selected_row == idx;
                editor.set_selected_row(idx);
                activate = was_selected;
                (true, McpSettingsMode::EditToolScheduling(editor))
            }
            McpSettingsMode::Main => (false, McpSettingsMode::Main),
        };
        self.mode = next_mode;

        if activate {
            return self.handle_policy_editor_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        }

        handled
    }
}

