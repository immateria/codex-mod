use super::*;

impl ThemeSelectionView {
    pub(super) fn render_overview_mode(
        &self,
        body_area: Rect,
        theme: &crate::theme::Theme,
        buf: &mut Buffer,
    ) {
        let theme_label_owned = Self::theme_display_name(self.current_theme);

        let mut lines = Vec::new();

        // Row 0: Theme selector.
        {
            let selected = self.overview_selected_index == 0;
            let mut spans = vec![Span::raw(" ")];
            if selected {
                spans.push(Span::styled("› ", Style::default().fg(theme.keyword)));
            } else {
                spans.push(Span::raw("  "));
            }

            let label = "Change Theme";
            if selected {
                spans.push(Span::styled(
                    label,
                    Style::default().fg(theme.primary).add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(label, Style::default().fg(theme.text)));
            }
            spans.push(Span::raw(" — "));
            spans.push(Span::styled(
                theme_label_owned,
                Style::default().fg(theme.text_dim),
            ));
            lines.push(Line::from(spans));
        }

        // Row 1: Spinner selector.
        {
            let selected = self.overview_selected_index == 1;
            let mut spans = vec![Span::raw(" ")];
            if selected {
                spans.push(Span::styled("› ", Style::default().fg(theme.keyword)));
            } else {
                spans.push(Span::raw("  "));
            }

            let label = "Change Spinner";
            if selected {
                spans.push(Span::styled(
                    label,
                    Style::default().fg(theme.primary).add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(label, Style::default().fg(theme.text)));
            }
            spans.push(Span::raw(" — "));
            let spinner_label = crate::spinner::spinner_label_for(&self.current_spinner);
            spans.push(Span::styled(spinner_label, Style::default().fg(theme.text_dim)));
            lines.push(Line::from(spans));
            lines.push(Line::default());
        }

        // Row 2: Close action.
        {
            let selected = self.overview_selected_index == 2;
            let mut spans = vec![Span::raw(" ")];
            if selected {
                spans.push(Span::styled("› ", Style::default().fg(theme.keyword)));
            } else {
                spans.push(Span::raw("  "));
            }

            let style = if selected {
                Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };
            spans.push(Span::styled("[ Close ]", style));
            lines.push(Line::from(spans));
        }

        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .render(body_area, buf);
    }
}
