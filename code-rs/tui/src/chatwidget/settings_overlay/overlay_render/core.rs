impl SettingsOverlayView {
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let block = Block::default()
            .title(self.block_title_line())
            .title_alignment(Alignment::Left)
            .borders(Borders::ALL)
            .style(Style::default().bg(crate::colors::background()))
            .border_style(
                Style::default()
                    .fg(crate::colors::border())
                    .bg(crate::colors::background()),
            );
        let inner = block.inner(area);
        block.render(area, buf);

        let bg = Style::default().bg(crate::colors::background());
        for y in inner.y..inner.y.saturating_add(inner.height) {
            for x in inner.x..inner.x.saturating_add(inner.width) {
                buf[(x, y)].set_style(bg);
            }
        }

        let content = inner.inner(Margin::new(1, 1));
        if content.width == 0 || content.height == 0 {
            return;
        }

        // Store content area for mouse hit testing
        *self.last_content_area.borrow_mut() = content;

        if self.is_menu_active() {
            self.render_overview(content, buf);
        } else {
            self.render_section_layout(content, buf);
        }

        if let Some(help) = &self.help {
            self.render_help_overlay(inner, buf, help);
        }
    }

    fn block_title_line(&self) -> Line<'static> {
        if self.is_menu_active() {
            Line::from(vec![
                Span::styled("Settings", Style::default().fg(crate::colors::text())),
                Span::styled(" ▸ ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Overview", Style::default().fg(crate::colors::text())),
            ])
        } else {
            Line::from(vec![
                Span::styled("Settings", Style::default().fg(crate::colors::text())),
                Span::styled(" ▸ ", Style::default().fg(crate::colors::text_dim())),
                Span::styled(
                    self.active_section().label(),
                    Style::default().fg(crate::colors::text()),
                ),
            ])
        }
    }

    fn render_overview(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let (list_area, hint_area) = match area.height {
            0 => return,
            1 => (area, None),
            _ => {
                let [list, hint] =
                    Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
                (list, Some(hint))
            }
        };

        self.render_overview_list(list_area, buf);
        if let Some(hint_area) = hint_area {
            self.render_footer_hints_overview(hint_area, buf);
        }
    }

    fn render_overview_list(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        fill_rect(
            buf,
            area,
            Some(' '),
            Style::default().bg(crate::colors::background()),
        );

        if self.overview_rows.is_empty() {
            *self.last_overview_list_area.borrow_mut() = area;
            *self.last_overview_line_sections.borrow_mut() = Vec::new();
            *self.last_overview_line_hit_ranges.borrow_mut() = Vec::new();
            *self.last_overview_scroll.borrow_mut() = 0;
            let line = Line::from(vec![Span::styled(
                "No settings available.",
                Style::default().fg(crate::colors::text_dim()),
            )]);
            Paragraph::new(line)
                .style(Style::default().bg(crate::colors::background()))
                .render(area, buf);
            return;
        }

        let active_section = self.active_section();
        let content_width = area.width as usize;
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut line_sections: Vec<Option<SettingsSection>> = Vec::new();
        let mut line_hit_ranges: Vec<[Option<(u16, u16)>; 2]> = Vec::new();
        let mut selected_range: Option<(usize, usize)> = None;

        for (idx, row) in self.overview_rows.iter().enumerate() {
            let is_active = row.section == active_section;
            let indicator = if is_active { "›" } else { " " };

            if row.section == SettingsSection::Limits && !lines.is_empty() {
                lines.push(Line::from(""));
                line_sections.push(None);
                line_hit_ranges.push([None, None]);
                let dash_count = content_width.saturating_sub(2);
                if dash_count > 0 {
                    lines.push(Line::from(vec![Span::styled(
                        format!("  {}", "─".repeat(dash_count)),
                        Style::default().fg(crate::colors::border_dim()),
                    )]));
                    line_sections.push(None);
                    line_hit_ranges.push([None, None]);
                    lines.push(Line::from(""));
                    line_sections.push(None);
                    line_hit_ranges.push([None, None]);
                }
            }
            // Anchor selection to the row itself, not any pre-row separators.
            let row_start = lines.len();

            let label_style = if is_active {
                Style::default()
                    .fg(crate::colors::text_bright())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::colors::text_mid())
            };

            let label_text = format!("{:<width$}", row.section.label(), width = LABEL_COLUMN_WIDTH);

            let summary_src = row.summary.as_deref().unwrap_or("—");
            let base_width = 1 + 1 + LABEL_COLUMN_WIDTH;
            let available_tail = content_width.saturating_sub(base_width);
            let label_hit_start = area.x.saturating_add(2);
            let label_hit_end_offset =
                2usize.saturating_add(UnicodeWidthStr::width(row.section.label()));
            let label_hit_end = area
                .x
                .saturating_add(label_hit_end_offset.min(content_width) as u16);
            let label_hit_range = if label_hit_end > label_hit_start {
                Some((label_hit_start, label_hit_end))
            } else {
                None
            };
            let mut summary_hit_range: Option<(u16, u16)> = None;

            let mut summary_line = Line::from(vec![
                Span::styled(indicator.to_string(), Style::default().fg(crate::colors::text())),
                Span::raw(" "),
                Span::styled(label_text, label_style),
            ]);

            if available_tail > 0 {
                summary_line.spans.push(Span::raw(" "));
                let summary_budget = available_tail.saturating_sub(1);

                if summary_budget > 0 {
                    let summary_trimmed = self.trim_with_ellipsis(summary_src, summary_budget);
                    if !summary_trimmed.is_empty() {
                        let summary_hit_start_offset = 2usize
                            .saturating_add(LABEL_COLUMN_WIDTH)
                            .saturating_add(1);
                        let summary_hit_start = area
                            .x
                            .saturating_add(summary_hit_start_offset.min(content_width) as u16);
                        let summary_hit_end_offset = summary_hit_start_offset
                            .saturating_add(UnicodeWidthStr::width(summary_trimmed.as_str()));
                        let summary_hit_end = area
                            .x
                            .saturating_add(summary_hit_end_offset.min(content_width) as u16);
                        if summary_hit_end > summary_hit_start {
                            summary_hit_range = Some((summary_hit_start, summary_hit_end));
                        }
                        self.push_summary_spans(&mut summary_line, &summary_trimmed);
                    }
                }
            }

            if is_active {
                summary_line = summary_line.style(
                    Style::default()
                        .bg(crate::colors::selection())
                        .fg(crate::colors::text()),
                );
            }
            lines.push(summary_line);
            line_sections.push(Some(row.section));
            line_hit_ranges.push([label_hit_range, summary_hit_range]);

            let info_text = row.section.help_line();
            let info_trimmed = self.trim_with_ellipsis(info_text, content_width.saturating_sub(8));
            let info_trimmed_width = UnicodeWidthStr::width(info_trimmed.as_str());
            let info_style = Style::default().fg(crate::colors::text_dim());
            let mut info_line = Line::from(vec![
                Span::raw("  "),
                Span::styled("└ ", info_style),
                Span::styled(info_trimmed, info_style),
            ]);
            if is_active {
                info_line = info_line.style(
                    Style::default()
                        .bg(crate::colors::selection())
                        .fg(crate::colors::text()),
                );
            }
            lines.push(info_line);
            line_sections.push(Some(row.section));
            let info_hit_start = area.x.saturating_add(4);
            let info_hit_end_offset = 4usize.saturating_add(info_trimmed_width);
            let info_hit_end = area
                .x
                .saturating_add(info_hit_end_offset.min(content_width) as u16);
            let info_hit_range = if info_hit_end > info_hit_start {
                Some((info_hit_start, info_hit_end))
            } else {
                None
            };
            line_hit_ranges.push([info_hit_range, None]);

            if is_active {
                let row_end = lines.len().saturating_sub(1);
                selected_range = Some((row_start, row_end));
            }

            if idx != self.overview_rows.len().saturating_sub(1) {
                lines.push(Line::from(""));
                line_sections.push(None);
                line_hit_ranges.push([None, None]);
                if matches!(row.section, SettingsSection::Updates) {
                    let dash_count = content_width.saturating_sub(2);
                    if dash_count > 0 {
                        lines.push(Line::from(vec![Span::styled(
                            format!("  {}", "─".repeat(dash_count)),
                            Style::default().fg(crate::colors::border_dim()),
                        )]));
                        line_sections.push(None);
                        line_hit_ranges.push([None, None]);
                        lines.push(Line::from(""));
                        line_sections.push(None);
                        line_hit_ranges.push([None, None]);
                    }
                }
            }
        }

        let total_lines = lines.len();
        let visible_lines = area.height as usize;
        let mut scroll = 0usize;
        if visible_lines > 0 && total_lines > visible_lines {
            if let Some((start, end)) = selected_range {
                let max_scroll = total_lines.saturating_sub(visible_lines);
                let mut candidate = end.saturating_add(1).saturating_sub(visible_lines);
                if candidate > max_scroll {
                    candidate = max_scroll;
                }
                if start < candidate {
                    candidate = start.min(max_scroll);
                }
                if end >= candidate.saturating_add(visible_lines) {
                    candidate = end
                        .saturating_add(1)
                        .saturating_sub(visible_lines)
                        .min(max_scroll);
                }
                scroll = candidate;
            } else {
                scroll = total_lines.saturating_sub(visible_lines);
            }
        }
        let scroll = scroll.min(u16::MAX as usize) as u16;
        *self.last_overview_list_area.borrow_mut() = area;
        *self.last_overview_line_sections.borrow_mut() = line_sections;
        *self.last_overview_line_hit_ranges.borrow_mut() = line_hit_ranges;
        *self.last_overview_scroll.borrow_mut() = scroll as usize;

        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(crate::colors::background()))
            .scroll((scroll, 0))
            .render(area, buf);
    }
}
