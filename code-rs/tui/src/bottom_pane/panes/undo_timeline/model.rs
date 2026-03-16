use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::{UndoTimelineEntry, UndoTimelineEntryKind, UndoTimelineView, MAX_VISIBLE_LIST_ROWS};

impl UndoTimelineEntry {
    pub(super) fn list_line_count(&self) -> usize {
        let mut rows = 1;
        if self.summary.is_some() {
            rows += 1;
        }
        if self.timestamp_line.is_some() || self.relative_time.is_some() {
            rows += 1;
        }
        if self.stats_line.is_some() {
            rows += 1;
        }
        rows + 1
    }
}

impl UndoTimelineView {
    pub fn new(
        entries: Vec<UndoTimelineEntry>,
        initial_selected: usize,
        app_event_tx: AppEventSender,
    ) -> Self {
        let selected = initial_selected.min(entries.len().saturating_sub(1));
        let mut view = Self {
            entries,
            selected,
            top_row: 0,
            restore_files: true,
            restore_conversation: true,
            restore_files_forced_off: false,
            restore_conversation_forced_off: false,
            app_event_tx,
            is_complete: false,
        };
        view.align_toggles_to_selection();
        view.ensure_visible();
        view
    }

    pub(super) fn selected_entry(&self) -> Option<&UndoTimelineEntry> {
        self.entries.get(self.selected)
    }

    pub(super) fn move_up(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.entries.len().saturating_sub(1);
        } else {
            self.selected -= 1;
        }
        self.align_toggles_to_selection();
        self.ensure_visible();
    }

    pub(super) fn move_down(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.entries.len();
        self.align_toggles_to_selection();
        self.ensure_visible();
    }

    pub(super) fn page_up(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let mut remaining = MAX_VISIBLE_LIST_ROWS;
        while remaining > 0 && self.selected > 0 {
            self.selected -= 1;
            remaining = remaining.saturating_sub(self.entries[self.selected].list_line_count());
        }
        self.align_toggles_to_selection();
        self.ensure_visible();
    }

    pub(super) fn page_down(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let mut remaining = MAX_VISIBLE_LIST_ROWS;
        while remaining > 0 && self.selected + 1 < self.entries.len() {
            self.selected += 1;
            remaining = remaining.saturating_sub(self.entries[self.selected].list_line_count());
        }
        self.align_toggles_to_selection();
        self.ensure_visible();
    }

    pub(super) fn go_home(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = 0;
        self.align_toggles_to_selection();
        self.ensure_visible();
    }

    pub(super) fn go_end(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = self.entries.len().saturating_sub(1);
        self.align_toggles_to_selection();
        self.ensure_visible();
    }

    pub(super) fn align_toggles_to_selection(&mut self) {
        let Some(entry) = self.entries.get(self.selected) else {
            return;
        };
        let files_available = entry.files_available;
        let conversation_available = entry.conversation_available;

        if files_available {
            if self.restore_files_forced_off {
                self.restore_files = true;
            }
            self.restore_files_forced_off = false;
        } else {
            self.restore_files = false;
            self.restore_files_forced_off = true;
        }

        if conversation_available {
            if self.restore_conversation_forced_off {
                self.restore_conversation = true;
            }
            self.restore_conversation_forced_off = false;
        } else {
            self.restore_conversation = false;
            self.restore_conversation_forced_off = true;
        }
    }

    pub(super) fn ensure_visible(&mut self) {
        if self.entries.is_empty() {
            self.top_row = 0;
            return;
        }

        let mut cumulative = 0usize;
        for (idx, entry) in self.entries.iter().enumerate() {
            if idx < self.selected {
                cumulative = cumulative.saturating_add(entry.list_line_count());
            }
        }
        let selected_height = self
            .entries
            .get(self.selected)
            .map(UndoTimelineEntry::list_line_count)
            .unwrap_or(1);

        if cumulative < self.top_row {
            self.top_row = cumulative;
        } else {
            let bottom = cumulative + selected_height;
            let window_bottom = self.top_row + MAX_VISIBLE_LIST_ROWS;
            if bottom > window_bottom {
                self.top_row = bottom.saturating_sub(MAX_VISIBLE_LIST_ROWS);
            }
        }
    }

    pub(super) fn toggle_files(&mut self) -> bool {
        let Some(entry) = self.selected_entry() else {
            return false;
        };
        if !entry.files_available {
            return false;
        }
        self.restore_files = !self.restore_files;
        self.restore_files_forced_off = false;
        true
    }

    pub(super) fn toggle_conversation(&mut self) -> bool {
        let Some(entry) = self.selected_entry() else {
            return false;
        };
        if !entry.conversation_available {
            return false;
        }
        self.restore_conversation = !self.restore_conversation;
        self.restore_conversation_forced_off = false;
        true
    }

    pub(super) fn confirm(&mut self) {
        if let Some(entry) = self.selected_entry() {
            match entry.kind {
                UndoTimelineEntryKind::Snapshot { ref commit } => {
                    self.app_event_tx.send(AppEvent::PerformUndoRestore {
                        commit: Some(commit.clone()),
                        restore_files: self.restore_files && entry.files_available,
                        restore_conversation: self.restore_conversation && entry.conversation_available,
                    });
                    self.is_complete = true;
                }
                UndoTimelineEntryKind::Current => {
                    self.is_complete = true;
                }
            }
        }
    }

    fn total_list_height(&self) -> usize {
        self.entries.iter().map(UndoTimelineEntry::list_line_count).sum()
    }

    pub(super) fn visible_range(&self) -> (usize, usize) {
        let total = self.total_list_height();
        if total <= MAX_VISIBLE_LIST_ROWS {
            return (0, self.entries.len());
        }

        let mut start_entry = 0usize;
        let mut spent = 0usize;
        while start_entry < self.entries.len()
            && spent + self.entries[start_entry].list_line_count() <= self.top_row
        {
            spent = spent.saturating_add(self.entries[start_entry].list_line_count());
            start_entry += 1;
        }

        if start_entry > self.selected {
            start_entry = self.selected;
        }

        let mut span: usize = self.entries[start_entry..=self.selected]
            .iter()
            .map(UndoTimelineEntry::list_line_count)
            .sum();
        while start_entry < self.selected && span > MAX_VISIBLE_LIST_ROWS {
            span = span.saturating_sub(self.entries[start_entry].list_line_count());
            start_entry += 1;
        }

        let mut end_entry = start_entry;
        let mut used = 0usize;
        while end_entry < self.entries.len() {
            let lines = self.entries[end_entry].list_line_count();
            if used + lines > MAX_VISIBLE_LIST_ROWS && end_entry > self.selected {
                break;
            }
            used = used.saturating_add(lines);
            end_entry += 1;
            if used >= MAX_VISIBLE_LIST_ROWS && end_entry > self.selected {
                break;
            }
        }

        if end_entry <= self.selected && end_entry < self.entries.len() {
            while end_entry <= self.selected && end_entry < self.entries.len() {
                used = used.saturating_add(self.entries[end_entry].list_line_count());
                end_entry += 1;
            }
        }

        (start_entry, end_entry)
    }
}

