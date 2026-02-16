/// Generic scroll/selection state for a vertical list menu.
///
/// Encapsulates the common behavior of a selectable list that supports:
/// - Optional selection (None when list is empty)
/// - Wrap-around navigation on Up/Down
/// - Maintaining a scroll window (`scroll_top`) so the selected row stays visible
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ScrollState {
    pub selected_idx: Option<usize>,
    pub scroll_top: usize,
}

impl ScrollState {
    pub fn new() -> Self {
        Self {
            selected_idx: None,
            scroll_top: 0,
        }
    }

    /// Reset selection and scroll.
    pub fn reset(&mut self) {
        self.selected_idx = None;
        self.scroll_top = 0;
    }

    /// Clamp selection to be within the [0, len-1] range, or None when empty.
    pub fn clamp_selection(&mut self, len: usize) {
        self.selected_idx = match len {
            0 => None,
            _ => Some(self.selected_idx.unwrap_or(0).min(len - 1)),
        };
        if len == 0 {
            self.scroll_top = 0;
        }
    }

    /// Move selection up by one, wrapping to the bottom when necessary.
    pub fn move_up_wrap(&mut self, len: usize) {
        if len == 0 {
            self.selected_idx = None;
            self.scroll_top = 0;
            return;
        }
        self.selected_idx = Some(match self.selected_idx {
            Some(idx) if idx > 0 => idx - 1,
            Some(_) => len - 1,
            None => 0,
        });
    }

    /// Move selection down by one, wrapping to the top when necessary.
    pub fn move_down_wrap(&mut self, len: usize) {
        if len == 0 {
            self.selected_idx = None;
            self.scroll_top = 0;
            return;
        }
        self.selected_idx = Some(match self.selected_idx {
            Some(idx) if idx + 1 < len => idx + 1,
            _ => 0,
        });
    }

    /// Adjust `scroll_top` so that the current `selected_idx` is visible within
    /// the window of `visible_rows`.
    pub fn ensure_visible(&mut self, len: usize, visible_rows: usize) {
        if len == 0 || visible_rows == 0 {
            self.scroll_top = 0;
            return;
        }
        if let Some(sel) = self.selected_idx {
            if sel < self.scroll_top {
                self.scroll_top = sel;
            } else {
                let bottom = self.scroll_top + visible_rows - 1;
                if sel > bottom {
                    self.scroll_top = sel + 1 - visible_rows;
                }
            }
        } else {
            self.scroll_top = 0;
        }
    }

    /// Return popup height in rows for a list, clamped to `[1, max_rows]`.
    pub fn popup_required_height(total_rows: usize, max_rows: usize) -> u16 {
        total_rows.clamp(1, max_rows.max(1)) as u16
    }

    /// Move selection up with wrap and keep it visible within `max_visible_rows`.
    pub fn move_up_wrap_visible(&mut self, len: usize, max_visible_rows: usize) {
        self.move_up_wrap(len);
        self.ensure_visible(len, max_visible_rows.min(len));
    }

    /// Move selection down with wrap and keep it visible within `max_visible_rows`.
    pub fn move_down_wrap_visible(&mut self, len: usize, max_visible_rows: usize) {
        self.move_down_wrap(len);
        self.ensure_visible(len, max_visible_rows.min(len));
    }

    /// Select by visible row index (relative to the current `scroll_top`).
    ///
    /// Returns `true` when `visible_row` maps to a valid item.
    pub fn select_visible_row(&mut self, total_rows: usize, visible_row: usize) -> bool {
        if total_rows == 0 {
            return false;
        }

        let scroll_top = self.scroll_top.min(total_rows.saturating_sub(1));
        let actual_idx = scroll_top.saturating_add(visible_row);
        if actual_idx >= total_rows {
            return false;
        }

        self.selected_idx = Some(actual_idx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollState;

    #[test]
    fn popup_required_height_clamps_to_one_and_max() {
        assert_eq!(ScrollState::popup_required_height(0, 7), 1);
        assert_eq!(ScrollState::popup_required_height(3, 7), 3);
        assert_eq!(ScrollState::popup_required_height(20, 7), 7);
        assert_eq!(ScrollState::popup_required_height(20, 0), 1);
    }

    #[test]
    fn select_visible_row_translates_with_scroll_top() {
        let mut state = ScrollState::new();
        state.scroll_top = 4;
        assert!(state.select_visible_row(10, 2));
        assert_eq!(state.selected_idx, Some(6));

        assert!(!state.select_visible_row(10, 99));
        assert_eq!(state.selected_idx, Some(6));
    }

    #[test]
    fn move_wrap_visible_keeps_selection_in_window() {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        state.scroll_top = 0;
        state.move_up_wrap_visible(5, 3);
        assert_eq!(state.selected_idx, Some(4));
        assert_eq!(state.scroll_top, 2);

        state.move_down_wrap_visible(5, 3);
        assert_eq!(state.selected_idx, Some(0));
        assert_eq!(state.scroll_top, 0);
    }
}
