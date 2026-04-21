impl SettingsOverlayView {
    fn render_sidebar(&self, area: Rect, buf: &mut Buffer) {

        if area.is_empty() {
            self.last_sidebar_line_hit_ranges.borrow_mut().clear();
            self.last_sidebar_line_indices.borrow_mut().clear();
            return;
        }

        fill_rect(
            buf,
            area,
            Some(' '),
            crate::colors::style_on_background(),
        );

        let visible = area.height as usize;
        if visible == 0 {
            self.last_sidebar_line_hit_ranges.borrow_mut().clear();
            self.last_sidebar_line_indices.borrow_mut().clear();
            return;
        }

        let active_section = self.active_section();
        let sidebar_focused = self.is_sidebar_focused();
        let hovered = *self.hovered_section.borrow();

        let mut lines: Vec<Line<'static>> = Vec::with_capacity(visible);
        let mut line_indices: Vec<Option<usize>> = Vec::with_capacity(visible);
        let mut line_hit_ranges: Vec<Option<(u16, u16)>> = Vec::with_capacity(visible);

        let area_end_x = area.x.saturating_add(area.width);

        let mut push_row = |idx: usize,
                            section: SettingsSection,
                            is_first_visible: bool,
                            is_last_visible: bool,
                            start: usize,
                            end: usize,
                            total: usize,
                            selected_idx: usize| {
            let is_active = idx == selected_idx;
            let is_hovered = hovered == Some(section) && !is_active;

            let selection_indicator: &'static str = if is_active {
                if sidebar_focused {
                    crate::icons::pointer_focused()
                } else {
                    crate::icons::pointer_active()
                }
            } else if is_hovered {
                crate::icons::arrow_right()
            } else {
                " "
            };
            let overflow_indicator: &'static str = if is_first_visible && start > 0 {
                crate::icons::arrow_up()
            } else if is_last_visible && end < total {
                crate::icons::arrow_down()
            } else {
                " "
            };

            let prefix_width = unicode_width::UnicodeWidthStr::width(selection_indicator)
                .saturating_add(unicode_width::UnicodeWidthStr::width(overflow_indicator))
                .saturating_add(1);
            let label_start = area
                .x
                .saturating_add(u16::try_from(prefix_width).unwrap_or(u16::MAX));

            let icon_prefix = crate::icons::section_icon(section.label());
            let icon_width = unicode_width::UnicodeWidthStr::width(icon_prefix);
            let label_width = unicode_width::UnicodeWidthStr::width(section.label());
            let label_end = label_start
                .saturating_add(u16::try_from(icon_width.saturating_add(label_width)).unwrap_or(u16::MAX))
                .min(area_end_x);
            let hit_range = (label_start < label_end).then_some((label_start, label_end));

            let label_style = if is_active {
                crate::colors::style_text_bold()
            } else if is_hovered {
                Style::default()
                    .fg(crate::colors::text_bright())
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                crate::colors::style_text_dim()
            };

            let mut spans: Vec<Span<'static>> = Vec::with_capacity(5);
            spans.push(Span::styled(selection_indicator, crate::colors::style_text()));
            spans.push(Span::styled(
                overflow_indicator,
                crate::colors::style_text_dim(),
            ));
            spans.push(Span::raw(" "));
            if !icon_prefix.is_empty() {
                spans.push(Span::styled(icon_prefix, label_style));
            }
            spans.push(Span::styled(section.label(), label_style));

            let line = if is_active {
                Line::from(spans).style(
                    Style::default()
                        .bg(crate::colors::selection())
                        .fg(crate::colors::text()),
                )
            } else if is_hovered {
                Line::from(spans).style(
                    Style::default()
                        .bg(crate::colors::border_focused())
                        .fg(crate::colors::text()),
                )
            } else {
                Line::from(spans)
            };

            lines.push(line);
            line_indices.push(Some(idx));
            line_hit_ranges.push(hit_range);
        };

        if self.overview_rows.is_empty() {
            let total = SettingsSection::ALL.len();
            if total == 0 {
                self.last_sidebar_line_hit_ranges.borrow_mut().clear();
                self.last_sidebar_line_indices.borrow_mut().clear();
                return;
            }
            let selected_idx = SettingsSection::ALL
                .iter()
                .position(|section| *section == active_section)
                .unwrap_or(0);
            let window = ListWindow::centered(total, visible, selected_idx);
            let start = window.start;
            let end = window.end;
            for idx in start..end {
                let section = SettingsSection::ALL[idx];
                push_row(
                    idx,
                    section,
                    idx == start,
                    idx + 1 == end,
                    start,
                    end,
                    total,
                    selected_idx,
                );
            }
        } else {
            let total = self.overview_rows.len();
            if total == 0 {
                self.last_sidebar_line_hit_ranges.borrow_mut().clear();
                self.last_sidebar_line_indices.borrow_mut().clear();
                return;
            }
            let selected_idx = self
                .overview_rows
                .iter()
                .position(|row| row.section == active_section)
                .unwrap_or(0);
            let window = ListWindow::centered(total, visible, selected_idx);
            let start = window.start;
            let end = window.end;
            for idx in start..end {
                let section = self.overview_rows[idx].section;
                push_row(
                    idx,
                    section,
                    idx == start,
                    idx + 1 == end,
                    start,
                    end,
                    total,
                    selected_idx,
                );
            }
        }

        while lines.len() < visible {
            lines.push(Line::from(" "));
            line_indices.push(None);
            line_hit_ranges.push(None);
        }

        *self.last_sidebar_line_hit_ranges.borrow_mut() = line_hit_ranges;
        *self.last_sidebar_line_indices.borrow_mut() = line_indices;

        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(crate::colors::style_on_background())
            .render(area, buf);
    }

    fn render_content(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        // Cache the panel inner area for mouse event forwarding
        *self.last_panel_inner_area.borrow_mut() = area;

        match self.active_section() {
            SettingsSection::Model => {
                if let Some(content) = self.model_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Model.placeholder());
            }
            SettingsSection::Planning => {
                if let Some(content) = self.planning_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Planning.placeholder());
            }
            SettingsSection::Personality => {
                if let Some(content) = self.personality_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Personality.placeholder());
            }
            SettingsSection::Theme => {
                if let Some(content) = self.theme_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Theme.placeholder());
            }
            SettingsSection::Interface => {
                if let Some(content) = self.interface_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Interface.placeholder());
            }
            SettingsSection::Shell => {
                if let Some(content) = self.shell_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Shell.placeholder());
            }
            SettingsSection::ShellEscalation => {
                if let Some(content) = self.shell_escalation_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::ShellEscalation.placeholder());
            }
            SettingsSection::ShellProfiles => {
                if let Some(content) = self.shell_profiles_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::ShellProfiles.placeholder());
            }
            SettingsSection::ExecLimits => {
                if let Some(content) = self.exec_limits_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::ExecLimits.placeholder());
            }
            SettingsSection::Updates => {
                if let Some(content) = self.updates_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Updates.placeholder());
            }
            SettingsSection::Agents => {
                if let Some(content) = self.agents_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Agents.placeholder());
            }
            SettingsSection::Accounts => {
                if let Some(content) = self.accounts_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Accounts.placeholder());
            }
            SettingsSection::Secrets => {
                if let Some(content) = self.secrets_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Secrets.placeholder());
            }
            SettingsSection::Apps => {
                if let Some(content) = self.apps_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Apps.placeholder());
            }
            SettingsSection::Memories => {
                if let Some(content) = self.memories_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Memories.placeholder());
            }
            SettingsSection::Prompts => {
                if let Some(content) = self.prompts_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Prompts.placeholder());
            }
            SettingsSection::Skills => {
                if let Some(content) = self.skills_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Skills.placeholder());
            }
            SettingsSection::Plugins => {
                if let Some(content) = self.plugins_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Plugins.placeholder());
            }
            SettingsSection::AutoDrive => {
                if let Some(content) = self.auto_drive_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::AutoDrive.placeholder());
            }
            SettingsSection::Review => {
                if let Some(content) = self.review_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Review.placeholder());
            }
            SettingsSection::Validation => {
                if let Some(content) = self.validation_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Validation.placeholder());
            }
            SettingsSection::Limits => {
                if let Some(content) = self.limits_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Limits.placeholder());
            }
            #[cfg(feature = "browser-automation")]
            SettingsSection::Chrome => {
                if let Some(content) = self.chrome_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Chrome.placeholder());
            }
            SettingsSection::Notifications => {
                if let Some(content) = self.notifications_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Notifications.placeholder());
            }
            SettingsSection::Repl => {
                if let Some(content) = self.repl_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Repl.placeholder());
            }
            #[cfg(feature = "managed-network-proxy")]
            SettingsSection::Network => {
                if let Some(content) = self.network_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Network.placeholder());
            }
            SettingsSection::Mcp => {
                if let Some(content) = self.mcp_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::Mcp.placeholder());
            }
        }
    }

    fn render_placeholder(&self, area: Rect, buf: &mut Buffer, text: &'static str) {
        let paragraph = Paragraph::new(text)
            .wrap(ratatui::widgets::Wrap { trim: true })
            .style(crate::colors::style_text_dim());
        paragraph.render(area, buf);
    }
}
