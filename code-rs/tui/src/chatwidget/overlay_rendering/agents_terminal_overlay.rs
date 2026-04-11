use super::*;

impl ChatWidget<'_> {
    pub(super) fn render_agents_terminal_overlay(
        &self,
        frame_area: Rect,
        history_area: Rect,
        bottom_pane_area: Rect,
        buf: &mut Buffer,
    ) {
        use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect as RtRect};
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{
            Block, Borders, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph,
        };

        let s_on_bg = crate::colors::style_on_background();
        let s_primary_bold = crate::colors::style_primary_bold();
        let s_text_bold = crate::colors::style_text_bold();
        let s_text_dim = crate::colors::style_text_dim();
        let s_function = crate::colors::style_function();
        let s_text = crate::colors::style_text();
        let c_primary = crate::colors::primary();
        let c_border = crate::colors::border();

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

        let title_spans = vec![
            Span::styled(" Agents ", s_text),
            Span::styled("— Ctrl+A to close", s_text_dim),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(title_spans))
            .style(s_on_bg)
            .border_style(
                crate::colors::style_border_on_bg(),
            );
        let inner = block.inner(window_area);
        block.render(window_area, buf);

        fill_rect(buf, inner, None, s_on_bg);

        // Remove vertical padding so the filter row sits directly below the title.
        let content = inner.inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        if content.width == 0 || content.height == 0 {
            return;
        }

        let tab_height = u16::from(content.height >= 3);
        let hint_height = u16::from(content.height >= 2);
        let body_height = content
            .height
            .saturating_sub(hint_height + tab_height);
        let tabs_area = RtRect {
            x: content.x,
            y: content.y,
            width: content.width,
            height: tab_height,
        };
        let body_area = RtRect {
            x: content.x,
            y: content.y.saturating_add(tab_height),
            width: content.width,
            height: body_height,
        };
        let hint_area = RtRect {
            x: content.x,
            y: content.y.saturating_add(tab_height + body_height),
            width: content.width,
            height: hint_height,
        };

        let sidebar_has_focus = self.agents_terminal.focus() == AgentsTerminalFocus::Sidebar;
        let sidebar_border_color = if sidebar_has_focus {
            c_primary
        } else {
            c_border
        };
        let filter_title_style = s_text_dim;

        if tab_height > 0 {
            let filter_row = tabs_area;
            let mut spans: Vec<Span> = Vec::new();
            spans.push(Span::styled("Filter", filter_title_style));
            spans.push(Span::raw("   "));
            let tabs = [
                (AgentsTerminalTab::All, "1", "All"),
                (AgentsTerminalTab::Running, "2", "Running"),
                (AgentsTerminalTab::Failed, "3", "Failed"),
                (AgentsTerminalTab::Completed, "4", "Done"),
                (AgentsTerminalTab::Review, "5", "Review"),
            ];
            for (idx, (tab, number, label)) in tabs.iter().enumerate() {
                if idx > 0 {
                    spans.push(Span::styled(
                        " - ",
                        s_text_dim,
                    ));
                }
                let active = *tab == self.agents_terminal.active_tab;
                let style = if active {
                    s_primary_bold
                } else {
                    s_text_dim
                };
                spans.push(Span::styled(format!("{number} {label}"), style));
            }
            let sort_label = match self.agents_terminal.sort_mode {
                AgentsSortMode::Recent => "Recent",
                AgentsSortMode::RunningFirst => "Running",
                AgentsSortMode::Name => "Name",
            };
            let sort_spans = vec![
                Span::styled("Sort: ", s_text_dim),
                Span::raw("( "),
                Span::styled(
                    format!("{sort_label} {}", crate::icons::sort_desc()),
                    s_primary_bold,
                ),
                Span::raw(" )"),
            ];

            let filter_width: u16 = spans.iter().map(|span| span.width() as u16).sum();
            let sort_width: u16 = sort_spans.iter().map(|span| span.width() as u16).sum();
            let gap = filter_row
                .width
                .saturating_sub(filter_width + sort_width)
                .max(1);
            spans.push(Span::raw(" ".repeat(gap as usize)));
            spans.extend(sort_spans);

            Paragraph::new(Line::from(spans))
                .alignment(Alignment::Left)
                .render(filter_row, buf);
        }

        let longest_name_width: u16 = self
            .agents_terminal
            .entries
            .values()
            .map(|entry| {
                let label = entry
                    .model
                    .as_ref().map_or_else(|| crate::text_formatting::format_model_label(&entry.name), |m| crate::text_formatting::format_model_label(m));
                UnicodeWidthStr::width(label.as_str()) as u16
            })
            .max()
            .unwrap_or(10);
        let status_icon_width = UnicodeWidthStr::width(agent_status_icon(AgentStatus::Running)) as u16;
        let desired_sidebar = longest_name_width
            .saturating_add(status_icon_width)
            .saturating_add(8);
        let sidebar_width = if body_area.width <= 50 {
            body_area.width
        } else {
            let max_allowed = body_area.width.saturating_sub(30).max(18);
            let min_allowed = 24.min(max_allowed);
            desired_sidebar.clamp(min_allowed, max_allowed)
        };

        let constraints = if body_area.width <= sidebar_width {
            [Constraint::Length(body_area.width), Constraint::Length(0)]
        } else {
            [Constraint::Length(sidebar_width), Constraint::Min(12)]
        };

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(body_area);

        // Sidebar list of agents grouped by batch id
        let mut items: Vec<ListItem> = Vec::new();
        let mut row_entries: Vec<Option<AgentsSidebarEntry>> = Vec::new();
        let groups = self.agents_terminal.sidebar_groups();
        let last_group_idx = groups.len().saturating_sub(1);

        for (group_idx, group) in groups.into_iter().enumerate() {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    group.label.clone(),
                    s_text_bold,
                ),
            ])));
            row_entries.push(None);

            let selected_entry = self.agents_terminal.current_sidebar_entry();

            for agent_id in group.agent_ids {
                if let Some(entry) = self.agents_terminal.entries.get(&agent_id) {
                    let model_label = entry
                        .model
                        .as_ref().map_or_else(|| crate::text_formatting::format_model_label(&entry.name), |value| crate::text_formatting::format_model_label(value));
                    let status = entry.status;
                    let status_icon = agent_status_icon(status);
                    let name_room = sidebar_width
                        .saturating_sub((UnicodeWidthStr::width(status_icon) as u16).saturating_add(5))
                        .max(4) as usize;
                    let mut display_name = model_label.clone();
                    if display_name.chars().count() > name_room {
                        display_name = display_name
                            .chars()
                            .take(name_room.saturating_sub(1))
                            .collect::<String>();
                        display_name.push('…');
                    }
                    let color = agent_status_color(status);
                    let is_selected = selected_entry
                        .as_ref()
                        .is_some_and(|entry| matches!(entry, AgentsSidebarEntry::Agent(id) if *id == agent_id));
                    let prefix_span = if is_selected {
                        Span::styled(
                            crate::icons::selection_prefix(true),
                            crate::colors::style_primary(),
                        )
                    } else {
                        Span::raw("  ")
                    };

                    let line = Line::from(vec![
                        prefix_span,
                        Span::styled(
                            display_name,
                            s_text,
                        ),
                        Span::raw(" "),
                        Span::styled(status_icon, Style::default().fg(color)),
                    ]);
                    items.push(ListItem::new(line));
                    row_entries.push(Some(AgentsSidebarEntry::Agent(agent_id.clone())));
                }
            }

            if group_idx < last_group_idx {
                items.push(ListItem::new(Line::from(vec![Span::raw(" ")])));
                row_entries.push(None);
            }
        }

        if items.is_empty() {
            let empty_text = if self.agents_terminal.order.is_empty() {
                "No agents yet"
            } else {
                "No agents match filters"
            };
            items.push(ListItem::new(Line::from(vec![Span::styled(
                empty_text,
                s_text_dim,
            )])));
            row_entries.push(None);
        }

        let mut list_state = ListState::default();
        if let Some(selected_entry) = self.agents_terminal.current_sidebar_entry()
            && let Some(row_idx) = row_entries
                .iter()
                .position(|entry| entry.as_ref() == Some(&selected_entry))
            {
                list_state.select(Some(row_idx));
            }

        // Keep the selected agent vivid even when detail pane holds focus so users
        // don’t lose their place while reading logs.
        let highlight_style = s_primary_bold;
        let sidebar = List::new(items)
            .highlight_style(highlight_style)
            .highlight_spacing(HighlightSpacing::Never);

        let sidebar_block = Block::default()
            .borders(Borders::ALL)
            .style(s_on_bg)
            .border_style(Style::default().fg(sidebar_border_color));

        let sidebar_area = chunks[0];
        let sidebar_inner = sidebar_block.inner(sidebar_area);
        sidebar_block.render(sidebar_area, buf);

        fill_rect(
            buf,
            sidebar_inner,
            None,
            s_on_bg,
        );

        ratatui::widgets::StatefulWidget::render(sidebar, sidebar_inner, buf, &mut list_state);

        let right_area = if chunks.len() > 1 { chunks[1] } else { chunks[0] };
        let detail_width = right_area.width.saturating_sub(2).max(1);
        let mut lines: Vec<Line> = Vec::new();

        match self.agents_terminal.current_sidebar_entry() {
            Some(AgentsSidebarEntry::Agent(agent_id)) => {
                if let Some(entry) = self.agents_terminal.entries.get(agent_id.as_str()) {
                    let status = entry.status;
                    let status_color = agent_status_color(status);
                    let display_name = entry
                        .model
                        .as_ref().map_or_else(|| crate::text_formatting::format_model_label(&entry.name), |m| crate::text_formatting::format_model_label(m));
                    let title_text = entry
                        .batch_label
                        .as_ref()
                        .and_then(|b| {
                            let trimmed = b.trim();
                            (!trimmed.is_empty()).then(|| trimmed.to_owned())
                        }).map_or_else(|| display_name.clone(), |batch| format!("{batch} / {display_name}"));

                    lines.push(Line::from(vec![
                        Span::raw(" "),
                        Span::styled(
                            title_text,
                            s_text_bold,
                        ),
                    ]));

                    let id_short = format!("#{}", &agent_id[..agent_id.len().min(7)]);
                    let status_chip = format!("{} {}", agent_status_icon(status), agent_status_label(status));
                    let model_meta = entry
                        .model
                        .as_ref().map_or_else(|| display_name.clone(), |m| crate::text_formatting::format_model_label(m));
                    let mut meta_line: Vec<Span> = vec![
                        Span::raw(" "),
                        Span::styled("Status:", s_text_dim),
                        Span::raw(" "),
                        Span::styled(status_chip, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
                        Span::raw("   "),
                        Span::styled("Model:", s_text_dim),
                        Span::raw(" "),
                        Span::styled(
                            model_meta,
                            s_text,
                        ),
                        Span::raw("   "),
                        Span::styled("ID:", s_text_dim),
                        Span::raw(" "),
                        Span::styled(id_short, s_text_dim),
                    ];
                    if let Some(batch_id) = entry.batch_id.as_ref() {
                        meta_line.push(Span::raw("   "));
                        meta_line.push(Span::styled(
                            format!("Batch: {}", short_batch_label(batch_id)),
                            s_text_dim,
                        ));
                    }
                    lines.push(Line::from(meta_line));

                    self.ensure_trailing_blank_line(&mut lines);

                    self.append_agent_highlights(
                        &mut lines,
                        entry,
                        detail_width,
                        self.agents_terminal.highlights_collapsed,
                    );

                    if let Some(context_text) = entry
                        .batch_context
                        .as_ref()
                        .filter(|value| !value.trim().is_empty())
                    {
                        self.ensure_trailing_blank_line(&mut lines);
                        self.append_agents_overlay_section(&mut lines, "Context", context_text);
                    }

                    self.ensure_trailing_blank_line(&mut lines);

                    // Action log box
                    let action_header_style = s_text_bold;
                    let chevron = if self.agents_terminal.actions_collapsed {
                        crate::icons::collapse_closed()
                    } else {
                        crate::icons::collapse_open()
                    };
                    let header_text = format!(
                        "╭ Action Log (a) {chevron} — {} entries ",
                        entry.logs.len()
                    );
                    let header_width = UnicodeWidthStr::width(header_text.as_str()) as u16;
                    let pad = detail_width
                        .saturating_sub(header_width)
                        .saturating_sub(1);
                    let mut action_header = header_text;
                    action_header.push_str(&"─".repeat(pad as usize));
                    action_header.push('╮');
                    lines.push(Line::from(Span::styled(action_header, action_header_style)));

                    if self.agents_terminal.actions_collapsed {
                        let mut footer = String::from("╰");
                        footer.push_str(&"─".repeat(detail_width.saturating_sub(1) as usize));
                        lines.push(Line::from(footer));
                        self.ensure_trailing_blank_line(&mut lines);
                    } else if entry.logs.is_empty() {
                        lines.push(Line::from(vec![
                            Span::raw("│   "),
                            Span::styled(
                                "No updates yet",
                                s_text_dim,
                            ),
                        ]));
                        let mut footer = String::from("╰");
                        footer.push_str(&"─".repeat(detail_width.saturating_sub(1) as usize));
                        lines.push(Line::from(footer));
                        self.ensure_trailing_blank_line(&mut lines);
                    } else {
                        let mut log_lines: Vec<Line> = Vec::new();
                        let mut last_kind: Option<AgentLogKind> = None;
                        for (idx, log) in entry.logs.iter().enumerate() {
                            let is_new_kind = last_kind != Some(log.kind);
                            self.append_agent_log_lines(
                                &mut log_lines,
                                idx,
                                log,
                                detail_width.saturating_sub(4),
                                is_new_kind,
                            );
                            last_kind = Some(log.kind);
                        }
                        for mut line in log_lines {
                            line.spans.insert(0, Span::raw("│   "));
                            lines.push(line);
                        }
                        let mut footer = String::from("╰");
                        footer.push_str(&"─".repeat(detail_width.saturating_sub(1) as usize));
                        lines.push(Line::from(footer));
                        self.ensure_trailing_blank_line(&mut lines);
                    }
                } else {
                    lines.push(Line::from(vec![
                        Span::raw(" "),
                        Span::styled(
                            "No data for selected agent",
                            s_text_dim,
                        ),
                    ]));
                }
            }
            None => {
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(
                        "No agents available",
                        s_text_dim,
                    ),
                ]));
            }
        }

        let content_width = right_area.width.saturating_sub(2).max(1);
        let wrapped_lines = word_wrap_lines(&lines, content_width as u16);
        let viewport_height = right_area.height.saturating_sub(2).max(1);
        let total_lines = wrapped_lines.len() as u16;
        let max_scroll = total_lines.saturating_sub(viewport_height);
        self.layout.last_history_viewport_height.set(viewport_height);
        self.layout.last_max_scroll.set(max_scroll);

        // scroll_offset is bottom‑anchored; Paragraph expects top‑anchored scroll.
        let preferred_offset = self
            .agents_terminal
            .current_sidebar_entry()
            .and_then(|entry| {
                self.agents_terminal
                    .scroll_offsets
                    .get(&entry.scroll_key())
                    .copied()
            })
            .unwrap_or(max_scroll);
        let clamped_offset = preferred_offset.min(max_scroll);
        self
            .agents_terminal
            .last_render_scroll
            .set(clamped_offset);
        let scroll_from_top = max_scroll.saturating_sub(clamped_offset);

        let detail_has_focus = self.agents_terminal.focus() == AgentsTerminalFocus::Detail;
        let detail_border_color = if detail_has_focus {
            c_primary
        } else {
            c_border
        };
        let history_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(detail_border_color));

        Paragraph::new(wrapped_lines)
            .block(history_block)
            .scroll((scroll_from_top, 0))
            .render(right_area, buf);

        if hint_height == 1 {
            let hint_line = if let Some(pending) = self.agents_terminal.pending_stop.as_ref() {
                Line::from(vec![
                    Span::styled(
                        "Stop agent? ",
                        crate::colors::style_error_bold(),
                    ),
                    Span::styled(
                        pending.agent_name.clone(),
                        s_text,
                    ),
                    Span::styled(
                        " — Enter/Y stop  ",
                        s_text_dim,
                    ),
                    Span::styled(
                        "Esc/N cancel",
                        s_text_dim,
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled(format!("[{ud}/{lr}]", ud = crate::icons::nav_up_down(), lr = crate::icons::nav_left_right()), s_function),
                    Span::styled(" navigate   ", s_text_dim),
                    Span::styled("[1-5]", s_function),
                    Span::styled(" filter   ", s_text_dim),
                    Span::styled("[S]", s_function),
                    Span::styled(" sort   ", s_text_dim),
                    Span::styled("[H/A]", s_function),
                    Span::styled(" toggle details   ", s_text_dim),
                    Span::styled("[X]", s_function),
                    Span::styled(" stop   ", s_text_dim),
                    Span::styled("[Ctrl+A]", s_function),
                    Span::styled(" exit", s_text_dim),
                ])
            };
            Paragraph::new(hint_line)
                .style(s_on_bg)
                .alignment(ratatui::layout::Alignment::Center)
                .render(hint_area, buf);
        }
    }
}
