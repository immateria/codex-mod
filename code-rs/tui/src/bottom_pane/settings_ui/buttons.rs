use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use crate::colors;

use super::layout::DEFAULT_BUTTON_GAP;

pub(crate) struct TextButton<'a> {
    pub(crate) label: &'a str,
    pub(crate) focused: bool,
    pub(crate) hovered: bool,
    pub(crate) style: Style,
}

pub(crate) fn render_text_button_strip(area: Rect, buf: &mut Buffer, buttons: &[TextButton<'_>]) {
    let mut spans = Vec::new();
    for (index, button) in buttons.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(DEFAULT_BUTTON_GAP));
        }
        let span_style = if button.focused {
            button.style.bg(colors::primary()).fg(colors::background())
        } else if button.hovered {
            button
                .style
                .bg(colors::border())
                .fg(colors::text())
                .add_modifier(Modifier::BOLD)
        } else {
            button.style
        };
        spans.push(Span::styled(button.label, span_style));
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}

pub(crate) fn text_button_at(x: u16, y: u16, row: Rect, labels: &[&str]) -> Option<usize> {
    if !row.contains(ratatui::layout::Position { x, y }) {
        return None;
    }

    let mut cursor_x = row.x;
    for (index, label) in labels.iter().enumerate() {
        let len = u16::try_from(label.width()).unwrap_or(u16::MAX);
        if x >= cursor_x && x < cursor_x.saturating_add(len) {
            return Some(index);
        }
        cursor_x = cursor_x.saturating_add(len);
        if index + 1 < labels.len() {
            cursor_x = cursor_x.saturating_add(DEFAULT_BUTTON_GAP.len() as u16);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_button_hit_testing_uses_shared_gap_width() {
        let row = Rect::new(10, 4, 40, 1);
        let labels = ["Apply", "Close"];
        let apply_width = u16::try_from("Apply".width()).unwrap_or(u16::MAX);
        let gap_width = DEFAULT_BUTTON_GAP.len() as u16;
        assert_eq!(text_button_at(10, 4, row, &labels), Some(0));
        assert_eq!(text_button_at(10 + apply_width + gap_width, 4, row, &labels), Some(1));
        assert_eq!(text_button_at(10 + apply_width, 4, row, &labels), None);
    }
}
