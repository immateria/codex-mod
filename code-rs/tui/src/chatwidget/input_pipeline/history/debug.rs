use super::super::prelude::*;

impl ChatWidget<'_> {
    pub(in crate::chatwidget) fn history_debug(&self, message: impl Into<String>) {
        if !history_cell_logging_enabled() {
            return;
        }
        let message = message.into();
        tracing::trace!(target: "code_history", "{message}");
        if let Some(buffer) = &self.history_debug_events {
            buffer.borrow_mut().push(message);
        }
    }

    pub(in crate::chatwidget) fn rehydrate_system_order_cache(&mut self, preserved: &[(String, HistoryId)]) {
        let prev = self.system_cell_by_id.len();
        self.system_cell_by_id.clear();

        for (key, hid) in preserved {
            if let Some(idx) = self
                .history_cell_ids
                .iter()
                .position(|maybe| maybe.map(|stored| stored == *hid).unwrap_or(false))
            {
                self.system_cell_by_id.insert(key.clone(), idx);
            }
        }

        self.history_debug(format!(
            "system_order_cache.rehydrate prev={} restored={} entries={}",
            prev,
            preserved.len(),
            self.system_cell_by_id.len()
        ));
    }
}
