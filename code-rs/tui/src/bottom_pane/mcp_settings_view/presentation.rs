use std::time::Duration;

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::{McpPaneHit, McpSettingsFocus, McpSettingsView, McpToolEntry, McpToolHoverPart};

impl McpSettingsView {
    fn selected_tool_entry(&self) -> Option<McpToolEntry<'_>> {
        self.tool_entries().get(self.tools_selected).copied()
    }

    fn format_tool_annotations(annotations: &mcp_types::ToolAnnotations) -> Option<String> {
        let mut hints = Vec::new();
        if annotations.read_only_hint == Some(true) {
            hints.push("read-only");
        }
        if annotations.idempotent_hint == Some(true) {
            hints.push("idempotent");
        }
        if annotations.destructive_hint == Some(true) {
            hints.push("destructive");
        }
        if annotations.open_world_hint == Some(true) {
            hints.push("open-world");
        }
        if hints.is_empty() {
            None
        } else {
            Some(hints.join(", "))
        }
    }

    fn schema_property_names(properties: Option<&serde_json::Value>) -> Vec<String> {
        let Some(properties) = properties else {
            return Vec::new();
        };
        let Some(object) = properties.as_object() else {
            return Vec::new();
        };
        let mut names: Vec<String> = object.keys().cloned().collect();
        names.sort();
        names
    }

    fn format_duration(duration: Duration) -> String {
        let secs = duration.as_secs_f64();
        if secs.fract() == 0.0 {
            let whole = duration.as_secs();
            if whole == 1 {
                "1 second".to_string()
            } else {
                format!("{whole} seconds")
            }
        } else {
            format!("{secs:.2} seconds")
        }
    }

    fn join_names_limited(names: &[String], max_items: usize) -> String {
        if names.is_empty() {
            return "(none)".to_string();
        }
        if names.len() <= max_items {
            return names.join(", ");
        }
        let shown = names[..max_items].join(", ");
        let remaining = names.len().saturating_sub(max_items);
        format!("{shown} (+{remaining} more)")
    }

    fn list_row_prefix(is_selected: bool, is_hovered: bool) -> &'static str {
        if is_selected {
            "› "
        } else if is_hovered {
            "> "
        } else {
            "  "
        }
    }

    fn push_list_row(
        lines: &mut Vec<Line<'static>>,
        is_selected: bool,
        is_hovered: bool,
        prefix_style: Style,
        label: String,
        label_style: Style,
    ) {
        lines.push(Line::from(vec![
            Span::styled(Self::list_row_prefix(is_selected, is_hovered), prefix_style),
            Span::styled(label, label_style),
        ]));
    }

    fn push_key_value_line(
        lines: &mut Vec<Line<'static>>,
        key: &str,
        value: impl Into<String>,
        key_style: Style,
        value_style: Style,
    ) {
        lines.push(Line::from(vec![
            Span::styled(key.to_string(), key_style),
            Span::styled(value.into(), value_style),
        ]));
    }

    pub(super) fn list_lines(&self, width: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let selected_style = Style::default()
            .bg(crate::colors::selection())
            .add_modifier(Modifier::BOLD);
        let dim_style = Style::default().fg(crate::colors::text_dim());
        let accent_style = Style::default().fg(crate::colors::primary());

        let content_width = width.saturating_sub(2);
        let label_width = content_width.saturating_sub(3);
        let hovered_style = Style::default().fg(crate::colors::function());

        if self.rows.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                crate::text_formatting::truncate_chars_with_ellipsis(
                    "No MCP servers configured.",
                    content_width,
                ),
                dim_style,
            )]));
            lines.push(Line::from(""));
        }

        for (i, row) in self.rows.iter().enumerate() {
            let is_selected = i == self.selected;
            let is_hovered = self.hovered_pane == McpPaneHit::Servers
                && self.hovered_list_index == Some(i);
            let style = if is_selected {
                selected_style
            } else if is_hovered {
                hovered_style
            } else {
                Style::default()
            };

            let check = if row.enabled { "[on ]" } else { "[off]" };
            let label = crate::text_formatting::truncate_chars_with_ellipsis(
                &format!("{check} {}", row.name),
                label_width,
            );
            Self::push_list_row(&mut lines, is_selected, is_hovered, style, label, style);
        }

        lines.push(Line::from(""));
        let refresh_sel = self.selected == self.refresh_index();
        let refresh_hover = self.hovered_pane == McpPaneHit::Servers
            && self.hovered_list_index == Some(self.refresh_index());
        let refresh_style = if refresh_sel {
            selected_style
        } else if refresh_hover {
            hovered_style
        } else {
            accent_style
        };
        Self::push_list_row(
            &mut lines,
            refresh_sel,
            refresh_hover,
            Style::default(),
            crate::text_formatting::truncate_chars_with_ellipsis("Refresh tools/status", label_width),
            refresh_style,
        );

        let add_sel = self.selected == self.add_index();
        let add_hover = self.hovered_pane == McpPaneHit::Servers
            && self.hovered_list_index == Some(self.add_index());
        let add_style = if add_sel {
            selected_style
        } else if add_hover {
            hovered_style
        } else {
            accent_style
        };
        Self::push_list_row(
            &mut lines,
            add_sel,
            add_hover,
            Style::default(),
            crate::text_formatting::truncate_chars_with_ellipsis("Add new server…", label_width),
            add_style,
        );

        let close_sel = self.selected == self.close_index();
        let close_hover = self.hovered_pane == McpPaneHit::Servers
            && self.hovered_list_index == Some(self.close_index());
        let close_style = if close_sel {
            selected_style
        } else if close_hover {
            hovered_style
        } else {
            Style::default()
        };
        Self::push_list_row(
            &mut lines,
            close_sel,
            close_hover,
            Style::default(),
            crate::text_formatting::truncate_chars_with_ellipsis("Close", label_width),
            close_style,
        );

        lines
    }

    pub(super) fn summary_lines(&self) -> Vec<Line<'static>> {
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

                                let required = tool.input_schema.required.clone().unwrap_or_default();
                                Self::push_key_value_line(
                                    &mut lines,
                                    "Required args: ",
                                    Self::join_names_limited(&required, 8),
                                    key_style,
                                    value_style,
                                );

                                let input_properties =
                                    Self::schema_property_names(tool.input_schema.properties.as_ref());
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
                                        let output_required =
                                            output_schema.required.clone().unwrap_or_default();
                                        Self::push_key_value_line(
                                            &mut lines,
                                            "Output fields: ",
                                            Self::join_names_limited(&output_properties, 10),
                                            key_style,
                                            value_style,
                                        );
                                        Self::push_key_value_line(
                                            &mut lines,
                                            "Output required: ",
                                            Self::join_names_limited(&output_required, 8),
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

    pub(super) fn tools_lines(&self, width: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let entries = self.tool_entries();
        let selected_style = Style::default()
            .bg(crate::colors::selection())
            .add_modifier(Modifier::BOLD);
        let dim_style = Style::default().fg(crate::colors::text_dim());
        let label_width = width.saturating_sub(10);

        if entries.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "No tools discovered yet.",
                dim_style,
            )]));
            lines.push(Line::from(vec![Span::styled(
                "Press R to refresh or S for /mcp status.",
                dim_style,
            )]));
            return lines;
        }

        for (idx, entry) in entries.iter().enumerate() {
            let focused = self.focus == McpSettingsFocus::Tools && idx == self.tools_selected;
            let hovered_row = self.hovered_pane == McpPaneHit::Tools
                && self.hovered_tool_index == Some(idx);
            let hover_part = if hovered_row {
                self.hovered_tool_part
            } else {
                None
            };
            let row_style = if focused {
                selected_style
            } else if hovered_row {
                Style::default().fg(crate::colors::function())
            } else {
                Style::default()
            };
            let marker = if entry.enabled { "[x]" } else { "[ ]" };
            let expansion = if self.is_tool_expanded(entry.name) { "▼" } else { "▶" };
            let label = crate::text_formatting::truncate_chars_with_ellipsis(entry.name, label_width);
            let marker_style = if hover_part == Some(McpToolHoverPart::Toggle) {
                row_style.fg(crate::colors::primary()).add_modifier(Modifier::BOLD)
            } else if entry.enabled {
                row_style.fg(crate::colors::success())
            } else {
                row_style
            };
            let expansion_style = if hover_part == Some(McpToolHoverPart::Expand) {
                row_style.fg(crate::colors::function()).add_modifier(Modifier::BOLD)
            } else {
                row_style.fg(crate::colors::primary())
            };
            let label_style = if hover_part == Some(McpToolHoverPart::Label) && !focused {
                row_style.add_modifier(Modifier::UNDERLINED)
            } else {
                row_style
            };
            lines.push(Line::from(vec![
                Span::styled(if focused { "› " } else if hovered_row { "> " } else { "  " }, row_style),
                Span::styled(marker, marker_style),
                Span::raw(" "),
                Span::styled(expansion.to_string(), expansion_style),
                Span::raw(" "),
                Span::styled(label, label_style),
            ]));
        }

        lines
    }
}
