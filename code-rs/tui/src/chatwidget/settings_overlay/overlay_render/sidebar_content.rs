impl SettingsOverlayView {
    fn render_sidebar(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let sections: Vec<SettingsSection> = if self.overview_rows.is_empty() {
            SettingsSection::ALL.to_vec()
        } else {
            self.overview_rows.iter().map(|row| row.section).collect()
        };

        fill_rect(
            buf,
            area,
            Some(' '),
            Style::default().bg(crate::colors::background()),
        );

        if sections.is_empty() {
            return;
        }

        let visible = area.height as usize;
        if visible == 0 {
            return;
        }

        let selected_idx = sections
            .iter()
            .position(|section| *section == self.active_section())
            .unwrap_or(0);

        let total = sections.len();
        let window = ListWindow::centered(total, visible, selected_idx);
        let start = window.start;
        let end = window.end;

        // Get current hover state
        let hovered = *self.hovered_section.borrow();

        let mut lines: Vec<Line<'static>> = Vec::new();
        for (idx, section) in sections.iter().copied().enumerate().take(end).skip(start) {
            let is_active = idx == selected_idx;
            let is_hovered = hovered == Some(section) && !is_active;
            let is_first_visible = idx == start;
            let is_last_visible = idx + 1 == end;

            let selection_indicator = if is_active {
                if self.is_sidebar_focused() {
                    "»"
                } else {
                    "›"
                }
            } else if is_hovered {
                "▸"
            } else {
                " "
            };
            let overflow_indicator = if is_first_visible && start > 0 {
                "↑"
            } else if is_last_visible && end < total {
                "↓"
            } else {
                " "
            };

            let mut spans: Vec<Span<'static>> = Vec::new();
            spans.push(Span::styled(
                selection_indicator.to_string(),
                Style::default().fg(crate::colors::text()),
            ));
            spans.push(Span::styled(
                overflow_indicator.to_string(),
                Style::default().fg(crate::colors::text_dim()),
            ));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                section.label(),
                if is_active {
                    Style::default()
                        .fg(crate::colors::text())
                        .add_modifier(Modifier::BOLD)
                } else if is_hovered {
                    Style::default()
                        .fg(crate::colors::text_bright())
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    Style::default().fg(crate::colors::text_dim())
                },
            ));

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
        }

        while lines.len() < visible {
            lines.push(Line::from(" "));
        }

        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(crate::colors::background()))
            .render(area, buf);
    }

    fn render_content(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
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
            SettingsSection::JsRepl => {
                if let Some(content) = self.js_repl_content.as_ref() {
                    content.render(area, buf);
                    return;
                }
                self.render_placeholder(area, buf, SettingsSection::JsRepl.placeholder());
            }
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
            .style(Style::default().fg(crate::colors::text_dim()));
        paragraph.render(area, buf);
    }
}
