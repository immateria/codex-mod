use ratatui::buffer::Buffer;
use ratatui::prelude::*;
use ratatui::style::Color;
use std::borrow::Cow;
use unicode_width::UnicodeWidthStr;

use crate::card_theme;
use crate::card_theme::{CardThemeDefinition, GradientSpec};
use crate::colors;
use crate::gradient_background::GradientBackground;
use crate::theme::{palette_mode, PaletteMode};
use crate::util::buffer::fill_rect;

#[derive(Clone, Copy)]
pub(crate) struct CardStyle {
    pub accent_fg: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub title_text: Color,
    pub gradient: GradientSpec,
}

#[derive(Clone, Debug)]
pub(crate) struct CardSegment {
    pub text: Cow<'static, str>,
    pub style: Style,
    pub inherit_background: bool,
}

impl CardSegment {
    pub fn new(text: impl Into<Cow<'static, str>>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
            inherit_background: true,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CardRow {
    pub accent: Cow<'static, str>,
    pub accent_style: Style,
    pub segments: Vec<CardSegment>,
    pub body_bg: Option<Color>,
}

impl CardRow {
    pub fn new(
        accent: impl Into<Cow<'static, str>>,
        accent_style: Style,
        segments: Vec<CardSegment>,
        body_bg: Option<Color>,
    ) -> Self {
        Self {
            accent: accent.into(),
            accent_style,
            segments,
            body_bg,
        }
    }
}

pub(crate) const CARD_ACCENT_WIDTH: usize = 2;

/// Compute the usable body width inside a card given the total available
/// terminal columns. Returns `None` when there is no room for content.
pub(crate) fn card_body_width(total_width: u16) -> Option<usize> {
    if total_width == 0 {
        return None;
    }
    let accent = CARD_ACCENT_WIDTH.min(total_width as usize);
    let body = total_width.saturating_sub(accent as u16).saturating_sub(1) as usize;
    (body != 0).then_some(body)
}

pub(crate) const BORDER_TOP: &str = "╭─";
pub(crate) const BORDER_BODY: &str = "│";
pub(crate) const BORDER_BOTTOM: &str = "╰─";

pub(crate) const HINT_SETTINGS_STOP: &str = " [Ctrl+S] Settings · [Esc] Stop";
/// Agent-run expand shortcut + stop hint.
pub(crate) const HINT_EXPAND_STOP: &str = " [Ctrl+A] Expand · [Esc] Stop";
/// Agent-run expand shortcut (no stop — run already finished).
pub(crate) const HINT_EXPAND: &str = " [Ctrl+A] Expand";
/// Browser view shortcut + stop hint.
pub(crate) const HINT_BROWSER_STOP: &str = " [Ctrl+B] View · [Esc] Stop";

// Shared layout constants for image and screenshot rendering.
pub(crate) const MEDIA_MIN_WIDTH: usize = 18;
pub(crate) const MEDIA_MAX_WIDTH: usize = 64;
pub(crate) const MEDIA_MAX_ROWS: usize = 60;
pub(crate) const MEDIA_TEXT_RIGHT_PADDING: usize = 2;
pub(crate) const MEDIA_MIN_TEXT_WIDTH: usize = 28;
/// Gap between the media/image column and the text column in side-by-side layout.
pub(crate) const MEDIA_GAP: usize = 2;
/// Left padding before the media/image column.
pub(crate) const MEDIA_LEFT_PAD: usize = 1;

// Shared layout constants for action/time column cards (browser, auto_drive,
// web_search, agent runs).
/// Space between the main content area and the time column.
pub(crate) const ACTION_TIME_SEPARATOR_WIDTH: usize = 2;
/// Minimum width for the elapsed-time column in card layouts.
pub(crate) const ACTION_TIME_COLUMN_MIN_WIDTH: usize = 2;
/// Default left indent for wrapped body text inside bordered cards.
pub(crate) const DEFAULT_TEXT_INDENT: usize = 2;

/// Build a `CardStyle` from a pair of light/dark theme definitions, applying
/// ANSI-16 fallback when needed.
fn card_style_for(
    dark_def: CardThemeDefinition,
    light_def: CardThemeDefinition,
) -> CardStyle {
    let is_dark = colors::is_dark_theme();
    let definition = if is_dark { dark_def } else { light_def };
    let mut style = style_from_theme(definition, is_dark);
    if palette_mode() == PaletteMode::Ansi16 {
        strip_ansi16_background(&mut style);
    }
    style
}

pub(crate) fn agent_card_style(_write_enabled: Option<bool>) -> CardStyle {
    card_style_for(
        card_theme::agent_read_only_dark_theme(),
        card_theme::agent_read_only_light_theme(),
    )
}

pub(crate) fn browser_card_style() -> CardStyle {
    card_style_for(
        card_theme::browser_dark_theme(),
        card_theme::browser_light_theme(),
    )
}

pub(crate) fn auto_drive_card_style() -> CardStyle {
    let mut style = card_style_for(
        card_theme::auto_drive_dark_theme(),
        card_theme::auto_drive_light_theme(),
    );
    if palette_mode() == PaletteMode::Ansi16 {
        let text_color = ansi16_inverse_color();
        style.title_text = text_color;
        style.accent_fg = text_color;
        style.text_primary = text_color;
        style.text_secondary = colors::warning();
    }
    style
}

pub(crate) fn web_search_card_style() -> CardStyle {
    card_style_for(
        card_theme::search_dark_theme(),
        card_theme::search_light_theme(),
    )
}

pub(crate) fn ansi16_inverse_color() -> Color {
    colors::text_bright()
}

fn strip_ansi16_background(style: &mut CardStyle) {
    style.gradient = GradientSpec {
        left: Color::Reset,
        right: Color::Reset,
        bias: 0.0,
    };
}

fn style_from_theme(definition: CardThemeDefinition, is_dark: bool) -> CardStyle {
    let theme = definition.theme;
    let mut text_primary = theme.palette.text;
    let mut text_secondary = theme.palette.footer;

    let is_rgb = |color: Color, r: u8, g: u8, b: u8| matches!(color, Color::Rgb(rr, gg, bb) if rr == r && gg == g && bb == b);

    if !is_dark && is_rgb(text_primary, 0, 0, 0) {
        let left = theme.gradient.left;
        text_primary = Color::Black;
        text_secondary = colors::mix_toward(Color::Black, left, 0.35);
    } else if is_dark && is_rgb(text_primary, 255, 255, 255) {
        let left = theme.gradient.left;
        text_primary = Color::White;
        text_secondary = colors::mix_toward(Color::White, left, 0.25);
    }

    let adjust = |color: Color, target: Color| match color {
        Color::Rgb(_, _, _) => colors::mix_toward(color, target, 0.15),
        other => other,
    };

    if is_dark {
        text_primary = adjust(text_primary, Color::White);
    } else {
        text_primary = adjust(text_primary, Color::Black);
    }

    let title_text = text_primary;

    CardStyle {
        accent_fg: theme.palette.border,
        text_primary,
        text_secondary,
        title_text,
        gradient: adjust_gradient(theme.gradient, is_dark),
    }
}

fn adjust_gradient(gradient: GradientSpec, _is_dark: bool) -> GradientSpec {
    gradient
}

pub(crate) fn fill_card_background(buf: &mut Buffer, area: Rect, style: &CardStyle) {
    if palette_mode() == PaletteMode::Ansi16
        && matches!(style.gradient.left, Color::Reset)
        && matches!(style.gradient.right, Color::Reset)
    {
        return;
    }

    if palette_mode() == PaletteMode::Ansi16
        && style.gradient.left == style.gradient.right
        && !matches!(style.gradient.left, Color::Rgb(_, _, _))
    {
        fill_rect(
            buf,
            area,
            Some(' '),
            Style::default()
                .bg(style.gradient.left)
                .fg(style.text_primary),
        );
    } else {
        GradientBackground::render(buf, area, &style.gradient, style.text_primary, None);
    }
}

pub(crate) fn pad_icon(icon: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let trimmed = crate::text_formatting::pad_to_display_width(icon, width);
    let current = UnicodeWidthStr::width(trimmed.as_str());
    if current < width {
        let mut result = trimmed;
        result.extend(std::iter::repeat_n(' ', width - current));
        return result;
    }
    trimmed
}



pub(crate) fn truncate_with_ellipsis(text: &str, width: usize) -> String {
    crate::text_formatting::pad_to_display_width(
        &crate::text_formatting::truncate_to_display_width_with_suffix(text, width, "..."),
        width,
    )
}

pub(crate) fn rows_to_lines(rows: Vec<CardRow>, _style: &CardStyle, total_width: u16) -> Vec<Line<'static>> {
    if total_width == 0 {
        return Vec::new();
    }
    let has_accent = rows.iter().any(|row| !row.accent.trim().is_empty());
    let accent_width = if has_accent {
        CARD_ACCENT_WIDTH.min(total_width as usize)
    } else {
        0
    };
    let body_width = total_width.saturating_sub(accent_width as u16) as usize;
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(rows.len());
    for row in rows {
        let mut spans: Vec<Span<'static>> = Vec::new();
        if accent_width > 0 {
            let accent_text = pad_icon(&row.accent, accent_width);
            let accent_span = Span::styled(accent_text, row.accent_style);
            spans.push(accent_span);
        }

        let row_bg = row.body_bg;
        let mut used_width = 0;
        for segment in row.segments {
            let mut seg_style = segment.style;
            if let (true, Some(bg)) = (segment.inherit_background, row_bg) {
                seg_style = seg_style.bg(bg);
            }
            let width = UnicodeWidthStr::width(&*segment.text);
            used_width += width;
            spans.push(Span::styled(segment.text, seg_style));
        }
        if used_width < body_width {
            let filler = " ".repeat(body_width - used_width);
            let filler_style = row_bg
                .map(|bg| Style::default().bg(bg))
                .unwrap_or_else(Style::default);
            spans.push(Span::styled(filler, filler_style));
        }
        lines.push(Line::from(spans));
    }
    lines
}

pub(crate) fn primary_text_style(style: &CardStyle) -> Style {
    Style::default().fg(style.text_primary)
}

pub(crate) fn secondary_text_style(style: &CardStyle) -> Style {
    Style::default().fg(style.text_secondary)
}

pub(crate) fn title_text_style(style: &CardStyle) -> Style {
    Style::default().fg(style.title_text)
}

pub(crate) fn hint_text_style(style: &CardStyle) -> Style {
    Style::default().fg(style.text_secondary)
}

/// Card accent column style shared by browser, image, agent, and web_search
/// cards. auto_drive uses a different formula.
pub(crate) fn accent_style(style: &CardStyle) -> Style {
    if palette_mode() == PaletteMode::Ansi16 {
        return Style::default().fg(ansi16_inverse_color());
    }
    let dim = colors::mix_toward(style.accent_fg, style.text_secondary, 0.85);
    Style::default().fg(dim)
}

/// Empty body row with a border glyph in the accent column.
pub(crate) fn blank_border_row(
    border: &'static str,
    body_width: usize,
    style: &CardStyle,
) -> CardRow {
    CardRow::new(
        border,
        accent_style(style),
        vec![CardSegment::new(" ".repeat(body_width), Style::default())],
        None,
    )
}

/// Single-line text row inside a card body, with optional indent and right
/// padding. Text is truncated with ellipsis when it exceeds the available
/// width.
pub(crate) fn body_text_row(
    border: &'static str,
    text: impl Into<String>,
    body_width: usize,
    style: &CardStyle,
    text_style: Style,
    indent_cols: usize,
    right_padding_cols: usize,
) -> CardRow {
    if body_width == 0 {
        return CardRow::new(border, accent_style(style), Vec::new(), None);
    }
    let indent = indent_cols.min(body_width.saturating_sub(1));
    let available = body_width.saturating_sub(indent);
    let mut segments = Vec::new();
    if indent > 0 {
        segments.push(CardSegment::new(" ".repeat(indent), Style::default()));
    }
    let text: String = text.into();
    if available == 0 {
        return CardRow::new(border, accent_style(style), segments, None);
    }
    let usable_width = available.saturating_sub(right_padding_cols);
    let display = if usable_width == 0 {
        String::new()
    } else {
        truncate_with_ellipsis(text.as_str(), usable_width)
    };
    segments.push(CardSegment::new(display, text_style));
    if right_padding_cols > 0 && available > 0 {
        let pad = right_padding_cols.min(available);
        segments.push(CardSegment::new(" ".repeat(pad), Style::default()));
    }
    CardRow::new(border, accent_style(style), segments, None)
}

/// Top border row with a title. Used by browser and image cards.
pub(crate) fn top_border_row_with_title(
    border: &'static str,
    title: &str,
    body_width: usize,
    style: &CardStyle,
) -> CardRow {
    let mut segments = Vec::new();
    if body_width == 0 {
        return CardRow::new(border, accent_style(style), segments, None);
    }

    let title_style = if palette_mode() == PaletteMode::Ansi16 {
        Style::default().fg(ansi16_inverse_color())
    } else {
        title_text_style(style)
    };

    segments.push(CardSegment::new(" ", title_style));
    let remaining = body_width.saturating_sub(1);
    let text = truncate_with_ellipsis(title, remaining);
    if !text.is_empty() {
        segments.push(CardSegment::new(text, title_style));
    }
    CardRow::new(border, accent_style(style), segments, None)
}

/// Bottom border row with hint text. Used by browser, image, and web_search.
pub(crate) fn bottom_border_row_with_hint(
    border: &'static str,
    hint: &str,
    body_width: usize,
    style: &CardStyle,
) -> CardRow {
    let text = truncate_with_ellipsis(hint, body_width);
    let hint_style = if palette_mode() == PaletteMode::Ansi16 {
        Style::default().fg(ansi16_inverse_color())
    } else {
        hint_text_style(style)
    };
    let segment = CardSegment::new(text, hint_style);
    CardRow::new(border, accent_style(style), vec![segment], None)
}
