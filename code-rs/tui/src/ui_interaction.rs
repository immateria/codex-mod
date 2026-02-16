use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::buffer::Buffer;
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::bottom_pane::ConditionalUpdate;

/// A vertical hit region in a view, expressed as line offsets from an area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RelativeHitRegion {
    /// Logical id returned when this region is hit.
    pub id: usize,
    /// Starting row offset from `area.y`.
    pub start_row: u16,
    /// Number of rows in this region.
    pub row_count: u16,
}

impl RelativeHitRegion {
    pub const fn new(id: usize, start_row: u16, row_count: u16) -> Self {
        Self {
            id,
            start_row,
            row_count,
        }
    }

    fn contains_relative_row(self, rel_y: u16) -> bool {
        rel_y >= self.start_row && rel_y < self.start_row.saturating_add(self.row_count)
    }
}

pub(crate) fn contains_point(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x
        && x < area.x.saturating_add(area.width)
        && y >= area.y
        && y < area.y.saturating_add(area.height)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeaderBodyFooterLayout {
    pub header: Rect,
    pub body: Rect,
    pub footer: Rect,
}

/// Split an area into header/body/footer sections with minimum body space.
///
/// Returns `None` when the area is too short to reserve the requested header/footer
/// rows while still leaving at least `min_body_rows` for the body.
pub(crate) fn split_header_body_footer(
    area: Rect,
    header_rows: usize,
    footer_rows: usize,
    min_body_rows: u16,
) -> Option<HeaderBodyFooterLayout> {
    if area.width == 0 || area.height == 0 {
        return None;
    }

    let header_h = (header_rows as u16).min(area.height);
    let footer_h = (footer_rows as u16).min(area.height.saturating_sub(1));
    let min_body = min_body_rows.max(1);
    if area.height <= header_h.saturating_add(footer_h).saturating_add(min_body) {
        return None;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_h),
            Constraint::Min(min_body),
            Constraint::Length(footer_h),
        ])
        .split(area);

    Some(HeaderBodyFooterLayout {
        header: chunks[0],
        body: chunks[1],
        footer: chunks[2],
    })
}

/// Split an area into left/right panes only when the viewport is large enough.
pub(crate) fn split_two_pane_when_room(
    area: Rect,
    min_width: u16,
    min_height: u16,
    left_percent: u16,
) -> Option<(Rect, Rect)> {
    if area.width < min_width || area.height < min_height || left_percent == 0 || left_percent >= 100
    {
        return None;
    }

    let right_percent = 100u16.saturating_sub(left_percent);
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_percent),
            Constraint::Percentage(right_percent),
        ])
        .split(area);
    Some((panes[0], panes[1]))
}

/// Shared layout for views with a scrollable content viewport and pinned footer rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PinnedFooterLayout {
    /// Scrollable content viewport (top section).
    pub viewport: Rect,
    /// Bottom action/hotkey row (always pinned when height permits).
    pub action_row: Rect,
    /// Optional status row pinned above `action_row`.
    pub status_row: Rect,
}

/// Split a vertical area into a scrollable viewport with pinned bottom rows.
///
/// `action_rows` are always reserved first. `status_rows` are only reserved when there is
/// enough room to also keep at least `min_viewport_rows_for_status` rows in the viewport.
pub(crate) fn split_pinned_footer_layout(
    area: Rect,
    action_rows: u16,
    status_rows: u16,
    min_viewport_rows_for_status: u16,
) -> PinnedFooterLayout {
    if area.width == 0 || area.height == 0 {
        return PinnedFooterLayout {
            viewport: Rect::default(),
            action_row: Rect::default(),
            status_row: Rect::default(),
        };
    }

    let available = area.height;
    let action_h = action_rows.min(available);
    let remaining_after_action = available.saturating_sub(action_h);

    let status_h = if remaining_after_action >= min_viewport_rows_for_status.saturating_add(status_rows) {
        status_rows.min(remaining_after_action)
    } else {
        0
    };

    let viewport_h = available.saturating_sub(action_h).saturating_sub(status_h);
    let viewport = Rect::new(area.x, area.y, area.width, viewport_h);
    let status_row = if status_h > 0 {
        Rect::new(
            area.x,
            area.y.saturating_add(viewport_h),
            area.width,
            status_h,
        )
    } else {
        Rect::default()
    };
    let action_row = if action_h > 0 {
        Rect::new(
            area.x,
            area.y
                .saturating_add(viewport_h)
                .saturating_add(status_h),
            area.width,
            action_h,
        )
    } else {
        Rect::default()
    };

    PinnedFooterLayout {
        viewport,
        action_row,
        status_row,
    }
}

