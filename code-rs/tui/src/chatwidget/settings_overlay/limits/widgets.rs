struct LimitsHintRowWidget {
    has_tabs: bool,
    layout_mode: LimitsLayoutMode,
    pane_focus: LimitsPaneFocus,
}

impl Widget for LimitsHintRowWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let hint_style = Style::default().fg(crate::colors::text_dim());
        let accent_style = Style::default().fg(crate::colors::function());
        let mut spans = vec![
            Span::styled("↑↓", accent_style),
            Span::styled(" scroll  ", hint_style),
            Span::styled("PgUp/PgDn", accent_style),
            Span::styled(" page  ", hint_style),
            Span::styled("Home/End", accent_style),
            Span::styled(" jump  ", hint_style),
            Span::styled("V", accent_style),
            Span::styled(format!(" layout:{}  ", self.layout_mode.label()), hint_style),
            Span::styled("F", accent_style),
            Span::styled(format!(" focus:{}  ", self.pane_focus.label()), hint_style),
        ];
        if self.has_tabs {
            spans.push(Span::styled("◂ ▸", accent_style));
            spans.push(Span::styled(" change tab", hint_style));
        }

        Paragraph::new(Line::from(spans))
            .alignment(Alignment::Left)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text_dim()))
            .render(area, buf);
    }
}

struct LimitsTabsRowWidget<'a> {
    tabs: &'a [LimitsTab],
    selected_tab: usize,
}

impl Widget for LimitsTabsRowWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let mut spans = Vec::new();
        for (idx, tab) in self.tabs.iter().enumerate() {
            let selected = idx == self.selected_tab;
            let style = if selected {
                Style::default()
                    .fg(crate::colors::text())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::colors::text_dim())
            };
            spans.push(Span::styled(format!(" {} ", tab.title), style));
            spans.push(Span::raw(" "));
        }

        Paragraph::new(Line::from(spans))
            .style(Style::default().bg(crate::colors::background()))
            .render(area, buf);
    }
}

struct LimitsSingleBodyWidget {
    lines: Vec<Line<'static>>,
}

impl Widget for LimitsSingleBodyWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(self.lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .render(area, buf);
    }
}

struct LimitsPaneWidget {
    title: &'static str,
    lines: Vec<Line<'static>>,
    focused: bool,
}

impl Widget for LimitsPaneWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Fill(1)])
            .split(area);
        let title_area = chunks[0];
        let body_area = chunks[1];

        let title_style = if self.focused {
            Style::default()
                .fg(crate::colors::function())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(crate::colors::text())
                .add_modifier(Modifier::BOLD)
        };

        Paragraph::new(Line::from(vec![
            Span::styled(self.title.to_string(), title_style),
            Span::styled(" ─".to_string(), Style::default().fg(crate::colors::text_dim())),
        ]))
        .style(Style::default().bg(crate::colors::background()))
        .render(title_area, buf);

        Paragraph::new(self.lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .render(body_area, buf);
    }
}
