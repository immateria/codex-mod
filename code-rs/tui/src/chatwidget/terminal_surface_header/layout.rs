use super::*;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;

pub(super) fn render_dynamic_header_line(input: &DynamicHeaderLayoutInput<'_>) -> HeaderTemplateRender {
    let mut include_reasoning = !input.minimal_header;
    let mut include_model = !input.minimal_header;
    let mut include_shell = !input.minimal_header;
    let mut include_mcp = !input.minimal_header && input.mcp_indicator.is_some();
    let mut include_branch = !input.minimal_header && input.branch.is_some();
    let mut include_dir = !input.minimal_header && !input.demo_mode;
    let mut use_short_dir = false;

    let build = |include_reasoning: bool,
                 include_model: bool,
                 include_shell: bool,
                 include_mcp: bool,
                 include_branch: bool,
                 include_dir: bool,
                 dir_display: &str| {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut ranges: Vec<(std::ops::Range<usize>, ClickableAction)> = Vec::new();
        let mut width = 0usize;

        let push_text =
            |spans: &mut Vec<Span<'static>>, width: &mut usize, text: &str, style: Style| {
                *width += text.chars().count();
                spans.push(Span::styled(text.to_string(), style));
            };

        let push_separator = |spans: &mut Vec<Span<'static>>, width: &mut usize| {
            push_text(
                spans,
                width,
                "  •  ",
                Style::default().fg(crate::colors::text_dim()),
            );
        };

        let push_clickable_labeled_segment = |spans: &mut Vec<Span<'static>>,
                                              ranges: &mut Vec<(std::ops::Range<usize>, ClickableAction)>,
                                              width: &mut usize,
                                              label: &str,
                                              value: &str,
                                              value_style: Style,
                                              action: ClickableAction| {
            let is_hovered = input
                .hovered_action
                .as_ref()
                .is_some_and(|hovered| hovered == &action);
            let start = *width;
            push_text(
                spans,
                width,
                label,
                apply_hover_style(
                    Style::default().fg(crate::colors::text_dim()),
                    input.hover_style,
                    is_hovered,
                ),
            );
            push_text(
                spans,
                width,
                value,
                apply_hover_style(value_style, input.hover_style, is_hovered),
            );
            let end = *width;
            if end > start {
                ranges.push((start..end, action));
            }
        };

        push_text(
            &mut spans,
            &mut width,
            input.title,
            Style::default()
                .fg(crate::colors::text())
                .add_modifier(Modifier::BOLD),
        );

        if include_model {
            push_separator(&mut spans, &mut width);
            push_clickable_labeled_segment(
                &mut spans,
                &mut ranges,
                &mut width,
                "Model: ",
                input.model,
                Style::default().fg(crate::colors::info()),
                ClickableAction::ShowModelSelector,
            );
        }

        if include_shell {
            push_separator(&mut spans, &mut width);
            push_clickable_labeled_segment(
                &mut spans,
                &mut ranges,
                &mut width,
                "Shell: ",
                input.shell,
                Style::default().fg(crate::colors::info()),
                ClickableAction::ShowShellSelector,
            );
        }

        if include_mcp
            && let Some((kind, value)) = input.mcp_indicator
        {
            push_separator(&mut spans, &mut width);
            push_text(
                &mut spans,
                &mut width,
                "MCP: ",
                Style::default().fg(crate::colors::text_dim()),
            );
            let value_style = match kind {
                McpHeaderIndicatorKind::Connecting => Style::default().fg(crate::colors::info()),
                McpHeaderIndicatorKind::Error => Style::default()
                    .fg(crate::colors::error())
                    .add_modifier(Modifier::BOLD),
            };
            push_text(&mut spans, &mut width, value, value_style);
        }

        if include_reasoning {
            push_separator(&mut spans, &mut width);
            push_clickable_labeled_segment(
                &mut spans,
                &mut ranges,
                &mut width,
                "Reasoning: ",
                input.reasoning,
                Style::default().fg(crate::colors::info()),
                ClickableAction::ShowReasoningSelector,
            );
        }

        if include_dir {
            push_separator(&mut spans, &mut width);
            push_text(
                &mut spans,
                &mut width,
                "Directory: ",
                Style::default().fg(crate::colors::text_dim()),
            );
            push_text(
                &mut spans,
                &mut width,
                dir_display,
                Style::default().fg(crate::colors::info()),
            );
        }

        if include_branch
            && let Some(branch) = input.branch
        {
            push_separator(&mut spans, &mut width);
            push_text(
                &mut spans,
                &mut width,
                "Branch: ",
                Style::default().fg(crate::colors::text_dim()),
            );
            push_text(
                &mut spans,
                &mut width,
                branch,
                Style::default().fg(crate::colors::success_green()),
            );
        }

        HeaderTemplateRender {
            line: Line::from(spans),
            clickable_ranges: ranges,
            width,
        }
    };

    let mut render = build(
        include_reasoning,
        include_model,
        include_shell,
        include_mcp,
        include_branch,
        include_dir,
        input.directory_full,
    );

    if include_dir && !use_short_dir && render.width > input.inner_width {
        use_short_dir = true;
        render = build(
            include_reasoning,
            include_model,
            include_shell,
            include_mcp,
            include_branch,
            include_dir,
            input.directory_short,
        );
    }

    while render.width > input.inner_width {
        if include_reasoning {
            include_reasoning = false;
        } else if include_model {
            include_model = false;
        } else if include_shell {
            include_shell = false;
        } else if include_mcp
            && matches!(
                input.mcp_indicator.map(|(kind, _)| kind),
                Some(McpHeaderIndicatorKind::Connecting)
            )
        {
            include_mcp = false;
        } else if include_branch {
            include_branch = false;
        } else if include_dir {
            include_dir = false;
        } else if include_mcp {
            include_mcp = false;
        } else {
            break;
        }
        render = build(
            include_reasoning,
            include_model,
            include_shell,
            include_mcp,
            include_branch,
            include_dir,
            if use_short_dir {
                input.directory_short
            } else {
                input.directory_full
            },
        );
    }

    render
}