/// Clip a virtual vertical section into a viewport, applying `scroll_top`.
pub(crate) fn clipped_vertical_rect_with_scroll(
    viewport: Rect,
    content_top: usize,
    content_h: usize,
    scroll_top: usize,
) -> Rect {
    if viewport.width == 0 || viewport.height == 0 || content_h == 0 {
        return Rect::new(viewport.x, viewport.y, viewport.width, 0);
    }

    let y = i32::from(viewport.y) + content_top as i32 - scroll_top as i32;
    let rect_top = y;
    let rect_bottom = y + content_h as i32;
    let viewport_top = i32::from(viewport.y);
    let viewport_bottom = viewport_top + i32::from(viewport.height);

    let clipped_top = rect_top.max(viewport_top);
    let clipped_bottom = rect_bottom.min(viewport_bottom);
    if clipped_bottom <= clipped_top {
        return Rect::new(viewport.x, viewport.y, viewport.width, 0);
    }

    Rect::new(
        viewport.x,
        clipped_top as u16,
        viewport.width,
        (clipped_bottom - clipped_top) as u16,
    )
}

/// Return the next scroll offset that keeps `[item_top, item_top + item_h)` visible.
pub(crate) fn scroll_top_to_keep_visible(
    current_scroll_top: usize,
    max_scroll: usize,
    viewport_h: usize,
    item_top: usize,
    item_h: usize,
) -> usize {
    if max_scroll == 0 || viewport_h == 0 || item_h == 0 {
        return current_scroll_top.min(max_scroll);
    }

    let mut scroll_top = current_scroll_top.min(max_scroll);
    let item_bottom = item_top.saturating_add(item_h);
    let viewport_bottom = scroll_top.saturating_add(viewport_h);

    if item_h >= viewport_h || item_top < scroll_top {
        scroll_top = item_top;
    } else if item_bottom > viewport_bottom {
        scroll_top = item_bottom.saturating_sub(viewport_h);
    }

    scroll_top.min(max_scroll)
}

/// Clamp-scroll helper for line/page wheel/key deltas.
pub(crate) fn next_scroll_top_with_delta(current: usize, max_scroll: usize, delta: isize) -> usize {
    let clamped_current = current.min(max_scroll);
    if delta < 0 {
        clamped_current.saturating_sub(delta.unsigned_abs())
    } else {
        clamped_current
            .saturating_add(delta as usize)
            .min(max_scroll)
    }
}

/// Return an area inset from the right by `columns`, clamped to non-negative width.
pub(crate) fn inset_rect_right(area: Rect, columns: u16) -> Rect {
    Rect {
        x: area.x,
        y: area.y,
        width: area.width.saturating_sub(columns),
        height: area.height,
    }
}

