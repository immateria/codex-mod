use super::*;
use super::pending_command::{pending_command_box_lines, render_text_box};
use unicode_width::UnicodeWidthStr;

impl ChatWidget<'_> {
    pub(super) fn render_terminal_overlay_and_bottom_pane(
        &self,
        area: Rect,
        history_area: Rect,
        bottom_pane_area: Rect,
        buf: &mut Buffer,
    ) {
        let s_on_bg = crate::colors::style_on_background();
        let s_text_dim = crate::colors::style_text_dim();
        let s_text = crate::colors::style_text();

        if self.terminal.overlay().is_some() || self.agents_terminal.active {
            let bg_style = s_on_bg;
            fill_bg(buf, bottom_pane_area, bg_style);
        } else {
            // Render the bottom pane directly without a border for now
            // The composer has its own layout with hints at the bottom
            (&self.bottom_pane).render(bottom_pane_area, buf);
            self.render_bottom_status_line(bottom_pane_area, buf);
        }

        if let Some(overlay) = self.terminal.overlay() {
            let scrim_style = Style::default()
                .bg(crate::colors::overlay_scrim())
                .fg(crate::colors::text_dim());
            fill_rect(buf, area, None, scrim_style);

            let padding = 1u16;
            let footer_reserved = 1.min(bottom_pane_area.height);
            let overlay_bottom = (bottom_pane_area.y + bottom_pane_area.height)
                .saturating_sub(footer_reserved);
            let overlay_height = overlay_bottom
                .saturating_sub(history_area.y)
                .max(1)
                .min(area.height);
            let window_area = Rect {
                x: history_area.x + padding,
                y: history_area.y,
                width: history_area.width.saturating_sub(padding * 2),
                height: overlay_height,
            };
            Clear.render(window_area, buf);

            let block = Block::default()
                .borders(Borders::ALL)
                .title(ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled(
                        format!(" Terminal - {} ", overlay.title),
                        s_text,
                    ),
                ]))
                .style(s_on_bg)
                .border_style(
                    crate::colors::style_border_on_bg(),
                );
            let inner = block.inner(window_area);
            block.render(window_area, buf);

            let inner_bg = s_on_bg;
            fill_rect(buf, inner, None, inner_bg);

            let content = inner.inner(crate::ui_consts::HORIZONTAL_PAD);
            if content.height == 0 || content.width == 0 {
                self.terminal.last_visible_rows.set(0);
                self.terminal.last_visible_cols.set(0);
            } else {
                let header_height = 1.min(content.height);
                let footer_height = if content.height >= 2 { 2 } else { 0 };

                let header_area = Rect {
                    x: content.x,
                    y: content.y,
                    width: content.width,
                    height: header_height,
                };
                let footer_area = if footer_height > 0 {
                    Rect {
                        x: content.x,
                        y: content
                            .y
                            .saturating_add(content.height.saturating_sub(footer_height)),
                        width: content.width,
                        height: footer_height,
                    }
                } else {
                    header_area
                };

                if header_height > 0 {
                    fill_bg(buf, header_area, inner_bg);
                    let width_limit = header_area.width as usize;
                    let mut header_spans: Vec<ratatui::text::Span<'static>> = Vec::new();
                    let mut consumed_width: usize = 0;

                    if overlay.running {
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis();
                        let frame = crate::spinner::frame_at_time(
                            crate::spinner::current_spinner(),
                            now_ms,
                        );
                        if !frame.is_empty() {
                            consumed_width += UnicodeWidthStr::width(frame.as_str());
                            header_spans.push(ratatui::text::Span::styled(
                                frame,
                                Style::default().fg(crate::colors::spinner()),
                            ));
                            header_spans.push(ratatui::text::Span::raw(" "));
                            consumed_width = consumed_width.saturating_add(1);
                        }

                        let status_text = overlay
                            .start_time.map_or_else(|| "Running…".to_owned(), |start| format!("Running… ({})", format_duration(start.elapsed())));
                        consumed_width = consumed_width
                            .saturating_add(UnicodeWidthStr::width(status_text.as_str()));
                        header_spans.push(ratatui::text::Span::styled(
                            status_text,
                            s_text_dim,
                        ));

                        let interval = crate::spinner::current_spinner().interval_ms.max(50);
                        self.app_event_tx
                            .send(AppEvent::ScheduleFrameIn(Duration::from_millis(interval)));
                    } else {
                        let (icon, color, status_text) = match overlay.exit_code {
                            Some(0) => (
                                crate::icons::status_ok(),
                                crate::colors::success(),
                                overlay
                                    .duration.map_or_else(|| "Completed".to_owned(), |d| format!("Completed in {}", format_duration(d))),
                            ),
                            Some(code) => (
                                crate::icons::status_fail(),
                                crate::colors::error(),
                                overlay
                                    .duration.map_or_else(|| format!("Exit {code}"), |d| format!("Exit {code} in {}", format_duration(d))),
                            ),
                            None => (
                                "!",
                                crate::colors::warning(),
                                overlay
                                    .duration.map_or_else(|| "Stopped".to_owned(), |d| format!("Stopped after {}", format_duration(d))),
                            ),
                        };

                        header_spans.push(ratatui::text::Span::styled(
                            format!("{icon} "),
                            Style::default().fg(color),
                        ));
                        consumed_width = consumed_width.saturating_add(UnicodeWidthStr::width(icon) + 1);

                        consumed_width = consumed_width
                            .saturating_add(UnicodeWidthStr::width(status_text.as_str()));
                        header_spans.push(ratatui::text::Span::styled(
                            status_text,
                            s_text_dim,
                        ));
                    }

                    if !overlay.command_display.is_empty() && width_limit > consumed_width + 5 {
                        let remaining = width_limit.saturating_sub(consumed_width + 5);
                        if remaining > 0 {
                            let truncated = crate::text_formatting::truncate_chars_with_ellipsis(
                                &overlay.command_display,
                                remaining,
                            );
                            if !truncated.is_empty() {
                                header_spans.push(ratatui::text::Span::styled(
                                    "  •  ",
                                    s_text_dim,
                                ));
                                header_spans.push(ratatui::text::Span::styled(
                                    truncated,
                                    s_text,
                                ));
                            }
                        }
                    }

                    let header_line = ratatui::text::Line::from(header_spans);
                    Paragraph::new(RtText::from(vec![header_line]))
                        .wrap(ratatui::widgets::Wrap { trim: true })
                        .render(header_area, buf);
                }

                let mut body_space = content
                    .height
                    .saturating_sub(header_height.saturating_add(footer_height));
                let body_top = header_area.y.saturating_add(header_area.height);
                let mut bottom_cursor = body_top.saturating_add(body_space);

                let mut pending_visible = false;
                let mut pending_box: Option<(Rect, Vec<RtLine<'static>>)> = None;
                if let Some(pending) = overlay.pending_command.as_ref()
                    && let Some((pending_lines, pending_height)) =
                        pending_command_box_lines(pending, content.width)
                        && pending_height <= body_space && pending_height > 0 {
                            bottom_cursor = bottom_cursor.saturating_sub(pending_height);
                            let pending_area = Rect {
                                x: content.x,
                                y: bottom_cursor,
                                width: content.width,
                                height: pending_height,
                            };
                            body_space = body_space.saturating_sub(pending_height);
                            pending_box = Some((pending_area, pending_lines));
                            pending_visible = true;
                        }

                let body_area = Rect {
                    x: content.x,
                    y: body_top,
                    width: content.width,
                    height: body_space,
                };

                // Body content
                let rows = body_area.height;
                let cols = body_area.width;
                let prev_rows = self.terminal.last_visible_rows.replace(rows);
                let prev_cols = self.terminal.last_visible_cols.replace(cols);
                if rows > 0 && cols > 0 && (prev_rows != rows || prev_cols != cols) {
                    self.app_event_tx.send(AppEvent::TerminalResize {
                        id: overlay.id,
                        rows,
                        cols,
                    });
                }

                if rows > 0 && cols > 0 {
                    let truncation_offset = usize::from(overlay.truncated);
                    let total = truncation_offset + overlay.lines.len();
                    let visible = rows as usize;
                    if visible > 0 {
                        let max_scroll = total.saturating_sub(visible);
                        let scroll = (overlay.scroll as usize).min(max_scroll);
                        let end = (scroll + visible).min(total);
                        let mut window_lines: Vec<RtLine<'static>> =
                            Vec::with_capacity(end - scroll);
                        for i in scroll..end {
                            if i < truncation_offset {
                                window_lines.push(ratatui::text::Line::from(vec![
                                    ratatui::text::Span::styled(
                                        "… output truncated (showing last 10,000 lines)",
                                        s_text_dim,
                                    ),
                                ]));
                            } else {
                                let idx = i - truncation_offset;
                                if let Some(line) = overlay.lines.get(idx) {
                                    window_lines.push(line.clone());
                                }
                            }
                        }
                        Paragraph::new(RtText::from(window_lines))
                            .wrap(ratatui::widgets::Wrap { trim: false })
                            .render(body_area, buf);
                    }
                }

                if let Some((pending_area, pending_lines)) = pending_box {
                    render_text_box(
                        pending_area,
                        " Command ",
                        crate::colors::function(),
                        pending_lines,
                        buf,
                    );
                }

                // Footer hints
                let mut footer_spans = vec![
                    ratatui::text::Span::styled(
                        crate::icons::nav_up_down(),
                        crate::colors::style_function(),
                    ),
                    ratatui::text::Span::styled(
                        " Scroll  ",
                        s_text_dim,
                    ),
                    ratatui::text::Span::styled(
                        crate::icons::escape(),
                        crate::colors::style_error(),
                    ),
                    ratatui::text::Span::styled(
                        if overlay.running { " Cancel  " } else { " Close  " },
                        s_text_dim,
                    ),
                ];
                if overlay.running {
                    footer_spans.push(ratatui::text::Span::styled(
                        crate::icons::ctrl_combo("C"),
                        crate::colors::style_warning(),
                    ));
                    footer_spans.push(ratatui::text::Span::styled(
                        " cancel",
                        s_text_dim,
                    ));
                } else if pending_visible {
                    footer_spans.push(ratatui::text::Span::styled(
                        crate::icons::enter(),
                        crate::colors::style_primary(),
                    ));
                    footer_spans.push(ratatui::text::Span::styled(
                        " run",
                        s_text_dim,
                    ));
                }
                if footer_height > 1 {
                    let spacer_area = Rect {
                        x: footer_area.x,
                        y: footer_area.y,
                        width: footer_area.width,
                        height: footer_area.height.saturating_sub(1),
                    };
                    fill_bg(buf, spacer_area, inner_bg);
                }

                let instructions_area = Rect {
                    x: footer_area.x,
                    y: footer_area.y.saturating_add(footer_area.height.saturating_sub(1)),
                    width: footer_area.width,
                    height: 1,
                };

                Paragraph::new(RtText::from(vec![ratatui::text::Line::from(footer_spans)]))
                    .wrap(ratatui::widgets::Wrap { trim: true })
                    .alignment(ratatui::layout::Alignment::Left)
                    .render(instructions_area, buf);
            }
        }
    }
}
