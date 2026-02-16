use super::*;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;

pub(super) fn render_plain_header_template(
    template: &str,
    context: &HeaderTemplateContext<'_>,
) -> String {
    template
        .replace("{title}", context.title)
        .replace("{model}", context.model)
        .replace("{shell}", context.shell)
        .replace("{reasoning}", context.reasoning)
        .replace("{directory}", context.directory)
        .replace("{branch}", context.branch)
        .replace("{mcp}", context.mcp)
}

pub(super) fn render_styled_header_template(
    template: &str,
    context: &HeaderTemplateContext<'_>,
) -> HeaderTemplateRender {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut span_starts: Vec<usize> = Vec::new();
    let mut clickable_ranges: Vec<(std::ops::Range<usize>, ClickableAction)> = Vec::new();
    let mut width = 0usize;
    let mut index = 0usize;
    let dim_style = Style::default().fg(crate::colors::text_dim());

    let push_literal = |spans: &mut Vec<Span<'static>>,
                        span_starts: &mut Vec<usize>,
                        width: &mut usize,
                        value: &str| {
        if value.is_empty() {
            return;
        }
        let start = *width;
        *width += value.chars().count();
        span_starts.push(start);
        spans.push(Span::styled(
            value.to_string(),
            Style::default().fg(crate::colors::text_dim()),
        ));
    };

    while index < template.len() {
        let Some(open_rel) = template[index..].find('{') else {
            push_literal(&mut spans, &mut span_starts, &mut width, &template[index..]);
            break;
        };
        let open = index + open_rel;
        push_literal(&mut spans, &mut span_starts, &mut width, &template[index..open]);

        let Some(close_rel) = template[open + 1..].find('}') else {
            push_literal(&mut spans, &mut span_starts, &mut width, &template[open..]);
            break;
        };

        let close = open + 1 + close_rel;
        let key = &template[open + 1..close];
        let replacement_and_action: Option<(&str, Style, Option<ClickableAction>)> = match key {
            "title" => Some((
                context.title,
                Style::default().fg(crate::colors::text()).add_modifier(ratatui::style::Modifier::BOLD),
                None,
            )),
            "model" => Some((
                context.model,
                Style::default().fg(crate::colors::info()),
                Some(ClickableAction::ShowModelSelector),
            )),
            "shell" => Some((
                context.shell,
                Style::default().fg(crate::colors::info()),
                Some(ClickableAction::ShowShellSelector),
            )),
            "reasoning" => Some((
                context.reasoning,
                Style::default().fg(crate::colors::info()),
                Some(ClickableAction::ShowReasoningSelector),
            )),
            "directory" => Some((
                context.directory,
                Style::default().fg(crate::colors::info()),
                None,
            )),
            "branch" => Some((
                context.branch,
                Style::default().fg(crate::colors::success_green()),
                None,
            )),
            "mcp" => {
                let mcp_style = match context.mcp_kind {
                    Some(McpHeaderIndicatorKind::Connecting) => {
                        Style::default().fg(crate::colors::info())
                    }
                    Some(McpHeaderIndicatorKind::Error) => Style::default()
                        .fg(crate::colors::error())
                        .add_modifier(Modifier::BOLD),
                    None => Style::default().fg(crate::colors::success_green()),
                };
                Some((context.mcp, mcp_style, None))
            }
            _ => None,
        };

        if let Some((value, style, maybe_action)) = replacement_and_action {
            let is_hovered = maybe_action
                .as_ref()
                .zip(context.hovered_action.as_ref())
                .is_some_and(|(action, hovered)| action == hovered);
            let mut click_start = width;

            // If the literal segment before this placeholder ends with a label
            // fragment (e.g. "Model: "), treat that label as part of the same
            // interactive target so custom templates match default header UX.
            if maybe_action.is_some()
                && let (Some(last_span), Some(last_start)) = (spans.pop(), span_starts.pop())
            {
                let content = last_span.content.to_string();
                let label_start_byte = if let Some(pos) = content.rfind('•') {
                    let mut i = pos + '•'.len_utf8();
                    while i < content.len() && content[i..].starts_with(' ') {
                        i += 1;
                    }
                    i
                } else {
                    content.len() - content.trim_start().len()
                };
                let label_suffix = content.get(label_start_byte..).unwrap_or_default();
                let should_split = !label_suffix.trim().is_empty() && label_suffix.contains(':');

                if should_split {
                    let prefix = content.get(..label_start_byte).unwrap_or_default().to_string();
                    let suffix = label_suffix.to_string();
                    if !prefix.is_empty() {
                        span_starts.push(last_start);
                        spans.push(Span::styled(prefix.clone(), last_span.style));
                    }
                    let suffix_start = last_start + prefix.chars().count();
                    click_start = suffix_start;
                    span_starts.push(suffix_start);
                    spans.push(Span::styled(
                        suffix,
                        apply_hover_style(dim_style, context.hover_style, is_hovered),
                    ));
                } else {
                    span_starts.push(last_start);
                    spans.push(last_span);
                }
            }

            let value_start = width;
            width += value.chars().count();
            let end = width;
            span_starts.push(value_start);
            spans.push(Span::styled(
                value.to_string(),
                apply_hover_style(style, context.hover_style, is_hovered),
            ));
            if let Some(action) = maybe_action
                && end > click_start
            {
                clickable_ranges.push((click_start..end, action));
            }
        } else {
            let raw_token = &template[open..=close];
            let start = width;
            width += raw_token.chars().count();
            span_starts.push(start);
            spans.push(Span::styled(raw_token.to_string(), dim_style));
        }

        index = close + 1;
    }

    HeaderTemplateRender {
        line: Line::from(spans),
        clickable_ranges,
        width,
    }
}
