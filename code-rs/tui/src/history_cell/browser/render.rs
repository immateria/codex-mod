use super::actions::{ActionDisplayLine, ActionEntry, format_action_line};
use super::screenshot::ScreenshotLayout;
use super::super::{HistoryCell, HistoryCellType, ToolCellStatus};
use super::super::card_style::{
    ansi16_inverse_color,
    browser_card_style,
    fill_card_background,
    hint_text_style,
    primary_text_style,
    rows_to_lines,
    secondary_text_style,
    title_text_style,
    truncate_with_ellipsis,
    CardRow,
    CardSegment,
    CardStyle,
    CARD_ACCENT_WIDTH,
};
use crate::colors;
use crate::theme::{palette_mode, PaletteMode};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Style;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use unicode_width::UnicodeWidthChar;
use url::Url;

use super::*;

#[derive(Copy, Clone)]
struct ActionColumns {
    label_width: usize,
    time_width: usize,
}

struct ActionRowInput<'a> {
    indent: &'a str,
    time: &'a str,
    time_width: usize,
    time_gap: usize,
    label: &'a str,
    label_width: usize,
    gap: &'a str,
    detail: &'a str,
    indent_cols: usize,
    right_padding: usize,
    time_style: Style,
    label_style: Style,
    detail_style: Style,
}

impl BrowserSessionCell {
    pub(crate) fn summary_label(&self) -> String {
        self.display_label()
    }
    fn accent_style(style: &CardStyle) -> Style {
        if palette_mode() == PaletteMode::Ansi16 {
            return Style::default().fg(ansi16_inverse_color());
        }
        let dim = colors::mix_toward(style.accent_fg, style.text_secondary, 0.85);
        Style::default().fg(dim)
    }