/// Render a standard vertical scrollbar when content overflows.
pub(crate) fn render_vertical_scrollbar(
    buf: &mut Buffer,
    area: Rect,
    position: usize,
    max_scroll: usize,
    viewport_content_length: usize,
) {
    if area.width == 0 || area.height < 3 || max_scroll == 0 || viewport_content_length == 0 {
        return;
    }

    let mut state = ScrollbarState::new(max_scroll.saturating_add(1))
        .position(position.min(max_scroll))
        .viewport_content_length(viewport_content_length);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
    StatefulWidget::render(scrollbar, area, buf, &mut state);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VerticalScrollbarMetrics {
    pub content_length: usize,
    pub max_position: usize,
    pub viewport_length: usize,
    pub track_length: usize,
    pub thumb_start: usize,
    pub thumb_length: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VerticalScrollbarHit {
    BeginArrow,
    EndArrow,
    TrackAboveThumb,
    TrackBelowThumb,
    Thumb { offset_in_thumb: usize },
}

/// Compute scrollbar thumb placement using the same math as ratatui's `Scrollbar`.
///
/// Notes:
/// - `content_length` is the value passed to `ScrollbarState::new(content_length)`.
/// - `position` is the value passed to `ScrollbarState::position(position)`.
/// - `viewport_content_length` is the value passed to `ScrollbarState::viewport_content_length(...)`.
pub(crate) fn vertical_scrollbar_metrics(
    area: Rect,
    content_length: usize,
    position: usize,
    viewport_content_length: usize,
) -> Option<VerticalScrollbarMetrics> {
    if content_length == 0 || area.height < 3 {
        return None;
    }

    // ratatui's `Scrollbar` defaults to begin/end arrow heads of length 1 each.
    let track_length = area.height.saturating_sub(2) as usize;
    if track_length == 0 {
        return None;
    }

    let viewport_length = if viewport_content_length != 0 {
        viewport_content_length
    } else {
        area.height as usize
    };

    let max_position = content_length.saturating_sub(1);
    let start_position = (position as f64).clamp(0.0, max_position as f64);
    let track_length_f = track_length as f64;
    let viewport_length_f = viewport_length as f64;
    let max_viewport_position = max_position as f64 + viewport_length_f;
    let end_position = start_position + viewport_length_f;

    let thumb_start_f = start_position * track_length_f / max_viewport_position;
    let thumb_end_f = end_position * track_length_f / max_viewport_position;

    // Mirror ratatui: nearest-integer rounding + clamping.
    let thumb_start = thumb_start_f.round().clamp(0.0, track_length_f - 1.0) as usize;
    let thumb_end = thumb_end_f.round().clamp(0.0, track_length_f) as usize;
    let thumb_length = thumb_end.saturating_sub(thumb_start).max(1);

    Some(VerticalScrollbarMetrics {
        content_length,
        max_position,
        viewport_length,
        track_length,
        thumb_start,
        thumb_length,
    })
}

pub(crate) fn vertical_scrollbar_right_hit_test(
    area: Rect,
    x: u16,
    y: u16,
    metrics: VerticalScrollbarMetrics,
) -> Option<VerticalScrollbarHit> {
    if area.width == 0 || !contains_point(area, x, y) {
        return None;
    }

    let scrollbar_x = area.x.saturating_add(area.width.saturating_sub(1));
    if x != scrollbar_x {
        return None;
    }

    let rel_y = y.saturating_sub(area.y) as usize;
    if rel_y == 0 {
        return Some(VerticalScrollbarHit::BeginArrow);
    }
    if rel_y == area.height.saturating_sub(1) as usize {
        return Some(VerticalScrollbarHit::EndArrow);
    }

    let track_y = rel_y.saturating_sub(1);
    if track_y < metrics.thumb_start {
        Some(VerticalScrollbarHit::TrackAboveThumb)
    } else if track_y >= metrics.thumb_start.saturating_add(metrics.thumb_length) {
        Some(VerticalScrollbarHit::TrackBelowThumb)
    } else {
        Some(VerticalScrollbarHit::Thumb {
            offset_in_thumb: track_y.saturating_sub(metrics.thumb_start),
        })
    }
}

pub(crate) fn vertical_scrollbar_position_for_thumb_drag(
    area: Rect,
    y: u16,
    metrics: VerticalScrollbarMetrics,
    offset_in_thumb: usize,
) -> usize {
    if metrics.content_length == 0 || metrics.track_length == 0 {
        return 0;
    }

    let rel_y = y.saturating_sub(area.y) as usize;
    let track_y = rel_y.saturating_sub(1).min(metrics.track_length.saturating_sub(1));

    let offset = offset_in_thumb.min(metrics.thumb_length.saturating_sub(1));
    let max_thumb_start = metrics
        .track_length
        .saturating_sub(metrics.thumb_length)
        .min(metrics.track_length.saturating_sub(1));
    let desired_thumb_start = track_y.saturating_sub(offset).min(max_thumb_start);

    let track_length_f = metrics.track_length as f64;
    let viewport_length_f = metrics.viewport_length as f64;
    let max_viewport_position = metrics.max_position as f64 + viewport_length_f;

    let pos_f = (desired_thumb_start as f64) * max_viewport_position / track_length_f;
    pos_f.round().clamp(0.0, metrics.max_position as f64) as usize
}

pub(crate) fn hit_test_relative_regions(
    area: Rect,
    x: u16,
    y: u16,
    regions: &[RelativeHitRegion],
) -> Option<usize> {
    if !contains_point(area, x, y) {
        return None;
    }
    let rel_y = y.saturating_sub(area.y);
    regions
        .iter()
        .find(|region| region.contains_relative_row(rel_y))
        .map(|region| region.id)
}

/// Hit-test repeated vertical row groups.
///
/// Example: `first_row=4`, `active_rows_per_group=2`, `group_stride=3`
/// means each group starts every 3 rows and the first 2 rows are clickable.
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

/// High-level outcome for mouse interaction on selectable vertical lists.
///
/// This lets views implement mouse support with minimal boilerplate:
/// 1) provide `selected` index and item count,
/// 2) provide a row hit-test closure,
/// 3) react to `Activated` for click-to-open/toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectableListMouseResult {
    Ignored,
    SelectionChanged,
    Activated,
}

impl SelectableListMouseResult {
    pub const fn handled(self) -> bool {
        !matches!(self, Self::Ignored)
    }
}

/// How wheel movement should traverse list selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollSelectionBehavior {
    Wrap,
    Clamp,
}

