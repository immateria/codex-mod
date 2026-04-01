/// Hit-test repeated vertical row groups.
///
/// Example: `first_row=4`, `active_rows_per_group=2`, `group_stride=3`
/// means each group starts every 3 rows and the first 2 rows are clickable.
#[cfg(feature = "browser-automation")]
pub(crate) fn hit_test_repeating_rows(
    area: Rect,
    x: u16,
    y: u16,
    first_row: u16,
    active_rows_per_group: u16,
    group_stride: u16,
    group_count: usize,
) -> Option<usize> {
    if group_count == 0 || group_stride == 0 {
        return None;
    }
    if !contains_point(area, x, y) {
        return None;
    }

    let rel_y = y.saturating_sub(area.y);
    if rel_y < first_row {
        return None;
    }

    let offset = rel_y - first_row;
    let idx = (offset / group_stride) as usize;
    if idx >= group_count {
        return None;
    }

    if offset % group_stride < active_rows_per_group {
        Some(idx)
    } else {
        None
    }
}

pub(crate) fn wrap_prev(current: usize, count: usize) -> usize {
    if count == 0 {
        0
    } else if current == 0 {
        count - 1
    } else {
        current - 1
    }
}

pub(crate) fn wrap_next(current: usize, count: usize) -> usize {
    if count == 0 {
        0
    } else {
        (current + 1) % count
    }
}

pub(crate) fn clamp_index(current: usize, count: usize) -> usize {
    if count == 0 {
        0
    } else {
        current.min(count - 1)
    }
}

pub(crate) const fn redraw_if(handled: bool) -> ConditionalUpdate {
    if handled {
        ConditionalUpdate::NeedsRedraw
    } else {
        ConditionalUpdate::NoRedraw
    }
}

/// A centered visible window over a vertically scrollable list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ListWindow {
    pub start: usize,
    pub end: usize,
    pub visible: usize,
}

impl ListWindow {
    pub fn centered(total: usize, visible: usize, selected: usize) -> Self {
        if total == 0 || visible == 0 {
            return Self {
                start: 0,
                end: 0,
                visible,
            };
        }

        let clamped_selected = selected.min(total - 1);
        let mut start = 0usize;
        if total > visible {
            let half = visible / 2;
            if clamped_selected > half {
                start = clamped_selected - half;
            }
            if start + visible > total {
                start = total - visible;
            }
        }
        let end = (start + visible).min(total);
        Self {
            start,
            end,
            visible,
        }
    }

    pub fn index_for_relative_row(self, rel_y: usize) -> Option<usize> {
        if rel_y >= self.visible {
            return None;
        }
        let idx = self.start + rel_y;
        if idx < self.end { Some(idx) } else { None }
    }
}

pub(crate) fn centered_scroll_top(
    selected: usize,
    total_lines: usize,
    viewport_height: usize,
) -> usize {
    ListWindow::centered(total_lines, viewport_height, selected).start
}
