use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::super::{McpPaneHit, McpSettingsView};

impl McpSettingsView {
    pub(in crate::bottom_pane::settings_pages::mcp) fn list_lines(
        &self,
        width: usize,
    ) -> Vec<Line<'static>> {
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
            crate::text_formatting::truncate_chars_with_ellipsis(
                "Refresh tools/status",
                label_width,
            ),
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
            crate::text_formatting::truncate_chars_with_ellipsis(
                "Add new server…",
                label_width,
            ),
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
}

