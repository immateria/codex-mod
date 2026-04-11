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

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        let options = Self::get_theme_options();
        let theme = crate::theme::current_theme();
        let body_area = self.render_content_only_shell(area, buf);
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

    fn theme_shortcut_line(&self) -> Line<'static> {
        use crate::bottom_pane::settings_ui::hints::{hint_esc, hint_nav, shortcut_line, KeyHint};
        let esc_label = if matches!(self.mode, Mode::Overview) { " close" } else { " back" };
        shortcut_line(&[
            hint_nav(" navigate"),
            KeyHint::new(crate::icons::enter(), " choose"),
            hint_esc(esc_label),
        ])
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
            let pad_bg = Block::default().style(crate::colors::style_on_background());
            pad_bg.render(pad, buf);
        }

        let t_dim = crate::colors::style_text_dim();
        let t_fg = crate::colors::style_text();
        let title_spans = vec![Span::styled(" ", t_dim), Span::styled("/theme", t_fg)];

        let outer = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(title_spans))
            .style(crate::colors::style_on_background())
            .border_style(
                crate::colors::style_border_on_bg(),
            );
        let inner = outer.inner(render_area);
        outer.render(render_area, buf);

        // Paint inner content background as the normal theme background.
        let inner_bg_style = crate::colors::style_on_background();
        for y in inner.y..inner.y + inner.height {
            for x in inner.x..inner.x + inner.width {
                buf[(x, y)].set_style(inner_bg_style);
            }
        }

        // Reserve the last inner row for shortcut hints.
        let padded = inner.inner(crate::ui_consts::UNIFORM_PAD);
        if padded.height >= 2 {
            let hint_rect = Rect {
                x: padded.x,
                y: padded.y.saturating_add(padded.height.saturating_sub(1)),
                width: padded.width,
                height: 1,
            };
            Paragraph::new(self.theme_shortcut_line()).render(hint_rect, buf);
            Rect {
                x: padded.x,
                y: padded.y,
                width: padded.width,
                height: padded.height.saturating_sub(1),
            }
        } else {
            padded
        }
    }

    fn render_content_only_shell(&self, area: Rect, buf: &mut Buffer) -> Rect {
        if area.is_empty() {
            return Rect::default();
        }

        crate::util::buffer::fill_rect(
            buf,
            area,
            Some(' '),
            Style::new().bg(crate::colors::background()),
        );

        let header_rect = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };

        let t_dim = Style::new().fg(crate::colors::text_dim());
        let t_fg = Style::new().fg(crate::colors::text());

        let spans = vec![Span::styled(" ", t_dim), Span::styled("/theme", t_fg)];

        Paragraph::new(Line::from(spans))
            .style(Style::new().bg(crate::colors::background()))
            .render(header_rect, buf);

        // Reserve the last row for shortcut hints.
        let content_y = area.y.saturating_add(1);
        let remaining = area.height.saturating_sub(1);
        if remaining >= 2 {
            let hint_rect = Rect {
                x: area.x,
                y: content_y.saturating_add(remaining.saturating_sub(1)),
                width: area.width,
                height: 1,
            };
            Paragraph::new(self.theme_shortcut_line())
                .style(Style::new().bg(crate::colors::background()))
                .render(hint_rect, buf);

            Rect {
                x: area.x,
                y: content_y,
                width: area.width,
                height: remaining.saturating_sub(1),
            }
        } else {
            Rect {
                x: area.x,
                y: content_y,
                width: area.width,
                height: remaining,
            }
        }
    }
}
