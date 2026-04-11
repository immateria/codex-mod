use super::*;

impl ChatWidget<'_> {
    pub(super) fn render_overlay_stack(
        &self,
        area: Rect,
        history_area: Rect,
        bottom_pane_area: Rect,
        buf: &mut Buffer,
    ) {
        let s_on_bg = crate::colors::style_on_background();
        let s_text_bold = crate::colors::style_text_bold();
        let s_text_dim = crate::colors::style_text_dim();
        let s_border_on_bg = crate::colors::style_border_on_bg();
        let s_text = crate::colors::style_text();
        let c_overlay_scrim = crate::colors::overlay_scrim();
        let c_text_bright = crate::colors::text_bright();
        let c_text_dim = crate::colors::text_dim();
        let c_background = crate::colors::background();
        let c_text = crate::colors::text();

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
                    .bg(c_overlay_scrim)
                    .fg(c_text_dim);
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
                let bg_style = crate::colors::style_on_overlay_scrim();
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
                let t_dim = s_text_dim;
                let t_fg = s_text;
                let has_tabs = overlay.tabs.len() > 1;
                let mut title_spans: Vec<ratatui::text::Span<'static>> = vec![
                    ratatui::text::Span::styled(" ", t_dim),
                    ratatui::text::Span::styled("Diff viewer", t_fg),
                ];
                if has_tabs {
                    title_spans.extend_from_slice(&[
                        ratatui::text::Span::styled(crate::ui_consts::SEP_EM, t_dim),
                        ratatui::text::Span::styled(format!("{} {}", crate::icons::tab_prev(), crate::icons::tab_next()), t_fg),
                        ratatui::text::Span::styled(" change tabs ", t_dim),
                    ]);
                }
                title_spans.extend_from_slice(&[
                    ratatui::text::Span::styled(crate::ui_consts::SEP_EM_CONT, t_dim),
                    ratatui::text::Span::styled("e", t_fg),
                    ratatui::text::Span::styled(" explain ", t_dim),
                    ratatui::text::Span::styled(crate::ui_consts::SEP_EM_CONT, t_dim),
                    ratatui::text::Span::styled("u", t_fg),
                    ratatui::text::Span::styled(" undo ", t_dim),
                    ratatui::text::Span::styled(crate::ui_consts::SEP_EM_CONT, t_dim),
                    ratatui::text::Span::styled(crate::icons::escape(), t_fg),
                    ratatui::text::Span::styled(" close ", t_dim),
                ]);
                let block = Block::default()
                    .borders(Borders::ALL)
                    .title(ratatui::text::Line::from(title_spans))
                    // Use normal background for the window itself so it contrasts against the
                    // dimmed scrim behind
                    .style(s_on_bg)
                    .border_style(
                        s_border_on_bg,
                    );
                let inner = block.inner(area);
                block.render(area, buf);

                // Paint inner content background as the normal theme background
                let inner_bg = s_on_bg;
                let _perf_overlay_inner_bg_start = if self.perf_state.enabled {
                    Some(std::time::Instant::now())
                } else {
                    None
                };
                fill_rect(buf, inner, None, inner_bg);
                if let Some(t0) = _perf_overlay_inner_bg_start {
                    let dt = t0.elapsed().as_nanos();
                    let mut p = self.perf_state.stats.borrow_mut();
                    p.ns_overlay_body_bg = p.ns_overlay_body_bg.saturating_add(dt);
                    let cells = (inner.width as u64) * (inner.height as u64);
                    p.cells_overlay_body_bg = p.cells_overlay_body_bg.saturating_add(cells);
                }

                // Split into header tabs and body/footer
                // Add one cell padding around the entire inside of the window
                let padded_inner = inner.inner(crate::ui_consts::UNIFORM_PAD);
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
                    let mut constraints: Vec<Constraint> = Vec::with_capacity(labels.len().saturating_add(1));
                    let mut total: u16 = 0;
                    for label in &labels {
                        let w = (unicode_width::UnicodeWidthStr::width(label.as_str()) as u16)
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
                        .border_style(crate::colors::style_border());
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
                        let tab_bg = c_background;
                        let bg_style = Style::default().bg(tab_bg);
                        fill_rect(buf, rect, None, bg_style);

                        // Render label at the top line, with padding
                        let label_rect = Rect {
                            x: rect.x + 1,
                            y: rect.y,
                            width: rect.width.saturating_sub(2),
                            height: 1,
                        };
                        let label_style = if selected {
                            s_text_bold
                        } else {
                            s_text_dim
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
                            let label_len = unicode_width::UnicodeWidthStr::width(labels[i].as_str()) as u16;
                            let accent_w = label_len.min(rect.width.saturating_sub(2)).max(1);
                            let accent_rect = Rect {
                                x: label_rect.x,
                                y: rect.y + rect.height.saturating_sub(1),
                                width: accent_w,
                                height: 1,
                            };
                            let underline = Block::default()
                                .borders(Borders::BOTTOM)
                                .border_style(crate::colors::style_text_bright());
                            underline.render(accent_rect, buf);
                        }
                    }
                } else {
                    // Single-file header: show full path with (+adds -dels)
                    if let Some((label, _)) = overlay.tabs.get(overlay.selected) {
                        let header_line = ratatui::text::Line::from(ratatui::text::Span::styled(
                            label.clone(),
                            s_text_bold,
                        ));
                        let para = Paragraph::new(RtText::from(vec![header_line]))
                            .wrap(ratatui::widgets::Wrap { trim: true });
                        ratatui::widgets::Widget::render(para, tabs_area, buf);
                    }
                }

                // Render selected tab with vertical scroll
                if let Some((_, blocks)) = overlay.tabs.get(overlay.selected) {
                    // Compute total line count without cloning
                    let total_lines: usize = blocks.iter().map(|b| b.lines.len()).sum();

                    let raw_skip = overlay
                        .scroll_offsets
                        .get(overlay.selected)
                        .copied()
                        .unwrap_or(0) as usize;
                    let visible_rows = body_area.height as usize;
                    // Cache visible rows so key handler can clamp
                    self.diffs.body_visible_rows.set(body_area.height);
                    let max_off = total_lines.saturating_sub(visible_rows.max(1));
                    let skip = raw_skip.min(max_off);
                    let body_inner = body_area;
                    let visible_rows = body_inner.height as usize;
                    let end = (skip + visible_rows).min(total_lines);

                    // Clone only the visible window of lines across blocks
                    let mut visible_lines: Vec<ratatui::text::Line<'static>> = Vec::with_capacity(end.saturating_sub(skip));
                    {
                        let mut global_idx = 0usize;
                        for b in blocks {
                            let block_end = global_idx + b.lines.len();
                            if block_end <= skip {
                                global_idx = block_end;
                                continue;
                            }
                            if global_idx >= end {
                                break;
                            }
                            let local_start = skip.saturating_sub(global_idx);
                            let local_end = (end - global_idx).min(b.lines.len());
                            visible_lines.extend_from_slice(&b.lines[local_start..local_end]);
                            global_idx = block_end;
                        }
                    }

                    // Fill body background with a slightly lighter/darker paper-like background
                    let bg = c_background;
                    let contrast_target = crate::colors::text_bright();
                    let paper_color = crate::colors::mix_toward(bg, contrast_target, 0.06);
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
                    let paragraph = Paragraph::new(RtText::from(visible_lines))
                        .wrap(ratatui::widgets::Wrap { trim: false });
                    ratatui::widgets::Widget::render(paragraph, body_inner, buf);

                    // No explicit current-block highlight for a cleaner look

                    // Render confirmation dialog if active
                    if self.diffs.confirm.is_some() {
                        // Centered small box
                        let w = (body_inner.width as i16 - 6).max(14) as u16;
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
                        let dlg_block = crate::components::popup_frame::themed_block()
                            .title("Confirm Undo");
                        let dlg_inner = dlg_block.inner(dialog);
                        dlg_block.render(dialog, buf);
                        // Fill dialog inner area with theme background for consistent look
                        let dlg_bg = s_on_bg;
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
                                crate::colors::style_text_on_bg(),
                            )
                            .wrap(ratatui::widgets::Wrap { trim: true });
                        ratatui::widgets::Widget::render(para, dlg_inner, buf);
                    }
                }
            }

            // Render help overlay (covering the history area) if active
            if self.settings.overlay.is_none()
                && let Some(overlay) = &self.help.overlay {
                    use crate::chatwidget::internals::state::HelpTab;

                    // Global scrim across widget
                    let scrim_bg = Style::default()
                        .bg(c_overlay_scrim)
                        .fg(c_text_dim);
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
                    self.help.window_rect.set(window_area);
                    Clear.render(window_area, buf);

                    let border_style = s_border_on_bg;

                    let block = Block::default()
                        .borders(Borders::ALL)
                        .title(ratatui::text::Line::from(vec![
                            ratatui::text::Span::styled(
                                " ",
                                s_text_dim,
                            ),
                            ratatui::text::Span::styled(
                                "Guide",
                                s_text,
                            ),
                            ratatui::text::Span::styled(
                                crate::ui_consts::SEP_EM,
                                s_text_dim,
                            ),
                            ratatui::text::Span::styled(
                                crate::icons::escape(),
                                s_text,
                            ),
                            ratatui::text::Span::styled(
                                " close",
                                s_text_dim,
                            ),
                        ]))
                        .style(s_on_bg)
                        .border_style(border_style);
                    let inner = block.inner(window_area);
                    block.render(window_area, buf);

                    // Inset close button (same as settings overlay)
                    {
                        let box_w: u16 = 5;
                        let right_edge = window_area.x.saturating_add(window_area.width).saturating_sub(1);
                        let box_left = right_edge.saturating_sub(box_w.saturating_sub(1));
                        let box_top = window_area.y;

                        if box_left > window_area.x.saturating_add(2) && window_area.height >= 3 {
                            use crate::chatwidget::internals::state::HelpFocus;
                            let hovered = self.help.close_hovered.get();
                            let focused = self.help.focus.get() == HelpFocus::CloseButton;
                            let lit = hovered || focused;
                            let button_bg = if lit {
                                crate::colors::selection()
                            } else {
                                c_background
                            };
                            let glyph_style = Style::default()
                                .fg(if lit {
                                    c_text_bright
                                } else {
                                    c_text_dim
                                })
                                .bg(button_bg)
                                .add_modifier(if lit { ratatui::style::Modifier::BOLD } else { ratatui::style::Modifier::empty() });

                            let dismiss_glyph = crate::icons::dismiss();
                            let dismiss_glyph = if unicode_width::UnicodeWidthStr::width(dismiss_glyph) == 1 {
                                dismiss_glyph
                            } else {
                                "x"
                            };

                            // Top border
                            buf.set_string(box_left, box_top, "┬", border_style);
                            crate::util::buffer::fill_rect(
                                buf,
                                Rect::new(box_left.saturating_add(1), box_top, box_w.saturating_sub(2), 1),
                                Some('─'),
                                border_style,
                            );

                            // Content row
                            let content_y = box_top.saturating_add(1);
                            buf.set_string(box_left, content_y, "│", border_style);
                            buf.set_string(right_edge, content_y, "│", border_style);
                            crate::util::buffer::fill_rect(
                                buf,
                                Rect::new(box_left.saturating_add(1), content_y, box_w.saturating_sub(2), 1),
                                Some(' '),
                                Style::default().bg(button_bg),
                            );
                            buf.set_string(box_left.saturating_add(2), content_y, dismiss_glyph, glyph_style);

                            // Bottom border
                            let mut hit_height = 2u16;
                            if window_area.height >= 4 {
                                let bottom_y = content_y.saturating_add(1);
                                buf.set_string(box_left, bottom_y, "└", border_style);
                                crate::util::buffer::fill_rect(
                                    buf,
                                    Rect::new(box_left.saturating_add(1), bottom_y, box_w.saturating_sub(2), 1),
                                    Some('─'),
                                    border_style,
                                );
                                buf.set_string(right_edge, bottom_y, "┤", border_style);
                                hit_height = 3;
                            }

                            self.help.close_rect.set(Rect {
                                x: box_left,
                                y: box_top,
                                width: box_w,
                                height: hit_height,
                            });
                        } else {
                            self.help.close_rect.set(Rect::default());
                        }
                    }

                    // Paint inner bg
                    let inner_bg = s_on_bg;
                    for y in inner.y..inner.y + inner.height {
                        for x in inner.x..inner.x + inner.width {
                            buf[(x, y)].set_style(inner_bg);
                        }
                    }

                    // Tab bar (1 row)
                    let tab_area = Rect {
                        x: inner.x + 1,
                        y: inner.y,
                        width: inner.width.saturating_sub(2),
                        height: 1.min(inner.height),
                    };
                    if tab_area.height > 0 {
                        let active_num_style = Style::default()
                            .fg(c_text);
                        let active_label_style = Style::default()
                            .fg(c_text)
                            .add_modifier(ratatui::style::Modifier::BOLD | ratatui::style::Modifier::UNDERLINED);
                        let hover_style = Style::default()
                            .fg(c_text);
                        let inactive_style = Style::default()
                            .fg(c_text_dim);
                        let sep_style = Style::default()
                            .fg(crate::colors::border());

                        let tab_numbers: [&str; 3] = [
                            crate::icons::number_one(),
                            crate::icons::number_two(),
                            crate::icons::number_three(),
                        ];

                        // For ambiguous-width glyphs (eaw=A) the terminal may render 2 cells.
                        // Use CJK width (treats A=2) to match what most terminal emulators do.
                        let num_display_w = |s: &str| -> u16 {
                            s.chars().map(|c| unicode_width::UnicodeWidthChar::width_cjk(c).unwrap_or(1) as u16).sum()
                        };

                        let mut tab_rects: Vec<Rect> = Vec::with_capacity(HelpTab::ALL.len());
                        let mut col = tab_area.x;
                        for (i, tab) in HelpTab::ALL.iter().enumerate() {
                            if i > 0 {
                                let sep = " │ ";
                                buf.set_string(col, tab_area.y, sep, sep_style);
                                col += unicode_width::UnicodeWidthStr::width(sep) as u16;
                            }

                            let num = tab_numbers[i];
                            let label_text = tab.label();
                            let is_active = *tab == overlay.active_tab;
                            let is_hovered = !is_active && self.help.hovered_tab.get() == Some(i);

                            let tab_start = col;

                            // Left pad + number: no underline (avoids gap on ambiguous-width glyphs)
                            let (prefix_style, suffix_style) = if is_active {
                                (active_num_style, active_label_style)
                            } else if is_hovered {
                                (hover_style, hover_style)
                            } else {
                                (inactive_style, inactive_style)
                            };

                            buf.set_string(col, tab_area.y, " ", prefix_style);
                            col += 1;

                            buf.set_string(col, tab_area.y, num, prefix_style);
                            let nw = num_display_w(num);
                            // Clear continuation cells if terminal rendered as 2-wide
                            for cx in (col + 1)..=(col + nw.saturating_sub(1)) {
                                if buf[(cx, tab_area.y)].symbol().is_empty() {
                                    buf[(cx, tab_area.y)].set_symbol(" ");
                                }
                            }
                            col += nw;

                            // Space + label text + trailing space: underlined for active
                            let text_part = format!(" {label_text} ");
                            let text_w = unicode_width::UnicodeWidthStr::width(text_part.as_str()) as u16;
                            buf.set_string(col, tab_area.y, &text_part, suffix_style);
                            col += text_w;

                            let total_w = col - tab_start;
                            tab_rects.push(Rect {
                                x: tab_start,
                                y: tab_area.y,
                                width: total_w,
                                height: 1,
                            });
                        }
                        // Clickable arrows with spacing
                        use crate::chatwidget::internals::state::HelpFocus;
                        let focus = self.help.focus.get();

                        let arrow_normal = s_text_dim;
                        let arrow_highlight = Style::default()
                            .fg(c_text_bright)
                            .add_modifier(ratatui::style::Modifier::BOLD);

                        let spacer = "   ";
                        buf.set_string(col, tab_area.y, spacer, arrow_normal);
                        col += spacer.len() as u16;

                        let prev_arrow = crate::icons::arrow_left();
                        let prev_w = unicode_width::UnicodeWidthStr::width(prev_arrow).max(1) as u16;
                        let prev_lit = self.help.prev_hovered.get() || focus == HelpFocus::PrevArrow;
                        buf.set_string(col, tab_area.y, prev_arrow, if prev_lit { arrow_highlight } else { arrow_normal });
                        self.help.prev_arrow_rect.set(Rect {
                            x: col,
                            y: tab_area.y,
                            width: prev_w,
                            height: 1,
                        });
                        col += prev_w;

                        buf.set_string(col, tab_area.y, " ", arrow_normal);
                        col += 1;

                        let next_arrow = crate::icons::arrow_right();
                        let next_w = unicode_width::UnicodeWidthStr::width(next_arrow).max(1) as u16;
                        let next_lit = self.help.next_hovered.get() || focus == HelpFocus::NextArrow;
                        buf.set_string(col, tab_area.y, next_arrow, if next_lit { arrow_highlight } else { arrow_normal });
                        self.help.next_arrow_rect.set(Rect {
                            x: col,
                            y: tab_area.y,
                            width: next_w,
                            height: 1,
                        });

                        *self.help.tab_rects.borrow_mut() = tab_rects;
                    }

                    // Body area below tab bar with one cell padding
                    let body = Rect {
                        x: inner.x + 1,
                        y: inner.y + if tab_area.height > 0 { 2 } else { 0 },
                        width: inner.width.saturating_sub(2),
                        height: inner.height.saturating_sub(if tab_area.height > 0 { 2 } else { 0 }),
                    };

                    // Compute visible slice
                    let lines = overlay.lines();
                    let visible_rows = body.height as usize;
                    self.help.body_visible_rows.set(body.height);
                    let max_off = lines.len().saturating_sub(visible_rows.max(1));
                    let skip = (overlay.scroll() as usize).min(max_off);
                    let end = (skip + visible_rows).min(lines.len());
                    let visible = if skip < lines.len() {
                        &lines[skip..end]
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
