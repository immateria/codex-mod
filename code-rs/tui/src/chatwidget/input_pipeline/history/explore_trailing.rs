use super::super::prelude::*;

impl ChatWidget<'_> {
    /// Clean up faded-out animation cells
    pub(in crate::chatwidget) fn process_animation_cleanup(&mut self) {
        // With trait-based cells, we can't easily detect and clean up specific cell types
        // Animation cleanup is now handled differently
    }

    fn last_meaningful_history_index_for_explore(&self) -> Option<usize> {
        for idx in (0..self.history_cells.len()).rev() {
            let cell = &self.history_cells[idx];

            if cell.should_remove() {
                continue;
            }

            match cell.kind() {
                history_cell::HistoryCellType::Reasoning
                | history_cell::HistoryCellType::Loading
                | history_cell::HistoryCellType::PlanUpdate => continue,
                _ => {}
            }

            if cell
                .as_any()
                .downcast_ref::<history_cell::WaitStatusCell>()
                .is_some()
            {
                continue;
            }

            if self.cell_lines_trimmed_is_empty(idx, cell.as_ref()) {
                continue;
            }

            return Some(idx);
        }

        None
    }

    pub(in crate::chatwidget) fn refresh_explore_trailing_flags(&mut self) -> bool {
        let last_meaningful = self.last_meaningful_history_index_for_explore();
        let mut updated = false;
        for idx in 0..self.history_cells.len() {
            let is_explore = self.history_cells[idx]
                .as_any()
                .downcast_ref::<history_cell::ExploreAggregationCell>()
                .is_some();
            if !is_explore {
                continue;
            }

            let hold_title = last_meaningful.is_none_or(|last| idx >= last);

            if let Some(explore_cell) = self.history_cells[idx]
                .as_any_mut()
                .downcast_mut::<history_cell::ExploreAggregationCell>()
                && explore_cell.set_force_exploring_header(hold_title) {
                    updated = true;
                    if let Some(Some(id)) = self.history_cell_ids.get(idx) {
                        self.history_render.invalidate_history_id(*id);
                    }
                }
        }

        if updated {
            self.invalidate_height_cache();
            self.request_redraw();
        }

        updated
    }

    pub(in crate::chatwidget) fn rendered_explore_should_hold(&self, idx: usize) -> bool {
        if idx >= self.history_cells.len() {
            return true;
        }

        let Some(last_meaningful) = self.last_meaningful_history_index_for_explore() else {
            return true;
        };
        idx >= last_meaningful
    }
}
