use super::*;

impl ChatWidget<'_> {
    pub(super) fn render_overlay_stack(
        &self,
        area: Rect,
        history_area: Rect,
        bottom_pane_area: Rect,
        buf: &mut Buffer,
    ) {
        if self.terminal.overlay().is_none() && self.browser_overlay_visible {
            self.render_browser_overlay(area, history_area, bottom_pane_area, buf);
            return;
        }

        if self.terminal.overlay().is_none() && self.agents_terminal.active {
            self.render_agents_terminal_overlay(area, history_area, bottom_pane_area, buf);
        }

        // Terminal overlay takes precedence over other overlays

        // Welcome animation is kept as a normal cell in history; no overlay.

        // The welcome animation is no longer rendered as an overlay.

        let terminal_overlay_none = self.terminal.overlay().is_none();
        let agents_terminal_active = self.agents_terminal.active;
        if terminal_overlay_none && !agents_terminal_active {
            if let Some(overlay) = self.settings.overlay.as_ref() {
                self.render_settings_overlay(area, history_area, buf, overlay);
            } else if let Some(overlay) = &self.diffs.overlay {
                // Global scrim: dim the whole background to draw focus to the viewer
                // We intentionally do this across the entire widget area rather than just the
                // history area so the viewer stands out even with browser HUD or status bars.
                let scrim_bg = Style::default()
                    .bg(crate::colors::overlay_scrim())
                    .fg(crate::colors::text_dim());
                let _perf_scrim_start = if self.perf_state.enabled {
                    Some(std::time::Instant::now())
                } else {
                    None
                };
                fill_rect(buf, area, None, scrim_bg);
                if let Some(t0) = _perf_scrim_start {
                    let dt = t0.elapsed().as_nanos();
                    let mut p = self.perf_state.stats.borrow_mut();
                    p.ns_overlay_scrim = p.ns_overlay_scrim.saturating_add(dt);
                    let cells = (area.width as u64) * (area.height as u64);
                    p.cells_overlay_scrim = p.cells_overlay_scrim.saturating_add(cells);
                }
                // Match the horizontal padding used by status bar and input
                let padding = 1u16;
                let area = Rect {
                    x: history_area.x + padding,
                    y: history_area.y,
                    width: history_area.width.saturating_sub(padding * 2),
                    height: history_area.height,
                };

                // Clear and repaint the overlay area with theme scrim background
                Clear.render(area, buf);
                let bg_style = Style::default().bg(crate::colors::overlay_scrim());
                let _perf_overlay_area_bg_start = if self.perf_state.enabled {
                    Some(std::time::Instant::now())
                } else {
                    None
                };
                fill_rect(buf, area, None, bg_style);
                if let Some(t0) = _perf_overlay_area_bg_start {
                    let dt = t0.elapsed().as_nanos();
                    let mut p = self.perf_state.stats.borrow_mut();
                    p.ns_overlay_body_bg = p.ns_overlay_body_bg.saturating_add(dt);
                    let cells = (area.width as u64) * (area.height as u64);
                    p.cells_overlay_body_bg = p.cells_overlay_body_bg.saturating_add(cells);
                }

                // Build a styled title: keys/icons in normal text color; descriptors and dividers dim
                let t_dim = Style::default().fg(crate::colors::text_dim());
                let t_fg = Style::default().fg(crate::colors::text());
                let has_tabs = overlay.tabs.len() > 1;
                let mut title_spans: Vec<ratatui::text::Span<'static>> = vec![
                    ratatui::text::Span::styled(" ", t_dim),
                    ratatui::text::Span::styled("Diff viewer", t_fg),
                ];
                if has_tabs {
                    title_spans.extend_from_slice(&[
                        ratatui::text::Span::styled(" ——— ", t_dim),
                        ratatui::text::Span::styled("◂ ▸", t_fg),
                        ratatui::text::Span::styled(" change tabs ", t_dim),
                    ]);
                }
                title_spans.extend_from_slice(&[
                    ratatui::text::Span::styled("——— ", t_dim),
                    ratatui::text::Span::styled("e", t_fg),
                    ratatui::text::Span::styled(" explain ", t_dim),
                    ratatui::text::Span::styled("——— ", t_dim),
                    ratatui::text::Span::styled("u", t_fg),
                    ratatui::text::Span::styled(" undo ", t_dim),
                    ratatui::text::Span::styled("——— ", t_dim),
                    ratatui::text::Span::styled("Esc", t_fg),
                    ratatui::text::Span::styled(" close ", t_dim),
                ]);
                let block = Block::default()
                    .borders(Borders::ALL)
                    .title(ratatui::text::Line::from(title_spans))
                    // Use normal background for the window itself so it contrasts against the
                    // dimmed scrim behind
                    .style(Style::default().bg(crate::colors::background()))
                    .border_style(
                        Style::default()
                            .fg(crate::colors::border())
                            .bg(crate::colors::background()),
                    );
                let inner = block.inner(area);
                block.render(area, buf);

                // Paint inner content background as the normal theme background
                let inner_bg = Style::default().bg(crate::colors::background());
                let _perf_overlay_inner_bg_start = if self.perf_state.enabled {
                    Some(std::time::Instant::now())
                } else {
                    None
                };
                for y in inner.y..inner.y + inner.height {
                    for x in inner.x..inner.x + inner.width {
                        buf[(x, y)].set_style(inner_bg);
                    }
                }
                if let Some(t0) = _perf_overlay_inner_bg_start {
                    let dt = t0.elapsed().as_nanos();
                    let mut p = self.perf_state.stats.borrow_mut();
                    p.ns_overlay_body_bg = p.ns_overlay_body_bg.saturating_add(dt);
                    let cells = (inner.width as u64) * (inner.height as u64);
                    p.cells_overlay_body_bg = p.cells_overlay_body_bg.saturating_add(cells);
                }

                // Split into header tabs and body/footer
                // Add one cell padding around the entire inside of the window
                let padded_inner = inner.inner(ratatui::layout::Margin::new(1, 1));
                let [tabs_area, body_area] = if has_tabs {
                    Layout::vertical([Constraint::Length(2), Constraint::Fill(1)])
                        .areas(padded_inner)
                } else {
                    // Keep a small header row to show file path and counts
                    let [t, b] = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)])
                        .areas(padded_inner);
                    [t, b]
                };

                // Render tabs only if we have more than one file
                if has_tabs {
                    let labels: Vec<String> = overlay
                        .tabs
                        .iter()
                        .map(|(t, _)| format!("  {t}  "))
                        .collect();
                    let mut constraints: Vec<Constraint> = Vec::new();
                    let mut total: u16 = 0;
                    for label in &labels {
                        let w = (label.chars().count() as u16)
                            .min(tabs_area.width.saturating_sub(total));
                        constraints.push(Constraint::Length(w));
                        total = total.saturating_add(w);
                        if total >= tabs_area.width.saturating_sub(4) {
                            break;
                        }
                    }
                    constraints.push(Constraint::Fill(1));
                    let chunks = Layout::horizontal(constraints).split(tabs_area);
                    // Draw a light bottom border across the entire tabs strip
                    let tabs_bottom_rule = Block::default()
                        .borders(Borders::BOTTOM)
                        .border_style(Style::default().fg(crate::colors::border()));
                    tabs_bottom_rule.render(tabs_area, buf);
                    for i in 0..labels.len() {
                        // last chunk is filler; guard below
                        if i >= chunks.len().saturating_sub(1) {
                            break;
                        }
                        let rect = chunks[i];
                        if rect.width == 0 {
                            continue;
                        }
                        let selected = i == overlay.selected;

                        // Both selected and unselected tabs use the normal background
                        let tab_bg = crate::colors::background();
                        let bg_style = Style::default().bg(tab_bg);
                        for y in rect.y..rect.y + rect.height {
                            for x in rect.x..rect.x + rect.width {
                                buf[(x, y)].set_style(bg_style);
                            }
                        }

                        // Render label at the top line, with padding
                        let label_rect = Rect {
                            x: rect.x + 1,
                            y: rect.y,
                            width: rect.width.saturating_sub(2),
                            height: 1,
                        };
                        let label_style = if selected {
                            Style::default()
                                .fg(crate::colors::text())
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(crate::colors::text_dim())
                        };
                        let line = ratatui::text::Line::from(ratatui::text::Span::styled(
                            labels[i].clone(),
                            label_style,
                        ));
                        Paragraph::new(RtText::from(vec![line]))
                            .wrap(ratatui::widgets::Wrap { trim: true })
                            .render(label_rect, buf);
                        // Selected tab: thin underline using text_bright under the label width
                        if selected {
                            let label_len = labels[i].chars().count() as u16;
                            let accent_w = label_len.min(rect.width.saturating_sub(2)).max(1);
                            let accent_rect = Rect {
                                x: label_rect.x,
                                y: rect.y + rect.height.saturating_sub(1),
                                width: accent_w,
                                height: 1,
                            };
                            let underline = Block::default()
                                .borders(Borders::BOTTOM)
                                .border_style(Style::default().fg(crate::colors::text_bright()));
                            underline.render(accent_rect, buf);
                        }
                    }
                } else {
                    // Single-file header: show full path with (+adds -dels)
                    if let Some((label, _)) = overlay.tabs.get(overlay.selected) {
                        let header_line = ratatui::text::Line::from(ratatui::text::Span::styled(
                            label.clone(),
                            Style::default()
                                .fg(crate::colors::text())
                                .add_modifier(Modifier::BOLD),
                        ));
                        let para = Paragraph::new(RtText::from(vec![header_line]))
                            .wrap(ratatui::widgets::Wrap { trim: true });
                        ratatui::widgets::Widget::render(para, tabs_area, buf);
                    }
                }

                // Render selected tab with vertical scroll and highlight current diff block
                if let Some((_, blocks)) = overlay.tabs.get(overlay.selected) {
                    // Flatten blocks into lines and record block start indices
                    let mut all_lines: Vec<ratatui::text::Line<'static>> = Vec::new();
                    let mut block_starts: Vec<(usize, usize)> = Vec::new(); // (start_index, len)
                    for b in blocks {
                        let start = all_lines.len();
                        block_starts.push((start, b.lines.len()));
                        all_lines.extend(b.lines.clone());
                    }

                    let raw_skip = overlay
                        .scroll_offsets
                        .get(overlay.selected)
                        .copied()
                        .unwrap_or(0) as usize;
                    let visible_rows = body_area.height as usize;
                    // Cache visible rows so key handler can clamp
                    self.diffs.body_visible_rows.set(body_area.height);
                    let max_off = all_lines.len().saturating_sub(visible_rows.max(1));
                    let skip = raw_skip.min(max_off);
                    let body_inner = body_area;
                    let visible_rows = body_inner.height as usize;

                    // Collect visible slice
                    let end = (skip + visible_rows).min(all_lines.len());
                    let visible = if skip < all_lines.len() {
                        &all_lines[skip..end]
                    } else {
                        &[]
                    };
                    // Fill body background with a slightly lighter paper-like background
                    let bg = crate::colors::background();
                    let paper_color = crate::colors::mix_toward(bg, ratatui::style::Color::White, 0.06);
                    let body_bg = Style::default().bg(paper_color);
                    let _perf_overlay_body_bg2 = if self.perf_state.enabled {
                        Some(std::time::Instant::now())
                    } else {
                        None
                    };
                    for y in body_inner.y..body_inner.y + body_inner.height {
                        for x in body_inner.x..body_inner.x + body_inner.width {
                            buf[(x, y)].set_style(body_bg);
                        }
                    }
                    if let Some(t0) = _perf_overlay_body_bg2 {
                        let dt = t0.elapsed().as_nanos();
                        let mut p = self.perf_state.stats.borrow_mut();
                        p.ns_overlay_body_bg = p.ns_overlay_body_bg.saturating_add(dt);
                        let cells = (body_inner.width as u64) * (body_inner.height as u64);
                        p.cells_overlay_body_bg = p.cells_overlay_body_bg.saturating_add(cells);
                    }
                    let paragraph = Paragraph::new(RtText::from(visible.to_vec()))
                        .wrap(ratatui::widgets::Wrap { trim: false });
                    ratatui::widgets::Widget::render(paragraph, body_inner, buf);

                    // No explicit current-block highlight for a cleaner look

                    // Render confirmation dialog if active
                    if self.diffs.confirm.is_some() {
                        // Centered small box
                        let w = (body_inner.width as i16 - 10).max(20) as u16;
                        let h = 5u16;
                        let x = body_inner.x + (body_inner.width.saturating_sub(w)) / 2;
                        let y = body_inner.y + (body_inner.height.saturating_sub(h)) / 2;
                        let dialog = Rect {
                            x,
                            y,
                            width: w,
                            height: h,
                        };
                        Clear.render(dialog, buf);
                        let dlg_block = Block::default()
                            .borders(Borders::ALL)
                            .title("Confirm Undo")
                            .style(
                                Style::default()
                                    .bg(crate::colors::background())
                                    .fg(crate::colors::text()),
                            )
                            .border_style(Style::default().fg(crate::colors::border()));
                        let dlg_inner = dlg_block.inner(dialog);
                        dlg_block.render(dialog, buf);
                        // Fill dialog inner area with theme background for consistent look
                        let dlg_bg = Style::default().bg(crate::colors::background());
                        for y in dlg_inner.y..dlg_inner.y + dlg_inner.height {
                            for x in dlg_inner.x..dlg_inner.x + dlg_inner.width {
                                buf[(x, y)].set_style(dlg_bg);
                            }
                        }
                        let lines = vec![
                            ratatui::text::Line::from("Are you sure you want to undo this diff?"),
                            ratatui::text::Line::from(
                                "Press Enter to confirm • Esc to cancel".to_string().dim(),
                            ),
                        ];
                        let para = Paragraph::new(RtText::from(lines))
                            .style(
                                Style::default()
                                    .bg(crate::colors::background())
                                    .fg(crate::colors::text()),
                            )
                            .wrap(ratatui::widgets::Wrap { trim: true });
                        ratatui::widgets::Widget::render(para, dlg_inner, buf);
                    }
                }
            }

            // Render help overlay (covering the history area) if active
            if self.settings.overlay.is_none()
                && let Some(overlay) = &self.help.overlay {
                    // Global scrim across widget
                    let scrim_bg = Style::default()
                        .bg(crate::colors::overlay_scrim())
                        .fg(crate::colors::text_dim());
                    for y in area.y..area.y + area.height {
                        for x in area.x..area.x + area.width {
                            buf[(x, y)].set_style(scrim_bg);
                        }
                    }
                    let padding = 1u16;
                    let window_area = Rect {
                        x: history_area.x + padding,
                        y: history_area.y,
                        width: history_area.width.saturating_sub(padding * 2),
                        height: history_area.height,
                    };
                    Clear.render(window_area, buf);
                    let block = Block::default()
                        .borders(Borders::ALL)
                        .title(ratatui::text::Line::from(vec![
                            ratatui::text::Span::styled(
                                " ",
                                Style::default().fg(crate::colors::text_dim()),
                            ),
                            ratatui::text::Span::styled(
                                "Guide",
                                Style::default().fg(crate::colors::text()),
                            ),
                            ratatui::text::Span::styled(
                                " ——— ",
                                Style::default().fg(crate::colors::text_dim()),
                            ),
                            ratatui::text::Span::styled(
                                "Esc",
                                Style::default().fg(crate::colors::text()),
                            ),
                            ratatui::text::Span::styled(
                                " close ",
                                Style::default().fg(crate::colors::text_dim()),
                            ),
                        ]))
                        .style(Style::default().bg(crate::colors::background()))
                        .border_style(
                            Style::default()
                                .fg(crate::colors::border())
                                .bg(crate::colors::background()),
                        );
                    let inner = block.inner(window_area);
                    block.render(window_area, buf);

                    // Paint inner bg
                    let inner_bg = Style::default().bg(crate::colors::background());
                    for y in inner.y..inner.y + inner.height {
                        for x in inner.x..inner.x + inner.width {
                            buf[(x, y)].set_style(inner_bg);
                        }
                    }

                    // Body area with one cell padding
                    let body = inner.inner(ratatui::layout::Margin::new(1, 1));

                    // Compute visible slice
                    let visible_rows = body.height as usize;
                    self.help.body_visible_rows.set(body.height);
                    let max_off = overlay.lines.len().saturating_sub(visible_rows.max(1));
                    let skip = (overlay.scroll as usize).min(max_off);
                    let end = (skip + visible_rows).min(overlay.lines.len());
                    let visible = if skip < overlay.lines.len() {
                        &overlay.lines[skip..end]
                    } else {
                        &[]
                    };
                    let paragraph = Paragraph::new(RtText::from(visible.to_vec()))
                        .wrap(ratatui::widgets::Wrap { trim: false });
                    ratatui::widgets::Widget::render(paragraph, body, buf);
                }
    }
}
}
