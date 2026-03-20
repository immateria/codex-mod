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
