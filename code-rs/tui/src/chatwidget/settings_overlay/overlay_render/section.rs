impl SettingsOverlayView {
    fn render_section_layout(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
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
        if area.is_empty() {
            return;
        }

        // Auto-collapse the sidebar on very narrow screens (< 40 cols)
        // so the content pane gets enough room.
        let collapsed = self.sidebar_collapsed.get() || area.width < 40;

        if collapsed {
            // Collapsed: thin 2-col toggle strip + full-width content.
            let toggle_width: u16 = 2;
            let [toggle_col, main] =
                Layout::horizontal([Constraint::Length(toggle_width), Constraint::Fill(1)])
                    .areas(area);
            self.render_sidebar_toggle(toggle_col, buf, collapsed);
            *self.last_sidebar_area.borrow_mut() = Rect::default();
            self.render_section_panel(main, buf);
        } else {
            // Expanded: sidebar (with toggle row on top) + content.
            // Adaptive width: cap at 22 but shrink proportionally on
            // narrow screens so the content pane always gets ≥ 60% width.
            let sidebar_width = 22u16.min((u32::from(area.width) * 35 / 100) as u16).max(12);
            let [sidebar_col, main] =
                Layout::horizontal([Constraint::Length(sidebar_width), Constraint::Fill(1)])
                    .areas(area);

            if sidebar_col.height > 1 {
                let [toggle_row, sidebar_body] =
                    Layout::vertical([Constraint::Length(1), Constraint::Fill(1)])
                        .areas(sidebar_col);
                self.render_sidebar_toggle(toggle_row, buf, collapsed);
                *self.last_sidebar_area.borrow_mut() = sidebar_body;
                self.render_sidebar(sidebar_body, buf);
            } else {
                // Extremely short — just render sidebar, no toggle row.
                *self.last_sidebar_area.borrow_mut() = sidebar_col;
                *self.last_sidebar_toggle_area.borrow_mut() = Rect::default();
                self.render_sidebar(sidebar_col, buf);
            }
            self.render_section_panel(main, buf);
        }
    }

    fn render_sidebar_toggle(&self, area: Rect, buf: &mut Buffer, collapsed: bool) {
        if area.is_empty() {
            *self.last_sidebar_toggle_area.borrow_mut() = Rect::default();
            return;
        }

        fill_rect(
            buf,
            area,
            Some(' '),
            crate::colors::style_on_background(),
        );

        let symbol = if collapsed { crate::icons::sidebar_show() } else { crate::icons::sidebar_hide() };
        let style = Style::default()
            .fg(crate::colors::function())
            .bg(crate::colors::background());
        let draw_width = area.width.min(unicode_width::UnicodeWidthStr::width(symbol) as u16);
        let toggle_cell = Rect::new(area.x, area.y, draw_width, 1);
        Paragraph::new(Line::from(vec![Span::styled(symbol, style)]))
            .render(toggle_cell, buf);

        // Hit area covers the full row so it's easy to tap.
        let hit_area = Rect::new(area.x, area.y, area.width, 1);
        *self.last_sidebar_toggle_area.borrow_mut() = hit_area;
    }

    fn render_section_panel(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let title = self.active_section().panel_title();
        // Use horizontal-only margin so the shortcut bar sits flush against
        // the bottom border.  A 1-row top inset is applied manually below
        // to keep content from touching the title bar.
        let h_margin = u16::from(area.width >= 50);
        let mut style = SettingsPanelStyle::overlay().with_margin(Margin::new(h_margin, 0));
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
        // Inset 1 row from the top so content doesn't touch the title border,
        // but leave the bottom flush for the shortcut bar.
        let content = if layout.content.height > 1 {
            Rect {
                y: layout.content.y.saturating_add(1),
                height: layout.content.height.saturating_sub(1),
                ..layout.content
            }
        } else {
            layout.content
        };
        self.render_content(content, buf);
        self.strip_child_border(content, buf);
    }

    fn strip_child_border(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let background = crate::colors::style_text_on_bg();
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
        let bottom_is_frame = (area.height > 1).then(|| {
            bottom_has_corners
                && (area.x..=end_x).all(|x| {
                    let symbol = buf[(x, end_y)].symbol();
                    Self::is_border_symbol(symbol) || Self::is_corner_symbol(symbol)
                })
        });

        let left_has_corners =
            Self::is_corner_symbol(top_left_symbol) && Self::is_corner_symbol(bottom_left_symbol);
        let right_has_corners =
            Self::is_corner_symbol(top_right_symbol) && Self::is_corner_symbol(bottom_right_symbol);

        let left_is_frame = left_has_corners
            && (area.y..=end_y).all(|y| {
                let symbol = buf[(area.x, y)].symbol();
                Self::is_border_symbol(symbol) || Self::is_corner_symbol(symbol)
            });
        let right_is_frame = (area.width > 1).then(|| {
            right_has_corners
                && (area.y..=end_y).all(|y| {
                    let symbol = buf[(end_x, y)].symbol();
                    Self::is_border_symbol(symbol) || Self::is_corner_symbol(symbol)
                })
        });

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
            let hint = if area.width >= 10 { "↔ resize" } else { "…" };
            let hint_w = unicode_width::UnicodeWidthStr::width(hint) as u16;
            let x = area.x + area.width.saturating_sub(hint_w) / 2;
            let y = area.y + area.height / 2;
            buf.set_string(x, y, hint, crate::colors::style_text_dim());
            return;
        }

        let s_on_bg = crate::colors::style_on_background();

        fill_rect(
            buf,
            area,
            None,
            crate::colors::style_on_overlay_scrim(),
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
            s_on_bg,
        );

        Block::default()
            .borders(Borders::ALL)
            .border_style(crate::colors::style_border())
            .style(s_on_bg)
            .render(box_area, buf);

        let inner = box_area.inner(crate::ui_consts::UNIFORM_PAD);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        Paragraph::new(help.lines.clone())
            .alignment(Alignment::Left)
            .style(crate::colors::style_text_on_bg())
            .wrap(Wrap { trim: true })
            .render(inner, buf);
    }
}
