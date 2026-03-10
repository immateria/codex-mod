use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::block::Title;
use ratatui::widgets::{Block, Borders, Widget};

use crate::colors;
use crate::components::form_text_field::FormTextField;

pub(crate) fn bordered_field_block<'a, T>(title: T, focused: bool) -> Block<'a>
where
    T: Into<Title<'a>>,
{
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(if focused {
            colors::primary()
        } else {
            colors::border()
        }))
}

pub(crate) fn render_bordered_field(
    buf: &mut Buffer,
    outer: Rect,
    block: &Block<'_>,
    field: &FormTextField,
    focused: bool,
) {
    let inner = block.inner(outer);
    block.render(outer, buf);
    field.render(inner, buf, focused);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bordered_field_inner_rect_matches_for_focused_and_unfocused() {
        let area = Rect::new(0, 0, 40, 5);
        let focused_inner = bordered_field_block("Title", true).inner(area);
        let unfocused_inner = bordered_field_block("Title", false).inner(area);
        assert_eq!(focused_inner, unfocused_inner);
    }
}
