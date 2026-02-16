use super::*;

impl ChatWidget<'_> {
    /// Defer or handle an interrupt based on whether we're streaming
    pub(in super::super::super) fn defer_or_handle<F1, F2>(&mut self, defer_fn: F1, handle_fn: F2)
    where
        F1: FnOnce(&mut interrupts::InterruptManager),
        F2: FnOnce(&mut Self),
    {
        if self.is_write_cycle_active() {
            defer_fn(&mut self.interrupts);
            self.schedule_interrupt_flush_check();
        } else {
            handle_fn(self);
        }
    }

    // removed: next_sequence; plan updates are inserted immediately

    // Removed order-adjustment helpers; ordering now uses stable order keys on insert.

    /// Mark that the widget needs to be redrawn
    pub(in super::super::super) fn mark_needs_redraw(&mut self) {
        // Clean up fully faded cells before redraw. If any are removed,
        // invalidate the height cache since indices shift and our cache is
        // keyed by (idx,width).
        let before_len = self.history_cells.len();
        if before_len > 0 {
            let old_cells = std::mem::take(&mut self.history_cells);
            let old_ids = std::mem::take(&mut self.history_cell_ids);
            debug_assert_eq!(
                old_cells.len(),
                old_ids.len(),
                "history ids out of sync with cells"
            );
            let mut removed_any = false;
            let mut kept_cells = Vec::with_capacity(old_cells.len());
            let mut kept_ids = Vec::with_capacity(old_ids.len());
            for (cell, id) in old_cells.into_iter().zip(old_ids.into_iter()) {
                if cell.should_remove() {
                    removed_any = true;
                    continue;
                }
                kept_ids.push(id);
                kept_cells.push(cell);
            }
            self.history_cells = kept_cells;
            self.history_cell_ids = kept_ids;
            if removed_any {
                self.invalidate_height_cache();
            }
        } else if !self.history_cell_ids.is_empty() {
            self.history_cell_ids.clear();
        }

        // Send a redraw event to trigger UI update
        self.app_event_tx.send(AppEvent::RequestRedraw);
    }

    /// Periodic tick to commit at most one queued line to history,
    /// animating the output.
    pub(crate) fn on_commit_tick(&mut self) {
        streaming::on_commit_tick(self);
    }
    pub(in super::super::super) fn is_write_cycle_active(&self) -> bool {
        streaming::is_write_cycle_active(self)
    }

    pub(in super::super::super) fn flush_interrupt_queue(&mut self) {
        let mut mgr = std::mem::take(&mut self.interrupts);
        mgr.flush_all(self);
        self.interrupts = mgr;
    }
}