/// Configures common list mouse routing behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SelectableListMouseConfig {
    pub hover_select: bool,
    pub activate_on_left_click: bool,
    pub scroll_select: bool,
    pub require_pointer_hit_for_scroll: bool,
    pub scroll_behavior: ScrollSelectionBehavior,
}

impl Default for SelectableListMouseConfig {
    fn default() -> Self {
        Self {
            hover_select: true,
            activate_on_left_click: true,
            scroll_select: true,
            require_pointer_hit_for_scroll: false,
            scroll_behavior: ScrollSelectionBehavior::Wrap,
        }
    }
}

/// Route common mouse interactions for selectable lists.
///
/// Behavior:
/// - move: hover-select row under cursor
/// - left-click: select row and mark activated
/// - wheel: move selection up/down (wrapping)
pub(crate) fn route_selectable_list_mouse(
    mouse_event: MouseEvent,
    selected: &mut usize,
    item_count: usize,
    row_at_position: impl Fn(u16, u16) -> Option<usize>,
) -> SelectableListMouseResult {
    route_selectable_list_mouse_impl(
        mouse_event,
        selected,
        item_count,
        row_at_position,
        SelectableListMouseConfig::default(),
    )
}

pub(crate) fn route_selectable_list_mouse_with_config(
    mouse_event: MouseEvent,
    selected: &mut usize,
    item_count: usize,
    row_at_position: impl Fn(u16, u16) -> Option<usize>,
    config: SelectableListMouseConfig,
) -> SelectableListMouseResult {
    if config == SelectableListMouseConfig::default() {
        return route_selectable_list_mouse(mouse_event, selected, item_count, row_at_position);
    }

    route_selectable_list_mouse_impl(mouse_event, selected, item_count, row_at_position, config)
}

pub(crate) fn route_selectable_regions_mouse_with_config(
    mouse_event: MouseEvent,
    selected: &mut usize,
    item_count: usize,
    area: Rect,
    regions: &[RelativeHitRegion],
    config: SelectableListMouseConfig,
) -> SelectableListMouseResult {
    route_selectable_list_mouse_with_config(
        mouse_event,
        selected,
        item_count,
        |x, y| hit_test_relative_regions(area, x, y, regions),
        config,
    )
}

pub(crate) fn step_index_by_delta(
    current: usize,
    count: usize,
    delta: isize,
    behavior: ScrollSelectionBehavior,
) -> usize {
    if count == 0 || delta == 0 {
        return 0;
    }

    let mut index = clamp_index(current, count);
    let steps = delta.unsigned_abs();
    for _ in 0..steps {
        index = if delta < 0 {
            match behavior {
                ScrollSelectionBehavior::Wrap => wrap_prev(index, count),
                ScrollSelectionBehavior::Clamp => index.saturating_sub(1),
            }
        } else {
            match behavior {
                ScrollSelectionBehavior::Wrap => wrap_next(index, count),
                ScrollSelectionBehavior::Clamp => index.saturating_add(1).min(count - 1),
            }
        };
    }
    index
}

