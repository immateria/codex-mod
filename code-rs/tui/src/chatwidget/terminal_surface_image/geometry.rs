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
    let area_px_w = u32::from(area.width) * u32::from(cell_w);
    let area_px_h = u32::from(area.height) * u32::from(cell_h);
    if area_px_w == 0 || area_px_h == 0 {
        return None;
    }
    let scale_w = f64::from(area_px_w) / f64::from(img_w);
    let scale_h = f64::from(area_px_h) / f64::from(img_h);
    let scale = scale_w.min(scale_h).max(0.0);
    let target_w_cells = ((f64::from(img_w) * scale) / f64::from(cell_w)).floor() as u16;
    let target_h_cells = ((f64::from(img_h) * scale) / f64::from(cell_h)).floor() as u16;
    let target_w = target_w_cells.clamp(1, area.width);
    let target_h = target_h_cells.clamp(1, area.height);
    Some(Rect {
        x: area.x + (area.width.saturating_sub(target_w)) / 2,
        y: area.y + (area.height.saturating_sub(target_h)) / 2,
        width: target_w,
        height: target_h,
    })
}
