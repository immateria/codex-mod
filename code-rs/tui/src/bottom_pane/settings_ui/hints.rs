use std::borrow::Cow;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::colors;

use super::rows::StyledText;

#[derive(Clone, Debug)]
pub(crate) struct KeyHint<'a> {
    key: Cow<'a, str>,
    description: Cow<'a, str>,
    key_style: Style,
    description_style: Style,
}

impl<'a> KeyHint<'a> {
    pub(crate) fn new(
        key: impl Into<Cow<'a, str>>,
        description: impl Into<Cow<'a, str>>,
    ) -> Self {
        Self {
            key: key.into(),
            description: description.into(),
            key_style: Style::new().fg(colors::primary()),
            description_style: Style::new().fg(colors::text_dim()),
        }
    }

    pub(crate) fn with_key_style(mut self, key_style: Style) -> Self {
        self.key_style = key_style;
        self
    }

}

pub(crate) fn shortcut_line(hints: &[KeyHint<'_>]) -> Line<'static> {
    let mut spans = Vec::new();
    for (idx, hint) in hints.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled(hint.key.clone().into_owned(), hint.key_style));
        spans.push(Span::styled(
            hint.description.clone().into_owned(),
            hint.description_style,
        ));
    }
    Line::from(spans)
}

pub(crate) fn status_and_shortcuts(
    status: Option<StyledText<'_>>,
    hints: &[KeyHint<'_>],
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(status) = status {
        lines.push(Line::from(Span::styled(
            status.text.into_owned(),
            status.style,
        )));
        lines.push(Line::default());
    }
    lines.push(shortcut_line(hints));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_and_shortcuts_includes_separator_line_after_status() {
        let lines = status_and_shortcuts(
            Some(StyledText::new("warning", Style::new().fg(colors::warning()))),
            &[KeyHint::new("Enter", " save")],
        );

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans[0].content, "warning");
        assert!(lines[1].spans.is_empty());
    }

    #[test]
    fn shortcut_line_separates_hints_consistently() {
        let line = shortcut_line(&[
            KeyHint::new("Enter", " save"),
            KeyHint::new("Esc", " close"),
        ]);

        assert_eq!(line.spans[0].content, "Enter");
        assert_eq!(line.spans[1].content, " save");
        assert_eq!(line.spans[2].content, "   ");
        assert_eq!(line.spans[3].content, "Esc");
    }
}
