use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use crate::colors;

use super::layout::DEFAULT_BUTTON_GAP;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextButtonAlign {
    End,
}

pub(crate) struct TextButton<'a, Id> {
    pub(crate) id: Id,
    pub(crate) label: Cow<'a, str>,
    pub(crate) focused: bool,
    pub(crate) hovered: bool,
    pub(crate) style: Style,
}

impl<'a, Id> TextButton<'a, Id> {
    pub(crate) fn new(
        id: Id,
        label: impl Into<Cow<'a, str>>,
        focused: bool,
        hovered: bool,
        style: Style,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            focused,
            hovered,
            style,
        }
    }
}

fn button_label_width<Id>(button: &TextButton<'_, Id>) -> u16 {
    u16::try_from(button.label.width()).unwrap_or(u16::MAX)
}

fn gap_width() -> u16 {
    u16::try_from(DEFAULT_BUTTON_GAP.width()).unwrap_or(u16::MAX)
}

fn button_layouts<'a, Id>(
    origin_x: u16,
    buttons: &'a [TextButton<'a, Id>],
) -> impl Iterator<Item = (u16, u16, &'a TextButton<'a, Id>)> + 'a {
    let mut cursor_x = origin_x;
    buttons.iter().enumerate().map(move |(index, button)| {
        let x = cursor_x;
        let width = button_label_width(button);
        cursor_x = cursor_x.saturating_add(width);
        if index + 1 < buttons.len() {
            cursor_x = cursor_x.saturating_add(gap_width());
        }
        (x, width, button)
    })
}

pub(crate) fn text_button_strip_width<Id>(buttons: &[TextButton<'_, Id>]) -> u16 {
    button_layouts(0, buttons)
        .last()
        .map(|(x, width, _)| x.saturating_add(width))
        .unwrap_or(0)
}

pub(crate) fn aligned_text_button_strip_rect<Id>(
    row: Rect,
    buttons: &[TextButton<'_, Id>],
    align: TextButtonAlign,
) -> Rect {
    let width = text_button_strip_width(buttons).min(row.width);
    let x = match align {
        TextButtonAlign::End => row.x.saturating_add(row.width.saturating_sub(width)),
    };
    Rect::new(x, row.y, width, row.height)
}

pub(crate) fn render_text_button_strip<Id>(
    area: Rect,
    buf: &mut Buffer,
    buttons: &[TextButton<'_, Id>],
) {
    let mut spans = Vec::new();
    for (index, button) in buttons.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(DEFAULT_BUTTON_GAP));
        }
        let span_style = if button.focused {
            button.style.bg(colors::primary()).fg(colors::background())
        } else if button.hovered {
            button.style.bg(colors::border()).fg(colors::text()).bold()
        } else {
            button.style
        };
        spans.push(Span::styled(button.label.as_ref(), span_style));
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}

pub(crate) fn render_text_button_strip_aligned<Id>(
    row: Rect,
    buf: &mut Buffer,
    buttons: &[TextButton<'_, Id>],
    align: TextButtonAlign,
) {
    let area = aligned_text_button_strip_rect(row, buttons, align);
    render_text_button_strip(area, buf, buttons);
}

pub(crate) fn text_button_at<Id: Copy>(
    x: u16,
    y: u16,
    row: Rect,
    buttons: &[TextButton<'_, Id>],
) -> Option<Id> {
    if !row.contains(Position { x, y }) {
        return None;
    }

    for (button_x, button_width, button) in button_layouts(row.x, buttons) {
        if x >= button_x && x < button_x.saturating_add(button_width) {
            return Some(button.id);
        }
    }
    None
}

pub(crate) fn text_button_at_aligned<Id: Copy>(
    x: u16,
    y: u16,
    row: Rect,
    buttons: &[TextButton<'_, Id>],
    align: TextButtonAlign,
) -> Option<Id> {
    let area = aligned_text_button_strip_rect(row, buttons, align);
    text_button_at(x, y, area, buttons)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_button_hit_testing_uses_shared_gap_width() {
        let row = Rect::new(10, 4, 40, 1);
        let buttons = [
            TextButton::new(10usize, "Apply", false, false, Style::new()),
            TextButton::new(20usize, "Close", false, false, Style::new()),
        ];
        let apply_width = u16::try_from("Apply".width()).unwrap_or(u16::MAX);
        let gap_width = gap_width();
        assert_eq!(text_button_at(10, 4, row, &buttons), Some(10));
        assert_eq!(
            text_button_at(10 + apply_width + gap_width, 4, row, &buttons),
            Some(20)
        );
        assert_eq!(text_button_at(10 + apply_width, 4, row, &buttons), None);
        assert_eq!(text_button_strip_width(&buttons), apply_width + gap_width + 5);
    }

    #[test]
    fn aligned_rect_places_buttons_at_end() {
        let row = Rect::new(10, 4, 20, 1);
        let buttons = [
            TextButton::new(10usize, "Apply", false, false, Style::new()),
            TextButton::new(20usize, "Close", false, false, Style::new()),
        ];
        let rect = aligned_text_button_strip_rect(row, &buttons, TextButtonAlign::End);
        assert_eq!(rect.x + rect.width, row.x + row.width);
        assert_eq!(text_button_at_aligned(rect.x, 4, row, &buttons, TextButtonAlign::End), Some(10));
    }
}
