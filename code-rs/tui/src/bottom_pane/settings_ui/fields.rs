use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Widget};

use crate::colors;
use crate::components::form_text_field::FormTextField;

#[derive(Clone, Debug)]
pub(crate) struct BorderedField<'a> {
    title: Cow<'a, str>,
    focused: bool,
}

impl<'a> BorderedField<'a> {
    pub(crate) fn new(title: impl Into<Cow<'a, str>>, focused: bool) -> Self {
        Self {
            title: title.into(),
            focused,
        }
    }

    pub(crate) fn inner(&self, outer: Rect) -> Rect {
        self.block().inner(outer)
    }

    pub(crate) fn render_block(&self, outer: Rect, buf: &mut Buffer) -> Rect {
        let block = self.block();
        let inner = block.inner(outer);
        block.render(outer, buf);
        inner
    }

    pub(crate) fn render(&self, outer: Rect, buf: &mut Buffer, field: &FormTextField) -> Rect {
        let inner = self.render_block(outer, buf);
        field.render(inner, buf, self.focused);
        inner
    }

    fn block(&self) -> Block<'_> {
        Block::bordered()
            .title(self.title.as_ref())
            .border_style(Style::new().fg(if self.focused {
                colors::primary()
            } else {
                colors::border()
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bordered_field_inner_rect_matches_for_focused_and_unfocused() {
        let area = Rect::new(0, 0, 40, 5);
        let focused_inner = BorderedField::new("Title", true).inner(area);
        let unfocused_inner = BorderedField::new("Title", false).inner(area);
        assert_eq!(focused_inner, unfocused_inner);
    }
}
