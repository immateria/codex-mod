use super::{FOOTER_LINE_COUNT, ModelSelectionView, SUMMARY_LINE_COUNT};

use crate::app_event::AppEvent;

use super::super::model_selection_state::SelectionAction;

impl ModelSelectionView {
    pub(super) fn entry_count(&self) -> usize {
        self.data.entry_count()
    }

    pub(super) fn content_line_count(&self) -> u16 {
        self.data
            .content_line_count()
            .saturating_sub((SUMMARY_LINE_COUNT + FOOTER_LINE_COUNT) as u16)
    }

    pub(super) fn move_selection_up(&mut self) {
        let total = self.entry_count();
        if total == 0 {
            return;
        }
        self.selected_index = if self.selected_index == 0 {
            total - 1
        } else {
            self.selected_index.saturating_sub(1)
        };
        self.ensure_selected_visible();
    }

    pub(super) fn move_selection_down(&mut self) {
        let total = self.entry_count();
        if total == 0 {
            return;
        }
        self.selected_index = (self.selected_index + 1) % total;
        self.ensure_selected_visible();
    }

    pub(super) fn ensure_selected_visible(&mut self) {
        let body_height = self.visible_body_rows.get();
        if body_height == 0 {
            return;
        }

        let selected_line = self.selected_body_line(self.selected_index);
        if selected_line < self.scroll_offset {
            self.scroll_offset = selected_line;
        }
        let visible_end = self.scroll_offset + body_height;
        if selected_line >= visible_end {
            self.scroll_offset = selected_line.saturating_sub(body_height) + 1;
        }
    }

    pub(super) fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub(super) fn scroll_down(&mut self) {
        let total_lines = self.content_line_count() as usize;
        let max_scroll = total_lines.saturating_sub(self.visible_body_rows.get());
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        }
    }

    pub(super) fn select_item(&mut self, index: usize) {
        let total = self.entry_count();
        if index >= total {
            return;
        }
        self.selected_index = index;
        self.confirm_selection();
    }

    pub(super) fn selected_body_line(&self, entry_index: usize) -> usize {
        self.data.entry_line(entry_index).saturating_sub(SUMMARY_LINE_COUNT)
    }

    pub(super) fn confirm_selection(&mut self) {
        if let Some(entry) = self.data.entry_at(self.selected_index)
            && let Some(action) = self.data.apply_selection(entry)
        {
            self.dispatch_selection_action(action);
        }
    }

    pub(super) fn dispatch_selection_action(&mut self, action: SelectionAction) {
        let closes_view = action.closes_view();
        self.data
            .target
            .dispatch_selection_action(&self.app_event_tx, &action);
        if closes_view {
            self.send_closed(true);
        }
    }

    pub(super) fn send_closed(&mut self, accepted: bool) {
        if self.is_complete {
            return;
        }
        self.app_event_tx.send(AppEvent::ModelSelectionClosed {
            target: self.data.target.into(),
            accepted,
        });
        self.is_complete = true;
    }
}
