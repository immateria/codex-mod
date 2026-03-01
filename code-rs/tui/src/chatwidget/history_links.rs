use super::*;

impl ChatWidget<'_> {
    pub(in crate::chatwidget) fn record_js_repl_child_call(
        &mut self,
        parent_call_id: &str,
        child_call_id: &str,
    ) {
        for cell in self.history_cells.iter_mut().rev() {
            if let Some(js_cell) = cell
                .as_any_mut()
                .downcast_mut::<crate::history_cell::JsReplCell>()
                && js_cell.record.call_id.as_deref() == Some(parent_call_id)
            {
                if js_cell.record_child_call_id(child_call_id) {
                    // Changing the JS header line affects height caching and needs a redraw.
                    self.invalidate_height_cache();
                    self.request_redraw();
                }
                return;
            }
        }
    }
}

