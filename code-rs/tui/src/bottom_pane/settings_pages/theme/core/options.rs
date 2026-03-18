use super::super::*;

impl ThemeSelectionView {
    pub(in crate::bottom_pane::settings_pages::theme) fn custom_theme_display_name() -> String {
        let Some(label0) = crate::theme::custom_theme_label() else {
            return "Custom".to_string();
        };

        // Sanitize any leading Light/Dark prefix the model may have included.
        let mut label = label0.trim().to_string();
        for pref in ["Light - ", "Dark - ", "Light ", "Dark "] {
            if label.starts_with(pref) {
                label = label[pref.len()..].trim().to_string();
                break;
            }
        }

        if crate::theme::custom_theme_is_dark().unwrap_or(false) {
            format!("Dark - {label}")
        } else {
            format!("Light - {label}")
        }
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn get_theme_options() -> Vec<(ThemeName, Cow<'static, str>, &'static str)> {
        let builtins = Self::builtin_theme_options();
        let mut out: Vec<(ThemeName, Cow<'static, str>, &'static str)> =
            Vec::with_capacity(builtins.len() + 1);
        out.extend(
            builtins
                .iter()
                .map(|(theme, name, description)| (*theme, Cow::Borrowed(*name), *description)),
        );

        if !matches!(palette_mode(), PaletteMode::Ansi16)
            && crate::theme::custom_theme_label().is_some()
        {
            out.push((
                ThemeName::Custom,
                Cow::Owned(Self::custom_theme_display_name()),
                "Your saved custom theme",
            ));
        }

        out
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn builtin_theme_options() -> &'static [ThemeOption] {
        if matches!(palette_mode(), PaletteMode::Ansi16) {
            THEME_OPTIONS_ANSI16
        } else {
            THEME_OPTIONS_ANSI256
        }
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn has_custom_theme_option() -> bool {
        !matches!(palette_mode(), PaletteMode::Ansi16)
            && crate::theme::custom_theme_label().is_some()
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn theme_option_count() -> usize {
        Self::builtin_theme_options().len() + if Self::has_custom_theme_option() { 1 } else { 0 }
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn theme_index_for(theme_name: ThemeName) -> usize {
        if let Some(idx) = Self::builtin_theme_options()
            .iter()
            .position(|(theme, _, _)| *theme == theme_name)
        {
            return idx;
        }

        if matches!(theme_name, ThemeName::Custom) && Self::has_custom_theme_option() {
            return Self::builtin_theme_options().len();
        }

        0
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn theme_name_for_option_index(index: usize) -> Option<ThemeName> {
        let builtins = Self::builtin_theme_options();
        if index < builtins.len() {
            return Some(builtins[index].0);
        }

        if Self::has_custom_theme_option() && index == builtins.len() {
            Some(ThemeName::Custom)
        } else {
            None
        }
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn allow_custom_theme_generation() -> bool {
        !matches!(palette_mode(), PaletteMode::Ansi16)
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn theme_list_count(options_len: usize) -> usize {
        options_len + if Self::allow_custom_theme_generation() { 1 } else { 0 }
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn visible_theme_rows(list_height: u16) -> usize {
        (list_height as usize).saturating_sub(1).clamp(1, 9)
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn theme_index_at_mouse_position(
        &self,
        mouse_event: MouseEvent,
        list_area: Rect,
        options_len: usize,
    ) -> Option<usize> {
        if list_area.width == 0 || list_area.height == 0 {
            return None;
        }
        if mouse_event.column < list_area.x
            || mouse_event.column >= list_area.x.saturating_add(list_area.width)
            || mouse_event.row < list_area.y
            || mouse_event.row >= list_area.y.saturating_add(list_area.height)
        {
            return None;
        }

        let rel_y = mouse_event.row.saturating_sub(list_area.y) as usize;
        if rel_y == 0 {
            return None;
        }
        let visible = Self::visible_theme_rows(list_area.height);
        let row = rel_y - 1;
        if row >= visible {
            return None;
        }

        let count = Self::theme_list_count(options_len);
        if count == 0 {
            return None;
        }
        let (start, _, _) =
            crate::util::list_window::anchored_window(self.selected_theme_index, count, visible);
        let idx = start + row;
        if idx < count {
            Some(idx)
        } else {
            None
        }
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn theme_display_name(theme_name: ThemeName) -> String {
        if matches!(theme_name, ThemeName::Custom) {
            return crate::theme::custom_theme_label().unwrap_or_else(|| "Custom".to_string());
        }
        Self::builtin_theme_options()
            .iter()
            .find(|(candidate, _, _)| *candidate == theme_name)
            .map(|(_, name, _)| (*name).to_string())
            .unwrap_or_else(|| "Theme".to_string())
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn render_theme_option_lines_for_palette(
        &self,
        area: Rect,
        palette: &crate::theme::Theme,
        options: &[(ThemeName, Cow<'static, str>, &'static str)],
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        lines.push(Line::from(Span::styled(
            "Choose Theme",
            Style::default()
                .bg(palette.background)
                .fg(palette.text_bright)
                .add_modifier(Modifier::BOLD),
        )));

        let allow_custom = Self::allow_custom_theme_generation();
        let count = options.len() + if allow_custom { 1 } else { 0 };
        if count == 0 {
            return lines;
        }

        let visible = Self::visible_theme_rows(area.height);
        let (start, _, _) =
            crate::util::list_window::anchored_window(self.selected_theme_index, count, visible);
        let end = (start + visible).min(count);
        let hovered = self.hovered_theme_index;

        for i in start..end {
            let is_selected = i == self.selected_theme_index;
            let is_hovered = hovered == Some(i);
            if allow_custom && i >= options.len() {
                let mut spans = vec![Span::raw(" ")];
                if is_selected {
                    spans.push(Span::styled("› ", Style::default().fg(palette.keyword)));
                } else if is_hovered {
                    spans.push(Span::styled("• ", Style::default().fg(palette.info)));
                } else {
                    spans.push(Span::raw("  "));
                }
                let label_style = if is_selected || is_hovered {
                    Style::default()
                        .fg(palette.primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(palette.text_dim)
                };
                spans.push(Span::styled("Generate your own…", label_style));
                lines.push(Line::from(spans));
                continue;
            }

            let (theme_enum, name, description) = &options[i];
            let is_original = *theme_enum == self.original_theme;
            let mut spans = vec![Span::raw(" ")];
            if is_selected {
                spans.push(Span::styled("› ", Style::default().fg(palette.keyword)));
            } else if is_hovered {
                spans.push(Span::styled("• ", Style::default().fg(palette.info)));
            } else {
                spans.push(Span::raw("  "));
            }

            if is_selected || is_hovered {
                spans.push(Span::styled(
                    name.clone(),
                    Style::default()
                        .fg(palette.primary)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(name.clone(), Style::default().fg(palette.text)));
            }

            if is_original {
                spans.push(Span::styled(" (original)", Style::default().fg(palette.text_dim)));
                spans.push(Span::raw(" "));
            } else {
                spans.push(Span::raw("  "));
            }

            spans.push(Span::styled(
                *description,
                Style::default().fg(palette.text_dim),
            ));
            lines.push(Line::from(spans));
        }

        lines
    }

}
