use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::super::{McpServerRow, McpSettingsView};

impl McpSettingsView {
    pub(super) fn push_resource_sections(
        lines: &mut Vec<Line<'static>>,
        row: &McpServerRow,
        heading_style: Style,
        key_style: Style,
        value_style: Style,
        dim_style: Style,
    ) {
        let max_rows = 6;

        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            format!("Resources ({})", row.resources.len()),
            heading_style,
        )]));
        if row.resources.is_empty() {
            lines.push(Line::from(vec![Span::styled("none reported", dim_style)]));
        } else {
            for resource in row.resources.iter().take(max_rows) {
                Self::push_key_value_line(
                    lines,
                    "- ",
                    Self::format_resource_line(resource),
                    key_style,
                    value_style,
                );
            }
            if row.resources.len() > max_rows {
                lines.push(Line::from(vec![Span::styled(
                    format!("... +{} more resources", row.resources.len() - max_rows),
                    dim_style,
                )]));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            format!("Resource Templates ({})", row.resource_templates.len()),
            heading_style,
        )]));
        if row.resource_templates.is_empty() {
            lines.push(Line::from(vec![Span::styled("none reported", dim_style)]));
        } else {
            for template in row.resource_templates.iter().take(max_rows) {
                Self::push_key_value_line(
                    lines,
                    "- ",
                    Self::format_resource_template_line(template),
                    key_style,
                    value_style,
                );
            }
            if row.resource_templates.len() > max_rows {
                lines.push(Line::from(vec![Span::styled(
                    format!(
                        "... +{} more templates",
                        row.resource_templates.len() - max_rows
                    ),
                    dim_style,
                )]));
            }
        }
    }
}

