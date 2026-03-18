use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use super::super::{McpSettingsMode, McpSettingsView};

use super::model::{
    centered_overlay_rect,
    format_opt_secs_compact,
    parse_secs_field,
    parse_u32_field,
    ServerRow,
    ServerSchedulingEditor,
    ToolRow,
    ToolSchedulingEditor,
    SERVER_ROWS,
    TOOL_ROWS,
};

impl McpSettingsView {
    pub(in crate::bottom_pane::settings_pages::mcp) fn render_policy_editor_framed(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let outer = Block::default().borders(Borders::ALL).inner(area);
        self.render_policy_editor_in_outer(outer, buf);
    }

    pub(in crate::bottom_pane::settings_pages::mcp) fn render_policy_editor_content_only(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        self.render_policy_editor_in_outer(area, buf);
    }

    fn render_policy_editor_in_outer(&self, outer: Rect, buf: &mut Buffer) {
        let overlay = centered_overlay_rect(outer, 76, 14);

        Clear.render(overlay, buf);

        match &self.mode {
            McpSettingsMode::EditServerScheduling(editor) => {
                self.render_server_editor(editor, overlay, buf);
            }
            McpSettingsMode::EditToolScheduling(editor) => {
                self.render_tool_editor(editor, overlay, buf);
            }
            McpSettingsMode::Main => {}
        }
    }

