use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::layout::Rect;

use crate::app_event::AppEvent;
use crate::bottom_pane::settings_ui::list_detail_page::SettingsListDetailMode;
use crate::ui_interaction::{
    route_selectable_list_mouse_with_config,
    wrap_next,
    wrap_prev,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

use super::{LoginAccountsState, ViewMode};

impl LoginAccountsState {
    fn selectable_row_count(&self) -> usize {
        self.accounts.len().saturating_add(2)
    }

    fn select_previous_row(&mut self) {
        let total_rows = self.selectable_row_count();
        self.selected = wrap_prev(self.selected, total_rows);
    }

    fn select_next_row(&mut self) {
        let total_rows = self.selectable_row_count();
        self.selected = wrap_next(self.selected, total_rows);
    }

    fn activate_selected_row(&mut self) {
        let account_count = self.accounts.len();
        if self.selected < account_count {
            if let Some(account) = self.accounts.get(self.selected) {
                let label = account.label.clone();
                let mode = account.mode;
                if self.activate_account(account.id.clone(), mode) {
                    self.mode = ViewMode::List;
                    self.send_tail(format!("Switched to {label}"));
                    self.set_complete();
                }
            }
        } else if self.selected == account_count {
            self.set_complete();
            self.app_event_tx.send(AppEvent::ShowLoginAddAccount);
        } else {
            self.open_store_paths_editor();
        }
    }

    pub(super) fn list_hit_area_for_mouse(&self, area: Rect) -> Option<Rect> {
        let layout = self.accounts_page().layout(area)?;
        match layout.mode {
            SettingsListDetailMode::Split { list_inner, .. } => Some(list_inner),
            SettingsListDetailMode::Compact { content } => Some(content),
        }
    }

    pub(super) fn list_selection_for_position(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        if x < area.x
            || x >= area.x.saturating_add(area.width)
            || y < area.y
            || y >= area.y.saturating_add(area.height)
        {
            return None;
        }
        let rel_y = y.saturating_sub(area.y);

        let account_count = u16::try_from(self.accounts.len()).unwrap_or(u16::MAX);
        let empty_offset = u16::from(self.accounts.is_empty());

        if rel_y < account_count {
            return Some(rel_y as usize);
        }

        let add_row_y = account_count
            .saturating_add(2)
            .saturating_add(empty_offset);
        let store_paths_row_y = account_count
            .saturating_add(3)
            .saturating_add(empty_offset);
        if rel_y == add_row_y {
            Some(self.add_account_index())
        } else if rel_y == store_paths_row_y {
            Some(self.store_paths_index())
        } else {
            None
        }
    }

    pub(super) fn handle_list_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        let Some(list_area) = self.list_hit_area_for_mouse(area) else {
            return false;
        };

        let mut selected = self.selected;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            self.selectable_row_count(),
            |x, y| self.list_selection_for_position(list_area, x, y),
            SelectableListMouseConfig {
                require_pointer_hit_for_scroll: true,
                scroll_behavior: ScrollSelectionBehavior::Clamp,
                ..SelectableListMouseConfig::default()
            },
        );
        self.selected = selected;

        if matches!(result, SelectableListMouseResult::Activated) {
            self.activate_selected_row();
        }
        result.handled()
    }

    pub(super) fn handle_list_key(&mut self, key_event: KeyEvent) -> bool {
        let account_count = self.accounts.len();

        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.set_complete();
                true
            }
            KeyCode::Up => {
                self.select_previous_row();
                true
            }
            KeyCode::Down => {
                self.select_next_row();
                true
            }
            KeyCode::Char('d') => {
                if self.selected < account_count
                    && let Some(account) = self.accounts.get(self.selected)
                {
                    self.mode = ViewMode::ConfirmRemove { account_id: account.id.clone() };
                    return true;
                }
                false
            }
            KeyCode::Char('r') => {
                self.reload_accounts();
                true
            }
            KeyCode::Char('p') => {
                self.open_store_paths_editor();
                true
            }
            KeyCode::Enter => {
                self.activate_selected_row();
                true
            }
            _ => false,
        }
    }
}
