impl ChatWidget<'_> {
    /// Briefly show the vertical scrollbar and schedule a redraw to hide it.
    pub(in crate::chatwidget) fn flash_scrollbar(&self) {
        layout_scroll::flash_scrollbar(self);
    }

    pub(in crate::chatwidget) fn ensure_image_cell_picker(&self, cell: &dyn HistoryCell) {
        if let Some(image) = cell
            .as_any()
            .downcast_ref::<crate::history_cell::ImageOutputCell>()
        {
            let picker = self.terminal_info.picker.clone();
            let font_size = self.terminal_info.font_size;
            image.ensure_picker_initialized(picker, font_size);
        }
    }

    pub(in crate::chatwidget) fn history_insert_with_key_global(
        &mut self,
        cell: Box<dyn HistoryCell>,
        key: OrderKey,
    ) -> usize {
        self.history_insert_with_key_global_tagged(cell, key, "untagged", None)
    }
}
