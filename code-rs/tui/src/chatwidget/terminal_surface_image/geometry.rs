use ratatui::layout::Rect;

pub(super) fn centered_target_rect_for_image(
    area: Rect,
    cell_w: u16,
    cell_h: u16,
    img_w: u32,
    img_h: u32,
) -> Option<Rect> {
    if area.width == 0
        || area.height == 0
        || cell_w == 0
        || cell_h == 0
        || img_w == 0
        || img_h == 0
    {
        return None;
    }
    let area_px_w = (area.width as u32) * (cell_w as u32);
    let area_px_h = (area.height as u32) * (cell_h as u32);
    if area_px_w == 0 || area_px_h == 0 {
        return None;
    }
    let scale_w = area_px_w as f64 / img_w as f64;
    let scale_h = area_px_h as f64 / img_h as f64;
    let scale = scale_w.min(scale_h).max(0.0);
    let target_w_cells = ((img_w as f64 * scale) / (cell_w as f64)).floor() as u16;
    let target_h_cells = ((img_h as f64 * scale) / (cell_h as f64)).floor() as u16;
    let target_w = target_w_cells.clamp(1, area.width);
    let target_h = target_h_cells.clamp(1, area.height);
    Some(Rect {
        x: area.x + (area.width.saturating_sub(target_w)) / 2,
        y: area.y + (area.height.saturating_sub(target_h)) / 2,
        width: target_w,
        height: target_h,
    })
}
