use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::path::Path;

mod geometry;
mod placeholder;
mod render;

pub(super) fn centered_target_rect_for_image(
    area: Rect,
    cell_w: u16,
    cell_h: u16,
    img_w: u32,
    img_h: u32,
) -> Option<Rect> {
    geometry::centered_target_rect_for_image(area, cell_w, cell_h, img_w, img_h)
}

pub(super) fn render_image_placeholder(path: &Path, area: Rect, buf: &mut Buffer, title: &str) {
    placeholder::render_image_placeholder(path, area, buf, title);
}