    fn normalized_title(&self) -> Option<String> {
        self.title
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("(pending)"))
            .map(std::string::ToString::to_string)
    }

    fn header_summary_text(&self) -> String {
        let label = if self.headless.unwrap_or(true) {
            "Browser (headless)"
        } else {
            "Browser"
        };

        let mut title = format!("{}: {}", label, self.display_label());
        if let Some(code) = &self.status_code {
            title.push_str(&format!(" [{code}]"));
        }
        title
    }

    fn top_border_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        let mut segments = Vec::new();
        if body_width == 0 {
            return CardRow::new(
                BORDER_TOP.to_string(),
                Self::accent_style(style),
                segments,
                None,
            );
        }

        let title_style = if palette_mode() == PaletteMode::Ansi16 {
            Style::default().fg(ansi16_inverse_color())
        } else {
            title_text_style(style)
        };

        segments.push(CardSegment::new(" ".to_string(), title_style));
        let remaining = body_width.saturating_sub(1);
        let text = truncate_with_ellipsis(self.header_summary_text().as_str(), remaining);
        if !text.is_empty() {
            segments.push(CardSegment::new(text, title_style));
        }
        CardRow::new(BORDER_TOP.to_string(), Self::accent_style(style), segments, None)
    }

    fn blank_border_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        CardRow::new(
            BORDER_BODY.to_string(),
            Self::accent_style(style),
            vec![CardSegment::new(" ".repeat(body_width), Style::default())],
            None,
        )
    }

    fn body_text_row(
        &self,
        text: impl Into<String>,
        body_width: usize,
        style: &CardStyle,
        text_style: Style,
        indent_cols: usize,
        right_padding_cols: usize,
    ) -> CardRow {
        if body_width == 0 {
            return CardRow::new(BORDER_BODY.to_string(), Self::accent_style(style), Vec::new(), None);
        }
        let indent = indent_cols.min(body_width.saturating_sub(1));
        let available = body_width.saturating_sub(indent);
        let mut segments = Vec::new();
        if indent > 0 {
            segments.push(CardSegment::new(" ".repeat(indent), Style::default()));
        }
        let text: String = text.into();
        if available == 0 {
            return CardRow::new(BORDER_BODY.to_string(), Self::accent_style(style), segments, None);
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
        CardRow::new(BORDER_BODY.to_string(), Self::accent_style(style), segments, None)
    }

    fn bottom_border_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        let text_value = " [Ctrl+B] View · [Esc] Stop".to_string();
        let text = truncate_with_ellipsis(text_value.as_str(), body_width);
        let hint_style = if palette_mode() == PaletteMode::Ansi16 {
            Style::default().fg(ansi16_inverse_color())
        } else {
            hint_text_style(style)
        };
        let segment = CardSegment::new(text, hint_style);
        CardRow::new(BORDER_BOTTOM.to_string(), Self::accent_style(style), vec![segment], None)
    }

    fn display_host(&self) -> Option<String> {
        self
            .url
            .as_ref()
            .and_then(|url| Url::parse(url).ok())
            .and_then(|parsed| parsed.host_str().map(std::string::ToString::to_string))
    }

    fn display_label(&self) -> String {
        if let Some(title) = self.normalized_title() {
            return title;
        }
        if let Some(host) = self.display_host() {
            return host;
        }
        self
            .url.clone()
            .unwrap_or_else(|| "Browser Session".to_string())
    }

    fn build_card_rows(&self, width: u16, style: &CardStyle) -> (Vec<CardRow>, Option<ScreenshotLayout>) {
        if width == 0 {
            return (Vec::new(), None);
        }

        let accent_width = CARD_ACCENT_WIDTH.min(width as usize);
        let body_width = width
            .saturating_sub(accent_width as u16)
            .saturating_sub(1) as usize;
        if body_width == 0 {
            return (Vec::new(), None);
        }

        let mut rows: Vec<CardRow> = Vec::new();
        rows.push(self.top_border_row(body_width, style));
        rows.push(self.blank_border_row(body_width, style));

        let mut screenshot_layout = self.compute_screenshot_layout(body_width);
        let indent_cols = screenshot_layout
            .as_ref()
            .map(|layout| layout.indent_cols)
            .unwrap_or(DEFAULT_TEXT_INDENT);
        let indent_cols = indent_cols.min(body_width.saturating_sub(1));
        let right_padding = TEXT_RIGHT_PADDING.min(body_width);

        let content_start = rows.len();

        let show_minutes = self.total_duration.as_secs() >= 60;
        let action_display = self.formatted_action_display(show_minutes);
        let label_width = action_display
            .iter()
            .filter_map(|line| match line {
                ActionDisplayLine::Entry(entry) => Some(string_display_width(entry.label.as_str())),
                ActionDisplayLine::Ellipsis => None,
            })
            .max()
            .unwrap_or(0);
        let time_width = action_display
            .iter()
            .filter_map(|line| match line {
                ActionDisplayLine::Entry(entry) => {
                    Some(string_display_width(entry.time_label.as_str()))
                }
                ActionDisplayLine::Ellipsis => None,
            })
            .max()
            .unwrap_or(0)
            .max(ACTION_TIME_COLUMN_MIN_WIDTH);

        rows.push(self.body_text_row(
            "Actions",
            body_width,
            style,
            primary_text_style(style),
            indent_cols,
            right_padding,
        ));

        if action_display.is_empty() {
            for wrapped in wrap_card_lines(
                "No browser actions yet",
                body_width,
                indent_cols,
                right_padding,
            ) {
                rows.push(self.body_text_row(
                    wrapped,
                    body_width,
                    style,
                    secondary_text_style(style),
                    indent_cols,
                    right_padding,
                ));
            }
        } else {
            for line in action_display {
                match line {
                    ActionDisplayLine::Entry(entry) => {
                        let entry_rows = self.render_action_entry_rows(
                            &entry,
                            body_width,
                            style,
                            indent_cols,
                            right_padding,
                            ActionColumns {
                                label_width,
                                time_width,
                            },
                        );
                        rows.extend(entry_rows);
                    }
                    ActionDisplayLine::Ellipsis => {
                        let mut segments = Vec::new();
                        if indent_cols > 0 {
                            segments.push(CardSegment::new(" ".repeat(indent_cols), Style::default()));
                        }
                        let padded = format!("{:>width$}", "⋮", width = time_width.max(1));
                        segments.push(CardSegment::new(
                            padded,
                            secondary_text_style(style),
                        ));
                        if ACTION_TIME_GAP > 0 {
                            segments.push(CardSegment::new(
                                " ".repeat(ACTION_TIME_GAP),
                                Style::default(),
                            ));
                        }
                        rows.push(CardRow::new(
                            BORDER_BODY.to_string(),
                            Self::accent_style(style),
                            segments,
                            None,
                        ));
                    }
                }
            }
        }

        let console_rows = self.console_rows(body_width, style, indent_cols, right_padding);
        if !console_rows.is_empty() {
            rows.push(self.blank_border_row(body_width, style));
            rows.extend(console_rows);
        }

        if let Some(layout) = screenshot_layout.as_mut() {
            layout.start_row = content_start;
            let existing = rows.len().saturating_sub(content_start);
            if existing < layout.height_rows {
                let missing = layout.height_rows - existing;
                for _ in 0..missing {
                    rows.push(self.body_text_row(
                        "",
                        body_width,
                        style,
                        Style::default(),
                        indent_cols,
                        right_padding,
                    ));
                }
            }
        }

        rows.push(self.blank_border_row(body_width, style));
        rows.push(self.bottom_border_row(body_width, style));

        (rows, screenshot_layout)
    }

    fn console_rows(
        &self,
        body_width: usize,
        style: &CardStyle,
        indent_cols: usize,
        right_padding: usize,
    ) -> Vec<CardRow> {
        let Some(last) = self.console_messages.last().cloned() else { return Vec::new() };
        let upper = last.to_ascii_uppercase();
        let is_warning = upper.contains("WARN")
            || upper.contains("WARNING")
            || upper.contains("ERROR")
            || upper.contains("EXCEPTION")
            || upper.contains("UNHANDLEDREJECTION");
        let style_color = if is_warning {
            Style::default().fg(colors::warning())
        } else {
            secondary_text_style(style)
        };
        let text = format!("Console: {last}");
        wrap_card_lines(text.as_str(), body_width, indent_cols, right_padding)
            .into_iter()
            .map(|wrapped| {
                self.body_text_row(
                    wrapped,
                    body_width,
                    style,
                    style_color,
                    indent_cols,
                    right_padding,
                )
            })
            .collect()
    }

    fn render_action_entry_rows(
        &self,
        entry: &ActionEntry,
        body_width: usize,
        style: &CardStyle,
        indent_cols: usize,
        right_padding: usize,
        action_columns: ActionColumns,
    ) -> Vec<CardRow> {
        let ActionColumns {
            label_width,
            time_width,
        } = action_columns;
        if body_width == 0 || time_width == 0 {
            return Vec::new();
        }

        let indent = indent_cols.min(body_width.saturating_sub(1));
        let available = body_width.saturating_sub(indent);
        if available <= time_width {
            return self.render_fallback_entry(entry, body_width, style, indent_cols, right_padding, time_width);
        }

        const TIME_GAP: usize = ACTION_TIME_GAP;
        let after_time = available.saturating_sub(time_width);
        if after_time <= TIME_GAP {
            return self.render_fallback_entry(entry, body_width, style, indent_cols, right_padding, time_width);
        }
        let after_time_gap = after_time.saturating_sub(TIME_GAP);
        if after_time_gap == 0 {
            return self.render_fallback_entry(entry, body_width, style, indent_cols, right_padding, time_width);
        }

        let base_available = after_time_gap.saturating_sub(right_padding);
        if base_available == 0 {
            return self.render_fallback_entry(entry, body_width, style, indent_cols, right_padding, time_width);
        }

        let max_label_width = base_available.saturating_sub(ACTION_LABEL_GAP + 1);
        if max_label_width == 0 {
            return self.render_fallback_entry(entry, body_width, style, indent_cols, right_padding, time_width);
        }

        let effective_label_width = label_width.min(max_label_width);
        let detail_width = base_available
            .saturating_sub(effective_label_width + ACTION_LABEL_GAP);
        if detail_width == 0 {
            return self.render_fallback_entry(entry, body_width, style, indent_cols, right_padding, time_width);
        }

        let label_full_width = string_display_width(entry.label.as_str());
        if effective_label_width < label_full_width {
            return self.render_fallback_entry(entry, body_width, style, indent_cols, right_padding, time_width);
        }

        let label_display = entry.label.clone();
        let label_padding = effective_label_width.saturating_sub(label_full_width);
        let gap = " ".repeat(ACTION_LABEL_GAP);

        let mut lines = wrap_line_to_width(entry.detail.as_str(), detail_width);
        if lines.is_empty() {
            lines.push(String::new());
        }

        let time_column = format!("{:<width$}", entry.time_label.as_str(), width = time_width);
        let continuation_time = " ".repeat(time_width);

        let label_column = format!("{}{}", label_display, " ".repeat(label_padding));
        let continuation_label = " ".repeat(effective_label_width);

        let mut rows = Vec::new();
        let label_style = secondary_text_style(style);
        let detail_style = primary_text_style(style);
        let time_style = primary_text_style(style);

        let indent_string = if indent > 0 {
            Some(" ".repeat(indent))
        } else {
            None
        };

        if let Some(first) = lines.first() {
            rows.push(self.build_action_row(
                body_width,
                style,
                ActionRowInput {
                    indent: indent_string.as_deref().unwrap_or(""),
                    time: time_column.as_str(),
                    time_width,
                    time_gap: TIME_GAP,
                    label: label_column.as_str(),
                    label_width: effective_label_width,
                    gap: &gap,
                    detail: first,
                    indent_cols: indent,
                    right_padding,
                    time_style,
                    label_style,
                    detail_style,
                },
            ));
        }

        for detail_line in lines.iter().skip(1) {
            rows.push(self.build_action_row(
                body_width,
                style,
                ActionRowInput {
                    indent: indent_string.as_deref().unwrap_or(""),
                    time: continuation_time.as_str(),
                    time_width,
                    time_gap: TIME_GAP,
                    label: continuation_label.as_str(),
                    label_width: effective_label_width,
                    gap: &gap,
                    detail: detail_line,
                    indent_cols: indent,
                    right_padding,
                    time_style,
                    label_style,
                    detail_style,
                },
            ));
        }

        rows
    }

    fn build_action_row(
        &self,
        body_width: usize,
        style: &CardStyle,
        input: ActionRowInput<'_>,
    ) -> CardRow {
        let ActionRowInput {
            indent,
            time,
            time_width,
            time_gap,
            label,
            label_width,
            gap,
            detail,
            indent_cols,
            right_padding,
            time_style,
            label_style,
            detail_style,
        } = input;
        let mut segments = Vec::new();
        let mut consumed = 0usize;
        if !indent.is_empty() {
            segments.push(CardSegment::new(indent.to_string(), Style::default()));
            consumed += indent_cols;
        }

        if !time.is_empty() {
            segments.push(CardSegment::new(time.to_string(), time_style));
            consumed += time_width;
        }

        if time_gap > 0 {
            segments.push(CardSegment::new(" ".repeat(time_gap), Style::default()));
            consumed += time_gap;
        }

        if !label.is_empty() {
            segments.push(CardSegment::new(label.to_string(), label_style));
            consumed += label_width;
        }

        if !gap.is_empty() {
            segments.push(CardSegment::new(gap.to_string(), Style::default()));
            consumed += ACTION_LABEL_GAP;
        }

        segments.push(CardSegment::new(detail.to_string(), detail_style));
        consumed += string_display_width(detail);

        let available = body_width.saturating_sub(consumed);
        if available > 0 {
            let pad = available.min(right_padding);
            if pad > 0 {
                segments.push(CardSegment::new(" ".repeat(pad), Style::default()));
            }
        }

        CardRow::new(BORDER_BODY.to_string(), Self::accent_style(style), segments, None)
    }

    fn render_fallback_entry(
        &self,
        entry: &ActionEntry,
        body_width: usize,
        style: &CardStyle,
        indent_cols: usize,
        _right_padding: usize,
        time_width: usize,
    ) -> Vec<CardRow> {
        if body_width == 0 {
            return Vec::new();
        }

        let indent = indent_cols.min(body_width.saturating_sub(1));
        let available = body_width.saturating_sub(indent);
        if available == 0 {
            return Vec::new();
        }

        let mut segments = Vec::new();
        if indent > 0 {
            segments.push(CardSegment::new(" ".repeat(indent), Style::default()));
        }

        let time_style = Style::default().fg(colors::text());
        let effective_time_width = available.min(time_width).max(ACTION_TIME_COLUMN_MIN_WIDTH.min(available));
        if effective_time_width == 0 {
            return Vec::new();
        }
        let time_display = format!("{:<width$}", entry.time_label.as_str(), width = effective_time_width);
        segments.push(CardSegment::new(time_display, time_style));

        let mut remaining = available.saturating_sub(effective_time_width);
        if remaining >= ACTION_TIME_GAP {
            segments.push(CardSegment::new(" ".repeat(ACTION_TIME_GAP), Style::default()));
            remaining = remaining.saturating_sub(ACTION_TIME_GAP);
        }

        if remaining > 0 {
            let combined = if entry.detail.is_empty() {
                entry.label.clone()
            } else {
                format!("{} {}", entry.label.trim(), entry.detail.trim())
            };
            let rest_display = truncate_with_ellipsis(combined.trim(), remaining);
            segments.push(CardSegment::new(rest_display, primary_text_style(style)));
        }

        vec![CardRow::new(
            BORDER_BODY.to_string(),
            Self::accent_style(style),
            segments,
            None,
        )]
    }

    fn build_plain_summary(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let status = if self.completed { "done" } else { "running" };
        lines.push(format!("Browser Session: {} [{}]", self.display_label(), status));
        if let Some(url) = &self.url {
            lines.push(format!("Opened: {url}"));
        }
        if let Some(code) = &self.status_code {
            lines.push(format!("Status: {code}"));
        }
        for action in self
            .actions
            .iter()
            .rev()
            .take(3)
            .rev()
        {
            lines.push(format!("Action: {}", format_action_line(action)));
        }
        if let Some(path) = &self.screenshot_path {
            lines.push(format!("Screenshot: {path}"));
        }
        lines
    }
}
impl HistoryCell for BrowserSessionCell {
    impl_as_any!();

