impl SettingsOverlayView {
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let s_on_bg = crate::colors::style_on_background();

        let block = Block::default()
            .title(self.block_title_line())
            .title_alignment(Alignment::Left)
            .borders(Borders::ALL)
            .style(s_on_bg)
            .border_style(
                crate::colors::style_border_on_bg(),
            );
        let inner = block.inner(area);
        block.render(area, buf);

        // Fill the inner content region so we consistently repaint background even
        // when nested panels render sparse cells.
        fill_rect(
            buf,
            inner,
            Some(' '),
            s_on_bg,
        );

        // Render a close button as an inset box in the top-right corner:
        //   ────┬───┐
        //       │ x │
        //       └───┤
        //
        // The box must never be overwritten by the content panel. We draw a 3-row
        // box when there is enough vertical room to also reserve an extra header
        // row above content; otherwise we fall back to a 2-row variant.
        let render_close_box_bottom = area.height >= 6;
        let close_consumes_inner_rows = if self.render_close_button(area, buf, render_close_box_bottom) {
            if render_close_box_bottom { 2 } else { 1 }
        } else {
            1
        };

        let content = Self::overlay_content_area(inner, close_consumes_inner_rows);
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

    fn overlay_content_area(inner: Rect, reserved_header_rows: u16) -> Rect {
        // Keep the content inset from the overlay border; reserve additional top
        // rows when the close button uses a 3-row box so content doesn't overwrite
        // its bottom border row.
        let top_pad = reserved_header_rows.min(inner.height);
        let bottom_pad = 1u16.min(inner.height.saturating_sub(top_pad));

        let x = inner.x.saturating_add(1);
        let width = inner.width.saturating_sub(2);
        let y = inner.y.saturating_add(top_pad);
        let height = inner.height.saturating_sub(top_pad.saturating_add(bottom_pad));

        Rect { x, y, width, height }
    }

    fn render_close_button(&self, area: Rect, buf: &mut Buffer, draw_bottom_border: bool) -> bool {
        // Box is 5 chars wide: "│ x │" (border + space + glyph + space + border).
        let box_w: u16 = 5;
        let min_width = 8; // box_w + at least 3 cols of title space
        if area.width < min_width || area.height < 3 {
            *self.last_close_button_area.borrow_mut() = Rect::default();
            return false;
        }

        // Position: right edge shares the outer border's right column.
        let right_edge = area.x.saturating_add(area.width).saturating_sub(1);
        let box_left = right_edge.saturating_sub(box_w.saturating_sub(1));
        let box_top = area.y; // top border row

        // Ensure the inset box doesn't collide with the left border/title padding.
        if box_left <= area.x.saturating_add(2) || area.height < 3 {
            *self.last_close_button_area.borrow_mut() = Rect::default();
            return false;
        }

        let border_style = crate::colors::style_border_on_bg();
        let hovered = self.close_button_hovered.get();
        let button_bg = if hovered {
            crate::colors::selection()
        } else {
            crate::colors::background()
        };
        let glyph_style = Style::default()
            .fg(if hovered {
                crate::colors::text_bright()
            } else {
                crate::colors::text_dim()
            })
            .bg(button_bg)
            .add_modifier(if hovered { Modifier::BOLD } else { Modifier::empty() });

        // The dismiss icon can be multi-cell in plain mode (e.g. "[x]") or via
        // user overrides; the close box is fixed-width, so force a 1-cell glyph.
        let dismiss_glyph = crate::icons::dismiss();
        let dismiss_glyph = if UnicodeWidthStr::width(dismiss_glyph) == 1 {
            dismiss_glyph
        } else {
            "x"
        };

        // Top border row: overwrite any title spill in this region so the button
        // frame is always intact.
        buf.set_string(box_left, box_top, "┬", border_style);
        fill_rect(
            buf,
            Rect::new(box_left.saturating_add(1), box_top, box_w.saturating_sub(2), 1),
            Some('─'),
            border_style,
        );

        // Content row (one below top border).
        let content_y = box_top.saturating_add(1);
        buf.set_string(box_left, content_y, "│", border_style);
        buf.set_string(right_edge, content_y, "│", border_style);
        fill_rect(
            buf,
            Rect::new(
                box_left.saturating_add(1),
                content_y,
                box_w.saturating_sub(2),
                1,
            ),
            Some(' '),
            Style::default().bg(button_bg),
        );
        buf.set_string(box_left.saturating_add(2), content_y, dismiss_glyph, glyph_style);

        let hit_height = if draw_bottom_border && area.height >= 4 {
            let bottom_y = content_y.saturating_add(1);
            buf.set_string(box_left, bottom_y, "└", border_style);
            fill_rect(
                buf,
                Rect::new(
                    box_left.saturating_add(1),
                    bottom_y,
                    box_w.saturating_sub(2),
                    1,
                ),
                Some('─'),
                border_style,
            );
            buf.set_string(right_edge, bottom_y, "┤", border_style);
            3u16
        } else {
            2u16
        };

        *self.last_close_button_area.borrow_mut() = Rect {
            x: box_left,
            y: box_top,
            width: box_w,
            height: hit_height,
        };
        true
    }

