use super::*;
use crate::util::numeric::clamp_u16;

impl ChatWidget<'_> {
    pub(super) fn render_browser_overlay(
        &self,
        frame_area: Rect,
        history_area: Rect,
        bottom_pane_area: Rect,
        buf: &mut Buffer,
    ) {
        use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect as RtRect};
        use ratatui::style::Style;
        use ratatui::text::{Line as RLine, Span};
        use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
        use ratatui::widgets::Widget;

        let s_text = crate::colors::style_text();
        let s_text_dim = crate::colors::style_text_dim();
        let s_on_bg = crate::colors::style_on_background();


        let scrim_style = Style::default()
            .bg(crate::colors::overlay_scrim())
            .fg(crate::colors::text_dim());
        fill_rect(buf, frame_area, None, scrim_style);

        let padding = 1u16;
        let footer_reserved = bottom_pane_area.height.min(1);
        let overlay_bottom = (bottom_pane_area.y + bottom_pane_area.height).saturating_sub(footer_reserved);
        let overlay_height = overlay_bottom
            .saturating_sub(history_area.y)
            .max(1)
            .min(frame_area.height);

        let window_area = Rect {
            x: history_area.x + padding,
            y: history_area.y,
            width: history_area.width.saturating_sub(padding * 2),
            height: overlay_height,
        };
        Clear.render(window_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(RLine::from(vec![
                Span::styled(
                    format!(" {} ", self.browser_title()),
                    s_text,
                ),
                Span::styled(
                    "— Ctrl+B to close",
                    s_text_dim,
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
        if content.width == 0 || content.height == 0 {
            return;
        }

        let overlay_tracker = self.browser_overlay_tracker();
        let cell_opt = overlay_tracker.as_ref().map(|(_, tracker)| &tracker.cell);

        let (screenshot_history, mut selected_index) = if let Some(cell) = cell_opt {
            let history = cell.screenshot_history();
            if history.is_empty() {
                (None, 0usize)
            } else {
                let mut index = self.browser_overlay_state.screenshot_index();
                if index >= history.len() {
                    index = history.len().saturating_sub(1);
                    self.browser_overlay_state.set_screenshot_index(index);
                }
                (Some(history), index)
            }
        } else {
            (None, 0usize)
        };

        let screenshot_count = screenshot_history.map_or(0, <[_]>::len);
        if screenshot_count == 0 {
            selected_index = 0;
        }

        let mut screenshot_path = screenshot_history
            .and_then(|history| history.get(selected_index))
            .map(|record| record.path.clone());
        let mut screenshot_url = screenshot_history
            .and_then(|history| history.get(selected_index))
            .and_then(|record| record.url.clone());

        if screenshot_path.is_none()
            && let Ok(latest) = self.latest_browser_screenshot.lock()
                && let Some((path, url)) = latest.as_ref() {
                    screenshot_path = Some(path.clone());
                    if screenshot_url.is_none() {
                        screenshot_url = Some(url.clone());
                    }
                }

        let summary_label = cell_opt.map_or_else(|| self.browser_title().to_owned(), crate::history_cell::BrowserSessionCell::summary_label);
        let summary_value = screenshot_url
            
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| summary_label.clone());

        let screenshot_info = if screenshot_count > 0 {
            format!("Shot {}/{}", selected_index + 1, screenshot_count)
        } else {
            "No screenshots yet".to_owned()
        };

        let is_active = screenshot_path.is_some();
        let key_hint_style = crate::colors::style_function();
        let label_style = s_text_dim;
        let dot_style = if is_active {
            crate::colors::style_success_green()
        } else {
            s_text_dim
        };

        let header_height = u16::from(content.height >= 3);
        if header_height > 0 {
            let header_area = Rect {
                x: content.x,
                y: content.y,
                width: content.width,
                height: 1,
            };

            let mut left_spans: Vec<Span> = Vec::new();
            left_spans.push(Span::styled(crate::icons::bullet(), dot_style));
            if !summary_value.is_empty() {
                left_spans.push(Span::raw(" "));
                left_spans.push(Span::raw(summary_value));
            }
            left_spans.push(Span::raw("  "));
            left_spans.push(Span::styled(screenshot_info, label_style));

            let right_spans: Vec<Span> = vec![
                Span::from(crate::icons::ctrl_combo("B")).style(key_hint_style),
                Span::styled(" close", label_style),
            ];

            let measure = |spans: &[Span]| -> usize {
                spans.iter().map(|s| s.content.chars().count()).sum()
            };
            let left_len = measure(&left_spans);
            let right_len = measure(&right_spans);
            let total_width = header_area.width as usize;
            if total_width > left_len + right_len {
                let spacer = " ".repeat(total_width - left_len - right_len);
                left_spans.push(Span::from(spacer));
            }
            let mut spans = left_spans;
            spans.extend(right_spans);
            Paragraph::new(RLine::from(spans))
                .alignment(Alignment::Left)
                .render(header_area, buf);
        }

        let mut body_y = content.y + header_height;
        let mut body_height = content.height.saturating_sub(header_height);
        if header_height > 0 && body_height > 0 {
            body_y = body_y.saturating_add(1);
            body_height = body_height.saturating_sub(1);
        }

        if body_height == 0 {
            return;
        }

        let body_area = RtRect {
            x: content.x,
            y: body_y,
            width: content.width,
            height: body_height,
        };

        let column_constraints = if body_area.width <= 50 {
            // Single-pane: screenshot gets full width, info is hidden
            [
                Constraint::Length(body_area.width),
                Constraint::Length(0),
            ]
        } else {
            [Constraint::Percentage(62), Constraint::Percentage(38)]
        };
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(column_constraints)
            .split(body_area);

        let screenshot_column = columns[0];
        let info_column = if columns.len() > 1 { columns[1] } else { columns[0] };

        let progress_height = u16::from(screenshot_column.height > 3);
        let screenshot_display_height = screenshot_column.height.saturating_sub(progress_height);
        let screenshot_display_area = Rect {
            x: screenshot_column.x,
            y: screenshot_column.y,
            width: screenshot_column.width,
            height: screenshot_display_height,
        };
        let progress_area = (progress_height > 0).then(|| Rect {
            x: screenshot_column.x,
            y: screenshot_column
                .y
                .saturating_add(screenshot_column.height.saturating_sub(progress_height)),
            width: screenshot_column.width,
            height: progress_height,
        });

        if screenshot_display_area.width > 0 && screenshot_display_area.height > 0 {
            if let Some(path) = screenshot_path.as_ref() {
                self.render_screenshot_highlevel(path, screenshot_display_area, buf);
            } else {
                let message = Paragraph::new(RLine::from(vec![Span::raw(
                    "No browser session captured yet.",
                )]))
                .alignment(Alignment::Center)
                .style(s_text_dim);
                Widget::render(message, screenshot_display_area, buf);
            }
        }

        let current_time = screenshot_history
            .and_then(|history| history.get(selected_index)).map_or_else(|| Duration::ZERO, |record| record.timestamp);
        let mut total_time = overlay_tracker
            .as_ref().map_or_else(|| Duration::ZERO, |(_, tracker)| tracker.elapsed);
        if let Some(history) = screenshot_history
            && let Some(last) = history.last() {
                total_time = total_time.max(last.timestamp);
            }
        if let Some(cell) = cell_opt {
            total_time = total_time.max(cell.total_duration());
        }

        if let Some(area) = progress_area
            && area.height > 0 && area.width > 0 {
                let progress_line = self.browser_overlay_progress_line(area.width, current_time, total_time);
                Paragraph::new(progress_line)
                    .alignment(Alignment::Center)
                    .style(s_text)
                    .render(area, buf);
            }

        if info_column.width == 0 || info_column.height == 0 {
            return;
        }

        let header_style = crate::colors::style_text_bold();
        let secondary_style = s_text_dim;
        let primary_style = s_text;

        let mut info_lines: Vec<RLine<'static>> = Vec::new();

        info_lines.push(RLine::from(vec![Span::styled("Screenshots", header_style)]));

        if let Some(history) = screenshot_history {
            if history.is_empty() {
                info_lines.push(RLine::from(vec![Span::styled(
                    "No screenshots yet",
                    secondary_style,
                )]));
            } else {
                for (idx, entry) in history.iter().enumerate() {
                    let mut spans: Vec<Span> = Vec::new();
                    let marker = if idx == selected_index { crate::icons::radio_selected() } else { crate::icons::bullet() };
                    let marker_style = if idx == selected_index {
                        crate::colors::style_primary()
                    } else {
                        secondary_style
                    };
                    spans.push(Span::styled(marker.to_owned(), marker_style));
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        self.format_overlay_mm_ss(entry.timestamp),
                        secondary_style,
                    ));
                    if let Some(url) = entry.url.as_ref()
                        && !url.trim().is_empty() {
                            spans.push(Span::raw("  "));
                            spans.push(Span::styled(url.clone(), primary_style));
                        }
                    info_lines.push(RLine::from(spans));
                }
            }
        } else {
            info_lines.push(RLine::from(vec![Span::styled(
                "No browser session yet",
                secondary_style,
            )]));
        }

        info_lines.push(RLine::from(""));
        info_lines.push(RLine::from(vec![Span::styled("Actions", header_style)]));

        if let Some(cell) = cell_opt {
            let entries = cell.full_action_entries();
            if entries.is_empty() {
                info_lines.push(RLine::from(vec![Span::styled(
                    "No browser actions yet",
                    secondary_style,
                )]));
            } else {
                for (time, label, detail) in entries {
                    let mut spans: Vec<Span> = vec![
                        Span::styled(crate::icons::bullet(), secondary_style),
                        Span::raw(" "),
                        Span::styled(
                            self.normalize_action_time_label(time.as_str()),
                            secondary_style,
                        ),
                        Span::raw("  "),
                        Span::styled(label.clone(), primary_style),
                    ];
                    let detail_trimmed = detail.trim();
                    if !detail_trimmed.is_empty() {
                        spans.push(Span::raw(" "));
                        spans.push(Span::styled(detail_trimmed.to_owned(), secondary_style));
                    }
                    info_lines.push(RLine::from(spans));
                }
            }
        } else {
            info_lines.push(RLine::from(vec![Span::styled(
                "No browser session yet",
                secondary_style,
            )]));
        }

        info_lines.push(RLine::from(""));
        info_lines.push(RLine::from(vec![Span::styled(
            format!("Controls: {lr} or {ud} select screenshot • Shift+{ud} or j/k scroll actions", lr = crate::icons::nav_left_right(), ud = crate::icons::nav_up_down()),
            secondary_style,
        )]));

        let max_scroll = info_lines.len().saturating_sub(info_column.height as usize);
        let max_scroll_u16 = clamp_u16(max_scroll);
        self.browser_overlay_state
            .update_action_metrics(info_column.height, max_scroll_u16);
        let scroll = self
            .browser_overlay_state
            .action_scroll()
            .min(max_scroll_u16);

        let paragraph = Paragraph::new(info_lines)
            .style(s_text)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0));
        Widget::render(paragraph, info_column, buf);
    }
    fn browser_title(&self) -> &'static str {
        if self.browser_is_external {
            "Chrome"
        } else {
            "Browser"
        }
    }
}
