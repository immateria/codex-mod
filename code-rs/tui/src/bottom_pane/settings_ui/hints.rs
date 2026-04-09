use std::borrow::Cow;

use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use crate::colors;
use crate::icons;

use super::rows::StyledText;

#[derive(Clone, Debug)]
pub(crate) struct KeyHint<'a> {
    key: Cow<'a, str>,
    description: Cow<'a, str>,
    key_style: Style,
    description_style: Style,
    /// When set, these spans replace the single `key`/`key_style` pair in
    /// `shortcut_line()`, allowing multi-colored key glyphs (e.g. bi-color
    /// nav arrows).
    key_spans: Option<Vec<Span<'static>>>,
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
            key_spans: None,
        }
    }

    pub(crate) fn with_key_style(mut self, key_style: Style) -> Self {
        self.key_style = key_style;
        self
    }

    pub(crate) fn with_key_spans(mut self, spans: Vec<Span<'static>>) -> Self {
        self.key_spans = Some(spans);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShortcutPlacement {
    Top,
    Bottom,
}

#[derive(Clone, Debug)]
pub(crate) struct ShortcutBar {
    placement: ShortcutPlacement,
    hints: Vec<KeyHint<'static>>,
}

impl ShortcutBar {
    pub(crate) fn at(placement: ShortcutPlacement, hints: Vec<KeyHint<'static>>) -> Self {
        Self {
            placement,
            hints,
        }
    }

    pub(crate) fn placement(&self) -> ShortcutPlacement {
        self.placement
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.hints.is_empty()
    }

    pub(crate) fn line(&self) -> Line<'static> {
        shortcut_line(&self.hints)
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
        if let Some(key_spans) = &hint.key_spans {
            spans.extend(key_spans.iter().cloned());
        } else {
            spans.push(Span::styled(hint.key.clone().into_owned(), hint.key_style));
        }
        spans.push(Span::styled(
            hint.description.clone().into_owned(),
            hint.description_style,
        ));
    }
    Line::from(spans)
}

/// Navigate (↑ ↓) hint with bi-colored arrows — up in `function()`, down in
/// `primary()` — so the pair reads as two distinct arrows rather than a
/// monochrome zigzag.
pub(crate) fn hint_nav(description: &'static str) -> KeyHint<'static> {
    KeyHint::new(icons::nav_up_down(), description)
        .with_key_spans(vec![
            Span::styled(icons::arrow_up().to_string(), Style::new().fg(colors::function())),
            Span::raw(" "),
            Span::styled(icons::arrow_down().to_string(), Style::new().fg(colors::primary())),
        ])
}

/// Horizontal (◂ ▸) hint with bi-colored arrows — left in `function()`, right
/// in `primary()`.
pub(crate) fn hint_nav_horizontal(description: &'static str) -> KeyHint<'static> {
    KeyHint::new(icons::nav_left_right(), description)
        .with_key_spans(vec![
            Span::styled(icons::arrow_left().to_string(), Style::new().fg(colors::function())),
            Span::raw(" "),
            Span::styled(icons::arrow_right().to_string(), Style::new().fg(colors::primary())),
        ])
}

pub(crate) fn key_tab() -> &'static str {
    icons::tab()
}

pub(crate) fn key_reverse_tab() -> &'static str {
    icons::reverse_tab()
}

pub(crate) fn key_space() -> &'static str {
    icons::space()
}

pub(crate) fn key_ctrl(key: &str) -> String {
    icons::ctrl_combo(key)
}

/// Esc hint with `colors::error()` key style.
pub(crate) fn hint_esc(description: &'static str) -> KeyHint<'static> {
    KeyHint::new(icons::escape(), description).with_key_style(Style::new().fg(colors::error()))
}

/// Enter/confirm hint with `colors::success()` key style.
pub(crate) fn hint_enter(description: &'static str) -> KeyHint<'static> {
    KeyHint::new(icons::enter(), description).with_key_style(Style::new().fg(colors::success()))
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
