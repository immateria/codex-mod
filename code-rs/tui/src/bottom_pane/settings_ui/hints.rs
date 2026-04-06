use std::borrow::Cow;

use ratatui::style::{Style, Stylize};
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

pub(crate) fn title_line(text: impl Into<Cow<'static, str>>) -> Line<'static> {
    Line::from(Span::styled(
        text.into().into_owned(),
        Style::new().fg(colors::text_bright()).bold(),
    ))
}

pub(crate) fn status_line(status: StyledText<'_>) -> Line<'static> {
    Line::from(Span::styled(status.text.into_owned(), status.style))
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

/// Navigate (↑↓) hint with `colors::function()` key style.
pub(crate) fn hint_nav(description: &'static str) -> KeyHint<'static> {
    KeyHint::new("↑↓", description).with_key_style(Style::new().fg(colors::function()))
}

/// Esc hint with `colors::error()` key style.
pub(crate) fn hint_esc(description: &'static str) -> KeyHint<'static> {
    KeyHint::new("Esc", description).with_key_style(Style::new().fg(colors::error()))
}

/// Enter/confirm hint with `colors::success()` key style.
pub(crate) fn hint_enter(description: &'static str) -> KeyHint<'static> {
    KeyHint::new("Enter", description).with_key_style(Style::new().fg(colors::success()))
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

pub(crate) fn status_and_shortcuts_split(
    status: Option<StyledText<'_>>,
    hints: &[KeyHint<'_>],
) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
    let status_lines = status.map(status_line).into_iter().collect();
    let footer_lines = vec![shortcut_line(hints)];
    (status_lines, footer_lines)
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

    #[test]
    fn status_and_shortcuts_split_returns_status_separately() {
        let (status_lines, footer_lines) = status_and_shortcuts_split(
            Some(StyledText::new("warning", Style::new().fg(colors::warning()))),
            &[KeyHint::new("Enter", " save")],
        );

        assert_eq!(status_lines.len(), 1);
        assert_eq!(footer_lines.len(), 1);
        assert_eq!(status_lines[0].spans[0].content, "warning");
        assert_eq!(footer_lines[0].spans[0].content, "Enter");
    }
}
