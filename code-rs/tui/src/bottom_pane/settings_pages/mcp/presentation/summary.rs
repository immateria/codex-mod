use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use code_core::protocol::McpAuthStatus;

use super::super::{McpSettingsView, McpToolEntry};

impl McpSettingsView {
    pub(in crate::bottom_pane::settings_pages::mcp) fn selected_tool_entry(
        &self,
    ) -> Option<McpToolEntry<'_>> {
        self.tool_entries().get(self.tools_selected).copied()
    }

    pub(in crate::bottom_pane::settings_pages::mcp) fn summary_lines(
        &self,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let heading_style = Style::default()
            .fg(crate::colors::text())
            .add_modifier(Modifier::BOLD);
        let key_style = Style::default().fg(crate::colors::secondary());
        let value_style = Style::default().fg(crate::colors::text());
        let dim_style = Style::default().fg(crate::colors::text_dim());
        let ok_style = Style::default().fg(crate::colors::success());
        let err_style = Style::default().fg(crate::colors::error());

        match self.selected_server() {
            Some(row) => {
                lines.push(Line::from(vec![
                    Span::styled("Server: ", key_style),
                    Span::styled(row.name.clone(), heading_style),
                    Span::raw("  "),
                    Span::styled(
                        if row.enabled { "[on]" } else { "[off]" },
                        if row.enabled { ok_style } else { dim_style },
                    ),
                ]));
                Self::push_key_value_line(
                    &mut lines,
                    "Transport: ",
                    row.transport.clone(),
                    key_style,
                    value_style,
                );
                Self::push_key_value_line(
                    &mut lines,
                    "Status: ",
                    row.status.clone(),
                    key_style,
                    if row.failure.is_some() {
                        err_style
                    } else {
                        value_style
                    },
                );
                let auth_style = match row.auth_status {
                    McpAuthStatus::OAuth | McpAuthStatus::BearerToken => ok_style,
                    McpAuthStatus::NotLoggedIn => err_style,
                    McpAuthStatus::Unsupported => dim_style,
                };
                Self::push_key_value_line(
                    &mut lines,
                    "Auth: ",
                    row.auth_status.to_string(),
                    key_style,
                    auth_style,
                );
                if let Some(timeout) = row.startup_timeout {
                    Self::push_key_value_line(
                        &mut lines,
                        "Startup timeout: ",
                        Self::format_duration(timeout),
                        key_style,
                        value_style,
                    );
                }
                if let Some(timeout) = row.tool_timeout {
                    Self::push_key_value_line(
                        &mut lines,
                        "Tool timeout: ",
                        Self::format_duration(timeout),
                        key_style,
                        value_style,
                    );
                }

                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled("Scheduling", heading_style)]));
                Self::push_key_value_line(
                    &mut lines,
                    "Dispatch: ",
                    row.scheduling.dispatch.to_string(),
                    key_style,
                    value_style,
                );
                Self::push_key_value_line(
                    &mut lines,
                    "Max concurrent: ",
                    row.scheduling.max_concurrent.to_string(),
                    key_style,
                    value_style,
                );
                Self::push_key_value_line(
                    &mut lines,
                    "Min interval: ",
                    row.scheduling
                        .min_interval_sec
                        .map(Self::format_duration)
                        .unwrap_or_else(|| "none".to_string()),
                    key_style,
                    value_style,
                );
                Self::push_key_value_line(
                    &mut lines,
                    "Queue timeout: ",
                    row.scheduling
                        .queue_timeout_sec
                        .map(Self::format_duration)
                        .unwrap_or_else(|| "none".to_string()),
                    key_style,
                    value_style,
                );
                Self::push_key_value_line(
                    &mut lines,
                    "Max queue depth: ",
                    row.scheduling
                        .max_queue_depth
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    key_style,
                    value_style,
                );
                if !row.tool_scheduling.is_empty() {
                    Self::push_key_value_line(
                        &mut lines,
                        "Tool overrides: ",
                        row.tool_scheduling.len().to_string(),
                        key_style,
                        value_style,
                    );
                }
                Self::push_resource_sections(
                    &mut lines,
                    row,
                    heading_style,
                    key_style,
                    value_style,
                    dim_style,
                );
                if row.auth_status == McpAuthStatus::NotLoggedIn {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![Span::styled(
                        format!(
                            "Login required. Run `code mcp login {}` (or add a bearer token).",
                            row.name
                        ),
                        dim_style,
                    )]));
                }
                if let Some(failure) = &row.failure {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![Span::styled(
                        "Connection issue",
                        err_style.add_modifier(Modifier::BOLD),
                    )]));
                    lines.push(Line::from(vec![Span::styled(failure.clone(), err_style)]));
                }

                if let Some(entry) = self.selected_tool_entry() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![Span::styled("Selected Tool", heading_style)]));
                    let expanded = self.is_tool_expanded(entry.name);
                    lines.push(Line::from(vec![
                        Span::styled("Name: ", key_style),
                        Span::styled(entry.name.to_string(), value_style),
                        Span::raw("  "),
                        Span::styled(
                            if entry.enabled { "[enabled]" } else { "[disabled]" },
                            if entry.enabled { ok_style } else { dim_style },
                        ),
                        Span::raw("  "),
                        Span::styled(
                            if expanded { "[expanded]" } else { "[collapsed]" },
                            if expanded {
                                Style::default().fg(crate::colors::primary())
                            } else {
                                dim_style
                            },
                        ),
                    ]));
                    lines.push(Line::from(vec![Span::styled(
                        if expanded {
                            "Enter collapses details. Space toggles enabled/disabled."
                        } else {
                            "Enter expands details. Space toggles enabled/disabled."
                        },
                        dim_style,
                    )]));

                    if expanded {
                        if let Some(tool_override) = row.tool_scheduling.get(entry.name)
                            && (tool_override.max_concurrent.is_some()
                                || tool_override.min_interval_sec.is_some())
                        {
                            lines.push(Line::from(""));
                            lines.push(Line::from(vec![Span::styled(
                                "Scheduling Override",
                                heading_style,
                            )]));
                            Self::push_key_value_line(
                                &mut lines,
                                "Max concurrent: ",
                                tool_override
                                    .max_concurrent
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| "inherit".to_string()),
                                key_style,
                                value_style,
                            );
                            Self::push_key_value_line(
                                &mut lines,
                                "Min interval: ",
                                tool_override
                                    .min_interval_sec
                                    .map(Self::format_duration)
                                    .unwrap_or_else(|| "inherit".to_string()),
                                key_style,
                                value_style,
                            );
                            lines.push(Line::from(vec![Span::styled(
                                "Overrides are additive restrictions; server limits still apply.",
                                dim_style,
                            )]));
                        }

                        lines.push(Line::from(""));
                        match entry.definition {
                            Some(tool) => {
                                if let Some(title) = tool.title.as_deref() {
                                    Self::push_key_value_line(
                                        &mut lines,
                                        "Title: ",
                                        title.to_string(),
                                        key_style,
                                        value_style,
                                    );
                                }
                                if let Some(description) = tool.description.as_deref() {
                                    Self::push_key_value_line(
                                        &mut lines,
                                        "Description: ",
                                        description.to_string(),
                                        key_style,
                                        value_style,
                                    );
                                }
                                if let Some(annotations) = tool.annotations.as_ref()
                                    && let Some(hints) = Self::format_tool_annotations(annotations)
                                {
                                    Self::push_key_value_line(
                                        &mut lines,
                                        "Hints: ",
                                        hints,
                                        key_style,
                                        value_style,
                                    );
                                }

                                let required = tool
                                    .input_schema
                                    .required
                                    .clone()
                                    .unwrap_or_default();
                                Self::push_key_value_line(
                                    &mut lines,
                                    "Required args: ",
                                    Self::join_names_limited(&required, 8),
                                    key_style,
                                    value_style,
                                );

                                let input_properties = Self::schema_property_names(
                                    tool.input_schema.properties.as_ref(),
                                );
                                Self::push_key_value_line(
                                    &mut lines,
                                    "Input fields: ",
                                    Self::join_names_limited(&input_properties, 10),
                                    key_style,
                                    value_style,
                                );

                                match tool.output_schema.as_ref() {
                                    Some(output_schema) => {
                                        let output_properties = Self::schema_property_names(
                                            output_schema.properties.as_ref(),
                                        );
                                        let output_required = output_schema
                                            .required
                                            .clone()
                                            .unwrap_or_default();
                                        Self::push_key_value_line(
                                            &mut lines,
                                            "Output fields: ",
                                            Self::join_names_limited(
                                                &output_properties,
                                                10,
                                            ),
                                            key_style,
                                            value_style,
                                        );
                                        Self::push_key_value_line(
                                            &mut lines,
                                            "Output required: ",
                                            Self::join_names_limited(
                                                &output_required,
                                                8,
                                            ),
                                            key_style,
                                            value_style,
                                        );
                                    }
                                    None => {
                                        Self::push_key_value_line(
                                            &mut lines,
                                            "Output schema: ",
                                            "not provided",
                                            key_style,
                                            dim_style,
                                        );
                                    }
                                }
                            }
                            None => {
                                lines.push(Line::from(vec![Span::styled(
                                    "No metadata reported yet for this tool. Press R to refresh.",
                                    dim_style,
                                )]));
                            }
                        }
                    }
                }

                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "Tip: R refreshes MCP listing, S queues /mcp status diagnostics.",
                    dim_style,
                )]));
            }
            None => {
                lines.push(Line::from(vec![Span::styled(
                    "Select a server to inspect transport and tools.",
                    dim_style,
                )]));
            }
        }

        lines
    }
}

