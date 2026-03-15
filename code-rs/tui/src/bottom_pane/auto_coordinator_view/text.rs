use ratatui::text::Line;
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

use super::*;

impl AutoCoordinatorView {
    pub(super) fn wrap_text_segments(text: &str, width: usize) -> Vec<String> {
        if width == 0 {
            return vec![String::new()];
        }

        let mut lines: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut current_width = 0usize;

        for word in text.split_whitespace() {
            let word_width = UnicodeWidthStr::width(word);
            if word_width >= width {
                if !current.is_empty() {
                    lines.push(current);
                    current = String::new();
                    current_width = 0;
                }
                lines.push(Self::truncate_to_width(word, width));
                continue;
            }

            let separator_width = if current.is_empty() { 0 } else { 1 };
            if current_width + separator_width + word_width <= width {
                if !current.is_empty() {
                    current.push(' ');
                    current_width += 1;
                }
                current.push_str(word);
                current_width += word_width;
            } else {
                if !current.is_empty() {
                    lines.push(current);
                }
                current = word.to_string();
                current_width = word_width;
            }
        }

        if current.is_empty() {
            if lines.is_empty() {
                lines.push(String::new());
            }
        } else {
            lines.push(current);
        }

        lines
    }

    pub(super) fn truncate_to_width(text: &str, width: usize) -> String {
        crate::text_formatting::truncate_to_display_width(text, width)
    }

    pub(super) fn pad_to_width(text: &str, width: usize) -> String {
        let mut truncated = Self::truncate_to_width(text, width);
        let current_width = UnicodeWidthStr::width(truncated.as_str());
        if current_width < width {
            truncated.push_str(&" ".repeat(width - current_width));
        }
        truncated
    }

    pub(super) fn wrap_count(text: &str, width: u16) -> usize {
        if width == 0 {
            return text.lines().count().max(1);
        }
        let max_width = width as usize;
        text.lines()
            .map(|line| {
                let trimmed = line.trim_end();
                let w = UnicodeWidthStr::width(trimmed);
                let lines = if w == 0 { 1 } else { w.div_ceil(max_width) };
                lines.max(1)
            })
            .sum()
    }

    pub(super) fn lines_height(lines: &[Line<'static>], width: u16) -> u16 {
        if lines.is_empty() {
            return 0;
        }
        if width == 0 {
            return u16::try_from(lines.len()).unwrap_or(u16::MAX);
        }
        lines.iter().fold(0u16, |acc, line| {
            let line_width = u16::try_from(line.width()).unwrap_or(u16::MAX);
            let segments = if line_width == 0 { 1 } else { line_width.div_ceil(width) };
            acc.saturating_add(segments.max(1))
        })
    }

    pub(super) fn format_elapsed(duration: Duration) -> String {
        let total_seconds = duration.as_secs();
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;

        if hours > 0 {
            if minutes > 0 {
                format!("{hours}h {minutes:02}m")
            } else {
                format!("{hours}h")
            }
        } else if minutes > 0 {
            if seconds > 0 {
                format!("{minutes}m {seconds:02}s")
            } else {
                format!("{minutes}m")
            }
        } else {
            format!("{seconds}s")
        }
    }

    pub(super) fn format_turns(turns: usize) -> String {
        let label = if turns == 1 { "turn" } else { "turns" };
        format!("{turns} {label}")
    }

    pub(super) fn format_tokens(tokens: u64) -> String {
        if tokens >= 1_000 {
            format!("{}k tokens", tokens / 1_000)
        } else {
            format!("{tokens} tokens")
        }
    }
}