    fn gutter_symbol(&self) -> Option<&'static str> {
        if self.completed {
            Some("✓")
        } else {
            None
        }
    }

    fn kind(&self) -> HistoryCellType {
        let status = if self.completed {
            ToolCellStatus::Success
        } else {
            ToolCellStatus::Running
        };
        HistoryCellType::Tool { status }
    }

    fn call_id(&self) -> Option<&str> {
        self.cell_key.as_deref()
    }

    fn parent_call_id(&self) -> Option<&str> {
        self.parent_call_id.as_deref()
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        self.build_plain_summary().into_iter().map(Line::from).collect()
    }

    fn desired_height(&self, width: u16) -> u16 {
        let style = browser_card_style();
        let trimmed_width = width.saturating_sub(2);
        if trimmed_width == 0 {
            return 0;
        }
        let (rows, _) = self.build_card_rows(trimmed_width, &style);
        rows.len().max(1) as u16
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        if area.width <= 2 || area.height == 0 {
            return;
        }

        let style = browser_card_style();
        let draw_width = area.width - 2;
        let render_area = Rect {
            width: draw_width,
            ..area
        };

        fill_card_background(buf, render_area, &style);
        let (rows, screenshot_meta) = self.build_card_rows(render_area.width, &style);
        let lines = rows_to_lines(&rows, &style, render_area.width);
        let text = Text::from(lines);

        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((skip_rows, 0))
            .render(render_area, buf);

        if let Some(layout) = screenshot_meta.as_ref()
            && let Some(path) = self.screenshot_path.as_ref() {
                self.render_screenshot_preview(render_area, buf, skip_rows, layout, path);
            }

        let clear_start = area.x + draw_width;
        let clear_end = area.x + area.width;
        for x in clear_start..clear_end {
            for row in 0..area.height {
                let cell = &mut buf[(x, area.y + row)];
                cell.set_symbol(" ");
                cell.set_bg(crate::colors::background());
            }
        }
    }
}
fn wrap_card_lines(text: &str, body_width: usize, indent_cols: usize, right_padding: usize) -> Vec<String> {
    let available = body_width
        .saturating_sub(indent_cols)
        .saturating_sub(right_padding);
    if available == 0 {
        return vec![String::new()];
    }
    wrap_line_to_width(text, available)
}

