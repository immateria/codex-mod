impl SettingsOverlayView {
    fn render_section_layout(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let (main_area, hint_area) = if area.height <= 1 {
            (area, None)
        } else {
            let [main, hint] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
            (main, Some(hint))
        };

        self.render_section_main(main_area, buf);
        if let Some(hint_area) = hint_area {
            self.render_footer_hints_section(hint_area, buf);
        }
    }

    fn render_section_main(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let [sidebar, main] =
            Layout::horizontal([Constraint::Length(22), Constraint::Fill(1)]).areas(area);
        *self.last_sidebar_area.borrow_mut() = sidebar;

        self.render_sidebar(sidebar, buf);
        self.render_section_panel(main, buf);
    }

    fn render_section_panel(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let title = Self::section_panel_title(self.active_section());
        let mut style = SettingsPanelStyle::overlay().with_margin(Margin::new(1, 1));
        style.border_style = Style::default()
            .fg(if self.is_content_focused() {
                crate::colors::border_focused()
            } else {
                crate::colors::border_dim()
            })
            .bg(crate::colors::background());
        let panel = SettingsPanel::new(title, style);
        let Some(layout) = panel.render(area, buf) else {
            return;
        };
        self.render_content(layout.content, buf);
        self.strip_child_border(layout.content, buf);
    }

    fn section_panel_title(section: SettingsSection) -> &'static str {
        match section {
            SettingsSection::Model => "Select Model & Reasoning",
            SettingsSection::Theme => "Theme Settings",
            SettingsSection::Interface => "Interface",
            SettingsSection::Experimental => "Experimental Features",
            SettingsSection::Shell => "Shell Selection",
            SettingsSection::ShellProfiles => "Shell Profiles",
            SettingsSection::ExecLimits => "Exec Limits",
            SettingsSection::Planning => "Planning Settings",
            SettingsSection::Updates => "Upgrade",
            SettingsSection::Accounts => "Account Switching",
            SettingsSection::Secrets => "Secrets",
            SettingsSection::Apps => "Apps",
            SettingsSection::Agents => "Agents",
            SettingsSection::Memories => "Memories",
            SettingsSection::Skills => "Skills",
            SettingsSection::Plugins => "Plugins",
            SettingsSection::AutoDrive => "Auto Drive Settings",
            SettingsSection::Review => "Review Settings",
            SettingsSection::Validation => "Validation Settings",
            SettingsSection::Limits => "Rate Limits",
            SettingsSection::Chrome => "Chrome Launch Options",
            SettingsSection::Notifications => "Notifications",
            SettingsSection::JsRepl => "JS REPL",
            #[cfg(feature = "managed-network-proxy")]
            SettingsSection::Network => "Network Mediation",
            SettingsSection::Mcp => "MCP Servers",
            SettingsSection::Prompts => "Custom Prompts",
        }
    }

    fn strip_child_border(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let background = Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text());
        let end_x = area.x + area.width - 1;
        let end_y = area.y + area.height - 1;

        let top_left_symbol = buf[(area.x, area.y)].symbol();
        let top_right_symbol = buf[(end_x, area.y)].symbol();
        let bottom_left_symbol = buf[(area.x, end_y)].symbol();
        let bottom_right_symbol = buf[(end_x, end_y)].symbol();

        let top_has_corners =
            Self::is_corner_symbol(top_left_symbol) && Self::is_corner_symbol(top_right_symbol);
        let bottom_has_corners = Self::is_corner_symbol(bottom_left_symbol)
            && Self::is_corner_symbol(bottom_right_symbol);

        let top_is_frame = top_has_corners
            && (area.x..=end_x).all(|x| {
                let symbol = buf[(x, area.y)].symbol();
                Self::is_border_symbol(symbol) || Self::is_corner_symbol(symbol)
            });
        let bottom_is_frame = if area.height > 1 {
            Some(bottom_has_corners
                && (area.x..=end_x).all(|x| {
                    let symbol = buf[(x, end_y)].symbol();
                    Self::is_border_symbol(symbol) || Self::is_corner_symbol(symbol)
                }))
        } else {
            None
        };

        let left_has_corners =
            Self::is_corner_symbol(top_left_symbol) && Self::is_corner_symbol(bottom_left_symbol);
        let right_has_corners =
            Self::is_corner_symbol(top_right_symbol) && Self::is_corner_symbol(bottom_right_symbol);

        let left_is_frame = left_has_corners
            && (area.y..=end_y).all(|y| {
                let symbol = buf[(area.x, y)].symbol();
                Self::is_border_symbol(symbol) || Self::is_corner_symbol(symbol)
            });
        let right_is_frame = if area.width > 1 {
            Some(right_has_corners
                && (area.y..=end_y).all(|y| {
                    let symbol = buf[(end_x, y)].symbol();
                    Self::is_border_symbol(symbol) || Self::is_corner_symbol(symbol)
                }))
        } else {
            None
        };

        if top_is_frame {
            for x in area.x..=end_x {
                let cell = &mut buf[(x, area.y)];
                cell.set_symbol(" ");
                cell.set_style(background);
            }
        }

        if let Some(true) = bottom_is_frame {
            for x in area.x..=end_x {
                let cell = &mut buf[(x, end_y)];
                cell.set_symbol(" ");
                cell.set_style(background);
            }
        }

        if left_is_frame {
            for y in area.y..=end_y {
                let cell = &mut buf[(area.x, y)];
                cell.set_symbol(" ");
                cell.set_style(background);
            }
        }

        if let Some(true) = right_is_frame {
            for y in area.y..=end_y {
                let cell = &mut buf[(end_x, y)];
                cell.set_symbol(" ");
                cell.set_style(background);
            }
        }
    }

    fn is_border_symbol(symbol: &str) -> bool {
        matches!(
            symbol,
            "│" | "┃" | "║" | "╎" | "┆" | "┊" | "┇" | "╏" | "╿"
                | "─" | "━" | "═" | "╼" | "╾" | "┄" | "┈" | "╍"
                | "┬" | "┴" | "├" | "┤" | "┼" | "╞" | "╡" | "╪" | "╫"
        )
    }

    fn is_corner_symbol(symbol: &str) -> bool {
        matches!(symbol, "┌" | "┐" | "└" | "┘" | "╭" | "╮" | "╰" | "╯")
    }

    fn render_help_overlay(&self, area: Rect, buf: &mut Buffer, help: &SettingsHelpOverlay) {
        if area.width < 4 || area.height < 4 {
            return;
        }

        fill_rect(
            buf,
            area,
            None,
            Style::default().bg(crate::colors::overlay_scrim()),
        );

        let content_width = help.lines.iter().map(Line::width).max().unwrap_or(0);
        let content_height = help.lines.len() as u16;

        let max_box_width = area.width.saturating_sub(2);
        let mut box_width = content_width
            .saturating_add(4)
            .min(max_box_width as usize)
            .max(20.min(max_box_width as usize));
        if box_width == 0 {
            box_width = max_box_width as usize;
        }
        let box_width = box_width.min(area.width as usize) as u16;

        let max_box_height = area.height.saturating_sub(2);
        let mut box_height = content_height.saturating_add(2).min(max_box_height);
        if box_height < 4 {
            box_height = max_box_height.min(4);
        }
        if box_height == 0 {
            box_height = area.height;
        }

        let box_x = area.x + (area.width.saturating_sub(box_width)) / 2;
        let box_y = area.y + (area.height.saturating_sub(box_height)) / 2;
        let box_area = Rect::new(box_x, box_y, box_width, box_height);

        fill_rect(
            buf,
            box_area,
            Some(' '),
            Style::default().bg(crate::colors::background()),
        );

        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()))
            .render(box_area, buf);

        let inner = box_area.inner(Margin::new(1, 1));
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        Paragraph::new(help.lines.clone())
            .alignment(Alignment::Left)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .wrap(Wrap { trim: true })
            .render(inner, buf);
    }
}
