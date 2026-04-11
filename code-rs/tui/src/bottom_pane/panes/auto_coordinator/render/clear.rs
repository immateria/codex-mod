use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub(super) fn clear_row(area: Rect, buf: &mut Buffer) {
    if area.height == 0 {
        return;
    }
    for x in area.x..area.x.saturating_add(area.width) {
        let cell = &mut buf[(x, area.y)];
        cell.set_symbol(" ");
        cell.set_style(crate::colors::style_text_on_bg());
    }
}

pub(super) fn clear_rect(area: Rect, buf: &mut Buffer) {
    if area.is_empty() {
        return;
    }
    for offset in 0..area.height {
        let row = Rect {
            x: area.x,
            y: area.y + offset,
            width: area.width,
            height: 1,
        };
        clear_row(row, buf);
    }
}