fn route_selectable_list_mouse_impl(
    mouse_event: MouseEvent,
    selected: &mut usize,
    item_count: usize,
    row_at_position: impl Fn(u16, u16) -> Option<usize>,
    config: SelectableListMouseConfig,
) -> SelectableListMouseResult {
    if item_count == 0 {
        *selected = 0;
        return SelectableListMouseResult::Ignored;
    }
    *selected = clamp_index(*selected, item_count);

    let pointer_selection = || {
        row_at_position(mouse_event.column, mouse_event.row).filter(|idx| *idx < item_count)
    };

    match mouse_event.kind {
        MouseEventKind::Moved => {
            if !config.hover_select {
                return SelectableListMouseResult::Ignored;
            }
            let Some(next) = pointer_selection() else {
                return SelectableListMouseResult::Ignored;
            };
            if *selected == next {
                SelectableListMouseResult::Ignored
            } else {
                *selected = next;
                SelectableListMouseResult::SelectionChanged
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if !config.activate_on_left_click {
                return SelectableListMouseResult::Ignored;
            }
            let Some(next) = pointer_selection() else {
                return SelectableListMouseResult::Ignored;
            };
            *selected = next;
            SelectableListMouseResult::Activated
        }
        MouseEventKind::ScrollUp => {
            if !config.scroll_select {
                return SelectableListMouseResult::Ignored;
            }
            if config.require_pointer_hit_for_scroll && pointer_selection().is_none() {
                return SelectableListMouseResult::Ignored;
            }
            let next = match config.scroll_behavior {
                ScrollSelectionBehavior::Wrap => wrap_prev(*selected, item_count),
                ScrollSelectionBehavior::Clamp => selected.saturating_sub(1),
            };
            if next == *selected {
                SelectableListMouseResult::Ignored
            } else {
                *selected = next;
                SelectableListMouseResult::SelectionChanged
            }
        }
        MouseEventKind::ScrollDown => {
            if !config.scroll_select {
                return SelectableListMouseResult::Ignored;
            }
            if config.require_pointer_hit_for_scroll && pointer_selection().is_none() {
                return SelectableListMouseResult::Ignored;
            }
            let next = match config.scroll_behavior {
                ScrollSelectionBehavior::Wrap => wrap_next(*selected, item_count),
                ScrollSelectionBehavior::Clamp => selected.saturating_add(1).min(item_count - 1),
            };
            if next == *selected {
                SelectableListMouseResult::Ignored
            } else {
                *selected = next;
                SelectableListMouseResult::SelectionChanged
            }
        }
        _ => SelectableListMouseResult::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use crate::bottom_pane::ConditionalUpdate;

    use super::{
        HeaderBodyFooterLayout,
        ListWindow,
        PinnedFooterLayout,
        RelativeHitRegion,
        ScrollSelectionBehavior,
        SelectableListMouseConfig,
        SelectableListMouseResult,
        VerticalScrollbarHit,
        centered_scroll_top,
        clamp_index,
        clipped_vertical_rect_with_scroll,
        contains_point,
        hit_test_relative_regions,
        hit_test_repeating_rows,
        inset_rect_right,
        next_scroll_top_with_delta,
        redraw_if,
        render_vertical_scrollbar,
        route_selectable_list_mouse,
        route_selectable_list_mouse_with_config,
        route_selectable_regions_mouse_with_config,
        scroll_top_to_keep_visible,
        split_header_body_footer,
        split_pinned_footer_layout,
        split_two_pane_when_room,
        step_index_by_delta,
        vertical_scrollbar_metrics,
        vertical_scrollbar_position_for_thumb_drag,
        vertical_scrollbar_right_hit_test,
        wrap_next,
        wrap_prev,
    };
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::buffer::Buffer;

    #[test]
    fn relative_hit_regions_map_rows_to_ids() {
        let area = Rect::new(10, 20, 40, 12);
        let regions = [
            RelativeHitRegion::new(0, 1, 2),
            RelativeHitRegion::new(1, 4, 1),
        ];

        assert_eq!(hit_test_relative_regions(area, 15, 21, &regions), Some(0));
        assert_eq!(hit_test_relative_regions(area, 15, 22, &regions), Some(0));
        assert_eq!(hit_test_relative_regions(area, 15, 24, &regions), Some(1));
        assert_eq!(hit_test_relative_regions(area, 15, 25, &regions), None);
    }

    #[test]
    fn repeating_rows_skip_spacer_lines() {
        let area = Rect::new(0, 0, 80, 30);
        // first row=4, each group is 3 rows where top 2 are interactive.
        assert_eq!(
            hit_test_repeating_rows(area, 5, 4, 4, 2, 3, 3),
            Some(0)
        );
        assert_eq!(
            hit_test_repeating_rows(area, 5, 5, 4, 2, 3, 3),
            Some(0)
        );
        assert_eq!(
            hit_test_repeating_rows(area, 5, 6, 4, 2, 3, 3),
            None
        );
        assert_eq!(
            hit_test_repeating_rows(area, 5, 7, 4, 2, 3, 3),
            Some(1)
        );
    }

    #[test]
    fn selection_helpers_wrap_and_clamp() {
        assert_eq!(wrap_prev(0, 3), 2);
        assert_eq!(wrap_prev(2, 3), 1);
        assert_eq!(wrap_next(2, 3), 0);
        assert_eq!(wrap_next(1, 3), 2);
        assert_eq!(clamp_index(9, 3), 2);
        assert_eq!(clamp_index(0, 0), 0);
    }

    #[test]
    fn contains_point_checks_bounds() {
        let area = Rect::new(2, 3, 4, 5);
        assert!(contains_point(area, 2, 3));
        assert!(contains_point(area, 5, 7));
        assert!(!contains_point(area, 6, 7));
        assert!(!contains_point(area, 5, 8));
    }

    #[test]
    fn pinned_footer_layout_reserves_viewport_and_rows() {
        let area = Rect::new(10, 20, 80, 16);
        let layout = split_pinned_footer_layout(area, 1, 1, 4);

        assert_eq!(
            layout,
            PinnedFooterLayout {
                viewport: Rect::new(10, 20, 80, 14),
                status_row: Rect::new(10, 34, 80, 1),
                action_row: Rect::new(10, 35, 80, 1),
            }
        );
    }

    #[test]
    fn pinned_footer_layout_drops_status_when_height_is_tight() {
        let area = Rect::new(0, 0, 40, 4);
        let layout = split_pinned_footer_layout(area, 1, 1, 4);

        assert_eq!(layout.viewport, Rect::new(0, 0, 40, 3));
        assert_eq!(layout.status_row, Rect::default());
        assert_eq!(layout.action_row, Rect::new(0, 3, 40, 1));
    }

    #[test]
    fn header_body_footer_layout_splits_with_min_body() {
        let area = Rect::new(4, 5, 80, 20);
        let layout = split_header_body_footer(area, 2, 3, 2).expect("layout");
        assert_eq!(
            layout,
            HeaderBodyFooterLayout {
                header: Rect::new(4, 5, 80, 2),
                body: Rect::new(4, 7, 80, 15),
                footer: Rect::new(4, 22, 80, 3),
            }
        );
    }

    #[test]
    fn header_body_footer_layout_returns_none_when_too_short() {
        let area = Rect::new(0, 0, 40, 6);
        assert!(split_header_body_footer(area, 2, 2, 2).is_none());
    }

    #[test]
    fn split_two_pane_when_room_honors_minimums() {
        let area = Rect::new(10, 20, 100, 12);
        let (left, right) = split_two_pane_when_room(area, 96, 10, 42).expect("split");
        assert_eq!(left.height, area.height);
        assert_eq!(right.height, area.height);
        assert_eq!(left.width.saturating_add(right.width), area.width);

        let too_narrow = Rect::new(10, 20, 90, 12);
        assert!(split_two_pane_when_room(too_narrow, 96, 10, 42).is_none());

        let too_short = Rect::new(10, 20, 100, 8);
        assert!(split_two_pane_when_room(too_short, 96, 10, 42).is_none());
    }

    #[test]
    fn clipped_vertical_rect_with_scroll_clips_virtual_section() {
        let viewport = Rect::new(5, 10, 20, 6);
        let clipped = clipped_vertical_rect_with_scroll(viewport, 4, 5, 2);
        assert_eq!(clipped, Rect::new(5, 12, 20, 4));
    }

    #[test]
    fn scroll_top_to_keep_visible_moves_when_item_outside_viewport() {
        assert_eq!(scroll_top_to_keep_visible(0, 40, 10, 25, 3), 18);
        assert_eq!(scroll_top_to_keep_visible(20, 40, 10, 5, 3), 5);
        assert_eq!(scroll_top_to_keep_visible(6, 40, 10, 8, 2), 6);
    }

    #[test]
    fn next_scroll_top_with_delta_clamps_both_directions() {
        assert_eq!(next_scroll_top_with_delta(5, 20, 3), 8);
        assert_eq!(next_scroll_top_with_delta(19, 20, 5), 20);
        assert_eq!(next_scroll_top_with_delta(5, 20, -3), 2);
        assert_eq!(next_scroll_top_with_delta(1, 20, -8), 0);
    }

    #[test]
    fn inset_rect_right_reduces_width_only() {
        let area = Rect::new(4, 6, 12, 8);
        assert_eq!(inset_rect_right(area, 1), Rect::new(4, 6, 11, 8));
        assert_eq!(inset_rect_right(area, 32), Rect::new(4, 6, 0, 8));
    }

    #[test]
    fn render_vertical_scrollbar_is_noop_when_no_overflow() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 8, 6));
        let baseline = buf.clone();
        render_vertical_scrollbar(&mut buf, Rect::new(0, 0, 8, 6), 0, 0, 5);
        assert_eq!(buf, baseline);
    }

    #[test]
    fn redraw_if_maps_handled_state() {
        assert!(matches!(redraw_if(true), ConditionalUpdate::NeedsRedraw));
        assert!(matches!(redraw_if(false), ConditionalUpdate::NoRedraw));
    }

    #[test]
    fn selectable_list_router_handles_move_click_and_wheel() {
        let mut selected = 0usize;
        let row_at = |x: u16, y: u16| {
            if x == 1 && y <= 2 {
                Some(y as usize)
            } else {
                None
            }
        };

        let moved = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 1,
            row: 2,
            modifiers: KeyModifiers::empty(),
        };
        assert_eq!(
            route_selectable_list_mouse(moved, &mut selected, 3, row_at),
            SelectableListMouseResult::SelectionChanged
        );
        assert_eq!(selected, 2);

        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        assert_eq!(
            route_selectable_list_mouse(click, &mut selected, 3, row_at),
            SelectableListMouseResult::Activated
        );
        assert_eq!(selected, 1);

        let wheel_down = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        };
        assert_eq!(
            route_selectable_list_mouse(wheel_down, &mut selected, 3, row_at),
            SelectableListMouseResult::SelectionChanged
        );
        assert_eq!(selected, 2);
    }

    #[test]
    fn selectable_list_router_honors_scroll_clamp_and_pointer_gate() {
        let mut selected = 1usize;
        let row_at = |x: u16, y: u16| {
            if x == 2 && y <= 2 {
                Some(y as usize)
            } else {
                None
            }
        };
        let config = SelectableListMouseConfig {
            require_pointer_hit_for_scroll: true,
            scroll_behavior: ScrollSelectionBehavior::Clamp,
            ..SelectableListMouseConfig::default()
        };

        let off_target_wheel = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 9,
            row: 9,
            modifiers: KeyModifiers::empty(),
        };
        assert_eq!(
            route_selectable_list_mouse_with_config(
                off_target_wheel,
                &mut selected,
                3,
                row_at,
                config
            ),
            SelectableListMouseResult::Ignored
        );
        assert_eq!(selected, 1);

        let on_target_wheel = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 2,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        assert_eq!(
            route_selectable_list_mouse_with_config(
                on_target_wheel,
                &mut selected,
                3,
                row_at,
                config
            ),
            SelectableListMouseResult::SelectionChanged
        );
        assert_eq!(selected, 2);

        let clamp_end = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 2,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        assert_eq!(
            route_selectable_list_mouse_with_config(clamp_end, &mut selected, 3, row_at, config),
            SelectableListMouseResult::Ignored
        );
        assert_eq!(selected, 2);
    }

    #[test]
    fn selectable_regions_router_uses_relative_hit_regions() {
        let mut selected = 0usize;
        let area = Rect::new(10, 20, 40, 12);
        let regions = [
            RelativeHitRegion::new(0, 1, 1),
            RelativeHitRegion::new(1, 3, 1),
        ];
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 23,
            modifiers: KeyModifiers::empty(),
        };

        let result = route_selectable_regions_mouse_with_config(
            click,
            &mut selected,
            2,
            area,
            &regions,
            SelectableListMouseConfig::default(),
        );

        assert_eq!(result, SelectableListMouseResult::Activated);
        assert_eq!(selected, 1);
    }

    #[test]
    fn selectable_list_router_handles_empty_and_out_of_range_rows() {
        let mut selected = 7usize;
        let row_at = |_x: u16, _y: u16| Some(9usize);
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        };
        assert_eq!(
            route_selectable_list_mouse(click, &mut selected, 0, row_at),
            SelectableListMouseResult::Ignored
        );
        assert_eq!(selected, 0);

        selected = 0;
        assert_eq!(
            route_selectable_list_mouse(click, &mut selected, 3, row_at),
            SelectableListMouseResult::Ignored
        );
        assert_eq!(selected, 0);
    }

    #[test]
    fn centered_list_window_keeps_selected_visible() {
        let window = ListWindow::centered(20, 5, 0);
        assert_eq!(window.start, 0);
        assert_eq!(window.end, 5);

        let window = ListWindow::centered(20, 5, 10);
        assert_eq!(window.start, 8);
        assert_eq!(window.end, 13);

        let window = ListWindow::centered(20, 5, 19);
        assert_eq!(window.start, 15);
        assert_eq!(window.end, 20);
    }

    #[test]
    fn centered_list_window_maps_relative_rows() {
        let window = ListWindow::centered(10, 4, 6);
        assert_eq!(window.start, 4);
        assert_eq!(window.index_for_relative_row(0), Some(4));
        assert_eq!(window.index_for_relative_row(3), Some(7));
        assert_eq!(window.index_for_relative_row(4), None);
    }

    #[test]
    fn centered_scroll_top_matches_window_start() {
        assert_eq!(centered_scroll_top(0, 20, 5), 0);
        assert_eq!(centered_scroll_top(10, 20, 5), 8);
        assert_eq!(centered_scroll_top(19, 20, 5), 15);
    }

    #[test]
	    fn step_index_by_delta_supports_wrap_and_clamp() {
	        assert_eq!(
	            step_index_by_delta(0, 3, -1, ScrollSelectionBehavior::Wrap),
	            2
	        );
        assert_eq!(
            step_index_by_delta(0, 3, -1, ScrollSelectionBehavior::Clamp),
            0
        );
        assert_eq!(
            step_index_by_delta(1, 3, 3, ScrollSelectionBehavior::Wrap),
            1
        );
	        assert_eq!(
	            step_index_by_delta(1, 3, 3, ScrollSelectionBehavior::Clamp),
	            2
	        );
	    }

	    #[test]
	    fn vertical_scrollbar_hit_test_and_drag_round_trip() {
	        let area = Rect::new(10, 20, 4, 10);
	        let metrics = vertical_scrollbar_metrics(area, 6, 0, 3).expect("metrics");
	        let x = area.x + area.width - 1;

	        assert_eq!(
	            vertical_scrollbar_right_hit_test(area, x, area.y, metrics),
	            Some(VerticalScrollbarHit::BeginArrow)
	        );
	        assert_eq!(
	            vertical_scrollbar_right_hit_test(area, x, area.y + area.height - 1, metrics),
	            Some(VerticalScrollbarHit::EndArrow)
	        );

	        assert!(matches!(
	            vertical_scrollbar_right_hit_test(area, x, area.y + 2, metrics),
	            Some(VerticalScrollbarHit::Thumb { .. })
	        ));

	        let pos = vertical_scrollbar_position_for_thumb_drag(area, area.y + 8, metrics, 0);
	        assert_eq!(pos, metrics.max_position);
	    }
	}
