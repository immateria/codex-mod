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

/// How a [`ShortcutBar`] handles hints that exceed the available width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OverflowMode {
    /// Silently truncate at the width boundary (existing behaviour).
    #[default]
    Truncate,
    /// Wrap hints onto additional lines so nothing is hidden.
    Wrap,
}

#[derive(Clone, Debug)]
pub(crate) struct ShortcutBar {
    placement: ShortcutPlacement,
    hints: Vec<KeyHint<'static>>,
    overflow: OverflowMode,
}

impl ShortcutBar {
    pub(crate) fn at(placement: ShortcutPlacement, hints: Vec<KeyHint<'static>>) -> Self {
        Self {
            placement,
            hints,
            overflow: OverflowMode::default(),
        }
    }

    pub(crate) fn with_overflow(mut self, mode: OverflowMode) -> Self {
        self.overflow = mode;
        self
    }

    pub(crate) fn placement(&self) -> ShortcutPlacement {
        self.placement
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.hints.is_empty()
    }

    /// Width-aware rendering: returns 1+ lines depending on overflow mode.
    /// In `Truncate` mode this always returns exactly one line.
    /// In `Wrap` mode hints are distributed across lines so each fits within
    /// `max_width` columns (a single hint wider than `max_width` still gets
    /// its own line — we never break mid-hint).
    pub(crate) fn lines_for_width(&self, max_width: u16) -> Vec<Line<'static>> {
        match self.overflow {
            OverflowMode::Truncate => vec![shortcut_line(&self.hints)],
            OverflowMode::Wrap => shortcut_lines_wrapped(&self.hints, max_width),
        }
    }

    /// How many lines `lines_for_width` will produce.
    pub(crate) fn line_count_for_width(&self, max_width: u16) -> usize {
        match self.overflow {
            OverflowMode::Truncate => 1,
            OverflowMode::Wrap => {
                if self.hints.is_empty() {
                    return 1;
                }
                shortcut_lines_wrapped(&self.hints, max_width).len()
            }
        }
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

const HINT_SEPARATOR_WIDTH: u16 = 3; // "   "

/// Approximate display width of a single hint (key + description).
fn hint_display_width(hint: &KeyHint<'_>) -> u16 {
    use unicode_width::UnicodeWidthStr;

    let key_w: u16 = if let Some(spans) = &hint.key_spans {
        spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()) as u16)
            .sum()
    } else {
        UnicodeWidthStr::width(hint.key.as_ref()) as u16
    };
    let desc_w = UnicodeWidthStr::width(hint.description.as_ref()) as u16;
    key_w.saturating_add(desc_w)
}

/// Build the spans for a single hint (without leading separator).
fn hint_to_spans(hint: &KeyHint<'_>) -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(3);
    if let Some(key_spans) = &hint.key_spans {
        spans.extend(key_spans.iter().cloned());
    } else {
        spans.push(Span::styled(hint.key.clone().into_owned(), hint.key_style));
    }
    spans.push(Span::styled(
        hint.description.clone().into_owned(),
        hint.description_style,
    ));
    spans
}