    fn render_server_editor(&self, editor: &ServerSchedulingEditor, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" MCP Scheduling: {} ", editor.server))
            .border_style(Style::default().fg(crate::colors::primary()))
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            );
        let inner = block.inner(area);
        block.render(area, buf);

        let dim = Style::default().fg(crate::colors::text_dim());
        let key_style = Style::default().fg(crate::colors::secondary());
        let value_style = Style::default().fg(crate::colors::text());
        let selected_style = Style::default()
            .bg(crate::colors::selection())
            .add_modifier(Modifier::BOLD);
        let err_style = Style::default().fg(crate::colors::error());

        let help =
            "Up/Down move · Enter edit/toggle · Del clear optional · Ctrl+S save · Esc cancel";
        Paragraph::new(Line::from(vec![Span::styled(help, dim)]))
            .render(
                Rect {
                    x: inner.x,
                    y: inner.y,
                    width: inner.width,
                    height: 1,
                },
                buf,
            );

        let min_value_width = 14u16;
        let label_width = inner.width.saturating_sub(min_value_width).clamp(12, 28);
        let value_width = inner.width.saturating_sub(label_width);

        let rows_end_y = inner.y.saturating_add(inner.height).saturating_sub(1);
        let mut y = inner.y.saturating_add(1);
        for (idx, row) in SERVER_ROWS.iter().enumerate() {
            if y >= rows_end_y {
                break;
            }
            let selected = idx == editor.selected_row;
            let row_style = if selected {
                selected_style
            } else {
                Style::default()
            };
            let prefix = if selected { "› " } else { "  " };

            let (label, value_text, field_opt): (
                &str,
                Option<String>,
                Option<(&crate::components::form_text_field::FormTextField, bool)>,
            ) = match row {
                ServerRow::Dispatch => (
                    "Dispatch",
                    Some(editor.scheduling.dispatch.to_string()),
                    None,
                ),
                ServerRow::MaxConcurrent => (
                    "Max concurrent",
                    None,
                    Some((
                        &editor.max_concurrent_field,
                        editor.editing == Some(ServerRow::MaxConcurrent),
                    )),
                ),
                ServerRow::MinInterval => (
                    "Min interval (sec)",
                    None,
                    Some((
                        &editor.min_interval_field,
                        editor.editing == Some(ServerRow::MinInterval),
                    )),
                ),
                ServerRow::QueueTimeout => (
                    "Queue timeout (sec)",
                    None,
                    Some((
                        &editor.queue_timeout_field,
                        editor.editing == Some(ServerRow::QueueTimeout),
                    )),
                ),
                ServerRow::MaxQueueDepth => (
                    "Max queue depth",
                    None,
                    Some((
                        &editor.max_queue_depth_field,
                        editor.editing == Some(ServerRow::MaxQueueDepth),
                    )),
                ),
                ServerRow::Save => ("Save", Some("Ctrl+S".to_string()), None),
                ServerRow::Cancel => ("Cancel", Some("Esc".to_string()), None),
            };

            let label_rect = Rect {
                x: inner.x,
                y,
                width: label_width,
                height: 1,
            };
            let value_rect = Rect {
                x: inner.x.saturating_add(label_width),
                y,
                width: value_width,
                height: 1,
            };

            let label_line = Line::from(vec![
                Span::styled(prefix, row_style),
                Span::styled(
                    format!("{label}:"),
                    key_style.add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ),
            ]);
            Paragraph::new(label_line).render(label_rect, buf);

            if let Some((field, focused)) = field_opt {
                field.render(value_rect, buf, focused);
            } else {
                let value = value_text.unwrap_or_default();
                Paragraph::new(Line::from(vec![Span::styled(value, value_style)]))
                    .render(value_rect, buf);
            }
            y = y.saturating_add(1);
        }

        if let Some(err) = editor.error.as_deref() {
            let err_area = Rect {
                x: inner.x,
                y: rows_end_y,
                width: inner.width,
                height: 1,
            };
            Paragraph::new(Line::from(vec![Span::styled(
                err.to_string(),
                err_style,
            )]))
            .render(err_area, buf);
        }
    }

    fn render_tool_editor(&self, editor: &ToolSchedulingEditor, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(
                " MCP Tool Scheduling: {}/{} ",
                editor.server, editor.tool
            ))
            .border_style(Style::default().fg(crate::colors::primary()))
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            );
        let inner = block.inner(area);
        block.render(area, buf);

        let dim = Style::default().fg(crate::colors::text_dim());
        let key_style = Style::default().fg(crate::colors::secondary());
        let value_style = Style::default().fg(crate::colors::text());
        let selected_style = Style::default()
            .bg(crate::colors::selection())
            .add_modifier(Modifier::BOLD);
        let err_style = Style::default().fg(crate::colors::error());

        let help = "Enter toggle override · Del clear · Ctrl+S save · Esc cancel";
        Paragraph::new(Line::from(vec![Span::styled(help, dim)]))
            .render(
                Rect {
                    x: inner.x,
                    y: inner.y,
                    width: inner.width,
                    height: 1,
                },
                buf,
            );

        let override_min_interval_text = editor.min_interval_field.text();
        let (override_min_interval_value, override_min_interval_invalid) = if editor.override_min_interval {
            if override_min_interval_text.trim().is_empty() {
                (None, true)
            } else {
                match parse_secs_field("Min interval", override_min_interval_text) {
                    Ok(Some(v)) => (Some(v), false),
                    Ok(None) => (None, true),
                    Err(_) => (None, true),
                }
            }
        } else {
            (None, false)
        };

        let (override_max_concurrent_value, override_max_concurrent_invalid) =
            if editor.override_max_concurrent {
                match parse_u32_field("Max concurrent", editor.max_concurrent_field.text()) {
                    Ok(v) => (Some(v), false),
                    Err(_) => (None, true),
                }
            } else {
                (None, false)
            };

        let effective_min_interval = if override_min_interval_invalid {
            None
        } else {
            match (
                editor.server_scheduling.min_interval_sec,
                override_min_interval_value,
            ) {
                (None, None) => None,
                (Some(s), None) => Some(s),
                (None, Some(o)) => Some(o),
                (Some(s), Some(o)) => Some(s.max(o)),
            }
        };

        let effective_max_concurrent = if override_max_concurrent_invalid {
            None
        } else {
            match override_max_concurrent_value {
                Some(o) => Some(editor.server_scheduling.max_concurrent.min(o)),
                None => Some(editor.server_scheduling.max_concurrent),
            }
        };

        let effective_line = {
            let max_conc = if override_max_concurrent_invalid {
                "?".to_string()
            } else {
                effective_max_concurrent
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "?".to_string())
            };

            let min_int = if override_min_interval_invalid {
                "?".to_string()
            } else {
                format_opt_secs_compact(effective_min_interval)
            };
            format!("Effective: max_concurrent={max_conc}, min_interval={min_int}")
        };
        Paragraph::new(Line::from(vec![Span::styled(effective_line, dim)]))
            .render(
                Rect {
                    x: inner.x,
                    y: inner.y.saturating_add(1),
                    width: inner.width,
                    height: 1,
                },
                buf,
            );

        let min_value_width = 14u16;
        let label_width = inner.width.saturating_sub(min_value_width).clamp(12, 30);
        let value_width = inner.width.saturating_sub(label_width);

        let rows_end_y = inner.y.saturating_add(inner.height).saturating_sub(1);
        let mut y = inner.y.saturating_add(2);
        for (idx, row) in TOOL_ROWS.iter().enumerate() {
            if y >= rows_end_y {
                break;
            }
            let selected = idx == editor.selected_row;
            let row_style = if selected {
                selected_style
            } else {
                Style::default()
            };
            let prefix = if selected { "› " } else { "  " };

            let label_rect = Rect {
                x: inner.x,
                y,
                width: label_width,
                height: 1,
            };
            let value_rect = Rect {
                x: inner.x.saturating_add(label_width),
                y,
                width: value_width,
                height: 1,
            };

            let (label, value_text, field_opt): (
                String,
                Option<String>,
                Option<(&crate::components::form_text_field::FormTextField, bool)>,
            ) = match row {
                ToolRow::MinInterval => {
                    if editor.override_min_interval {
                        (
                            "Min interval (override)".to_string(),
                            None,
                            Some((
                                &editor.min_interval_field,
                                editor.editing == Some(ToolRow::MinInterval),
                            )),
                        )
                    } else {
                        let server_v =
                            format_opt_secs_compact(editor.server_scheduling.min_interval_sec);
                        (
                            "Min interval (inherit)".to_string(),
                            Some(format!("server {server_v}")),
                            None,
                        )
                    }
                }
                ToolRow::MaxConcurrent => {
                    if editor.override_max_concurrent {
                        (
                            "Max concurrent (override)".to_string(),
                            None,
                            Some((
                                &editor.max_concurrent_field,
                                editor.editing == Some(ToolRow::MaxConcurrent),
                            )),
                        )
                    } else {
                        (
                            "Max concurrent (inherit)".to_string(),
                            Some(format!("server {}", editor.server_scheduling.max_concurrent)),
                            None,
                        )
                    }
                }
                ToolRow::ClearOverride => (
                    "Clear override".to_string(),
                    Some("remove all tool limits".to_string()),
                    None,
                ),
                ToolRow::Save => ("Save".to_string(), Some("Ctrl+S".to_string()), None),
                ToolRow::Cancel => ("Cancel".to_string(), Some("Esc".to_string()), None),
            };

            let label_line = Line::from(vec![
                Span::styled(prefix, row_style),
                Span::styled(
                    format!("{label}:"),
                    key_style.add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ),
            ]);
            Paragraph::new(label_line).render(label_rect, buf);

            if let Some((field, focused)) = field_opt {
                field.render(value_rect, buf, focused);
            } else {
                let value = value_text.unwrap_or_default();
                Paragraph::new(Line::from(vec![Span::styled(value, value_style)]))
                    .render(value_rect, buf);
            }
            y = y.saturating_add(1);
        }

        if let Some(err) = editor.error.as_deref() {
            let err_area = Rect {
                x: inner.x,
                y: rows_end_y,
                width: inner.width,
                height: 1,
            };
            Paragraph::new(Line::from(vec![Span::styled(
                err.to_string(),
                err_style,
            )]))
            .render(err_area, buf);
        }
    }
}
