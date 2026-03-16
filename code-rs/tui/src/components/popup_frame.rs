use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Clear, Widget};

pub(crate) fn render_popup_frame(area: Rect, buf: &mut Buffer, title: &str) -> Option<Rect> {
    if area.is_empty() {
        return None;
    }

    Clear.render(area, buf);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(crate::colors::border()))
        .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
        .title(title)
        .title_alignment(Alignment::Center);

    let inner = block.inner(area);
    block.render(area, buf);
    Some(inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_frame_returns_none_for_empty_area() {
        let buf_area = Rect::new(0, 0, 10, 5);
        let mut buf = Buffer::empty(buf_area);
        assert_eq!(render_popup_frame(Rect::new(0, 0, 0, 5), &mut buf, "T"), None);
        assert_eq!(render_popup_frame(Rect::new(0, 0, 10, 0), &mut buf, "T"), None);
    }

    #[test]
    fn popup_frame_returns_inner_rect() {
        let area = Rect::new(0, 0, 10, 5);
        let mut buf = Buffer::empty(area);
        assert_eq!(
            render_popup_frame(area, &mut buf, "Title"),
            Some(Rect::new(1, 1, 8, 3))
        );
    }
}

