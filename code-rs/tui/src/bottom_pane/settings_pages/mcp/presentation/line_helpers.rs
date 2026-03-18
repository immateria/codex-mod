use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::super::McpSettingsView;

impl McpSettingsView {
    fn list_row_prefix(is_selected: bool, is_hovered: bool) -> &'static str {
        if is_selected {
            "› "
        } else if is_hovered {
            "> "
        } else {
            "  "
        }
    }

    pub(super) fn push_list_row(
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

    pub(super) fn push_key_value_line(
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
}

