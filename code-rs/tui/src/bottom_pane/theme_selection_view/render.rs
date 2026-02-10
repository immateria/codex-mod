use super::*;

impl ThemeSelectionView {
    pub(super) fn render_content(&self, area: Rect, buf: &mut Buffer) {
        let options = Self::get_theme_options();
        let theme = crate::theme::current_theme();
        let body_area = self.render_shell(area, buf);
        if body_area.width == 0 || body_area.height == 0 {
            return;
        }

        let available_height = body_area.height as usize;

        match self.mode {
            Mode::Overview => self.render_overview_mode(body_area, &theme, buf),
            Mode::Themes => self.render_themes_mode(body_area, &theme, &options, buf),
            Mode::Spinner => self.render_spinner_mode(body_area, available_height, &theme, buf),
            Mode::CreateSpinner(_) => self.render_create_spinner_mode(body_area, &theme, buf),
            Mode::CreateTheme(_) => self.render_create_theme_mode(body_area, &theme, buf),
        }
    }

    fn render_shell(&self, area: Rect, buf: &mut Buffer) -> Rect {
        // Use full width and draw an outer window styled like the Diff overlay.
        let render_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height,
        };
        Clear.render(render_area, buf);

        // Add one row of padding above the top border (clear + background).
        if render_area.y > 0 {
            let pad = Rect {
                x: render_area.x,
                y: render_area.y - 1,
                width: render_area.width,
                height: 1,
            };
            Clear.render(pad, buf);
            let pad_bg = Block::default().style(Style::default().bg(crate::colors::background()));
            pad_bg.render(pad, buf);
        }

        // Build a styled title with concise hints.
        let t_dim = Style::default().fg(crate::colors::text_dim());
        let t_fg = Style::default().fg(crate::colors::text());
        let mut title_spans = vec![Span::styled(" ", t_dim), Span::styled("/theme", t_fg)];
        title_spans.extend_from_slice(&[
            Span::styled(" ——— ", t_dim),
            Span::styled("▲ ▼ ◀ ▶", t_fg),
            Span::styled(" select ", t_dim),
            Span::styled("——— ", t_dim),
            Span::styled("Enter", t_fg),
            Span::styled(" choose ", t_dim),
            Span::styled("——— ", t_dim),
            Span::styled("Esc", t_fg),
        ]);
        if matches!(self.mode, Mode::Overview) {
            title_spans.push(Span::styled(" close ", t_dim));
        } else {
            title_spans.push(Span::styled(" back ", t_dim));
        }

        let outer = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(title_spans))
            .style(Style::default().bg(crate::colors::background()))
            .border_style(
                Style::default()
                    .fg(crate::colors::border())
                    .bg(crate::colors::background()),
            );
        let inner = outer.inner(render_area);
        outer.render(render_area, buf);

        // Paint inner content background as the normal theme background.
        let inner_bg_style = Style::default().bg(crate::colors::background());
        for y in inner.y..inner.y + inner.height {
            for x in inner.x..inner.x + inner.width {
                buf[(x, y)].set_style(inner_bg_style);
            }
        }

        // Add one cell padding around the inside; body occupies full padded area.
        inner.inner(ratatui::layout::Margin::new(1, 1))
    }
}