/// Wrap hints across multiple lines so each line fits within `max_width`.
///
/// A hint that is wider than `max_width` on its own still gets a dedicated
/// line (we never break mid-hint).  The three-space separator between hints
/// is only inserted between hints on the same line.
fn shortcut_lines_wrapped(hints: &[KeyHint<'_>], max_width: u16) -> Vec<Line<'static>> {
    if hints.is_empty() {
        return vec![Line::default()];
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut current_width: u16 = 0;

    for hint in hints {
        let hw = hint_display_width(hint);

        if !current_spans.is_empty() {
            // Would adding separator + this hint exceed the line?
            let needed = HINT_SEPARATOR_WIDTH.saturating_add(hw);
            if current_width.saturating_add(needed) > max_width {
                // Flush current line, start a new one.
                lines.push(Line::from(std::mem::take(&mut current_spans)));
                current_width = 0;
            } else {
                current_spans.push(Span::raw("   "));
                current_width = current_width.saturating_add(HINT_SEPARATOR_WIDTH);
            }
        }

        current_spans.extend(hint_to_spans(hint));
        current_width = current_width.saturating_add(hw);
    }

    if !current_spans.is_empty() {
        lines.push(Line::from(current_spans));
    }

    if lines.is_empty() {
        lines.push(Line::default());
    }

    lines
}

/// Navigate (↑ ↓) hint with bi-colored arrows — up in `function()`, down in
/// `primary()` — so the pair reads as two distinct arrows rather than a
/// monochrome zigzag.
pub(crate) fn hint_nav(description: &'static str) -> KeyHint<'static> {
    KeyHint::new(icons::nav_up_down(), description)
        .with_key_spans(vec![
            Span::styled(icons::arrow_up(), Style::new().fg(colors::function())),
            Span::raw(" "),
            Span::styled(icons::arrow_down(), Style::new().fg(colors::primary())),
        ])
}

/// Horizontal (◂ ▸) hint with bi-colored arrows — left in `function()`, right
/// in `primary()`.
pub(crate) fn hint_nav_horizontal(description: &'static str) -> KeyHint<'static> {
    KeyHint::new(icons::nav_left_right(), description)
        .with_key_spans(vec![
            Span::styled(icons::arrow_left(), Style::new().fg(colors::function())),
            Span::raw(" "),
            Span::styled(icons::arrow_right(), Style::new().fg(colors::primary())),
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

    #[test]
    fn wrap_fits_all_on_one_line_when_wide_enough() {
        let hints = vec![
            KeyHint::new("Enter", " save"),
            KeyHint::new("Esc", " close"),
        ];
        // "Enter save" = 10, "   " = 3, "Esc close" = 9 → total 22
        let lines = shortcut_lines_wrapped(&hints, 30);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn wrap_splits_when_too_narrow() {
        let hints = vec![
            KeyHint::new("Enter", " save"),
            KeyHint::new("Esc", " close"),
            KeyHint::new("Tab", " next"),
        ];
        // "Enter save" = 10, "Esc close" = 9, "Tab next" = 8
        // At width 20: first line fits "Enter save   Esc close" (22)? No, 22 > 20.
        // So: line 1 = "Enter save" (10), line 2 = "Esc close   Tab next" (20)? 9+3+8=20 fits.
        let lines = shortcut_lines_wrapped(&hints, 20);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn wrap_each_hint_on_own_line_when_very_narrow() {
        let hints = vec![
            KeyHint::new("Enter", " save"),
            KeyHint::new("Esc", " close"),
        ];
        // Each hint is ~10 chars. Width 12 means only 1 hint per line.
        let lines = shortcut_lines_wrapped(&hints, 12);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn wrap_empty_returns_single_empty_line() {
        let lines = shortcut_lines_wrapped(&[], 40);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].spans.is_empty());
    }

    #[test]
    fn wrap_single_oversized_hint_gets_own_line() {
        let hints = vec![
            KeyHint::new("VeryLongKeyName", " very long description text"),
        ];
        // This hint is wider than the max_width, but it should still get a line.
        let lines = shortcut_lines_wrapped(&hints, 10);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn shortcut_bar_line_count_matches_lines() {
        let bar = ShortcutBar::at(
            ShortcutPlacement::Bottom,
            vec![
                KeyHint::new("Enter", " save"),
                KeyHint::new("Esc", " close"),
                KeyHint::new("Tab", " next"),
            ],
        )
        .with_overflow(OverflowMode::Wrap);

        for width in [10u16, 20, 30, 50, 80] {
            assert_eq!(
                bar.line_count_for_width(width),
                bar.lines_for_width(width).len(),
                "mismatch at width {width}",
            );
        }
    }

    #[test]
    fn truncate_mode_always_returns_one_line() {
        let bar = ShortcutBar::at(
            ShortcutPlacement::Bottom,
            vec![
                KeyHint::new("Enter", " save"),
                KeyHint::new("Esc", " close"),
                KeyHint::new("Tab", " next"),
            ],
        )
        .with_overflow(OverflowMode::Truncate);

        assert_eq!(bar.line_count_for_width(5), 1);
        assert_eq!(bar.lines_for_width(5).len(), 1);
    }
}
