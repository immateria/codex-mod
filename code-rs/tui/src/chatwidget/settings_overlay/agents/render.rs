use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use super::model::{AgentsOverviewState, AgentsSettingsContent};

impl AgentsSettingsContent {
    pub(super) fn render_overview(&self, area: Rect, buf: &mut Buffer, state: &AgentsOverviewState) {
        let lines = Self::build_overview_lines(state, Some(area.width as usize));
        Paragraph::new(lines)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .render(area, buf);
    }

    fn build_overview_lines(
        state: &AgentsOverviewState,
        available_width: Option<usize>,
    ) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from(Span::styled(
            "Agents",
            Style::default().add_modifier(Modifier::BOLD),
        )));

        let max_name_chars = state
            .rows
            .iter()
            .map(|row| row.name.chars().count())
            .max()
            .unwrap_or(0);
        let max_name_width = state
            .rows
            .iter()
            .map(|row| UnicodeWidthStr::width(row.name.as_str()))
            .max()
            .unwrap_or(0);

        for (idx, row) in state.rows.iter().enumerate() {
            let selected = idx == state.selected;
            let status = if !row.enabled {
                ("disabled", crate::colors::error())
            } else if !row.installed {
                ("not installed", crate::colors::warning())
            } else {
                ("enabled", crate::colors::success())
            };

            let mut spans = Vec::new();
            spans.push(Span::styled(
                if selected { "› " } else { "  " },
                if selected {
                    Style::default().fg(crate::colors::primary())
                } else {
                    Style::default()
                },
            ));
            spans.push(Span::styled(
                format!("{:<width$}", row.name, width = max_name_chars),
                if selected {
                    Style::default()
                        .fg(crate::colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ));
            spans.push(Span::raw("  "));
            spans.push(Span::styled("•", Style::default().fg(status.1)));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                status.0.to_string(),
                Style::default().fg(status.1),
            ));

            let mut showed_desc = false;
            if let Some(desc) = row
                .description
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                && let Some(width) = available_width
            {
                let status_width = UnicodeWidthStr::width(status.0);
                let prefix_width = 2 + max_name_width + 2 + 2 + status_width;
                if width > prefix_width + 3 {
                    let desc_width = width - prefix_width - 3;
                    if desc_width > 0 {
                        let truncated = Self::truncate_to_width(desc, desc_width);
                        if !truncated.is_empty() {
                            spans.push(Span::raw("  "));
                            spans.push(Span::styled(
                                truncated,
                                Style::default().fg(crate::colors::text_dim()),
                            ));
                            showed_desc = true;
                        }
                    }
                }
            }

            if selected && !showed_desc {
                spans.push(Span::raw("  "));
                let hint = if !row.installed {
                    "Enter to install"
                } else {
                    "Enter to configure"
                };
                spans.push(Span::styled(hint, Style::default().fg(crate::colors::text_dim())));
            }

            lines.push(Line::from(spans));
        }

        lines.push(Line::from(""));

        let add_agent_idx = state.rows.len();
        let add_agent_selected = add_agent_idx == state.selected;
        let mut add_spans: Vec<Span<'static>> = Vec::new();
        add_spans.push(Span::styled(
            if add_agent_selected { "› " } else { "  " },
            if add_agent_selected {
                Style::default().fg(crate::colors::primary())
            } else {
                Style::default()
            },
        ));
        add_spans.push(Span::styled(
            "Add new agent…",
            if add_agent_selected {
                Style::default()
                    .fg(crate::colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ));
        if add_agent_selected {
            add_spans.push(Span::raw("  "));
            add_spans.push(Span::styled(
                "Enter to configure",
                Style::default().fg(crate::colors::text_dim()),
            ));
        }
        lines.push(Line::from(add_spans));

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Commands",
            Style::default().add_modifier(Modifier::BOLD),
        )));

        for (offset, cmd) in state.commands.iter().enumerate() {
            let idx = state.rows.len() + 1 + offset;
            let selected = idx == state.selected;
            let mut spans = Vec::new();
            spans.push(Span::styled(
                if selected { "› " } else { "  " },
                if selected {
                    Style::default().fg(crate::colors::primary())
                } else {
                    Style::default()
                },
            ));
            spans.push(Span::styled(
                format!("/{cmd}"),
                if selected {
                    Style::default()
                        .fg(crate::colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ));
            if selected {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    "Enter to configure",
                    Style::default().fg(crate::colors::text_dim()),
                ));
            }
            lines.push(Line::from(spans));
        }

        let add_idx = state.rows.len() + 1 + state.commands.len();
        let add_selected = add_idx == state.selected;
        let mut add_spans = Vec::new();
        add_spans.push(Span::styled(
            if add_selected { "› " } else { "  " },
            if add_selected {
                Style::default().fg(crate::colors::primary())
            } else {
                Style::default()
            },
        ));
        add_spans.push(Span::styled(
            "Add new…",
            if add_selected {
                Style::default()
                    .fg(crate::colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ));
        if add_selected {
            add_spans.push(Span::raw("  "));
            add_spans.push(Span::styled(
                "Enter to create",
                Style::default().fg(crate::colors::text_dim()),
            ));
        }
        lines.push(Line::from(add_spans));

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(crate::colors::function())),
            Span::styled(" Navigate  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Enter", Style::default().fg(crate::colors::success())),
            Span::styled(" Open", Style::default().fg(crate::colors::text_dim())),
            Span::styled("  Esc", Style::default().fg(crate::colors::error())),
            Span::styled(" Close", Style::default().fg(crate::colors::text_dim())),
        ]));

        lines
    }

    fn truncate_to_width(text: &str, max_width: usize) -> String {
        crate::text_formatting::truncate_to_display_width(text, max_width)
    }
}