fn wrap_line_to_width(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    if text.trim().is_empty() {
        return vec![String::new()];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for word in text.split_whitespace() {
        let mut word_parts = if string_display_width(word) > width {
            split_long_card_word(word, width)
        } else {
            vec![word.to_string()]
        };

        for part in word_parts.drain(..) {
            let part_width = string_display_width(part.as_str());
            if current.is_empty() {
                current.push_str(part.as_str());
                current_width = part_width;
            } else if current_width + 1 + part_width > width {
                lines.push(current);
                current = part.clone();
                current_width = part_width;
            } else {
                current.push(' ');
                current.push_str(part.as_str());
                current_width += 1 + part_width;
            }
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn split_long_card_word(word: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }

    let mut parts = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for ch in word.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(1);
        if current_width + ch_width > width && !current.is_empty() {
            parts.push(current);
            current = String::new();
            current_width = 0;
        }
        current.push(ch);
        current_width += ch_width;
    }

    if !current.is_empty() {
        parts.push(current);
    }

    if parts.is_empty() {
        parts.push(String::new());
    }
    parts
}

fn string_display_width(text: &str) -> usize {
    text
        .chars()
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}

impl crate::chatwidget::tool_cards::ToolCardCell for BrowserSessionCell {
    fn tool_card_key(&self) -> Option<&str> {
        self.cell_key()
    }

    fn set_tool_card_key(&mut self, key: Option<String>) {
        self.set_cell_key(key);
    }
}
