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
