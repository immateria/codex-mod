use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::super::{
    McpPaneHit,
    McpSettingsFocus,
    McpSettingsView,
    McpToolEntry,
    McpToolHoverPart,
};

impl McpSettingsView {
    pub(in crate::bottom_pane::settings_pages::mcp) fn tools_lines_for_entries(
        &self,
        width: usize,
        entries: &[McpToolEntry<'_>],
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let selected_style = Style::default()
            .bg(crate::colors::selection())
            .add_modifier(Modifier::BOLD);
        let dim_style = Style::default().fg(crate::colors::text_dim());
        let overrides = self.selected_server().map(|row| &row.tool_scheduling);

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
            let focused =
                self.focus == McpSettingsFocus::Tools && idx == self.tools_selected;
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
            let expansion = if self.is_tool_expanded(entry.name) {
                "▼"
            } else {
                "▶"
            };
            let has_override = overrides.is_some_and(|map| {
                map.get(entry.name).is_some_and(|cfg| {
                    cfg.max_concurrent.is_some() || cfg.min_interval_sec.is_some()
                })
            });
            let label_width = width.saturating_sub(if has_override { 12 } else { 10 });
            let label =
                crate::text_formatting::truncate_chars_with_ellipsis(entry.name, label_width);
            let marker_style = if hover_part == Some(McpToolHoverPart::Toggle) {
                row_style
                    .fg(crate::colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else if entry.enabled {
                row_style.fg(crate::colors::success())
            } else {
                row_style
            };
            let expansion_style = if hover_part == Some(McpToolHoverPart::Expand) {
                row_style
                    .fg(crate::colors::function())
                    .add_modifier(Modifier::BOLD)
            } else {
                row_style.fg(crate::colors::primary())
            };
            let label_style = if hover_part == Some(McpToolHoverPart::Label) && !focused {
                row_style.add_modifier(Modifier::UNDERLINED)
            } else {
                row_style
            };
            let mut spans = vec![
                Span::styled(
                    if focused {
                        "› "
                    } else if hovered_row {
                        "> "
                    } else {
                        "  "
                    },
                    row_style,
                ),
                Span::styled(marker, marker_style),
                Span::raw(" "),
                Span::styled(expansion.to_string(), expansion_style),
                Span::raw(" "),
                Span::styled(label, label_style),
            ];
            if has_override {
                spans.push(Span::styled(
                    " *",
                    row_style.fg(crate::colors::primary()),
                ));
            }
            lines.push(Line::from(spans));
        }

        lines
    }
}

