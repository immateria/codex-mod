impl SettingsOverlayView {
    fn trim_with_ellipsis(&self, text: &str, max_width: usize) -> String {
        if max_width == 0 || text.is_empty() {
            return String::new();
        }
        if UnicodeWidthStr::width(text) <= max_width {
            return text.to_string();
        }
        if max_width <= 3 {
            return "...".chars().take(max_width).collect();
        }
        let keep = max_width.saturating_sub(3);
        let (prefix, _, _) = take_prefix_by_width(text, keep);
        let mut result = prefix;
        result.push_str("...");
        result
    }

    fn push_summary_spans(&self, line: &mut Line<'static>, summary: &str) {
        let label_style = Style::default().fg(crate::colors::text_mid());
        let dim_style = Style::default().fg(crate::colors::text_dim());
        let mut first = true;
        for raw_segment in summary.split(" · ") {
            let segment = raw_segment.trim();
            if segment.is_empty() {
                continue;
            }
            if !first {
                line.spans.push(Span::styled(" · ".to_string(), dim_style));
            }
            first = false;

            if let Some((label, value)) = segment.split_once(':') {
                let label_trim = label.trim_end();
                let value_trim = value.trim_start();
                line.spans
                    .push(Span::styled(format!("{label_trim}:"), label_style));
                if !value_trim.is_empty() {
                    line.spans.push(Span::styled(" ".to_string(), dim_style));
                    let value_style = self.summary_value_style(value_trim);
                    line.spans
                        .push(Span::styled(value_trim.to_string(), value_style));
                }
            } else {
                let value_style = self.summary_value_style(segment);
                line.spans
                    .push(Span::styled(segment.to_string(), value_style));
            }
        }
    }

    fn summary_value_style(&self, value: &str) -> Style {
        let trimmed = value.trim();
        let normalized = trimmed
            .trim_end_matches(['.', '!', ','])
            .to_ascii_lowercase();
        if matches!(normalized.as_str(), "on" | "enabled" | "yes") {
            Style::default().fg(crate::colors::success())
        } else if matches!(normalized.as_str(), "off" | "disabled" | "no") {
            Style::default().fg(crate::colors::error())
        } else {
            Style::default().fg(crate::colors::info())
        }
    }

    fn render_footer_hints_overview(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let line = Line::from(vec![
            Span::styled("↑ ↓", Style::default().fg(crate::colors::text())),
            Span::styled(" Move    ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Enter", Style::default().fg(crate::colors::text())),
            Span::styled(" Open    ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Esc", Style::default().fg(crate::colors::text())),
            Span::styled(" Close    ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("?", Style::default().fg(crate::colors::text())),
            Span::styled(" Help", Style::default().fg(crate::colors::text_dim())),
        ]);

        Paragraph::new(line)
            .style(Style::default().bg(crate::colors::background()))
            .alignment(Alignment::Left)
            .render(area, buf);
    }

    fn render_footer_hints_section(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let key = Style::default().fg(crate::colors::text());
        let hint = Style::default().fg(crate::colors::text_dim());
        let focus = Style::default()
            .fg(crate::colors::primary())
            .add_modifier(Modifier::BOLD);
        let focus_label = if self.is_sidebar_focused() {
            "Sidebar"
        } else {
            "Content"
        };

        let line = Line::from(vec![
            Span::styled("Tab", key),
            Span::styled(" Content    ", hint),
            Span::styled("Shift+Tab", key),
            Span::styled(" Sidebar    ", hint),
            Span::styled("Esc", key),
            Span::styled(" Overview    ", hint),
            Span::styled("?", key),
            Span::styled(" Help    ", hint),
            Span::styled("Focus:", hint),
            Span::styled(format!(" {focus_label}"), focus),
        ]);

        Paragraph::new(line)
            .style(Style::default().bg(crate::colors::background()))
            .alignment(Alignment::Left)
            .render(area, buf);
    }
}
