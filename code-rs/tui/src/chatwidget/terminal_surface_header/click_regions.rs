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
    let mut out: Vec<ClickableRegion> = Vec::new();
    for (range, action) in ranges {
        let visible_start = range.start.min(visible_width);
        let visible_end = range.end.min(visible_width);
        if visible_end <= visible_start {
            continue;
        }
        out.push(ClickableRegion {
            rect: Rect {
                x: start_x + visible_start as u16,
                y: area.y,
                width: (visible_end - visible_start) as u16,
                height: 1,
            },
            action: action.clone(),
        });
    }
    out
}
