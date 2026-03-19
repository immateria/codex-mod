use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::colors;

use super::clear;
use super::super::{AutoActiveViewModel, AutoCoordinatorView};

pub(super) fn pending_prompt_content_lines(
    _view: &AutoCoordinatorView,
    model: &AutoActiveViewModel,
    inner_width: usize,
) -> Vec<(String, Style)> {
    if inner_width == 0 {
        return Vec::new();
    }

    let indent = "  ";
    let indent_width = UnicodeWidthStr::width(indent);
    let text_width = inner_width.saturating_sub(indent_width);

    let context_style = Style::default()
        .fg(colors::text_dim())
        .add_modifier(Modifier::ITALIC);
    let prompt_style = Style::default().fg(colors::text());

    let context = model
        .cli_context
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    let prompt = model
        .cli_prompt
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());

    let mut rows: Vec<(String, Style)> = Vec::new();

    let add_segments = |text: &str, style: Style, rows: &mut Vec<(String, Style)>| {
        if text_width == 0 {
            let padded = AutoCoordinatorView::pad_to_width(indent, inner_width);
            rows.push((padded, style));
            return;
        }

        if text.trim().is_empty() {
            let padded = AutoCoordinatorView::pad_to_width("", inner_width);
            rows.push((padded, style));
            return;
        }

        for segment in AutoCoordinatorView::wrap_text_segments(text, text_width) {
            let body = format!("{indent}{segment}");
            let padded = AutoCoordinatorView::pad_to_width(&body, inner_width);
            rows.push((padded, style));
        }
    };

    let mut inserted_context = false;
    if let Some(value) = context {
        for line in value.lines() {
            add_segments(line.trim_end(), context_style, &mut rows);
        }
        inserted_context = true;
    }

    let mut inserted_prompt = false;
    if let Some(value) = prompt {
        if inserted_context {
            add_segments("", prompt_style, &mut rows);
        }
        for line in value.lines() {
            add_segments(line.trim_end(), prompt_style, &mut rows);
        }
        inserted_prompt = true;
    }

    if !inserted_prompt {
        return Vec::new();
    }

    rows
}

pub(super) fn has_pending_prompt_content(model: &AutoActiveViewModel) -> bool {
    model.cli_prompt.is_some() || model.cli_context.is_some()
}

pub(super) fn render_pending_prompt_block(
    view: &AutoCoordinatorView,
    area: Rect,
    buf: &mut Buffer,
    model: &AutoActiveViewModel,
) -> u16 {
    if area.width < 4 || area.height < 3 {
        clear::clear_rect(area, buf);
        return 0;
    }

    let inner_width = area.width.saturating_sub(2) as usize;
    let content_rows = pending_prompt_content_lines(view, model, inner_width);
    if content_rows.is_empty() {
        clear::clear_rect(area, buf);
        return 0;
    }

    let max_content_rows = area.height.saturating_sub(2) as usize;
    let visible_rows: Vec<_> = content_rows.into_iter().take(max_content_rows).collect();

    clear::clear_rect(area, buf);

    let border_style = Style::default().fg(colors::text_dim());
    let title = " Next Prompt ";
    let title_width = title.chars().count();
    let mut top_line = String::from("╭");
    if title_width + 2 <= inner_width {
        top_line.push_str(title);
        top_line.push_str(&"╌".repeat(inner_width.saturating_sub(title_width)));
    } else {
        top_line.push_str(&"╌".repeat(inner_width));
    }
    top_line.push('╮');
    write_text_line(buf, area.x, area.y, &top_line, border_style);

    let mut used = 1u16;
    let mut current_y = area.y + 1;
    for (text, style) in visible_rows {
        if current_y >= area.y + area.height - 1 {
            break;
        }

        let left_cell = &mut buf[(area.x, current_y)];
        left_cell.set_symbol("│");
        left_cell.set_style(border_style);

        for (idx, ch) in text.chars().enumerate() {
            let cell = &mut buf[(area.x + 1 + idx as u16, current_y)];
            let mut utf8 = [0u8; 4];
            let sym = ch.encode_utf8(&mut utf8);
            cell.set_symbol(sym);
            cell.set_style(style);
        }

        let right_cell = &mut buf[(area.x + area.width - 1, current_y)];
        right_cell.set_symbol("│");
        right_cell.set_style(border_style);

        current_y += 1;
        used = used.saturating_add(1);
    }

    if current_y >= area.y + area.height {
        current_y = area.y + area.height - 1;
    }

    let bottom_line = format!("╰{}╯", "╌".repeat(inner_width));
    write_text_line(buf, area.x, current_y, &bottom_line, border_style);
    used = used.saturating_add(1);

    used
}

fn write_text_line(buf: &mut Buffer, x: u16, y: u16, text: &str, style: Style) {
    for (idx, ch) in text.chars().enumerate() {
        let cell = &mut buf[(x + idx as u16, y)];
        let mut utf8 = [0u8; 4];
        let sym = ch.encode_utf8(&mut utf8);
        cell.set_symbol(sym);
        cell.set_style(style);
    }
}

pub(super) fn status_message_line(
    view: &AutoCoordinatorView,
    display_message: &str,
) -> Option<Line<'static>> {
    let message = view.status_message.as_ref()?;
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.eq_ignore_ascii_case("auto drive") {
        return None;
    }
    if trimmed == display_message {
        return None;
    }

    let style = Style::default().fg(colors::info()).add_modifier(Modifier::ITALIC);

    Some(Line::from(vec![
        Span::raw("   "),
        Span::styled(trimmed.to_string(), style),
    ]))
}

pub(super) fn status_message_wrap_count(
    view: &AutoCoordinatorView,
    width: u16,
    display_message: &str,
) -> usize {
    if width == 0 {
        return 0;
    }
    let Some(message) = view.status_message.as_ref() else {
        return 0;
    };
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return 0;
    }
    if trimmed.eq_ignore_ascii_case("auto drive") {
        return 0;
    }
    if trimmed == display_message {
        return 0;
    }
    let display = format!("   {trimmed}");
    AutoCoordinatorView::wrap_count(display.as_str(), width)
}

pub(super) fn cli_prompt_lines(model: &AutoActiveViewModel) -> Option<Vec<Line<'static>>> {
    let prompt = model
        .cli_prompt
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    let context = model
        .cli_context
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());

    prompt?;

    let context_style = Style::default()
        .fg(colors::text_dim())
        .add_modifier(Modifier::ITALIC);
    let prompt_style = Style::default().fg(colors::text());
    let indent = "  ";

    let mut lines: Vec<Line<'static>> = Vec::new();

    if let Some(value) = context {
        for line in value.lines() {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                lines.push(Line::default());
            } else {
                lines.push(Line::from(vec![
                    Span::raw(indent),
                    Span::styled(trimmed.to_string(), context_style),
                ]));
            }
        }
        if prompt.is_some() {
            lines.push(Line::default());
        }
    }

    if let Some(value) = prompt {
        for line in value.lines() {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                lines.push(Line::default());
            } else {
                lines.push(Line::from(vec![
                    Span::raw(indent),
                    Span::styled(trimmed.to_string(), prompt_style),
                ]));
            }
        }
    }

    Some(lines)
}