    fn block_title_line(&self) -> Line<'static> {
        let sep_style = crate::colors::style_text_dim();
        let title_style = crate::colors::style_text();
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(5);
        spans.push(Span::styled("Settings", title_style));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(crate::icons::breadcrumb_sep(), sep_style));
        spans.push(Span::raw(" "));
        if self.is_menu_active() {
            spans.push(Span::styled("Overview", title_style));
        } else {
            spans.push(Span::styled(self.active_section().label(), title_style));
        }
        Line::from(spans)
    }

    fn render_overview(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
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
        if area.is_empty() {
            return;
        }

        let s_text_dim = crate::colors::style_text_dim();
        let s_border_dim = crate::colors::style_border_dim();
        let c_selection = crate::colors::selection();
        let c_text = crate::colors::text();

        let bg_style = crate::colors::style_on_background();
        fill_bg(buf, area, bg_style);

        if self.overview_rows.is_empty() {
            *self.last_overview_list_area.borrow_mut() = area;
            self.last_overview_line_sections.borrow_mut().clear();
            self.last_overview_line_hit_ranges.borrow_mut().clear();
            *self.last_overview_scroll.borrow_mut() = 0;
            let line = Line::from(vec![Span::styled(
                "No settings available.",
                s_text_dim,
            )]);
            Paragraph::new(line)
                .style(bg_style)
                .render(area, buf);
            return;
        }

        let active_section = self.active_section();
        let content_width = area.width as usize;
        let title_style = crate::colors::style_text();
        let mut lines: Vec<Line<'static>> =
            Vec::with_capacity(self.overview_rows.len().saturating_mul(4).saturating_add(8));
        let mut line_sections: Vec<Option<SettingsSection>> =
            Vec::with_capacity(lines.capacity());
        let mut line_hit_ranges: super::OverviewHitRanges =
            Vec::with_capacity(lines.capacity());
        let mut selected_range: Option<(usize, usize)> = None;

        for (idx, row) in self.overview_rows.iter().enumerate() {
            let is_active = row.section == active_section;
            let indicator = if is_active { crate::icons::pointer_active() } else { " " };

            if row.section == SettingsSection::Limits && !lines.is_empty() {
                lines.push(Line::from(""));
                line_sections.push(None);
                line_hit_ranges.push([None, None]);
                let dash_count = content_width.saturating_sub(2);
                if dash_count > 0 {
                    lines.push(Line::from(vec![Span::styled(
                        format!("  {}", "─".repeat(dash_count)),
                        s_border_dim,
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
                crate::colors::style_text_mid()
            };

            let icon_prefix = crate::icons::section_icon(row.section.label());
            let label_raw = format!("{}{}", icon_prefix, row.section.label());
            let label_text = format!("{label_raw:<LABEL_COLUMN_WIDTH$}");

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
                Span::styled(indicator, title_style),
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
                        .bg(c_selection)
                        .fg(c_text),
                );
            }
            lines.push(summary_line);
            line_sections.push(Some(row.section));
            line_hit_ranges.push([label_hit_range, summary_hit_range]);

            let info_text = row.section.help_line();
            let info_trimmed = self.trim_with_ellipsis(info_text, content_width.saturating_sub(8));
            let info_trimmed_width = UnicodeWidthStr::width(info_trimmed.as_str());
            let info_style = s_text_dim;
            let mut info_line = Line::from(vec![
                Span::raw("  "),
                Span::styled("└ ", info_style),
                Span::styled(info_trimmed, info_style),
            ]);
            if is_active {
                info_line = info_line.style(
                    Style::default()
                        .bg(c_selection)
                        .fg(c_text),
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
                            s_border_dim,
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
        let scroll = clamp_u16(scroll);
        *self.last_overview_list_area.borrow_mut() = area;
        *self.last_overview_line_sections.borrow_mut() = line_sections;
        *self.last_overview_line_hit_ranges.borrow_mut() = line_hit_ranges;
        *self.last_overview_scroll.borrow_mut() = scroll as usize;

        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(bg_style)
            .scroll((scroll, 0))
            .render(area, buf);
    }
}
