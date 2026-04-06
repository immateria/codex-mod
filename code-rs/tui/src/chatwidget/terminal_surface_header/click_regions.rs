use super::*;

pub(super) fn centered_clickable_regions_from_char_ranges(
    ranges: &[(std::ops::Range<usize>, ClickableAction)],
    area: Rect,
    total_width: usize,
) -> Vec<ClickableRegion> {
    let start_x = if total_width < area.width as usize {
        area.x + ((area.width as usize - total_width) / 2) as u16
    } else {
        area.x
    };
    let visible_width = area.width as usize;
    let area_right = area.x.saturating_add(area.width);
    let mut out: Vec<ClickableRegion> = Vec::new();
    // Horizontal padding around each clickable segment for easier touch.
    let h_pad: u16 = 1;
    for (range, action) in ranges {
        let visible_start = range.start.min(visible_width);
        let visible_end = range.end.min(visible_width);
        if visible_end <= visible_start {
            continue;
        }
        let raw_x = start_x + visible_start as u16;
        let raw_right = start_x + visible_end as u16;
        // Expand outward by h_pad but clamp to the area bounds.
        let padded_x = raw_x.saturating_sub(h_pad).max(area.x);
        let padded_right = raw_right.saturating_add(h_pad).min(area_right);
        out.push(ClickableRegion {
            rect: Rect {
                x: padded_x,
                y: area.y,
                width: padded_right.saturating_sub(padded_x),
                height: area.height.min(3).max(1),
            },
            action: action.clone(),
        });
    }
    out
}
